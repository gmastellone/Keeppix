use std::time::Duration;

use chrono::{DateTime, Utc};
use keeppix_domain::{AuthContext, SessionId, SessionToken, SystemRole, UserId};
use uuid::Uuid;

use crate::{Db, DbError};

/// Riga di `GET /users/me/sessions`: una per famiglia, cioè per
/// dispositivo/login (vedi `SessionId`), non per riga di `sessions` — le
/// righe consumate dalla rotazione non sono un dispositivo in più.
pub struct SessionSummary {
    pub id: SessionId,
    pub device_label: Option<String>,
    /// Quando la riga attiva della famiglia è nata: al login, o all'ultima
    /// rotazione (`POST /auth/refresh`) se ce n'è stata una. È
    /// l'approssimazione di "ultimo utilizzo" più economica disponibile
    /// senza un contatore aggiornato a ogni richiesta autenticata (che
    /// `authenticate()` esplicitamente non fa: vedi
    /// `authenticate_does_not_slide_expiry`).
    pub last_seen_at: DateTime<Utc>,
    pub current: bool,
}

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
    device_label: Option<String>,
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
        let device_label = device_label_from_user_agent(user_agent);

        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, device_label, expires_at) \
             VALUES ($1, $1, $2, $3, $4, now() + $5::interval)",
        )
        .bind(id)
        .bind(user_id.as_uuid())
        .bind(token.digest().as_slice())
        .bind(&device_label)
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
            "SELECT s.id, s.family_id, s.user_id, s.device_label, s.consumed_at, s.revoked_at, \
                    s.expires_at, now() AS db_now \
               FROM sessions s \
               JOIN users u ON u.id = s.user_id \
              WHERE s.refresh_token_hash = $1 AND u.disabled_at IS NULL \
              FOR UPDATE OF s",
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
                 (id, family_id, user_id, refresh_token_hash, parent_id, device_label, \
                  expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, now() + $7::interval)",
        )
        .bind(Uuid::now_v7())
        .bind(row.family_id)
        .bind(row.user_id)
        .bind(next.digest().as_slice())
        .bind(row.id)
        .bind(&row.device_label)
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

    /// Famiglia del token presentato — cioè l'id di sessione (`SessionId`)
    /// che `GET /users/me/sessions` deve marcare `current: true`. **Eccezione
    /// documentata** all'invariante "ogni metodo che legge dati di un utente
    /// prende `AuthContext`" per lo stesso motivo di `authenticate`: il
    /// token stesso è già la credenziale, e serve a costruire il confronto
    /// *prima* che l'elenco delle sessioni (che quello sì prende
    /// `AuthContext`) venga interrogato.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce. Un token sconosciuto non è
    /// un errore: restituisce `Ok(None)`, lasciando a chi chiama decidere se
    /// è una situazione anomala (l'estrattore `Auth` ha già verificato che il
    /// token autentica, quindi in pratica arriva sempre `Some`).
    pub async fn family_of(&self, token: &SessionToken) -> Result<Option<SessionId>, DbError> {
        let family: Option<Uuid> =
            sqlx::query_scalar("SELECT family_id FROM sessions WHERE refresh_token_hash = $1")
                .bind(token.digest().as_slice())
                .fetch_optional(self.db.pool())
                .await?;
        Ok(family.map(SessionId::from_uuid))
    }

    /// Elenco delle sessioni attive dell'utente autenticato, una per
    /// famiglia: `consumed_at IS NULL AND revoked_at IS NULL AND expires_at >
    /// now()` seleziona esattamente la riga viva di ciascuna famiglia, perché
    /// la rotazione (`rotate`) marca sempre `consumed_at` sulla riga
    /// superata prima di inserirne una nuova — non serve un `GROUP BY`.
    ///
    /// # Errors
    /// `DbError::Forbidden` se il chiamante non è un utente autenticato.
    pub async fn list_active(
        &self,
        ctx: &AuthContext,
        current: SessionId,
    ) -> Result<Vec<SessionSummary>, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let rows: Vec<ActiveRow> = sqlx::query_as(
            "SELECT family_id, device_label, created_at, (family_id = $2) AS current \
               FROM sessions \
              WHERE user_id = $1 AND consumed_at IS NULL AND revoked_at IS NULL \
                AND expires_at > now() \
              ORDER BY created_at DESC",
        )
        .bind(user_id.as_uuid())
        .bind(current.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(ActiveRow::into_domain).collect())
    }

    /// Revoca una famiglia di sessioni (`GET .../sessions` la elenca come una
    /// riga, non come le sue rotazioni). Solo il proprietario, mai un altro
    /// utente: un id che appartiene ad altri risponde `Forbidden`, mai
    /// `NotFound` — stessa regola di `AppPasswordRepo::revoke`, e per lo
    /// stesso motivo (oracolo di esistenza). **Non** verifica se `family` è
    /// la famiglia corrente: quel controllo (400, non 403/404 — non è un
    /// problema di proprietà) vive nell'handler HTTP, che ha già il token
    /// corrente sotto mano e non deve interrogare il database per farlo.
    ///
    /// # Errors
    /// `DbError::Forbidden` se il chiamante non possiede la famiglia, o se la
    /// famiglia non esiste.
    pub async fn revoke_family(&self, ctx: &AuthContext, family: SessionId) -> Result<(), DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;

        let owner: Option<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM sessions WHERE family_id = $1 LIMIT 1")
                .bind(family.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        let Some(owner) = owner else {
            return Err(DbError::Forbidden);
        };
        if owner != user_id.as_uuid() {
            return Err(DbError::Forbidden);
        }

        sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE family_id = $1 AND revoked_at IS NULL",
        )
        .bind(family.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct ActiveRow {
    family_id: Uuid,
    device_label: Option<String>,
    created_at: DateTime<Utc>,
    current: bool,
}

impl ActiveRow {
    fn into_domain(self) -> SessionSummary {
        SessionSummary {
            id: SessionId::from_uuid(self.family_id),
            device_label: self.device_label,
            last_seen_at: self.created_at,
            current: self.current,
        }
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

/// Etichetta breve ("Chrome on macOS") derivata da uno `User-Agent`, per
/// mostrare in `GET /users/me/sessions` *quale* dispositivo tiene una
/// sessione senza conservarne il valore completo (spec fase-10 §8.1: lo
/// `User-Agent` intero è più dato personale del necessario). `None` solo
/// quando l'header stesso manca; un header presente ma non riconosciuto
/// produce "Unknown device", non `None` — l'utente ha comunque un
/// dispositivo, semplicemente non ne sappiamo il nome.
///
/// L'ordine dei controlli conta: le sottostringhe dei browser/OS più diffusi
/// si contengono a vicenda nello `User-Agent` reale (Edge e Chrome contengono
/// entrambi `Safari/`; Chrome contiene `Safari/`; un iPhone dichiara "like
/// Mac OS X"), quindi il primo controllo che matcha deve essere il più
/// specifico.
fn device_label_from_user_agent(user_agent: Option<&str>) -> Option<String> {
    let ua = user_agent?;

    let os = if ua.contains("iPhone") || ua.contains("iPad") || ua.contains("iPod") {
        Some("iOS")
    } else if ua.contains("Android") {
        Some("Android")
    } else if ua.contains("Windows") {
        Some("Windows")
    } else if ua.contains("Mac OS X") {
        Some("macOS")
    } else if ua.contains("Linux") {
        Some("Linux")
    } else {
        None
    };

    let browser = if ua.contains("Edg/") || ua.contains("Edge/") {
        Some("Edge")
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        Some("Opera")
    } else if ua.contains("Firefox/") {
        Some("Firefox")
    } else if ua.contains("Chrome/") || ua.contains("CriOS/") {
        Some("Chrome")
    } else if ua.contains("Safari/") {
        Some("Safari")
    } else {
        None
    };

    Some(match (browser, os) {
        (Some(browser), Some(os)) => format!("{browser} on {os}"),
        (Some(browser), None) => browser.to_owned(),
        (None, Some(os)) => os.to_owned(),
        (None, None) => "Unknown device".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{device_label_from_user_agent, interval};
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

    const CHROME_MACOS: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
        AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
    const FIREFOX_WINDOWS: &str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0";
    const SAFARI_IOS: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5_1 like Mac OS X) \
        AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
    const CHROME_ANDROID: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 \
        (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36";
    const EDGE_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
        (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 Edg/126.0.0.0";

    #[test]
    fn known_browser_and_os_combinations_are_labelled() {
        assert_eq!(
            device_label_from_user_agent(Some(CHROME_MACOS)),
            Some("Chrome on macOS".to_owned())
        );
        assert_eq!(
            device_label_from_user_agent(Some(FIREFOX_WINDOWS)),
            Some("Firefox on Windows".to_owned())
        );
        assert_eq!(
            device_label_from_user_agent(Some(SAFARI_IOS)),
            Some("Safari on iOS".to_owned())
        );
        assert_eq!(
            device_label_from_user_agent(Some(CHROME_ANDROID)),
            Some("Chrome on Android".to_owned())
        );
        // Edge porta sia `Chrome/` sia `Safari/` nella propria stringa: deve
        // vincere `Edg/`, altrimenti ogni Edge si etichetterebbe "Chrome".
        assert_eq!(
            device_label_from_user_agent(Some(EDGE_WINDOWS)),
            Some("Edge on Windows".to_owned())
        );
    }

    #[test]
    fn a_missing_or_unrecognised_user_agent_is_none() {
        assert_eq!(device_label_from_user_agent(None), None);
        assert_eq!(
            device_label_from_user_agent(Some("curl/8.7.1")),
            Some("Unknown device".to_owned())
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn the_full_user_agent_never_survives_into_the_label() {
        // La proprietà che la migrazione dichiara: l'etichetta non deve mai
        // contenere la stringa originale per intero (né un numero di versione
        // specifico, che identificherebbe il dispositivo più di un'etichetta).
        let label = device_label_from_user_agent(Some(CHROME_MACOS)).unwrap();
        assert!(!label.contains("126.0.0.0"));
        assert!(!label.contains("AppleWebKit"));
        assert!(label.len() < CHROME_MACOS.len());
    }
}
