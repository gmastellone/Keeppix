use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use keeppix_db::{Db, JobRepo, NewMapRegion, RegionRepo};
use keeppix_domain::{AuthContext, Job, JobKind, JobPriority};
use reqwest::{Client, StatusCode, Url};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::JobError;

const ALLOWED_REGION_HOSTS: &[&str] = &["build.protomaps.com"];
const PROGRESS_STEP_BYTES: u64 = 1024 * 1024;
const CANCELLATION_POLL: Duration = Duration::from_secs(1);
const HTTP_READ_TIMEOUT: Duration = Duration::from_secs(30);
const STALE_JOB_THRESHOLD: Duration = Duration::from_secs(600);

#[derive(Debug, thiserror::Error)]
pub enum RegionError {
    #[error("region source URL is not allowed")]
    SourceNotAllowed,
    #[error("invalid region metadata")]
    InvalidRegion,
    #[error(transparent)]
    Db(#[from] keeppix_db::DbError),
}

#[derive(Debug, thiserror::Error)]
enum DownloadError {
    #[error("download cancelled")]
    Cancelled,
    #[error("download job lease was lost")]
    LeaseLost,
    #[error("transient HTTP error: {0}")]
    Transient(String),
    #[error("invalid download response: {0}")]
    InvalidResponse(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] keeppix_db::DbError),
    #[error("downloaded size differs from catalog")]
    SizeMismatch,
    #[error("checksum verification failed")]
    ChecksumMismatch,
}

impl DownloadError {
    const fn is_transient(&self) -> bool {
        matches!(self, Self::Transient(_) | Self::Db(_))
    }
}

struct Progress<'repo, 'db> {
    repo: &'repo RegionRepo<'db>,
    jobs: JobRepo<'db>,
    region_id: &'repo str,
    generation: uuid::Uuid,
    job_id: i64,
    worker_id: Option<uuid::Uuid>,
}

