use std::time::Duration;

use keeppix_domain::{AuthContext, SessionToken, SystemRole, UserId};
use uuid::Uuid;

use crate::{Db, DbError};

pub struct SessionRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct AuthRow {
    user_id: uuid::Uuid,
    role: String,
}

impl AuthRow {
    fn into_domain(self) -> Result<AuthContext, DbError> {
        // Stessa tassonomia di `users.rs` (ruling R3): un valore che il codice
        // non sa interpretare è `Corrupted`, non un ruolo degradato a `User`.
        // Il CHECK sulla colonna lo rende irraggiungibile e la degradazione
        // falliva chiusa, ma due moduli che trattano lo stesso dato in modo
        // diverso rendono inaffidabile il triage sugli errori.
        let role = match self.role.as_str() {
            "admin" => SystemRole::Admin,
            "user" => SystemRole::User,
            other => return Err(crate::row::corrupted("role", other)),
        };
        Ok(AuthContext::user(UserId::from_uuid(self.user_id), role))
    }
}

#[derive(sqlx::FromRow)]
struct RotateRow {
    id: uuid::Uuid,
    family_id: uuid::Uuid,
    user_id: uuid::Uuid,
    consumed_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: chrono::DateTime<chrono::Utc>,
    db_now: chrono::DateTime<chrono::Utc>,
}

impl<'a> SessionRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Apre una nuova famiglia di sessione. Il token in chiaro è restituito
    /// una sola volta: nel database resta solo il digest.
    ///
    /// # Errors
    /// `DbError::Connection` se l'inserimento fallisce.
    pub async fn create(
        &self,
        user_id: UserId,
        ttl: Duration,
        user_agent: Option<&str>,
    ) -> Result<SessionToken, DbError> {
        let token = SessionToken::generate();
        let id = Uuid::now_v7();

        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, user_agent, expires_at) \
             VALUES ($1, $1, $2, $3, $4, now() + $5::interval)",
        )
        .bind(id)
        .bind(user_id.as_uuid())
        .bind(token.digest().as_slice())
        .bind(user_agent)
        .bind(interval(ttl))
        .execute(self.db.pool())
        .await?;

        Ok(token)
    }

    /// # Errors
    /// `DbError::NotFound` se il token è sconosciuto, scaduto, consumato,
    /// revocato, oppure se l'utente è disabilitato.
    pub async fn authenticate(&self, token: &SessionToken) -> Result<AuthContext, DbError> {
        let row: Option<AuthRow> = sqlx::query_as(
            "SELECT u.id AS user_id, u.role \
               FROM sessions s JOIN users u ON u.id = s.user_id \
              WHERE s.refresh_token_hash = $1 \
                AND s.consumed_at IS NULL \
                AND s.revoked_at IS NULL \
                AND s.expires_at > now() \
                AND u.disabled_at IS NULL",
        )
        .bind(token.digest().as_slice())
        .fetch_optional(self.db.pool())
        .await?;

        row.ok_or(DbError::NotFound)?.into_domain()
    }

    /// Ruota il token. Se quello presentato risulta **già consumato**, l'unica
    /// spiegazione è che una copia sia in mano a qualcun altro: si revoca
    /// l'intera famiglia e si costringe a un nuovo login.
    ///
    /// # Errors
    /// `DbError::Forbidden` in caso di riuso rilevato; `DbError::NotFound` se
    /// il token non esiste o è scaduto.
    pub async fn rotate(
        &self,
        token: &SessionToken,
        ttl: Duration,
    ) -> Result<SessionToken, DbError> {
        let mut tx = self.db.pool().begin().await?;

        let row: Option<RotateRow> = sqlx::query_as(
            "SELECT id, family_id, user_id, consumed_at, revoked_at, expires_at, now() AS db_now \
               FROM sessions WHERE refresh_token_hash = $1 FOR UPDATE",
        )
        .bind(token.digest().as_slice())
        .fetch_optional(&mut *tx)
        .await?;
        let row = row.ok_or(DbError::NotFound)?;

        if row.consumed_at.is_some() {
            sqlx::query(
                "UPDATE sessions SET revoked_at = now() \
                  WHERE family_id = $1 AND revoked_at IS NULL",
            )
            .bind(row.family_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Err(DbError::Forbidden);
        }

        if row.revoked_at.is_some() {
            return Err(DbError::NotFound);
        }

        if row.expires_at <= row.db_now {
            return Err(DbError::NotFound);
        }

        sqlx::query("UPDATE sessions SET consumed_at = now() WHERE id = $1")
            .bind(row.id)
            .execute(&mut *tx)
            .await?;

        let next = SessionToken::generate();
        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, parent_id, expires_at) \
             VALUES ($1, $2, $3, $4, $5, now() + $6::interval)",
        )
        .bind(Uuid::now_v7())
        .bind(row.family_id)
        .bind(row.user_id)
        .bind(next.digest().as_slice())
        .bind(row.id)
        .bind(interval(ttl))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(next)
    }

    /// # Errors
    /// `DbError::Connection` se l'aggiornamento fallisce.
    pub async fn revoke(&self, token: &SessionToken) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE refresh_token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token.digest().as_slice())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Revoca tutte le sessioni di un utente. Usato quando lo si disabilita.
    ///
    /// # Errors
    /// `DbError::Connection`.
    pub async fn revoke_all_for_user(
        &self,
        user_id: keeppix_domain::UserId,
    ) -> Result<u64, DbError> {
        let result = sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Revoca tutte le famiglie dell'utente tranne quella del token corrente
    /// (cambio password: le altre sessioni devono cadere).
    ///
    /// # Errors
    /// `DbError::Connection`.
    pub async fn revoke_other_families(
        &self,
        user_id: keeppix_domain::UserId,
        keep_token: &SessionToken,
    ) -> Result<u64, DbError> {
        let result = sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE user_id = $1 AND revoked_at IS NULL \
                AND family_id <> ( \
                  SELECT family_id FROM sessions \
                   WHERE refresh_token_hash = $2 LIMIT 1 \
                )",
        )
        .bind(user_id.as_uuid())
        .bind(keep_token.digest().as_slice())
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// # Errors
    /// `DbError::Connection` se la cancellazione fallisce.
    pub async fn purge_expired(&self) -> Result<u64, DbError> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
            .execute(self.db.pool())
            .await?;
        Ok(result.rows_affected())
    }
}

