//! Operazioni lunghe con avanzamento e annullamento (Fase 10 Task 16).
//!
//! Il canale WebSocket resta ciò che è già: un canale di notifica, non fonte
//! di verità. Questa tabella *è* la fonte di verità — la rotta `/ws` la
//! legge a ogni giro di poll (come fa già con `change_log`), così un client
//! che si riconnette a metà operazione vede lo stato corrente al primo giro
//! utile, senza bisogno di un replay di eventi persi.
//!
//! **Ruling (Task 16): annullare a metà produce una riuscita parziale, non
//! un rollback.** `succeeded_asset_ids` accumula ciò che è già stato scritto
//! sul disco; `finish_cancelled` chiude lo stato senza svuotarlo — è
//! esattamente l'involucro `BulkOutcome` del Task 1, letto da qui invece che
//! costruito da zero.

use keeppix_domain::{AssetId, AuthContext, OperationId, OperationKind, OperationStatus, UserId};
use uuid::Uuid;

use crate::{Db, DbError};

pub struct OperationsRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone)]
pub struct Operation {
    pub id: OperationId,
    pub kind: OperationKind,
    pub owner_id: UserId,
    pub status: OperationStatus,
    pub done: i64,
    pub total: Option<i64>,
    pub phase: String,
    pub succeeded: Vec<AssetId>,
}

#[derive(sqlx::FromRow)]
struct OperationRow {
    id: Uuid,
    kind: String,
    owner_id: Uuid,
    status: String,
    done: i64,
    total: Option<i64>,
    phase: String,
    succeeded_asset_ids: Vec<Uuid>,
}

impl OperationRow {
    fn into_domain(self) -> Result<Operation, DbError> {
        Ok(Operation {
            id: OperationId::from_uuid(self.id),
            kind: OperationKind::parse(&self.kind)
                .map_err(|e| crate::row::corrupted("operation kind", e))?,
            owner_id: UserId::from_uuid(self.owner_id),
            status: OperationStatus::parse(&self.status)
                .map_err(|e| crate::row::corrupted("operation status", e))?,
            done: self.done,
            total: self.total,
            phase: self.phase,
            succeeded: self
                .succeeded_asset_ids
                .into_iter()
                .map(AssetId::from_uuid)
                .collect(),
        })
    }
}

const COLUMNS: &str = "id, kind, owner_id, status, done, total, phase, succeeded_asset_ids";

