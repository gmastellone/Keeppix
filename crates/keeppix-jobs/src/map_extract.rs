//! Server-side `pmtiles extract`: `JobKind::ExtractMapRegion`, the
//! mechanism behind picking a country from the map region search
//! (`crate::map_catalog`) instead of pasting a URL by hand
//! (`crate::regions`, `DownloadRegionRequest` — kept, unused by this new
//! flow, still reachable for anyone who really does have a direct
//! `PMTiles` URL).
//!
//! Reuses `RegionRepo`'s existing `begin_extraction`/`mark_error`/
//! `source_for_download` — same `map_regions` row, same status machine
//! (`downloading` -> `available`/`error`), same list/cancel/delete UI —
//! only the acquisition mechanism differs: a subprocess that fetches
//! just a country's tiles out of the remote 120 GB planet build, not a
//! byte-for-byte HTTP stream of a client-given URL.

use std::path::Path;
use std::time::Duration;

use keeppix_db::{Db, JobRepo, NewMapRegion, RegionRepo};
use keeppix_domain::{AuthContext, Job, JobKind, JobPriority};
use sha2::{Digest as _, Sha256};

use crate::JobError;

const PMTILES_BIN: &str = "/usr/bin/pmtiles";
const BUILD_HOST: &str = "https://build.protomaps.com";
/// Builds land at some point during each UTC day, not necessarily at
/// 00:00 — a HEAD probe walks back this many days from today to find the
/// most recent one actually published yet, instead of assuming "today"
/// always exists (`maps.protomaps.com/builds` publishes no "latest"
/// alias, only dated filenames — verified directly against the real
/// server, not assumed).
const MAX_BUILD_LOOKBACK_DAYS: i64 = 3;
/// `extract`'s own zoom range: 14 stays sharp at any reasonable viewing
/// distance while roughly halving the size a full 0..15 extract would
/// be (verified against Protomaps' own sizing guidance).
const EXTRACT_MAXZOOM: u32 = 14;
const EXTRACT_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const EXTRACT_CPU_SECS: u64 = 900;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum MapExtractError {
    #[error("unknown map region catalog id: {0}")]
    UnknownCatalogId(String),
    #[error(transparent)]
    Db(#[from] keeppix_db::DbError),
}

/// Registers the region and enqueues a single extraction via the
/// `dedup_key` — same pattern as [`crate::regions::enqueue_download`].
///
/// # Errors
/// `UnknownCatalogId` if `catalog_id` isn't in [`crate::map_catalog::CATALOG`],
/// `Db` if the queue is unavailable.
pub async fn enqueue_extraction(
    db: &Db,
    ctx: &AuthContext,
    catalog_id: &str,
) -> Result<Job, MapExtractError> {
    let entry = crate::map_catalog::find(catalog_id)
        .ok_or_else(|| MapExtractError::UnknownCatalogId(catalog_id.to_owned()))?;
    // Placeholders: `begin_extraction` needs a syntactically valid
    // checksum/URL up front, but the real ones are only known once the
    // extraction actually completes (`mark_available_with_actuals`).
    let region = NewMapRegion {
        id: entry.id.to_owned(),
        label: entry.label.to_owned(),
        size_bytes: entry.approx_size_bytes,
        version: "pending".to_owned(),
        source_url: format!("{BUILD_HOST}/pending"),
        checksum_sha256: "0".repeat(64),
    };
    let region_id = region.id.clone();
    let region = RegionRepo::new(db).begin_extraction(ctx, region).await?;
    let generation = region.download_generation;
    let file_path = region.file_path;
    let dedup_key = format!("map-region-extract:{region_id}:{generation}");
    match JobRepo::new(db)
        .enqueue(
            JobKind::ExtractMapRegion,
            serde_json::json!({
                "region_id": region_id,
                "download_generation": generation,
                "file_path": file_path,
            }),
            JobPriority::High,
            Some(&dedup_key),
        )
        .await
    {
        Ok(job) => Ok(job),
        Err(error) => {
            RegionRepo::new(db)
                .mark_error(&region_id, generation, "Could not enqueue region extraction")
                .await?;
            Err(MapExtractError::Db(error))
        }
    }
}

fn job_payload(job: &Job) -> Result<(&str, uuid::Uuid, &str), JobError> {
    let region_id = job
        .payload
        .get("region_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("missing region_id payload".to_owned()))?;
    let generation = job
        .payload
        .get("download_generation")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or_else(|| JobError::Worker("missing download_generation payload".to_owned()))?;
    let file_path = job
        .payload
        .get("file_path")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("missing file_path payload".to_owned()))?;
    Ok((region_id, generation, file_path))
}

/// Runs the job: resolves the latest available daily build, shells out to
/// `pmtiles extract` for the catalog entry's bbox, hashes and sizes the
/// result, and records it.
///
/// # Errors
/// Only for conditions a retry could plausibly fix (payload/DB failures);
/// an extraction failure itself is recorded via `mark_error` and returns
/// `Ok(())`, same convention as `crate::regions::run`.
pub async fn run(db: &Db, data_dir: &Path, job: &Job) -> Result<(), JobError> {
    let (region_id, generation, file_path) = job_payload(job)?;
    let repo = RegionRepo::new(db);
    let Some(source) = repo.source_for_download(region_id, generation).await? else {
        return Ok(());
    };
    if source.cancel_requested {
        remove_if_exists(&data_dir.join(file_path)).await;
        repo.finish_cancel(region_id, generation).await?;
        return Ok(());
    }
    let Some(entry) = crate::map_catalog::find(region_id) else {
        repo.mark_error(region_id, generation, "Region is no longer in the catalog")
            .await?;
        return Ok(());
    };

    let maps_dir = data_dir.join("maps");
    if let Err(error) = tokio::fs::create_dir_all(&maps_dir).await {
        repo.mark_error(
            region_id,
            generation,
            &format!("Cannot create maps directory: {error}"),
        )
        .await?;
        return Ok(());
    }
    let output_path = data_dir.join(file_path);

    let (build_url, build_stamp) = match resolve_latest_build().await {
        Ok(pair) => pair,
        Err(error) => {
            repo.mark_error(region_id, generation, &error).await?;
            return Ok(());
        }
    };

    if let Err(error) = extract_region(entry.bbox, &build_url, &output_path).await {
        remove_if_exists(&output_path).await;
        repo.mark_error(region_id, generation, &error).await?;
        return Ok(());
    }

    // Re-check cancellation: a cancel requested mid-extraction has no
    // subprocess-kill hook here (unlike crate::regions' byte-range HTTP
    // download, which polls this every second) — the file is simply
    // thrown away and the row already reflects `cancel_requested` from
    // whoever asked, same end state, one poll later instead of live.
    let Some(source) = repo.source_for_download(region_id, generation).await? else {
        remove_if_exists(&output_path).await;
        return Ok(());
    };
    if source.cancel_requested {
        remove_if_exists(&output_path).await;
        repo.finish_cancel(region_id, generation).await?;
        return Ok(());
    }

    finalize(&repo, region_id, generation, &output_path, &build_stamp).await
}

async fn finalize(
    repo: &RegionRepo<'_>,
    region_id: &str,
    generation: uuid::Uuid,
    output_path: &Path,
    build_stamp: &str,
) -> Result<(), JobError> {
    let (size_bytes, checksum) = match hash_and_size(output_path).await {
        Ok(pair) => pair,
        Err(error) => {
            remove_if_exists(output_path).await;
            repo.mark_error(region_id, generation, &error).await?;
            return Ok(());
        }
    };
    let source_url = format!("{BUILD_HOST}/{build_stamp}.pmtiles");
    repo.mark_available_with_actuals(region_id, generation, size_bytes, &checksum, &source_url)
        .await?;
    Ok(())
}

/// Shells out to the vendored `pmtiles` binary. Returns a human-readable
/// error (destined for `RegionRepo::mark_error`, not a retry) on either a
/// failed spawn or a non-zero exit.
async fn extract_region(
    bbox: crate::map_catalog::BBox,
    build_url: &str,
    output_path: &Path,
) -> Result<(), String> {
    let (min_lon, min_lat, max_lon, max_lat) = bbox;
    let bbox = format!("{min_lon},{min_lat},{max_lon},{max_lat}");
    let build_url = build_url.to_owned();
    let output_path = output_path.to_owned();
    let result = tokio::task::spawn_blocking(move || {
        keeppix_media::sandbox::run(
            PMTILES_BIN,
            &[
                "extract".to_owned(),
                build_url,
                output_path.display().to_string(),
                format!("--bbox={bbox}"),
                format!("--maxzoom={EXTRACT_MAXZOOM}"),
                "--quiet".to_owned(),
            ],
            EXTRACT_MEMORY_BYTES,
            EXTRACT_CPU_SECS,
        )
    })
    .await
    .map_err(|error| format!("extraction task panicked: {error}"))?;

    match result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("pmtiles extract failed: {}", truncate(&stderr, 500)))
        }
        Err(error) => Err(format!("could not run pmtiles extract: {error}")),
    }
}

