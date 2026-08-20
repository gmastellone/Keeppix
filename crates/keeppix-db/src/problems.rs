//! Problemi visibili (librerie offline, asset in errore, job falliti). I
//! duplicati per `content_hash` sono in [`crate::duplicates`] da Fase 2.
//!
//! [`ProblemsRepo::composed`] (Fase 10 Task 13) trasforma i tre secchi grezzi
//! di [`ProblemsRepo::list`] in un elenco piatto di problemi già in
//! linguaggio naturale — è il server che sa perché un job è fallito, non il
//! frontend (spec §47).

use keeppix_domain::{AssetId, AuthContext, FolderId, JobKind, LibraryId};

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
        let lib_filter = scope.filter_library("id", 1);
        let offline: Vec<(uuid::Uuid, String)> = sqlx::query_as(&format!(
            "SELECT id, name FROM libraries \
              WHERE status = 'offline' AND {} \
              ORDER BY name",
            lib_filter.sql()
        ))
        .bind(lib_filter.bind())
        .bind(lib_filter.holes())
        .bind(lib_filter.assets())
        .fetch_all(self.db.pool())
        .await?;

        let asset_filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let errors: Vec<(uuid::Uuid, String)> = sqlx::query_as(&format!(
            "SELECT a.id, a.filename FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
              WHERE a.status = 'error' AND {} \
              ORDER BY a.id \
              LIMIT 200",
            asset_filter.sql()
        ))
        .bind(asset_filter.bind())
        .bind(asset_filter.holes())
        .bind(asset_filter.assets())
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

    /// Elenco piatto composto (spec §47): ogni riga è già gravità, titolo,
    /// descrizione in linguaggio naturale, cartella/libreria coinvolta e
    /// azioni proposte — non i tre secchi grezzi di [`Self::list`].
    ///
    /// # Errors
    /// Come [`Self::list`], più `Connection`/`NotFound` se un asset o una
    /// cartella referenziati da un fallimento di `write_sidecar` sono spariti
    /// nel frattempo (caso raro: cancellati fra il fallimento e la lettura).
    pub async fn composed(
        &self,
        ctx: &AuthContext,
        lang: ProblemLanguage,
    ) -> Result<Vec<ComposedProblem>, DbError> {
        let set = self.list(ctx).await?;
        let mut problems = Vec::with_capacity(
            set.offline_libraries.len() + set.failed_jobs.len() + set.error_assets.len(),
        );

        for library in &set.offline_libraries {
            problems.push(offline_library_problem(library, lang));
        }

        for job in &set.failed_jobs {
            let permission_asset_ids = (job.kind == JobKind::WriteSidecar.as_str())
                .then(|| job.last_error.as_deref().map(permission_denied_asset_ids))
                .flatten()
                .filter(|ids| !ids.is_empty());

            match permission_asset_ids {
                Some(asset_ids) => {
                    problems.push(
                        self.sidecar_permission_problem(job.id, &asset_ids, lang)
                            .await?,
                    );
                }
                None => problems.push(generic_job_problem(job, lang)),
            }
        }

        for asset in &set.error_assets {
            problems.push(generic_asset_problem(asset, lang));
        }

        Ok(problems)
    }

    async fn sidecar_permission_problem(
        &self,
        job_id: i64,
        asset_ids: &[AssetId],
        lang: ProblemLanguage,
    ) -> Result<ComposedProblem, DbError> {
        let assets = crate::AssetRepo::new(self.db);
        let mut folder_ids: Vec<FolderId> = Vec::new();
        for asset_id in asset_ids {
            let asset = assets.get_for_scan(*asset_id).await?;
            if !folder_ids.contains(&asset.folder_id) {
                folder_ids.push(asset.folder_id);
            }
        }

        let mut folder_names = Vec::with_capacity(folder_ids.len());
        let mut common_library: Option<(LibraryId, String)> = None;
        for (i, folder_id) in folder_ids.iter().enumerate() {
            let (folder_name, library_id, library_name) =
                self.folder_and_library(*folder_id).await?;
            if !folder_name.is_empty() {
                folder_names.push(folder_name);
            }
            if i == 0 {
                common_library = Some((library_id, library_name));
            } else if common_library.as_ref().map(|(id, _)| *id) != Some(library_id) {
                common_library = None;
            }
        }

        Ok(sidecar_permission_problem_view(
            job_id,
            asset_ids.len(),
            &folder_ids,
            &folder_names,
            common_library,
            lang,
        ))
    }

    async fn folder_and_library(
        &self,
        folder_id: FolderId,
    ) -> Result<(String, LibraryId, String), DbError> {
        let row: Option<(String, uuid::Uuid, String)> = sqlx::query_as(
            "SELECT f.name, f.library_id, l.name FROM folders f \
             JOIN libraries l ON l.id = f.library_id \
              WHERE f.id = $1",
        )
        .bind(folder_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        row.map(|(folder_name, library_id, library_name)| {
            (folder_name, LibraryId::from_uuid(library_id), library_name)
        })
        .ok_or(DbError::NotFound)
    }
}