impl<'a> OperationsRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Crea una nuova operazione posseduta dal chiamante.
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato; `Connection` su errore DB.
    pub async fn create(
        &self,
        ctx: &AuthContext,
        kind: OperationKind,
    ) -> Result<Operation, DbError> {
        let owner = ctx.user_id().ok_or(DbError::Forbidden)?;
        let id = Uuid::now_v7();
        let row: OperationRow = sqlx::query_as(&format!(
            "INSERT INTO operations (id, kind, owner_id) VALUES ($1, $2, $3) \
             RETURNING {COLUMNS}"
        ))
        .bind(id)
        .bind(kind.as_str())
        .bind(owner.as_uuid())
        .fetch_one(self.db.pool())
        .await?;
        row.into_domain()
    }

    /// Recupera un'operazione. `Forbidden` — non `NotFound` — se non è
    /// visibile: altrimenti l'endpoint diventerebbe un oracolo di esistenza.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non è owner né admin; `Connection` DB.
    pub async fn find(&self, ctx: &AuthContext, id: OperationId) -> Result<Operation, DbError> {
        let row: Option<OperationRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM operations WHERE id = $1"))
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        let Some(row) = row else {
            return Err(DbError::Forbidden);
        };
        let operation = row.into_domain()?;
        if ctx.is_admin() || ctx.user_id() == Some(operation.owner_id) {
            Ok(operation)
        } else {
            Err(DbError::Forbidden)
        }
    }

    /// Operazioni ancora `running` possedute dal chiamante. Usata dal poll
    /// del WebSocket per emettere `operation.progress` (spec Task 16).
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn list_running(&self, ctx: &AuthContext) -> Result<Vec<Operation>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Ok(Vec::new());
        };
        let rows: Vec<OperationRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM operations \
              WHERE owner_id = $1 AND status = 'running' \
              ORDER BY created_at"
        ))
        .bind(user_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter().map(OperationRow::into_domain).collect()
    }

    /// Imposta il totale atteso (`None` quando non è noto in anticipo, come
    /// alla prima scansione di una libreria vuota di dati). Chiamata dalla
    /// pipeline, non dall'utente: nessun `AuthContext`, come `JobRepo::claim`.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn set_total(&self, id: OperationId, total: Option<i64>) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET total = $2, updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .bind(total)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Aggiorna la fase testuale mostrata sul WebSocket (es. `"scanning"`).
    /// Pipeline interna: nessun `AuthContext`.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn set_phase(&self, id: OperationId, phase: &str) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET phase = $2, updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .bind(phase)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Registra un elemento riuscito: incrementa `done` e lo appende
    /// all'involucro parziale. No-op silenzioso se l'operazione è già
    /// conclusa — il worker deve aver già smesso di chiamarlo, ma un
    /// late-write vagante non deve riaprire uno stato terminale.
    ///
    /// Pipeline interna: nessun `AuthContext`, come `JobRepo::complete`.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn record_success(&self, id: OperationId, asset_id: AssetId) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET \
                 done = done + 1, \
                 succeeded_asset_ids = succeeded_asset_ids || $2::uuid, \
                 updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .bind(asset_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Come [`Self::record_success`], ma per un intero lotto in una sola
    /// istruzione (Fase 10 Task 21): un solo giro di rete invece di uno per
    /// asset. No-op se il lotto è vuoto o l'operazione è già conclusa —
    /// stesso comportamento di [`Self::record_success`].
    ///
    /// Pipeline interna: nessun `AuthContext`.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn record_success_many(
        &self,
        id: OperationId,
        asset_ids: &[AssetId],
    ) -> Result<(), DbError> {
        if asset_ids.is_empty() {
            return Ok(());
        }
        let ids: Vec<Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        sqlx::query(
            "UPDATE operations SET \
                 done = done + array_length($2::uuid[], 1), \
                 succeeded_asset_ids = succeeded_asset_ids || $2::uuid[], \
                 updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .bind(&ids)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// `true` se il chiamante ha chiesto l'annullamento. Interrogata dal
    /// worker fra un elemento e il successivo — mai dall'utente
    /// direttamente, quindi nessun `AuthContext`.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn is_cancel_requested(&self, id: OperationId) -> Result<bool, DbError> {
        let flag: Option<bool> =
            sqlx::query_scalar("SELECT cancel_requested FROM operations WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        Ok(flag.unwrap_or(false))
    }

    /// Chiede l'annullamento. Solo owner o admin — altrimenti chiunque
    /// potrebbe interrompere l'operazione di un altro utente.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non è owner né admin; `Connection` DB.
    pub async fn request_cancel(&self, ctx: &AuthContext, id: OperationId) -> Result<(), DbError> {
        self.find(ctx, id).await?;
        sqlx::query("UPDATE operations SET cancel_requested = true WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Chiude l'operazione come annullata **senza** svuotare l'esito
    /// parziale già accumulato (Ruling Task 16). Pipeline interna.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn finish_cancelled(&self, id: OperationId) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET status = 'cancelled', updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Chiude l'operazione come conclusa con successo. Pipeline interna.
    ///
    /// # Errors
    /// `Connection` su errore DB.
    pub async fn finish_done(&self, id: OperationId) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE operations SET status = 'done', updated_at = now() \
              WHERE id = $1 AND status = 'running'",
        )
        .bind(id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}
