//! Problemi visibili (librerie offline, asset in errore, job falliti). I
//! duplicati per `content_hash` sono in [`crate::duplicates`] da Fase 2.

use keeppix_domain::{AssetId, AuthContext, LibraryId};

use crate::visibility::VisibilityScope;
use crate::{Db, DbError};

pub struct ProblemsRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone)]
pub struct OfflineLibrary {
    pub id: LibraryId,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct FailedJob {
    pub id: i64,
    pub kind: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ErrorAsset {
    pub id: AssetId,
    pub filename: String,
}

#[derive(Debug, Clone)]
pub struct ProblemSet {
    pub offline_libraries: Vec<OfflineLibrary>,
    pub failed_jobs: Vec<FailedJob>,
    pub error_assets: Vec<ErrorAsset>,
}

impl<'a> ProblemsRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// `Connection` se una delle query fallisce.
    pub async fn list(&self, ctx: &AuthContext) -> Result<ProblemSet, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let lib_filter = scope.filter("id", 1);
        let offline: Vec<(uuid::Uuid, String)> = sqlx::query_as(&format!(
            "SELECT id, name FROM libraries \
              WHERE status = 'offline' AND {} \
              ORDER BY name",
            lib_filter.sql()
        ))
        .bind(lib_filter.bind())
        .fetch_all(self.db.pool())
        .await?;

        let asset_filter = scope.filter("f.library_id", 1);
        let errors: Vec<(uuid::Uuid, String)> = sqlx::query_as(&format!(
            "SELECT a.id, a.filename FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
              WHERE a.status = 'error' AND {} \
              ORDER BY a.id \
              LIMIT 200",
            asset_filter.sql()
        ))
        .bind(asset_filter.bind())
        .fetch_all(self.db.pool())
        .await?;

        // I job non hanno proprietario: solo l'admin li vede, così un utente
        // non legge errori di ingest di librerie altrui.
        let failed_jobs = if ctx.is_admin() {
            let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
                "SELECT id, kind, last_error FROM jobs \
                  WHERE status = 'failed' \
                  ORDER BY id DESC \
                  LIMIT 200",
            )
            .fetch_all(self.db.pool())
            .await?;
            rows.into_iter()
                .map(|(id, kind, last_error)| FailedJob {
                    id,
                    kind,
                    last_error,
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(ProblemSet {
            offline_libraries: offline
                .into_iter()
                .map(|(id, name)| OfflineLibrary {
                    id: LibraryId::from_uuid(id),
                    name,
                })
                .collect(),
            failed_jobs,
            error_assets: errors
                .into_iter()
                .map(|(id, filename)| ErrorAsset {
                    id: AssetId::from_uuid(id),
                    filename,
                })
                .collect(),
        })
    }
}