/// Postgres non accetta un `Duration` di Rust: si passa un intervallo in
/// secondi, **frazionari**. `as_secs()` troncava: un TTL di 500 ms diventava
/// `"0 seconds"`, cioè un token nato scaduto senza alcun errore — silenzioso e
/// insidioso, e il primo test che vorrà osservare una scadenza senza aspettare
/// un secondo intero ci sbatterebbe contro. `as_secs_f64()` arriva ben oltre la
/// precisione al microsecondo di `interval`, e un TTL zero resta `"0 seconds"`,
/// cioè un token già scaduto: proprietà su cui poggiano i test esistenti.
fn interval(ttl: Duration) -> String {
    format!("{} seconds", ttl.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::interval;
    use std::time::Duration;

    #[test]
    fn sub_second_ttls_survive_the_round_trip() {
        assert_eq!(interval(Duration::from_millis(500)), "0.5 seconds");
        assert_eq!(interval(Duration::from_micros(1500)), "0.0015 seconds");
        // Un TTL zero deve restare zero: i test sulla scadenza immediata ci
        // contano.
        assert_eq!(interval(Duration::ZERO), "0 seconds");
        // I valori interi non guadagnano decimali: `2592000 seconds`, non
        // `2592000.0 seconds`.
        assert_eq!(interval(Duration::from_secs(2_592_000)), "2592000 seconds");
    }
}
