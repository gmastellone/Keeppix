//! Restore wizard API (Fase 6 Task 4). Admin-only.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use axum::extract::State;
use keeppix_jobs::backup::{
    BackupManifest, backup_newer_than_installed, restore_maps_only, unpack_kpxb,
};
use serde::{Deserialize, Serialize};

use crate::extract::AdminAuth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

const INSTALLED_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize, utoipa::ToSchema)]
pub struct InspectRequest {
    pub path: String,
    pub passphrase: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct InspectResponse {
    pub manifest: serde_json::Value,
    pub rejected: bool,
    pub reject_reason: Option<&'static str>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RestoreRequest {
    pub path: String,
    pub passphrase: String,
    #[serde(default)]
    pub components: RestoreComponents,
    /// When true, only simulate — no writes.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[allow(clippy::struct_excessive_bools)]
pub struct RestoreComponents {
    #[serde(default = "default_true")]
    pub database: bool,
    #[serde(default = "default_true")]
    pub config: bool,
    #[serde(default = "default_true")]
    pub maps: bool,
}

fn default_true() -> bool {
    true
}

impl Default for RestoreComponents {
    fn default() -> Self {
        Self {
            database: true,
            config: true,
            maps: true,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RestorePreview {
    pub would_restore_database: bool,
    pub would_restore_config: bool,
    pub would_restore_maps: bool,
    pub safety_dump_path: Option<String>,
    pub keeppix_version: String,
}

/// Read a backup manifest without writing. Rejects backups newer than this build.
///
/// # Errors
/// `401`/`403`/`400`.
#[utoipa::path(
    post,
    path = "/api/v1/restore/inspect",
    tag = "restore",
    operation_id = "restore_inspect",
    summary = "Inspect a backup archive without restoring it",
    security(("session_cookie" = [])),
    request_body = InspectRequest,
    responses(
        (status = 200, description = "Manifest letto (eventualmente respinto)", body = InspectResponse),
        (status = 400, description = "Archivio illeggibile o passphrase errata", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem)
    )
)]
pub async fn inspect(
    AdminAuth(_ctx): AdminAuth,
    Json(body): Json<InspectRequest>,
) -> Result<Json<InspectResponse>, Problem> {
    let tmp = std::env::temp_dir().join(format!("keeppix-inspect-{}", uuid::Uuid::now_v7()));
    unpack_kpxb(std::path::Path::new(&body.path), &body.passphrase, &tmp)
        .map_err(|e| Problem::bad_request("backup-unreadable", &e.to_string()))?;
    let raw = fs::read(tmp.join("manifest.json")).map_err(|e| {
        let _ = fs::remove_dir_all(&tmp);
        Problem::bad_request("backup-unreadable", &e.to_string())
    })?;
    let manifest: BackupManifest = serde_json::from_slice(&raw).map_err(|e| {
        let _ = fs::remove_dir_all(&tmp);
        Problem::bad_request("backup-unreadable", &e.to_string())
    })?;
    let manifest_json = serde_json::to_value(&manifest)
        .map_err(|e| Problem::bad_request("backup-unreadable", &e.to_string()))?;
    let _ = fs::remove_dir_all(&tmp);

    let rejected = backup_newer_than_installed(&manifest.keeppix_version, INSTALLED_VERSION);
    Ok(Json(InspectResponse {
        reject_reason: rejected.then_some(
            "This backup was created by a newer Keeppix version than the one installed.",
        ),
        rejected,
        manifest: manifest_json,
    }))
}

/// Restore from a `.kpxb`. Creates a safety dump of the current database
/// before overwriting when `database` is selected and `dry_run` is false.
///
/// Known limit: `pg_restore` is not transactional across the whole dump —
/// an interrupted restore can leave a partial schema. Documented, not pretended.
///
/// # Errors
/// `401`/`403`/`400`.
#[utoipa::path(
    post,
    path = "/api/v1/restore",
    tag = "restore",
    operation_id = "restore_restore",
    summary = "Restore from a backup archive",
    security(("session_cookie" = [])),
    request_body = RestoreRequest,
    responses(
        (status = 200, description = "Componenti ripristinati (o simulati, se dry_run)", body = RestorePreview),
        (status = 400, description = "Archivio illeggibile, troppo nuovo, o ripristino fallito", body = Problem),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Solo admin", body = Problem)
    )
)]
#[allow(clippy::too_many_lines)]
pub async fn restore(
    State(state): State<AppState>,
    AdminAuth(ctx): AdminAuth,
    Json(body): Json<RestoreRequest>,
) -> Result<Json<RestorePreview>, Problem> {
    let tmp = std::env::temp_dir().join(format!("keeppix-restore-{}", uuid::Uuid::now_v7()));
    unpack_kpxb(std::path::Path::new(&body.path), &body.passphrase, &tmp)
        .map_err(|e| Problem::bad_request("backup-unreadable", &e.to_string()))?;

    let raw = fs::read(tmp.join("manifest.json")).map_err(|e| {
        let _ = fs::remove_dir_all(&tmp);
        Problem::bad_request("backup-unreadable", &e.to_string())
    })?;
    let manifest: BackupManifest = serde_json::from_slice(&raw).map_err(|e| {
        let _ = fs::remove_dir_all(&tmp);
        Problem::bad_request("backup-unreadable", &e.to_string())
    })?;

    if backup_newer_than_installed(&manifest.keeppix_version, INSTALLED_VERSION) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(Problem::bad_request(
            "backup-too-new",
            "This backup was created by a newer Keeppix version than the one installed.",
        ));
    }