impl Progress<'_, '_> {
    async fn checkpoint(&self, bytes: u64) -> Result<(), DownloadError> {
        if let Some(worker_id) = self.worker_id
            && !self.jobs.renew_lock(self.job_id, worker_id).await?
        {
            return Err(DownloadError::LeaseLost);
        }
        let bytes = i64::try_from(bytes).unwrap_or(i64::MAX);
        if !self
            .repo
            .record_progress(self.region_id, self.generation, bytes)
            .await?
        {
            return Err(DownloadError::Cancelled);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairResult {
    pub reaped: u64,
    pub reenqueued: u64,
}

/// Checks the URL against the list compiled into the binary. HTTP redirects
/// are disabled on the client so that an allowed host can't be used as a
/// stepping stone.
#[must_use]
pub fn host_allowed(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url
            .host_str()
            .is_some_and(|host| ALLOWED_REGION_HOSTS.contains(&host))
}

/// Repairs interrupted downloads: first frees expired leases, then recreates
/// missing jobs for regions still downloading.
///
/// # Errors
/// `DbError` if either queue operation fails.
pub async fn repair_interrupted_downloads(db: &Db) -> Result<RepairResult, keeppix_db::DbError> {
    let jobs = JobRepo::new(db);
    let reaped = jobs.reap_stale(STALE_JOB_THRESHOLD).await?;
    let reenqueued = jobs.enqueue_missing_region_downloads().await?;
    Ok(RepairResult { reaped, reenqueued })
}

/// Recovers downloads at boot, when every job still `running` necessarily
/// belongs to the dead process. Resets both region job kinds — a manual
/// URL download and a catalog `pmtiles extract` are the same `map_regions`
/// status machine, just different acquisition mechanisms
/// (`RegionRepo::begin_download` vs `begin_extraction`), and either can be
/// the one a dead process left `running`.
///
/// # Errors
/// `DbError` if the reset or the queue rebuild fails.
pub async fn recover_interrupted_downloads(db: &Db) -> Result<RepairResult, keeppix_db::DbError> {
    let jobs = JobRepo::new(db);
    let reaped_download = jobs.reset_running(JobKind::DownloadMapRegion).await?;
    let reaped_extract = jobs.reset_running(JobKind::ExtractMapRegion).await?;
    let reenqueued = jobs.enqueue_missing_region_downloads().await?;
    Ok(RepairResult {
        reaped: reaped_download + reaped_extract,
        reenqueued,
    })
}

/// Enqueues the periodic run that reaps genuinely stale leases.
///
/// `High`, not `Background`: this is the watchdog that un-sticks jobs a
/// `RamGate` permit (or anything else) has wedged for 10+ minutes —
/// including, in practice, other `Background` jobs. At `Background` itself
/// it inherits the exact problem it exists to fix: `EnergyProfile::
/// Interactive`'s ceiling is `Visible`, so as long as someone keeps a page
/// open that's polling anything authenticated (e.g. watching an import's
/// progress), the reaper could never run either — the one thing capable of
/// recovering a stuck queue would itself be the thing stuck. The work here
/// is a single `UPDATE`, not per-photo processing, so there's no real cost
/// to running it above `Background`.
///
/// # Errors
/// Database.
pub async fn schedule_reap_stale(db: &Db) -> Result<(), JobError> {
    JobRepo::new(db)
        .enqueue(
            JobKind::ReapStale,
            serde_json::json!({}),
            JobPriority::High,
            Some("reap_stale"),
        )
        .await?;
    Ok(())
}

/// Registers the region and enqueues a single writer via the `dedup_key`.
///
/// # Errors
/// `SourceNotAllowed` for URLs outside the allowlist, `InvalidRegion` for
/// malformed metadata, or `Db` if the queue is unavailable.
pub async fn enqueue_download(
    db: &Db,
    ctx: &AuthContext,
    region: NewMapRegion,
) -> Result<Job, RegionError> {
    if !host_allowed(&region.source_url) {
        return Err(RegionError::SourceNotAllowed);
    }
    if !valid_region_id(&region.id)
        || region.size_bytes <= 0
        || !valid_checksum(&region.checksum_sha256)
    {
        return Err(RegionError::InvalidRegion);
    }
    let region_id = region.id.clone();
    let region = RegionRepo::new(db).begin_download(ctx, region).await?;
    let generation = region.download_generation;
    let file_path = region.file_path;
    let dedup_key = format!("map-region:{region_id}:{generation}");
    match JobRepo::new(db)
        .enqueue(
            JobKind::DownloadMapRegion,
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
                .mark_error(&region_id, generation, "Could not enqueue region download")
                .await?;
            Err(RegionError::Db(error))
        }
    }
}

/// Runs the job and rechecks the allowlist immediately before hitting the
/// network.
///
/// # Errors
/// Transient network errors are returned to the worker for retry.
pub async fn run(db: &Db, data_dir: &Path, job: &Job) -> Result<(), JobError> {
    let (region_id, generation, file_path) = job_download(job)?;
    if !valid_region_id(region_id) {
        return Err(JobError::Worker("invalid region_id payload".to_owned()));
    }
    let repo = RegionRepo::new(db);
    let Some(source) = repo.source_for_download(region_id, generation).await? else {
        cleanup_partial(data_dir, file_path).await?;
        return Ok(());
    };
    if source.file_path != file_path {
        return Err(JobError::Worker(
            "region download file ownership was lost".to_owned(),
        ));
    }
    if source.cancel_requested {
        cleanup_paths(data_dir, file_path).await?;
        repo.finish_cancel(region_id, generation).await?;
        return Ok(());
    }
    if !host_allowed(&source.source_url) {
        cleanup_paths(data_dir, file_path).await?;
        repo.mark_error(region_id, generation, "Region source URL is not allowed")
            .await?;
        return Ok(());
    }
    let url = Url::parse(&source.source_url)
        .map_err(|error| JobError::Worker(format!("invalid region URL: {error}")))?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(HTTP_READ_TIMEOUT)
        .build()
        .map_err(|error| JobError::Worker(format!("HTTP client: {error}")))?;
    run_download_from_url(db, data_dir, job, &url, &client).await
}

async fn run_download_from_url(
    db: &Db,
    data_dir: &Path,
    job: &Job,
    url: &Url,
    client: &Client,
) -> Result<(), JobError> {
    let (region_id, generation, file_path) = job_download(job)?;
    let repo = RegionRepo::new(db);
    let Some(source) = repo.source_for_download(region_id, generation).await? else {
        cleanup_partial(data_dir, file_path).await?;
        return Ok(());
    };
    if source.file_path != file_path {
        return Err(JobError::Worker(
            "region download file ownership was lost".to_owned(),
        ));
    }
    if source.cancel_requested {
        cleanup_paths(data_dir, file_path).await?;
        repo.finish_cancel(region_id, generation).await?;
        return Ok(());
    }
    let Ok(expected_size) = u64::try_from(source.size_bytes) else {
        cleanup_paths(data_dir, file_path).await?;
        repo.mark_error(region_id, generation, "Invalid region size")
            .await?;
        return Ok(());
    };
    let maps = data_dir.join("maps");
    if let Err(error) = tokio::fs::create_dir_all(&maps).await {
        cleanup_paths(data_dir, file_path).await?;
        repo.mark_error(
            region_id,
            generation,
            &format!("Cannot create maps directory: {error}"),
        )
        .await?;
        return Ok(());
    }
    let final_path = final_path(data_dir, file_path);
    let partial = partial_path(data_dir, file_path);
    let progress = Progress {
        repo: &repo,
        jobs: JobRepo::new(db),
        region_id,
        generation,
        job_id: job.id,
        worker_id: job.locked_by,
    };
    match download_to_partial_with_progress(
        client,
        url,
        &partial,
        expected_size,
        &source.checksum_sha256,
        Some(progress),
    )
    .await
    {
        Ok(_) => {
            if !may_finalize_download(&repo, data_dir, region_id, generation, file_path).await? {
                return Ok(());
            }
            if let Err(error) = tokio::fs::rename(&partial, &final_path).await {
                cleanup_paths(data_dir, file_path).await?;
                repo.mark_error(
                    region_id,
                    generation,
                    &format!("Cannot finalize region file: {error}"),
                )
                .await?;
                return Ok(());
            }
            if !repo.mark_available(region_id, generation).await? {
                cleanup_paths(data_dir, file_path).await?;
                repo.finish_cancel(region_id, generation).await?;
            }
            Ok(())
        }
        Err(DownloadError::Cancelled) => {
            cleanup_paths(data_dir, file_path).await?;
            repo.finish_cancel(region_id, generation).await?;
            Ok(())
        }
        Err(DownloadError::LeaseLost) => {
            Err(JobError::Worker(DownloadError::LeaseLost.to_string()))
        }
        Err(error) if error.is_transient() => {
            if job.attempts >= job.max_attempts {
                cleanup_paths(data_dir, file_path).await?;
                repo.mark_error(region_id, generation, &error.to_string())
                    .await?;
            }
            Err(JobError::Worker(error.to_string()))
        }
        Err(error) => {
            cleanup_paths(data_dir, file_path).await?;
            repo.mark_error(region_id, generation, &error.to_string())
                .await?;
            Ok(())
        }
    }
}

async fn may_finalize_download(
    repo: &RegionRepo<'_>,
    data_dir: &Path,
    region_id: &str,
    generation: uuid::Uuid,
    file_path: &str,
) -> Result<bool, JobError> {
    let still_owned = repo
        .source_for_download(region_id, generation)
        .await?
        .is_some_and(|source| !source.cancel_requested && source.file_path == file_path);
    if still_owned {
        return Ok(true);
    }
    cleanup_paths(data_dir, file_path).await?;
    repo.finish_cancel(region_id, generation).await?;
    Ok(false)
}

#[cfg(test)]
async fn download_to_partial(
    client: &Client,
    url: &Url,
    partial: &Path,
    expected_size: u64,
    expected_checksum: &str,
) -> Result<u64, DownloadError> {
    download_to_partial_with_progress(client, url, partial, expected_size, expected_checksum, None)
        .await
}

async fn download_to_partial_with_progress(
    client: &Client,
    url: &Url,
    partial: &Path,
    expected_size: u64,
    expected_checksum: &str,
    progress: Option<Progress<'_, '_>>,
) -> Result<u64, DownloadError> {
    let mut offset = match tokio::fs::metadata(partial).await {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(DownloadError::Io(error)),
    };
    if offset > expected_size {
        return Err(DownloadError::SizeMismatch);
    }
    if offset < expected_size {
        let mut request = client.get(url.clone());
        if offset > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|error| DownloadError::Transient(error.to_string()))?;
        let status = response.status();
        if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
            return Err(DownloadError::Transient(format!("HTTP {status}")));
        }
        let append = if offset == 0 {
            if status != StatusCode::OK && status != StatusCode::PARTIAL_CONTENT {
                return Err(DownloadError::InvalidResponse(format!("HTTP {status}")));
            }
            if status == StatusCode::PARTIAL_CONTENT {
                validate_content_range(&response, 0)?;
            }
            false
        } else if status == StatusCode::PARTIAL_CONTENT {
            validate_content_range(&response, offset)?;
            true
        } else if status == StatusCode::OK {
            offset = 0;
            false
        } else {
            return Err(DownloadError::InvalidResponse(format!("HTTP {status}")));
        };

        let mut options = tokio::fs::OpenOptions::new();
        options.create(true).write(true);
        if append {
            options.append(true);
        } else {
            options.truncate(true);
        }
        let mut file = options.open(partial).await?;
        let mut response = response;
        let mut last_recorded = offset;
        let mut last_poll = Instant::now();
        loop {
            let chunk = if let Some(progress) = progress.as_ref() {
                tokio::select! {
                    result = response.chunk() => Some(result),
                    () = tokio::time::sleep(CANCELLATION_POLL) => {
                        progress.checkpoint(offset).await?;
                        last_recorded = offset;
                        last_poll = Instant::now();
                        None
                    }
                }
            } else {
                Some(response.chunk().await)
            };
            let Some(chunk) = chunk else {
                continue;
            };
            let Some(chunk) = chunk.map_err(|error| DownloadError::Transient(error.to_string()))?
            else {
                break;
            };
            let chunk_len = u64::try_from(chunk.len()).map_err(|_| DownloadError::SizeMismatch)?;
            offset = offset
                .checked_add(chunk_len)
                .ok_or(DownloadError::SizeMismatch)?;
            if offset > expected_size {
                return Err(DownloadError::SizeMismatch);
            }
            file.write_all(&chunk).await?;
            if let Some(progress) = progress.as_ref()
                && (offset.saturating_sub(last_recorded) >= PROGRESS_STEP_BYTES
                    || last_poll.elapsed() >= CANCELLATION_POLL)
            {
                progress.checkpoint(offset).await?;
                last_recorded = offset;
                last_poll = Instant::now();
            }
        }
        file.flush().await?;
        file.sync_all().await?;
    }
    let progress = progress.as_ref();
    verify_completed(partial, expected_size, expected_checksum, progress, offset).await?;
    Ok(offset)
}

