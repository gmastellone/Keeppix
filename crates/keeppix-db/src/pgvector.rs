//! Probe della presenza di pgvector (estensione `vector`) sul Postgres collegato.
//!
//! Fase 7 Task 3: se manca, Keeppix **non** rifiuta l'avvio — le funzioni AI
//! restano spente e lo stato persistito spiega perché, con il comando da
//! eseguire. Lo schema (`asset_embeddings`, …) arriva al Task 4.

use serde::{Deserialize, Serialize};

use crate::{Db, DbError, SettingsRepo};

/// Comando SQL da mostrare all'operatore quando pgvector è assente o non ancora
/// creato. La migrazione del Task 4 lo eseguirà sul percorso bundled; qui serve
/// solo come istruzione leggibile per Postgres esterni.
pub const ENABLE_VECTOR_SQL: &str = "CREATE EXTENSION IF NOT EXISTS vector;";

/// Esito del probe di pgvector all'avvio (Fase 7 Task 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PgVectorStatus {
    /// Il pacchetto/estensione è installato sul server (`pg_available_extensions`).
    pub available: bool,
    /// `CREATE EXTENSION vector` è già stato eseguito su questo database.
    pub enabled: bool,
    /// Messaggio inglese per log e pannello quando le funzioni AI restano spente.
    /// `None` se pgvector è utilizzabile (o lo sarà dopo la migrazione Task 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Comando da eseguire dopo aver installato il pacchetto sul server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_command: Option<String>,
}

impl PgVectorStatus {
    pub const ENABLE_SQL: &'static str = ENABLE_VECTOR_SQL;

    /// Stato quando l'estensione non è installata sul server Postgres.
    #[must_use]
    pub fn missing() -> Self {
        Self {
            available: false,
            enabled: false,
            message: Some(format!(
                "AI features (semantic search and automatic tags) are disabled \
                 because the PostgreSQL `vector` extension (pgvector) is not \
                 installed on this database. Install pgvector for your Postgres \
                 major version, then run: {ENABLE_VECTOR_SQL}"
            )),
            enable_command: Some(ENABLE_VECTOR_SQL.to_owned()),
        }
    }

    /// Stato quando l'estensione è installata sul server.
    ///
    /// `enabled` distingue «pacchetto presente» da «già `CREATE EXTENSION`».
    /// Finché Task 4 non crea lo schema, `enabled == false` resta legittimo
    /// sul percorso bundled: le funzioni AI attendono la migrazione, non un
    /// errore di installazione.
    #[must_use]
    pub fn present(enabled: bool) -> Self {
        Self {
            available: true,
            enabled,
            message: None,
            enable_command: None,
        }
    }

    /// `true` quando l'assenza di pgvector spegne le funzioni AI.
    #[must_use]
    pub const fn ai_disabled(&self) -> bool {
        !self.available
    }
}

#[derive(sqlx::FromRow)]
struct ProbeRow {
    available: bool,
    enabled: bool,
}

/// Controlla se `vector` (pgvector) è installabile o già attiva.
///
/// Nessun `AuthContext`: è un probe di capacità dell'istanza, eseguito all'avvio
/// prima che esista una sessione utente (stesso motivo di `count` /
/// `create_bootstrap_admin` — non legge dati di un utente).
///
/// # Errors
/// `DbError::Connection` se la query fallisce.
pub async fn probe_pgvector(db: &Db) -> Result<PgVectorStatus, DbError> {
    let row: ProbeRow = sqlx::query_as(
        "SELECT \
            EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'vector') AS available, \
            EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector') AS enabled",
    )
    .fetch_one(db.pool())
    .await?;

    Ok(if row.available {
        PgVectorStatus::present(row.enabled)
    } else {
        PgVectorStatus::missing()
    })
}

/// Scrive lo stato in `system_settings` sotto la chiave `pgvector`, così il
/// pannello (Task successivi) può spiegare perché la sezione AI è spenta senza
/// rieseguire il probe a ogni richiesta.
///
/// # Errors
/// Database.
pub async fn persist_pgvector_status(db: &Db) -> Result<PgVectorStatus, DbError> {
    let status = probe_pgvector(db).await?;
    let value = serde_json::to_value(&status)
        .map_err(|e| DbError::Corrupted(format!("pgvector status serialize: {e}")))?;
    SettingsRepo::new(db).put_json("pgvector", &value).await?;
    Ok(status)
}
