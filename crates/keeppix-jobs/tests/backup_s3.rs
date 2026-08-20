//! S3 destination must send AWS `SigV4` — not invented Keeppix headers.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use keeppix_db::{BackupDestination, BackupKind};
use keeppix_jobs::backup::{test_destination, upload_s3_for_test};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use uuid::Uuid;

#[tokio::test]
async fn s3_upload_sends_aws4_authorization_header() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let (addr, shutdown) = spawn_mock(Arc::clone(&seen), Mode::Upload).await;

    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("keeppix.kpxb");
    std::fs::write(&file, b"fake-backup-bytes").unwrap();

    let dest = sample_dest(&addr);
    upload_s3_for_test(&dest, &file, "probe.kpxb")
        .await
        .expect("signed upload must succeed against the mock");

    let _ = shutdown.send(());
    let auths = seen.lock().unwrap().clone();
    assert!(!auths.is_empty(), "mock must see at least one request");
    assert!(
        auths
            .iter()
            .any(|a| a.starts_with("AWS4-HMAC-SHA256 Credential=")),
        "expected SigV4 Authorization, got {auths:?}"
    );
    assert!(
        auths.iter().all(|a| !a.is_empty()),
        "Authorization must not be empty"
    );
}

#[tokio::test]
async fn s3_test_destination_performs_a_signed_probe_write() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let (addr, shutdown) = spawn_mock(Arc::clone(&seen), Mode::Probe).await;

    let dest = sample_dest(&addr);
    test_destination(&dest)
        .await
        .expect("signed probe must pass");
    let _ = shutdown.send(());

    let methods = seen.lock().unwrap().clone();
    assert!(
        methods.iter().any(|m| m.starts_with("PUT ")),
        "test_destination must PUT a probe object, got {methods:?}"
    );
    assert!(
        methods.iter().any(|m| m.starts_with("DELETE ")),
        "test_destination must DELETE the probe object, got {methods:?}"
    );
    assert!(
        methods
            .iter()
            .all(|m| m.contains("AWS4-HMAC-SHA256 Credential=")),
        "every probe request must be SigV4-signed, got {methods:?}"
    );
}

fn sample_dest(addr: &str) -> BackupDestination {
    BackupDestination {
        id: Uuid::now_v7(),
        kind: BackupKind::S3,
        label: "mock".into(),
        config: json!({
            "endpoint": format!("http://{addr}"),
            "bucket": "backups",
            "prefix": "keeppix",
            "access_key": "AKIAIOSFODNN7EXAMPLE",
            "secret_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "region": "us-east-1",
        }),
        enabled: true,
        created_at: chrono::Utc::now(),
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Upload,
    Probe,
}

async fn spawn_mock(seen: Arc<Mutex<Vec<String>>>, mode: Mode) -> (String, oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (tx, mut rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut rx => break,
                accepted = listener.accept() => {
                    let Ok((mut socket, _)) = accepted else { break };
                    let mut buf = vec![0u8; 64 * 1024];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let first = req.lines().next().unwrap_or("").to_owned();
                    let auth = req
                        .lines()
                        .find_map(|l| l.strip_prefix("authorization: ").or_else(|| l.strip_prefix("Authorization: ")))
                        .unwrap_or("")
                        .to_owned();
                    let fake = req.to_ascii_lowercase().contains("x-keeppix-s3-access-key");
                    match mode {
                        Mode::Upload => {
                            seen.lock().unwrap().push(auth.clone());
                            let body: &[u8] = if fake {
                                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 4\r\nConnection: close\r\n\r\nfake"
                            } else if auth.starts_with("AWS4-HMAC-SHA256 ") {
                                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            } else {
                                b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            };
                            let _ = socket.write_all(body).await;
                        }
                        Mode::Probe => {
                            seen.lock().unwrap().push(format!("{first} | {auth}"));
                            let ok = auth.starts_with("AWS4-HMAC-SHA256 ")
                                && (first.starts_with("PUT ") || first.starts_with("DELETE "));
                            let body = if ok {
                                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                                    as &[u8]
                            } else {
                                b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            };
                            let _ = socket.write_all(body).await;
                        }
                    }
                }
            }
        }
    });
    (addr, tx)
}