/// Le due lingue in cui il backend traduce le descrizioni composte —
/// **Ruling** (Task 13): il backend produce il testo in linguaggio naturale,
/// non il frontend, quindi la lingua dell'utente deve arrivare nella
/// richiesta (query o `Accept-Language`, risolta dal livello HTTP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemLanguage {
    It,
    En,
}

impl ProblemLanguage {
    /// `"en"`, `"en-US"`, `"en_GB"`... → inglese; qualunque altra cosa
    /// (incluso assente) → italiano, il default già in vigore per
    /// `UserPreferences::language` (Task 9).
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        if raw.trim().to_ascii_lowercase().starts_with("en") {
            Self::En
        } else {
            Self::It
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemSeverity {
    Warning,
    Error,
}

impl ProblemSeverity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemAction {
    pub action: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedProblem {
    pub id: String,
    pub severity: ProblemSeverity,
    pub title: String,
    pub description: String,
    pub library_id: Option<LibraryId>,
    pub library_name: Option<String>,
    pub folder_id: Option<FolderId>,
    pub folder_name: Option<String>,
    pub actions: Vec<ProblemAction>,
}

fn offline_library_problem(library: &OfflineLibrary, lang: ProblemLanguage) -> ComposedProblem {
    let (title, description, retry_label, details_label) = match lang {
        ProblemLanguage::It => (
            format!("Libreria offline: {}", library.name),
            "Il percorso di rete non risponde. Le foto restano visibili dalla cache, \
             ma non è possibile importarne di nuove."
                .to_owned(),
            "Riprova connessione",
            "Dettagli",
        ),
        ProblemLanguage::En => (
            format!("Library offline: {}", library.name),
            "The network path is not responding. Photos remain visible from cache, \
             but new ones cannot be imported."
                .to_owned(),
            "Retry connection",
            "Details",
        ),
    };

    ComposedProblem {
        id: format!("library-offline:{}", library.id),
        severity: ProblemSeverity::Error,
        title,
        description,
        library_id: Some(library.id),
        library_name: Some(library.name.clone()),
        folder_id: None,
        folder_name: None,
        actions: vec![
            ProblemAction {
                action: "retry-connection".to_owned(),
                label: retry_label.to_owned(),
            },
            ProblemAction {
                action: "details".to_owned(),
                label: details_label.to_owned(),
            },
        ],
    }
}

fn generic_job_problem(job: &FailedJob, lang: ProblemLanguage) -> ComposedProblem {
    let (title, description) = match lang {
        ProblemLanguage::It => (
            format!("Operazione pianificata non riuscita: {}", job.kind),
            job.last_error
                .clone()
                .unwrap_or_else(|| "Errore non specificato.".to_owned()),
        ),
        ProblemLanguage::En => (
            format!("Scheduled job failed: {}", job.kind),
            job.last_error
                .clone()
                .unwrap_or_else(|| "Unspecified error.".to_owned()),
        ),
    };

    ComposedProblem {
        id: format!("job-failed:{}", job.id),
        severity: ProblemSeverity::Error,
        title,
        description,
        library_id: None,
        library_name: None,
        folder_id: None,
        folder_name: None,
        actions: Vec::new(),
    }
}

fn generic_asset_problem(asset: &ErrorAsset, lang: ProblemLanguage) -> ComposedProblem {
    let (title, description) = match lang {
        ProblemLanguage::It => (
            format!("File illeggibile: {}", asset.filename),
            "Il file non è stato elaborato correttamente e resta escluso dalla libreria \
             finché non viene ripristinato o rimosso."
                .to_owned(),
        ),
        ProblemLanguage::En => (
            format!("Unreadable file: {}", asset.filename),
            "The file could not be processed correctly and stays excluded from the \
             library until it is restored or removed."
                .to_owned(),
        ),
    };

    ComposedProblem {
        id: format!("asset-error:{}", asset.id),
        severity: ProblemSeverity::Error,
        title,
        description,
        library_id: None,
        library_name: None,
        folder_id: None,
        folder_name: None,
        actions: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn sidecar_permission_problem_view(
    job_id: i64,
    file_count: usize,
    folder_ids: &[FolderId],
    folder_names: &[String],
    library: Option<(LibraryId, String)>,
    lang: ProblemLanguage,
) -> ComposedProblem {
    let folder_label = join_names(folder_names, lang);
    let (title, description, view_label, ignore_label) = match lang {
        ProblemLanguage::It => (
            count_files_it(file_count),
            sidecar_description_it(&folder_label),
            format!("Vedi {}", count_files_it(file_count).to_lowercase()),
            "Ignora".to_owned(),
        ),
        ProblemLanguage::En => (
            count_files_en(file_count),
            sidecar_description_en(&folder_label),
            format!("View {}", count_files_en(file_count).to_lowercase()),
            "Dismiss".to_owned(),
        ),
    };

    ComposedProblem {
        id: format!("sidecar-permission:{job_id}"),
        severity: ProblemSeverity::Warning,
        title,
        description,
        library_id: library.as_ref().map(|(id, _)| *id),
        library_name: library.map(|(_, name)| name),
        folder_id: (folder_ids.len() == 1).then(|| folder_ids[0]),
        folder_name: (folder_names.len() == 1).then(|| folder_names[0].clone()),
        actions: vec![
            ProblemAction {
                action: "view-files".to_owned(),
                label: view_label,
            },
            ProblemAction {
                action: "ignore".to_owned(),
                label: ignore_label,
            },
        ],
    }
}

fn count_files_it(count: usize) -> String {
    format!("{count} file con sidecar XMP non scrivibile")
}

fn count_files_en(count: usize) -> String {
    if count == 1 {
        "1 file with unwritable XMP sidecar".to_owned()
    } else {
        format!("{count} files with unwritable XMP sidecar")
    }
}

fn sidecar_description_it(folder_label: &str) -> String {
    let prefix = if folder_label.is_empty() {
        String::new()
    } else {
        format!("{folder_label} — ")
    };
    format!(
        "{prefix}permessi di scrittura mancanti sulla cartella. Il rating resta salvato \
         nel database, ma non sincronizzato sul file."
    )
}

fn sidecar_description_en(folder_label: &str) -> String {
    let prefix = if folder_label.is_empty() {
        String::new()
    } else {
        format!("{folder_label} — ")
    };
    format!(
        "{prefix}missing write permission on the folder. The rating stays saved in the \
         database, but is not synced to the file."
    )
}

fn join_names(names: &[String], lang: ProblemLanguage) -> String {
    let conjunction = match lang {
        ProblemLanguage::It => "e",
        ProblemLanguage::En => "and",
    };
    match names {
        [] => String::new(),
        [only] => only.clone(),
        _ => {
            let last = &names[names.len() - 1];
            let rest = &names[..names.len() - 1];
            format!("{} {conjunction} {last}", rest.join(", "))
        }
    }
}

/// Un fallimento di `write_sidecar` per singolo asset ha, in `jobs.last_error`,
/// la forma `"{asset_id}: {message}"` (più fallimenti separati da `"; "`),
/// avvolta a sua volta in un `JobError::Worker` di livello batch — quindi
/// l'intera stringa porta un prefisso `"worker: "` in più (vedi
/// `keeppix-jobs::xmp::run`). Un fallimento per permessi porta ovunque nel
/// suo segmento il marcatore stabile `permission-denied:`, messo lì apposta
/// da `keeppix-jobs` per non dover indovinare dal testo libero di un
/// `io::Error` che cambia formulazione a seconda della piattaforma.
fn permission_denied_asset_ids(last_error: &str) -> Vec<AssetId> {
    let body = last_error.strip_prefix("worker: ").unwrap_or(last_error);
    body.split("; ")
        .filter_map(|entry| {
            if !entry.contains("permission-denied:") {
                return None;
            }
            let (id_part, _) = entry.split_once(": ")?;
            uuid::Uuid::parse_str(id_part).ok().map(AssetId::from_uuid)
        })
        .collect()
}
