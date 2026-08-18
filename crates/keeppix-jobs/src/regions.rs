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
    region_id: &'repo str,
}

/// Verifica l'URL contro la lista compilata nel binario. Redirect HTTP
/// disabilitati nel client impediscono che un host ammesso faccia da trampolino.
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

/// Registra la regione e accoda un solo writer tramite la `dedup_key`.
///
/// # Errors
/// `SourceNotAllowed` per URL fuori allowlist, `InvalidRegion` per metadati
/// malformati, o `Db` se la coda non è disponibile.
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
    RegionRepo::new(db).begin_download(ctx, region).await?;
    let dedup_key = format!("map-region:{region_id}");
    match JobRepo::new(db)
        .enqueue(
            JobKind::DownloadMapRegion,
            serde_json::json!({ "region_id": region_id }),
            JobPriority::High,
            Some(&dedup_key),
        )
        .await
    {
        Ok(job) => Ok(job),
        Err(error) => {
            RegionRepo::new(db)
                .mark_error(&region_id, "Could not enqueue region download")
                .await?;
            Err(RegionError::Db(error))
        }
    }
}

/// Esegue il job e ricontrolla l'allowlist immediatamente prima della rete.
///
/// # Errors
/// Errori transitori di rete vengono restituiti al worker per il retry.
pub async fn run(db: &Db, data_dir: &Path, job: &Job) -> Result<(), JobError> {
    let region_id = job
        .payload
        .get("region_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("missing region_id payload".to_owned()))?;
    if !valid_region_id(region_id) {
        return Err(JobError::Worker("invalid region_id payload".to_owned()));
    }
    let repo = RegionRepo::new(db);
    let Some(source) = repo.source_for_download(region_id).await? else {
        return Ok(());
    };
    if !host_allowed(&source.source_url) {
        cleanup_paths(data_dir, region_id).await;
        repo.mark_error(region_id, "Region source URL is not allowed")
            .await?;
        return Ok(());
    }
    let url = Url::parse(&source.source_url)
        .map_err(|error| JobError::Worker(format!("invalid region URL: {error}")))?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| JobError::Worker(format!("HTTP client: {error}")))?;
    run_download_from_url(db, data_dir, region_id, &url, &client).await
}

async fn run_download_from_url(
    db: &Db,
    data_dir: &Path,
    region_id: &str,
    url: &Url,
    client: &Client,
) -> Result<(), JobError> {
    let repo = RegionRepo::new(db);
    let Some(source) = repo.source_for_download(region_id).await? else {
        cleanup_paths(data_dir, region_id).await;
        return Ok(());
    };
    let Ok(expected_size) = u64::try_from(source.size_bytes) else {
        repo.mark_error(region_id, "Invalid region size").await?;
        return Ok(());
    };
    let maps = data_dir.join("maps");
    if let Err(error) = tokio::fs::create_dir_all(&maps).await {
        repo.mark_error(region_id, &format!("Cannot create maps directory: {error}"))
            .await?;
        return Ok(());
    }
    let partial = partial_path(data_dir, region_id);
    let final_path = final_path(data_dir, region_id);
    let progress = Progress {
        repo: &repo,
        region_id,
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
            if let Err(error) = tokio::fs::rename(&partial, &final_path).await {
                cleanup_paths(data_dir, region_id).await;
                repo.mark_error(region_id, &format!("Cannot finalize region file: {error}"))
                    .await?;
                return Ok(());
            }
            if !repo.mark_available(region_id).await? {
                let _ = tokio::fs::remove_file(&final_path).await;
            }
            Ok(())
        }
        Err(DownloadError::Cancelled) => {
            cleanup_paths(data_dir, region_id).await;
            Ok(())
        }
        Err(error) if error.is_transient() => Err(JobError::Worker(error.to_string())),
        Err(error) => {
            cleanup_paths(data_dir, region_id).await;
            repo.mark_error(region_id, &error.to_string()).await?;
            Ok(())
        }
    }
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
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| DownloadError::Transient(error.to_string()))?
        {
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
                let bytes = i64::try_from(offset).unwrap_or(i64::MAX);
                if !progress
                    .repo
                    .record_progress(progress.region_id, bytes)
                    .await?
                {
                    return Err(DownloadError::Cancelled);
                }
                last_recorded = offset;
                last_poll = Instant::now();
            }
        }
        file.flush().await?;
        file.sync_all().await?;
    }
    if offset != expected_size {
        return Err(DownloadError::SizeMismatch);
    }
    if let Some(progress) = progress.as_ref() {
        let bytes = i64::try_from(offset).unwrap_or(i64::MAX);
        if !progress
            .repo
            .record_progress(progress.region_id, bytes)
            .await?
        {
            return Err(DownloadError::Cancelled);
        }
    }
    verify_checksum(partial, expected_checksum).await?;
    Ok(offset)
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

async fn verify_checksum(path: &Path, expected: &str) -> Result<(), DownloadError> {
    if !valid_checksum(expected) {
        return Err(DownloadError::ChecksumMismatch);
    }
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
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

fn final_path(data_dir: &Path, region_id: &str) -> PathBuf {
    data_dir.join("maps").join(format!("{region_id}.pmtiles"))
}

fn partial_path(data_dir: &Path, region_id: &str) -> PathBuf {
    data_dir
        .join("maps")
        .join(format!("{region_id}.pmtiles.part"))
}

async fn cleanup_paths(data_dir: &Path, region_id: &str) {
    for path in [
        partial_path(data_dir, region_id),
        final_path(data_dir, region_id),
    ] {
        match tokio::fs::remove_file(path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(%error, region_id, "cannot clean region file"),
        }
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
        keeppix_db::RegionRepo::new(&db)
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
            "IT",
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
        assert!(!temp.path().join("maps/IT.pmtiles.part").exists());
        assert!(!temp.path().join("maps/IT.pmtiles").exists());
    }

    #[tokio::test]
    async fn worker_revalidates_the_stored_url_before_any_request() {
        let (db, _container) = test_db().await;
        let admin = seed_admin(&db).await;
        let ctx = keeppix_domain::AuthContext::user(admin, keeppix_domain::SystemRole::Admin);
        keeppix_db::RegionRepo::new(&db)
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
        let job = keeppix_domain::Job {
            id: 1,
            kind: keeppix_domain::JobKind::DownloadMapRegion,
            payload: serde_json::json!({ "region_id": "IT" }),
            priority: keeppix_domain::JobPriority::High,
            status: keeppix_domain::JobStatus::Running,
            attempts: 1,
            max_attempts: 3,
            last_error: None,
            run_after: chrono::Utc::now(),
            locked_by: None,
            dedup_key: Some("map-region:IT".to_owned()),
        };
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
