//! SFTP destinations must authenticate and write — TCP connect alone is not enough.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use keeppix_db::{BackupDestination, BackupKind};
use keeppix_jobs::backup::test_destination;
use serde_json::json;
use testcontainers_modules::testcontainers::core::{ContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{GenericImage, ImageExt};
use uuid::Uuid;

#[tokio::test]
async fn sftp_test_destination_rejects_missing_credentials() {
    let dest = BackupDestination {
        id: Uuid::now_v7(),
        kind: BackupKind::Sftp,
        label: "no-creds".into(),
        config: json!({
            "host": "127.0.0.1",
            "port": 22,
            "username": "keeppix",
            "path": "."
        }),
        enabled: true,
        created_at: chrono::Utc::now(),
    };
    let err = test_destination(&dest)
        .await
        .expect_err("TCP-only config must not look successful");
    assert!(
        err.to_string().contains("password") || err.to_string().contains("private_key"),
        "expected credential error, got {err}"
    );
}

#[tokio::test]
async fn sftp_test_destination_writes_and_deletes_probe_with_password() {
    let container = GenericImage::new("atmoz/sftp", "alpine-3.7")
        .with_exposed_port(ContainerPort::Tcp(22))
        .with_wait_for(WaitFor::message_on_stderr("Server listening on"))
        .with_cmd(["keeppix:secret:1001:1001:upload"])
        .start()
        .await
        .expect("start atmoz/sftp");
    let port = container
        .get_host_port_ipv4(ContainerPort::Tcp(22))
        .await
        .expect("mapped ssh port");

    let dest = BackupDestination {
        id: Uuid::now_v7(),
        kind: BackupKind::Sftp,
        label: "real".into(),
        config: json!({
            "host": "127.0.0.1",
            "port": port,
            "username": "keeppix",
            "password": "secret",
            "path": "upload"
        }),
        enabled: true,
        created_at: chrono::Utc::now(),
    };

    test_destination(&dest)
        .await
        .expect("authenticated SFTP probe must succeed against a real server");
}