    let maps_only = body.components.maps && !body.components.database && !body.components.config;

    if body.dry_run {
        let preview = RestorePreview {
            would_restore_database: body.components.database,
            would_restore_config: body.components.config,
            would_restore_maps: body.components.maps,
            safety_dump_path: None,
            keeppix_version: manifest.keeppix_version,
        };
        let _ = fs::remove_dir_all(&tmp);
        return Ok(Json(preview));
    }

    // Hot path: maps only — never touches the primary database dump.
    if maps_only {
        let maps_path = tmp.join("maps.json");
        let bytes = fs::read(&maps_path).map_err(|e| {
            let _ = fs::remove_dir_all(&tmp);
            Problem::bad_request("backup-unreadable", &e.to_string())
        })?;
        restore_maps_only(&state.db, &ctx, &bytes)
            .await
            .map_err(|e| {
                let _ = fs::remove_dir_all(&tmp);
                Problem::bad_request("restore-failed", &e.to_string())
            })?;
        let _ = fs::remove_dir_all(&tmp);
        return Ok(Json(RestorePreview {
            would_restore_database: false,
            would_restore_config: false,
            would_restore_maps: true,
            safety_dump_path: None,
            keeppix_version: manifest.keeppix_version,
        }));
    }

    let mut safety_dump_path = None;
    if body.components.database {
        let safety = safety_dump_path_for(&state.data_dir);
        pg_dump_file(&state.database_url, &safety).map_err(|e| {
            let _ = fs::remove_dir_all(&tmp);
            Problem::bad_request("safety-dump-failed", &e.to_string())
        })?;
        safety_dump_path = Some(safety.to_string_lossy().into_owned());

        let dump = tmp.join("database.dump");
        if dump.exists() {
            let status = Command::new("pg_restore")
                .args([
                    "--dbname",
                    &state.database_url,
                    "--clean",
                    "--if-exists",
                    "--no-owner",
                    "--no-privileges",
                ])
                .arg(&dump)
                .status()
                .map_err(|e| {
                    let _ = fs::remove_dir_all(&tmp);
                    Problem::bad_request("restore-failed", &e.to_string())
                })?;
            if !status.success() {
                let _ = fs::remove_dir_all(&tmp);
                return Err(Problem::bad_request(
                    "restore-failed",
                    &format!("pg_restore failed: {status}"),
                ));
            }
            // Older dumps: apply pending migrations after restore.
            state.db.migrate().await.map_err(Problem::from)?;
        }
    }

    if body.components.config {
        let config_src = tmp.join("config.toml");
        if config_src.exists() {
            let dest = state.data_dir.join("restored-config.toml");
            let _ = fs::copy(&config_src, &dest);
        }
    }

    if body.components.maps {
        let maps_path = tmp.join("maps.json");
        if maps_path.exists()
            && let Ok(bytes) = fs::read(&maps_path)
        {
            let _ = restore_maps_only(&state.db, &ctx, &bytes).await;
        }
    }

    let _ = fs::remove_dir_all(&tmp);
    Ok(Json(RestorePreview {
        would_restore_database: body.components.database,
        would_restore_config: body.components.config,
        would_restore_maps: body.components.maps,
        safety_dump_path,
        keeppix_version: manifest.keeppix_version,
    }))
}

fn safety_dump_path_for(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join(format!(
        "safety-dump-{}.dump",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ))
}

fn pg_dump_file(database_url: &str, out: &std::path::Path) -> Result<(), String> {
    let status = Command::new("pg_dump")
        .args(["--format=custom", "--file"])
        .arg(out)
        .arg(database_url)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("pg_dump failed: {status}"))
    }
}
