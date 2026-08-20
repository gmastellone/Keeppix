//! Backup wizard API (Fase 6 Task 3). Admin-only.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use keeppix_db::{
    BackupKind, BackupPreferences, BackupRepo, BackupRetention, BackupSchedule,
    NewBackupDestination,
};
use keeppix_jobs::backup::{self, BackupContext};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::extract::AdminAuth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
#[allow(clippy::struct_excessive_bools)]
pub struct BackupPreferencesView {
    pub include_database: bool,
    pub include_config: bool,
    pub include_maps: bool,
    pub include_sidecars: bool,
    pub include_derivatives: bool,
    pub include_originals: bool,
    pub schedule: String,
    pub retention: String,
    pub retention_n: Option<u32>,
    pub verify_after_write: bool,
    pub monthly_restore_proof: bool,
    pub passphrase_set: bool,
    /// Spec §2.4 mandatory warning when originals are excluded.
    pub originals_warning: bool,
    pub originals_warning_message: Option<&'static str>,
}

const ORIGINALS_WARNING: &str =
    "Without Originals this backup does NOT contain your photos. It contains the catalogue.";

impl BackupPreferencesView {
    fn from_prefs(p: &BackupPreferences) -> Self {
        let (retention, retention_n) = match &p.retention {
            BackupRetention::Smart => ("smart".to_owned(), None),
            BackupRetention::LastN { n } => ("last_n".to_owned(), Some(*n)),
        };
        let originals_warning = p.requires_originals_warning();
        Self {
            include_database: p.include_database,
            include_config: p.include_config,
            include_maps: p.include_maps,
            include_sidecars: p.include_sidecars,
            include_derivatives: p.include_derivatives,
            include_originals: p.include_originals,
            schedule: match p.schedule {
                BackupSchedule::Manual => "manual".to_owned(),
                BackupSchedule::Nightly => "nightly".to_owned(),
            },
            retention,
            retention_n,
            verify_after_write: p.verify_after_write,
            monthly_restore_proof: p.monthly_restore_proof,
            passphrase_set: p.passphrase.as_ref().is_some_and(|s| !s.is_empty()),
            originals_warning,
            originals_warning_message: originals_warning.then_some(ORIGINALS_WARNING),
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BackupPreferencesPatch {
    pub include_database: Option<bool>,
    pub include_config: Option<bool>,
    pub include_maps: Option<bool>,
    pub include_sidecars: Option<bool>,
    pub include_derivatives: Option<bool>,
    pub include_originals: Option<bool>,
    pub schedule: Option<String>,
    pub retention: Option<String>,
    pub retention_n: Option<u32>,
    pub verify_after_write: Option<bool>,
    pub monthly_restore_proof: Option<bool>,
    pub passphrase: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct DestinationView {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub config: serde_json::Value,
    pub enabled: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct NewDestinationRequest {
    pub kind: String,
    pub label: String,
    pub config: serde_json::Value,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RunView {
    pub id: String,
    pub destination_id: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub size_bytes: Option<i64>,
    pub verified_at: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub path: Option<String>,
    pub keeppix_version: Option<String>,
}

/// # Errors
/// `401`/`403`.
#[utoipa::path(
    get,
    path = "/api/v1/backup/preferences",
    tag = "backup",
    operation_id = "backup_preferences",
    security(("session_cookie" = [])),
    responses((status = 200, body = BackupPreferencesView), (status = 403, body = Problem))
)]
pub async fn get_preferences(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
) -> Result<Json<BackupPreferencesView>, Problem> {
    let prefs = BackupRepo::new(&state.db).get_preferences(&ctx).await?;
    Ok(Json(BackupPreferencesView::from_prefs(&prefs)))
}

/// # Errors
/// `401`/`403`/`400`.
#[utoipa::path(
    put,
    path = "/api/v1/backup/preferences",
    tag = "backup",
    operation_id = "backup_preferences_put",
    security(("session_cookie" = [])),
    request_body = BackupPreferencesPatch,
    responses((status = 200, body = BackupPreferencesView), (status = 403, body = Problem))
)]
pub async fn put_preferences(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Json(patch): Json<BackupPreferencesPatch>,
) -> Result<Json<BackupPreferencesView>, Problem> {
    let repo = BackupRepo::new(&state.db);
    let mut prefs = repo.get_preferences(&ctx).await?;
    if let Some(v) = patch.include_database {
        prefs.include_database = v;
    }
    if let Some(v) = patch.include_config {
        prefs.include_config = v;
    }
    if let Some(v) = patch.include_maps {
        prefs.include_maps = v;
    }
    if let Some(v) = patch.include_sidecars {
        prefs.include_sidecars = v;
    }
    if let Some(v) = patch.include_derivatives {
        prefs.include_derivatives = v;
    }
    if let Some(v) = patch.include_originals {
        prefs.include_originals = v;
    }
    if let Some(s) = patch.schedule.as_deref() {
        prefs.schedule = match s {
            "manual" => BackupSchedule::Manual,
            "nightly" => BackupSchedule::Nightly,
            _ => {
                return Err(Problem::bad_request(
                    "invalid-schedule",
                    "schedule must be manual or nightly",
                ));
            }
        };
    }
    if let Some(r) = patch.retention.as_deref() {
        prefs.retention = match r {
            "smart" => BackupRetention::Smart,
            "last_n" => BackupRetention::LastN {
                n: patch.retention_n.unwrap_or(14).max(1),
            },
            _ => {
                return Err(Problem::bad_request(
                    "invalid-retention",
                    "retention must be smart or last_n",
                ));
            }
        };
    }
    if let Some(v) = patch.verify_after_write {
        prefs.verify_after_write = v;
    }
    if let Some(v) = patch.monthly_restore_proof {
        prefs.monthly_restore_proof = v;
    }
    if let Some(p) = patch.passphrase {
        if p.is_empty() {
            prefs.passphrase = None;
        } else {
            prefs.passphrase = Some(p);
        }
    }
    repo.put_preferences(&ctx, &prefs).await?;
    Ok(Json(BackupPreferencesView::from_prefs(&prefs)))
}

/// # Errors
/// `401`/`403`.
pub async fn list_destinations(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
) -> Result<Json<Vec<DestinationView>>, Problem> {
    let list = BackupRepo::new(&state.db).list_destinations(&ctx).await?;
    Ok(Json(
        list.into_iter()
            .map(|d| DestinationView {
                id: d.id.to_string(),
                kind: d.kind.as_str().to_owned(),
                label: d.label,
                config: redact_secrets(d.config),
                enabled: d.enabled,
            })
            .collect(),
    ))
}

/// # Errors
/// `401`/`403`/`400`.
pub async fn create_destination(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Json(body): Json<NewDestinationRequest>,
) -> Result<(StatusCode, Json<DestinationView>), Problem> {
    let kind = parse_kind(&body.kind)?;
    let new = NewBackupDestination {
        kind,
        label: body.label,
        config: body.config,
    };
    let dest = keeppix_db::BackupDestination {
        id: Uuid::nil(),
        kind: new.kind,
        label: new.label.clone(),
        config: new.config.clone(),
        enabled: true,
        created_at: chrono::Utc::now(),
    };
    backup::test_destination(&dest)
        .await
        .map_err(|e| Problem::bad_request("destination-unreachable", &e.to_string()))?;
    let created = BackupRepo::new(&state.db)
        .create_destination(&ctx, new)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(DestinationView {
            id: created.id.to_string(),
            kind: created.kind.as_str().to_owned(),
            label: created.label,
            config: redact_secrets(created.config),
            enabled: created.enabled,
        }),
    ))
}