async fn verify_completed(
    partial: &Path,
    expected_size: u64,
    expected_checksum: &str,
    progress: Option<&Progress<'_, '_>>,
    offset: u64,
) -> Result<(), DownloadError> {
    if offset != expected_size {
        return Err(DownloadError::SizeMismatch);
    }
    if let Some(progress) = progress {
        progress.checkpoint(offset).await?;
    }
    verify_checksum(partial, expected_checksum, progress, offset).await?;
    if let Some(progress) = progress {
        progress.checkpoint(offset).await?;
    }
    Ok(())
}

fn validate_content_range(
    response: &reqwest::Response,
    expected_start: u64,
) -> Result<(), DownloadError> {
    let value = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| DownloadError::InvalidResponse("missing Content-Range".to_owned()))?;
    let expected = format!("bytes {expected_start}-");
    if value.starts_with(&expected) {
        Ok(())
    } else {
        Err(DownloadError::InvalidResponse(format!(
            "unexpected Content-Range {value}"
        )))
    }
}

async fn verify_checksum(
    path: &Path,
    expected: &str,
    progress: Option<&Progress<'_, '_>>,
    downloaded_bytes: u64,
) -> Result<(), DownloadError> {
    if !valid_checksum(expected) {
        return Err(DownloadError::ChecksumMismatch);
    }
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut last_poll = Instant::now();
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        if let Some(progress) = progress
            && last_poll.elapsed() >= CANCELLATION_POLL
        {
            progress.checkpoint(downloaded_bytes).await?;
            last_poll = Instant::now();
        }
    }
    let actual = hex(&hasher.finalize());
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(DownloadError::ChecksumMismatch)
    }
}