/// HEAD-probes `build.protomaps.com/{date}.pmtiles` starting today (UTC),
/// walking backward until one responds — no published "latest" alias
/// exists (verified against the real server), and a build can land any
/// time during its day, so "today" isn't always there yet. Returns both
/// the full URL and the bare `YYYYMMDD` stamp, so the caller can record
/// which dated build a region actually came from without a second probe.
async fn resolve_latest_build() -> Result<(String, String), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(HTTP_TIMEOUT)
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|error| format!("HTTP client: {error}"))?;
    let today = chrono::Utc::now().date_naive();
    for offset in 0..=MAX_BUILD_LOOKBACK_DAYS {
        let date = today - chrono::Duration::days(offset);
        let stamp = date.format("%Y%m%d").to_string();
        let url = format!("{BUILD_HOST}/{stamp}.pmtiles");
        if let Ok(response) = client.head(&url).send().await
            && response.status().is_success()
        {
            return Ok((url, stamp));
        }
    }
    Err(format!(
        "no Protomaps daily build found in the last {} days",
        MAX_BUILD_LOOKBACK_DAYS + 1
    ))
}

async fn hash_and_size(path: &Path) -> Result<(i64, String), String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| format!("cannot read extracted region: {error}"))?;
    let size = i64::try_from(bytes.len()).map_err(|_| "region file implausibly large".to_owned())?;
    let checksum = hex(&Sha256::digest(&bytes));
    Ok((size, checksum))
}

async fn remove_if_exists(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}