/// # Errors
/// `401`/`403`.
pub async fn delete_destination(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, Problem> {
    BackupRepo::new(&state.db)
        .delete_destination(&ctx, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `401`/`403`/`400`.
pub async fn test_destination(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, Problem> {
    let dest = BackupRepo::new(&state.db)
        .find_destination(&ctx, id)
        .await?;
    backup::test_destination(&dest)
        .await
        .map_err(|e| Problem::bad_request("destination-unreachable", &e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// # Errors
/// `401`/`403`.
pub async fn list_runs(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
) -> Result<Json<Vec<RunView>>, Problem> {
    let runs = BackupRepo::new(&state.db).list_runs(&ctx, 50).await?;
    Ok(Json(runs.into_iter().map(run_view).collect()))
}

/// Run backup now (synchronous for the local path; still records a run).
///
/// # Errors
/// `401`/`403`/`503`.
pub async fn run_now(
    State(state): State<AppState>,
    AdminAuth(_ctx): AdminAuth,
) -> Result<StatusCode, Problem> {
    let ctx = BackupContext {
        database_url: state.database_url.clone(),
        data_dir: state.data_dir.clone(),
        config_path: None,
    };
    backup::run(&state.db, &ctx)
        .await
        .map_err(|e| Problem::bad_request("backup-failed", &e.to_string()))?;
    Ok(StatusCode::ACCEPTED)
}

fn run_view(r: keeppix_db::BackupRun) -> RunView {
    RunView {
        id: r.id.to_string(),
        destination_id: r.destination_id.map(|id| id.to_string()),
        started_at: r.started_at.to_rfc3339(),
        completed_at: r.completed_at.map(|t| t.to_rfc3339()),
        size_bytes: r.size_bytes,
        verified_at: r.verified_at.map(|t| t.to_rfc3339()),
        status: r.status.as_str().to_owned(),
        error: r.error,
        path: r.path,
        keeppix_version: r.keeppix_version,
    }
}

fn parse_kind(raw: &str) -> Result<BackupKind, Problem> {
    match raw {
        "local" => Ok(BackupKind::Local),
        "s3" => Ok(BackupKind::S3),
        "webdav" => Ok(BackupKind::Webdav),
        "sftp" => Ok(BackupKind::Sftp),
        _ => Err(Problem::bad_request(
            "invalid-destination-kind",
            "kind must be local, s3, webdav, or sftp",
        )),
    }
}

fn redact_secrets(mut config: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = config.as_object_mut() {
        for key in ["password", "secret_key", "secret", "passphrase"] {
            if obj.contains_key(key) {
                obj.insert(key.to_owned(), serde_json::json!("***"));
            }
        }
    }
    config
}