fn valid_region_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn valid_checksum(checksum: &str) -> bool {
    checksum.len() == 64 && checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn job_download(job: &Job) -> Result<(&str, uuid::Uuid, &str), JobError> {
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
    let generated = format!("maps/{region_id}-{generation}.pmtiles");
    let migrated = format!("maps/{region_id}.pmtiles");
    if file_path != generated && file_path != migrated {
        return Err(JobError::Worker(
            "invalid region file_path payload".to_owned(),
        ));
    }
    Ok((region_id, generation, file_path))
}

fn final_path(data_dir: &Path, file_path: &str) -> PathBuf {
    data_dir.join(file_path)
}

fn partial_path(data_dir: &Path, file_path: &str) -> PathBuf {
    let mut partial = final_path(data_dir, file_path).into_os_string();
    partial.push(".part");
    PathBuf::from(partial)
}

async fn cleanup_paths(data_dir: &Path, file_path: &str) -> Result<(), JobError> {
    for path in [
        partial_path(data_dir, file_path),
        final_path(data_dir, file_path),
    ] {
        remove_path(&path).await?;
    }
    Ok(())
}

async fn cleanup_partial(data_dir: &Path, file_path: &str) -> Result<(), JobError> {
    remove_path(&partial_path(data_dir, file_path)).await
}

async fn remove_path(path: &Path) -> Result<(), JobError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(JobError::Worker(format!(
            "cannot clean region file {}: {error}",
            path.display()
        ))),
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use sha2::{Digest as _, Sha256};
    use testcontainers_modules::postgres::Postgres;
    use testcontainers_modules::testcontainers::ImageExt as _;
    use testcontainers_modules::testcontainers::runners::AsyncRunner as _;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    use super::{download_to_partial, host_allowed, run_download_from_url};

    #[test]
    fn only_compile_time_https_hosts_are_allowed() {
        assert!(host_allowed("https://build.protomaps.com/20260818.pmtiles"));
        for rejected in [
            "http://build.protomaps.com/20260818.pmtiles",
            "https://localhost/map.pmtiles",
            "https://127.0.0.1/map.pmtiles",
            "file:///etc/passwd",
            "https://user@build.protomaps.com/map.pmtiles",
            "https://build.protomaps.com.evil.example/map.pmtiles",
            "https://example.com/map.pmtiles",
        ] {
            assert!(!host_allowed(rejected), "accepted {rejected}");
        }
    }

    #[tokio::test]
    async fn download_resumes_from_the_partial_file_offset() {
        let temp = tempfile::tempdir().unwrap();
        let partial = temp.path().join("IT.pmtiles.part");
        tokio::fs::write(&partial, b"hello").await.unwrap();
        let full = b"hello world";
        let checksum = hex(&Sha256::digest(full));
        let url = serve_once(
            b" world",
            "HTTP/1.1 206 Partial Content",
            Some("bytes 5-10/11"),
            Some("range: bytes=5-"),
        )
        .await;

        download_to_partial(
            &test_client(),
            &url.parse().unwrap(),
            &partial,
            full.len() as u64,
            &checksum,
        )
        .await
        .unwrap();

        assert_eq!(tokio::fs::read(partial).await.unwrap(), full);
    }

    #[tokio::test]
    async fn bad_checksum_never_becomes_available_and_removes_partial_file() {
        let (db, _container) = test_db().await;
        let admin = seed_admin(&db).await;
        let ctx = keeppix_domain::AuthContext::user(admin, keeppix_domain::SystemRole::Admin);
        let temp = tempfile::tempdir().unwrap();
        let started = keeppix_db::RegionRepo::new(&db)
            .begin_download(
                &ctx,
                keeppix_db::NewMapRegion {
                    id: "IT".to_owned(),
                    label: "Italia".to_owned(),
                    size_bytes: 7,
                    version: "2026-08".to_owned(),
                    source_url: "https://build.protomaps.com/IT.pmtiles".to_owned(),
                    checksum_sha256: "00".repeat(32),
                },
            )
            .await
            .unwrap();
        let url = serve_once(b"corrupt", "HTTP/1.1 200 OK", None, None).await;

        run_download_from_url(
            &db,
            temp.path(),
            &test_job(&started),
            &url.parse().unwrap(),
            &test_client(),
        )
        .await
        .unwrap();

        let region = keeppix_db::RegionRepo::new(&db)
            .find(&ctx, "IT")
            .await
            .unwrap();
        assert_eq!(region.status, keeppix_db::RegionStatus::Error);
        assert!(region.last_error.as_deref().unwrap().contains("checksum"));
        assert!(!super::partial_path(temp.path(), &started.file_path).exists());
        assert!(!super::final_path(temp.path(), &started.file_path).exists());
    }

    #[tokio::test]
    async fn worker_revalidates_the_stored_url_before_any_request() {
        let (db, _container) = test_db().await;
        let admin = seed_admin(&db).await;
        let ctx = keeppix_domain::AuthContext::user(admin, keeppix_domain::SystemRole::Admin);
        let started = keeppix_db::RegionRepo::new(&db)
            .begin_download(
                &ctx,
                keeppix_db::NewMapRegion {
                    id: "IT".to_owned(),
                    label: "Italia".to_owned(),
                    size_bytes: 7,
                    version: "2026-08".to_owned(),
                    source_url: "https://127.0.0.1/private".to_owned(),
                    checksum_sha256: "00".repeat(32),
                },
            )
            .await
            .unwrap();
        let job = test_job(&started);
        let temp = tempfile::tempdir().unwrap();

        super::run(&db, temp.path(), &job).await.unwrap();

        let region = keeppix_db::RegionRepo::new(&db)
            .find(&ctx, "IT")
            .await
            .unwrap();
        assert_eq!(region.status, keeppix_db::RegionStatus::Error);
        assert_eq!(
            region.last_error.as_deref(),
            Some("Region source URL is not allowed")
        );
    }

    #[tokio::test]
    async fn stalled_body_observes_cancel_and_renews_the_job_lease() {
        let (db, _container) = test_db().await;
        let admin = seed_admin(&db).await;
        let ctx = keeppix_domain::AuthContext::user(admin, keeppix_domain::SystemRole::Admin);
        super::enqueue_download(&db, &ctx, region_fixture("IT", 11))
            .await
            .unwrap();
        let worker = uuid::Uuid::now_v7();
        let job = keeppix_db::JobRepo::new(&db)
            .claim(worker, keeppix_domain::JobPriority::High)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.attempts, 1);
        let file_path = job.payload["file_path"].as_str().unwrap().to_owned();
        sqlx::query("UPDATE jobs SET locked_at = now() - interval '20 minutes' WHERE id = $1")
            .bind(job.id)
            .execute(db.pool())
            .await
            .unwrap();
        let job_id = job.id;
        let temp = tempfile::tempdir().unwrap();
        let (url, first_chunk) = serve_stalled(b"hello", 11).await;
        let run_db = db.clone();
        let data_dir = temp.path().to_owned();
        let task = tokio::spawn(async move {
            run_download_from_url(
                &run_db,
                &data_dir,
                &job,
                &url.parse().unwrap(),
                &test_client(),
            )
            .await
        });
        first_chunk.await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(1_200)).await;

        assert_eq!(
            keeppix_db::JobRepo::new(&db)
                .reap_stale(std::time::Duration::from_secs(600))
                .await
                .unwrap(),
            0
        );
        keeppix_db::RegionRepo::new(&db)
            .request_cancel(&ctx, "IT")
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), task)
            .await
            .expect("stalled download ignored cancellation")
            .unwrap()
            .unwrap();

        assert!(!super::partial_path(temp.path(), &file_path).exists());
        assert_eq!(
            keeppix_db::RegionRepo::new(&db)
                .find(&ctx, "IT")
                .await
                .unwrap()
                .status,
            keeppix_db::RegionStatus::Error
        );
        let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(status, "running");
    }

    #[tokio::test]
    async fn cancel_retires_running_job_before_a_distinct_download_can_start() {
        let (db, _container) = test_db().await;
        let admin = seed_admin(&db).await;
        let ctx = keeppix_domain::AuthContext::user(admin, keeppix_domain::SystemRole::Admin);
        let old_contents = b"hello world";
        let old_checksum = hex(&Sha256::digest(old_contents));
        let old_job = super::enqueue_download(
            &db,
            &ctx,
            keeppix_db::NewMapRegion {
                checksum_sha256: old_checksum,
                ..region_fixture("IT", i64::try_from(old_contents.len()).unwrap())
            },
        )
        .await
        .unwrap();
        let old_generation =
            uuid::Uuid::parse_str(old_job.payload["download_generation"].as_str().unwrap())
                .unwrap();
        let old_file_path = old_job.payload["file_path"].as_str().unwrap().to_owned();
        let jobs = keeppix_db::JobRepo::new(&db);
        sqlx::query("UPDATE jobs SET max_attempts = 1 WHERE id = $1")
            .bind(old_job.id)
            .execute(db.pool())
            .await
            .unwrap();
        let claimed = jobs
            .claim(uuid::Uuid::now_v7(), keeppix_domain::JobPriority::High)
            .await
            .unwrap()
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let (url, first_chunk, release) = serve_paused(b"hello", b" world").await;
        let run_db = db.clone();
        let data_dir = temp.path().to_owned();
        let task = tokio::spawn(async move {
            run_download_from_url(
                &run_db,
                &data_dir,
                &claimed,
                &url.parse().unwrap(),
                &test_client(),
            )
            .await
        });
        first_chunk.await.unwrap();

        let regions = keeppix_db::RegionRepo::new(&db);
        regions.request_cancel(&ctx, "IT").await.unwrap();
        super::cleanup_paths(temp.path(), &old_file_path)
            .await
            .unwrap();
        jobs.retire_active(old_job.dedup_key.as_deref().unwrap(), "Download cancelled")
            .await
            .unwrap();
        regions.finish_cancel("IT", old_generation).await.unwrap();
        let new_contents = b"new contents!";
        let new_job = super::enqueue_download(
            &db,
            &ctx,
            keeppix_db::NewMapRegion {
                id: "IT".to_owned(),
                label: "Italia nuova".to_owned(),
                size_bytes: i64::try_from(new_contents.len()).unwrap(),
                version: "2026-09".to_owned(),
                source_url: "https://build.protomaps.com/IT-new.pmtiles".to_owned(),
                checksum_sha256: hex(&Sha256::digest(new_contents)),
            },
        )
        .await
        .unwrap();
        assert_ne!(new_job.id, old_job.id);

        release.send(()).unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(3), task)
                .await
                .expect("old worker did not stop")
                .unwrap()
                .is_err()
        );
        let region = regions.find(&ctx, "IT").await.unwrap();
        assert_eq!(region.status, keeppix_db::RegionStatus::Downloading);
        assert_eq!(
            region.source_url,
            "https://build.protomaps.com/IT-new.pmtiles"
        );
        assert!(!super::final_path(temp.path(), &old_file_path).exists());
    }

    #[tokio::test]
    async fn exhausted_download_marks_error_cleans_files_and_can_be_started_again() {
        let (db, _container) = test_db().await;
        let admin = seed_admin(&db).await;
        let ctx = keeppix_domain::AuthContext::user(admin, keeppix_domain::SystemRole::Admin);
        let queued = super::enqueue_download(&db, &ctx, region_fixture("IT", 11))
            .await
            .unwrap();
        let file_path = queued.payload["file_path"].as_str().unwrap().to_owned();
        sqlx::query("UPDATE jobs SET max_attempts = 1 WHERE id = $1")
            .bind(queued.id)
            .execute(db.pool())
            .await
            .unwrap();
        let job = keeppix_db::JobRepo::new(&db)
            .claim(uuid::Uuid::now_v7(), keeppix_domain::JobPriority::High)
            .await
            .unwrap()
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(temp.path().join("maps"))
            .await
            .unwrap();
        let final_path = super::final_path(temp.path(), &file_path);
        tokio::fs::write(&final_path, b"corrupt").await.unwrap();
        let url = serve_once(b"", "HTTP/1.1 500 Internal Server Error", None, None).await;

        let error = run_download_from_url(
            &db,
            temp.path(),
            &job,
            &url.parse().unwrap(),
            &test_client(),
        )
        .await
        .unwrap_err();
        keeppix_db::JobRepo::new(&db)
            .fail(job.id, &error.to_string())
            .await
            .unwrap();

        assert!(!final_path.exists());
        assert_eq!(
            keeppix_db::RegionRepo::new(&db)
                .find(&ctx, "IT")
                .await
                .unwrap()
                .status,
            keeppix_db::RegionStatus::Error
        );
        let restarted = super::enqueue_download(&db, &ctx, region_fixture("IT", 11))
            .await
            .unwrap();
        assert_ne!(restarted.id, queued.id);
    }

    #[tokio::test]
    async fn failed_exhausted_cleanup_keeps_region_downloading_for_cancel_retry() {
        let (db, _container) = test_db().await;
        let admin = seed_admin(&db).await;
        let ctx = keeppix_domain::AuthContext::user(admin, keeppix_domain::SystemRole::Admin);
        let queued = super::enqueue_download(&db, &ctx, region_fixture("IT", 11))
            .await
            .unwrap();
        let file_path = queued.payload["file_path"].as_str().unwrap().to_owned();
        sqlx::query("UPDATE jobs SET max_attempts = 1 WHERE id = $1")
            .bind(queued.id)
            .execute(db.pool())
            .await
            .unwrap();
        let job = keeppix_db::JobRepo::new(&db)
            .claim(uuid::Uuid::now_v7(), keeppix_domain::JobPriority::High)
            .await
            .unwrap()
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let final_path = super::final_path(temp.path(), &file_path);
        tokio::fs::create_dir_all(&final_path).await.unwrap();
        let url = serve_once(b"", "HTTP/1.1 500 Internal Server Error", None, None).await;

        let error = run_download_from_url(
            &db,
            temp.path(),
            &job,
            &url.parse().unwrap(),
            &test_client(),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("cannot clean region file"));
        assert_eq!(
            keeppix_db::RegionRepo::new(&db)
                .find(&ctx, "IT")
                .await
                .unwrap()
                .status,
            keeppix_db::RegionStatus::Downloading
        );
        keeppix_db::RegionRepo::new(&db)
            .request_cancel(&ctx, "IT")
            .await
            .unwrap();
    }

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
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

    async fn serve_once(
        body: &'static [u8],
        status: &'static str,
        content_range: Option<&'static str>,
        expected_header: Option<&'static str>,
    ) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]).to_ascii_lowercase();
            if let Some(expected) = expected_header {
                assert!(request.contains(expected), "request was {request}");
            }
            let range = content_range
                .map_or_else(String::new, |value| format!("Content-Range: {value}\r\n"));
            let headers = format!(
                "{status}\r\nContent-Length: {}\r\n{range}Connection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(body).await.unwrap();
        });
        format!("http://{address}/region.pmtiles")
    }

    async fn serve_stalled(
        first_chunk: &'static [u8],
        content_length: usize,
    ) -> (String, tokio::sync::oneshot::Receiver<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, received) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 4096];
            let _read = stream.read(&mut request).await.unwrap();
            let headers = format!("HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\n\r\n");
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(first_chunk).await.unwrap();
            sent.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        (format!("http://{address}/region.pmtiles"), received)
    }

    async fn serve_paused(
        first_chunk: &'static [u8],
        remainder: &'static [u8],
    ) -> (
        String,
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, first_received) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 4096];
            let _read = stream.read(&mut request).await.unwrap();
            let content_length = first_chunk.len() + remainder.len();
            let headers = format!("HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\n\r\n");
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(first_chunk).await.unwrap();
            sent.send(()).unwrap();
            released.await.unwrap();
            stream.write_all(remainder).await.unwrap();
        });
        (
            format!("http://{address}/region.pmtiles"),
            first_received,
            release,
        )
    }

    fn region_fixture(id: &str, size_bytes: i64) -> keeppix_db::NewMapRegion {
        keeppix_db::NewMapRegion {
            id: id.to_owned(),
            label: "Italia".to_owned(),
            size_bytes,
            version: "2026-08".to_owned(),
            source_url: "https://build.protomaps.com/IT.pmtiles".to_owned(),
            checksum_sha256: "ab".repeat(32),
        }
    }

    fn test_job(region: &keeppix_db::MapRegion) -> keeppix_domain::Job {
        keeppix_domain::Job {
            id: 1,
            kind: keeppix_domain::JobKind::DownloadMapRegion,
            payload: serde_json::json!({
                "region_id": region.id,
                "download_generation": region.download_generation,
                "file_path": region.file_path,
            }),
            priority: keeppix_domain::JobPriority::High,
            status: keeppix_domain::JobStatus::Running,
            attempts: 1,
            max_attempts: 3,
            last_error: None,
            run_after: chrono::Utc::now(),
            locked_by: None,
            dedup_key: Some(format!(
                "map-region:{}:{}",
                region.id, region.download_generation
            )),
        }
    }

    async fn test_db() -> (
        keeppix_db::Db,
        testcontainers_modules::testcontainers::ContainerAsync<Postgres>,
    ) {
        let container = Postgres::default()
            .with_tag("17-3.5")
            .with_name("postgis/postgis")
            .start()
            .await
            .expect("postgres");
        let port = container.get_host_port_ipv4(5432).await.expect("port");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
        let db = keeppix_db::Db::connect(&url, 5).await.expect("db");
        db.migrate().await.expect("migrations");
        (db, container)
    }

    /// If a real stuck job (e.g. a `RamGate` permit held by an oversized
    /// file) leaves the session looking `Interactive`, the reaper must
    /// still be claimable — it's the one thing that can recover the queue.
    /// `Background` would make it subject to the exact problem it exists
    /// to fix.
    #[tokio::test]
    async fn the_reaper_is_claimable_even_while_the_session_looks_interactive() {
        let (db, _container) = test_db().await;
        super::schedule_reap_stale(&db).await.unwrap();

        let claimed = keeppix_db::JobRepo::new(&db)
            .claim(uuid::Uuid::now_v7(), keeppix_domain::JobPriority::Visible)
            .await
            .unwrap();

        let job = claimed
            .expect("the reaper must be claimable at Visible priority, not stuck at Background");
        assert_eq!(job.kind, keeppix_domain::JobKind::ReapStale);
    }

    async fn seed_admin(db: &keeppix_db::Db) -> keeppix_domain::UserId {
        use keeppix_domain::{NewUser, Password, SystemRole, Username, hash_password};

        let password = Password::parse("correct horse battery staple").expect("password");
        keeppix_db::UserRepo::new(db)
            .create_bootstrap_admin(NewUser {
                username: Username::parse("region-admin").expect("username"),
                email: None,
                display_name: "Region admin".to_owned(),
                password_hash: hash_password(&password).expect("hash").as_str().to_owned(),
                role: SystemRole::Admin,
            })
            .await
            .expect("admin")
            .id
    }
}
