//! App-password: credenziali dedicate per client non interattivi (`WebDAV`,
//! Fase 5 Task 5+). Vedi `keeppix_domain::credential` per i tipi di
//! dominio.

use chrono::{DateTime, Utc};
use keeppix_domain::{
    AppPasswordId, AppPasswordSecret, AppPasswordSummary, AuthContext, Password, PasswordHash,
    UserId, hash_password, verify_password,
};
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(sqlx::FromRow)]
struct SummaryRow {
    id: Uuid,
    user_id: Uuid,
    label: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl SummaryRow {
    fn into_domain(self) -> AppPasswordSummary {
        AppPasswordSummary {
            id: AppPasswordId::from_uuid(self.id),
            user_id: UserId::from_uuid(self.user_id),
            label: self.label,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
        }
    }
}

pub struct AppPasswordRepo<'a> {
    db: &'a Db,
}

impl<'a> AppPasswordRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Crea un'app-password per l'utente autenticato. Il segreto in chiaro
    /// viene restituito una sola volta, insieme al riepilogo: da qui in
    /// avanti solo l'hash Argon2id resta in `secret_hash`, e nessuna
    /// chiamata successiva può più recuperarlo.
    ///
    /// # Errors
    /// `DbError::Forbidden` se il chiamante non è un utente autenticato — un
    /// link condiviso non ha un'identità a cui legare l'app-password.
    pub async fn create(
        &self,
        ctx: &AuthContext,
        label: String,
    ) -> Result<(AppPasswordSummary, AppPasswordSecret), DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let secret = AppPasswordSecret::generate();
        let password = Password::parse(secret.expose()).map_err(|e| {
            DbError::Corrupted(format!("generated app-password secret rejected: {e}"))
        })?;
        let hash = hash_password(&password)
            .map_err(|e| DbError::Corrupted(format!("app-password hashing failed: {e}")))?;

        let row: SummaryRow = sqlx::query_as(
            "INSERT INTO app_passwords (id, user_id, label, secret_hash) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, user_id, label, created_at, last_used_at",
        )
        .bind(AppPasswordId::new().as_uuid())
        .bind(user_id.as_uuid())
        .bind(&label)
        .bind(hash.as_str())
        .fetch_one(self.db.pool())
        .await?;

        Ok((row.into_domain(), secret))
    }

    /// Verifica `username:secret` per l'autenticazione HTTP Basic
    /// pre-sessione (`WebDAV`, Task 5+). **Eccezione documentata**
    /// all'invariante «ogni metodo che legge dati di un utente prende
    /// `AuthContext`»: qui non esiste ancora un contesto — esattamente come
    /// `UserRepo::find_by_username` per il login a sessione. Solo le
    /// app-password non revocate entrano nella lista dei candidati, quindi
    /// una revoca ha effetto immediato senza bisogno di invalidare alcuna
    /// cache: non ce n'è una. Un successo aggiorna `last_used_at` in
    /// fire-and-forget, per non far pagare al percorso di richiesta il
    /// costo di uno scritto.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn verify(&self, username: &str, secret: &str) -> Result<Option<UserId>, DbError> {
        let Ok(password) = Password::parse(secret) else {
            return Ok(None);
        };

        let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
            "SELECT ap.id, ap.user_id, ap.secret_hash \
               FROM app_passwords ap \
               JOIN users u ON u.id = ap.user_id \
              WHERE lower(u.username) = lower($1) AND ap.revoked_at IS NULL",
        )
        .bind(username)
        .fetch_all(self.db.pool())
        .await?;

        for (id, user_id, secret_hash) in rows {
            let stored = PasswordHash::from_stored(secret_hash);
            if verify_password(&password, &stored) {
                let db = self.db.clone();
                tokio::spawn(async move {
                    let _ =
                        sqlx::query("UPDATE app_passwords SET last_used_at = now() WHERE id = $1")
                            .bind(id)
                            .execute(db.pool())
                            .await;
                });
                return Ok(Some(UserId::from_uuid(user_id)));
            }
        }

        Ok(None)
    }

    /// Elenco delle app-password non revocate dell'utente autenticato.
    ///
    /// # Errors
    /// `DbError::Forbidden` se il chiamante non è un utente autenticato.
    pub async fn list(&self, ctx: &AuthContext) -> Result<Vec<AppPasswordSummary>, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let rows: Vec<SummaryRow> = sqlx::query_as(
            "SELECT id, user_id, label, created_at, last_used_at \
               FROM app_passwords \
              WHERE user_id = $1 AND revoked_at IS NULL \
              ORDER BY created_at DESC",
        )
        .bind(user_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(SummaryRow::into_domain).collect())
    }

    /// Revoca immediata: solo il proprietario, o un admin. Un id che
    /// appartiene a un altro utente restituisce `Forbidden`, mai
    /// `NotFound` — altrimenti l'endpoint diventa un oracolo di esistenza
    /// (stessa regola di `UploadSessionRepo::load_owned`). Idempotente: una
    /// seconda revoca sullo stesso id non fallisce e non tocca di nuovo
    /// `revoked_at`.
    ///
    /// # Errors
    /// `DbError::Forbidden` se il chiamante non possiede l'id, o se l'id non
    /// esiste e il chiamante non è admin. `DbError::NotFound` solo per un
    /// admin che chiede un id davvero inesistente.
    pub async fn revoke(&self, ctx: &AuthContext, id: AppPasswordId) -> Result<(), DbError> {
        let owner: Option<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM app_passwords WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;

        let Some(owner) = owner else {
            return Err(if ctx.is_admin() {
                DbError::NotFound
            } else {
                DbError::Forbidden
            });
        };

        if !ctx.is_admin() && ctx.user_id() != Some(UserId::from_uuid(owner)) {
            return Err(DbError::Forbidden);
        }

        sqlx::query(
            "UPDATE app_passwords SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}
