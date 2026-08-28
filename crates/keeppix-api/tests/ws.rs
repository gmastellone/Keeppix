mod harness;

use std::time::Duration;

use futures_util::StreamExt as _;
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, NewAsset, NewLibrary, SystemRole, Username,
};
use serde_json::json;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderValue, header};

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_websocket_ticket_cannot_be_reused() {
    let server = TestServer::start().await;
    setup(&server).await;

    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    assert_eq!(issued.status(), 200);
    let body: serde_json::Value = issued.json().await.unwrap();
    let ticket = body["ticket"].as_str().unwrap();
    assert_eq!(body["expires_in"], 30);

    let origin = server.base_url.clone();
    let first = handshake(&server, ticket, &origin).await;
    assert!(
        first == 101 || first == 400 || first == 426,
        "the first handshake must consume the ticket, got {first}"
    );

    let second = handshake(&server, ticket, &origin).await;
    assert_eq!(second, 403, "ticket already used");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_wrong_origin_is_forbidden() {
    let server = TestServer::start().await;
    setup(&server).await;
    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    let ticket = issued.json::<serde_json::Value>().await.unwrap()["ticket"]
        .as_str()
        .unwrap()
        .to_owned();

    let status = handshake(&server, &ticket, "https://evil.example").await;
    assert_eq!(status, 403);
}

/// The socket is a notification channel: a new asset must come out as
/// `assets.upserted` without the client reloading the page.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_new_asset_is_pushed_as_assets_upserted() {
    let server = TestServer::start().await;
    setup(&server).await;
    let (folder, _) = seed_library_after_setup(&server).await;

    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    let ticket = issued.json::<serde_json::Value>().await.unwrap()["ticket"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut ws = open_socket(&server, &ticket).await;
    wait_until_looping(&mut ws).await;

    let asset = AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("live.jpg").unwrap(),
            size_bytes: 10,
            mtime: chrono::Utc::now(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();

    // Not necessarily the very first message: with several event types
    // on the same channel, a library just created can also produce
    // `scan.progress` on the same poll round. A real client filters by
    // `type`, it doesn't assume an order.
    let msg = recv_matching(&mut ws, "assets.upserted").await;
    assert_eq!(msg["v"], 1);
    let ids = msg["payload"]["ids"].as_array().expect("ids");
    assert!(
        ids.iter()
            .any(|id| id.as_str() == Some(&asset.id.to_string())),
        "expected {} in {ids:?}",
        asset.id
    );
}

/// "Progress arrives even if the client reconnects mid-operation". Here
/// the client never connected before — it's the same property:
/// `operations` is the source of truth read by the poll, not state that
/// lives in the connection, so a connection opened mid-scan still sees
/// the current progress on the first useful round.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn operation_progress_arrives_over_a_connection_opened_mid_scan() {
    // Discover writes in multi-row batches of `PRODUCTION_BATCH_SIZE`
    // files — with fewer files than a full batch, the entire scan writes
    // in a single statement, with no "midway" window.
    const TOTAL: usize = 5 * keeppix_jobs::PRODUCTION_BATCH_SIZE;
    let server = TestServer::start().await;
    setup(&server).await;

    let root = server.photos_root.join("ws-op-progress");
    std::fs::create_dir_all(&root).unwrap();
    for n in 0..TOTAL {
        std::fs::write(root.join(format!("{n:03}.jpg")), b"x").unwrap();
    }

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "WsOpProgress",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);

    let library_id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let scan = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{library_id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(scan.status(), 202);
    let operation_id = scan.json::<serde_json::Value>().await.unwrap()["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let _worker = harness::spawn_worker_pool(
        server.db.clone(),
        server.database_url.clone(),
        server.data_dir.join("ws-op-progress-data"),
    );

    let admin = UserRepo::new(&server.db)
        .find_by_username(&Username::parse("giovanni").unwrap())
        .await
        .unwrap()
        .expect("admin")
        .0
        .id;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let ops = keeppix_db::OperationsRepo::new(&server.db);
    let op_id: keeppix_domain::OperationId = operation_id.parse().unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the scan did not make enough progress before connecting"
        );
        if ops.find(&ctx, op_id).await.unwrap().done >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }

    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    let ticket = issued.json::<serde_json::Value>().await.unwrap()["ticket"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut ws = open_socket(&server, &ticket).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            remaining > Duration::ZERO,
            "no operation.progress arrived after connecting mid-scan"
        );
        let msg = tokio::time::timeout(remaining, recv_json(&mut ws))
            .await
            .expect("timeout");
        if msg["type"] == "operation.progress" && msg["payload"]["operation_id"] == operation_id {
            assert!(msg["payload"]["done"].as_i64().unwrap() > 0);
            break;
        }
    }
}

/// A library going offline is a real problem (`ProblemsRepo::list`,
/// already used by `GET /problems`) — the WebSocket must only notify that
/// there's something new to reread, not carry the details (contract:
/// "notification channel, not source of truth").
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_offline_library_is_pushed_as_problems_changed() {
    let server = TestServer::start().await;
    setup(&server).await;
    let ctx = admin_ctx(&server).await;

    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Braies".to_owned(),
                owner_id: ctx.user_id().unwrap(),
                root_path: server.photos_root.join("ws-problems"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();

    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    let ticket = issued.json::<serde_json::Value>().await.unwrap()["ticket"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut ws = open_socket(&server, &ticket).await;
    wait_until_looping(&mut ws).await;

    LibraryRepo::new(&server.db)
        .set_status(&ctx, library.id, keeppix_domain::LibraryStatus::Offline)
        .await
        .unwrap();

    let msg = recv_matching(&mut ws, "problems.changed").await;
    assert_eq!(msg["v"], 1);
    assert!(msg["payload"]["count"].as_i64().unwrap() >= 1);
}

/// `suggestions.changed` was originally left unwired — there was no code
/// yet to read it from. Now that face proposals exist, the emitter is
/// wired up: the same tag+face sum as the `bootstrap.badges.revision`
/// badge.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_proposed_face_is_pushed_as_suggestions_changed() {
    let server = TestServer::start_with_vector().await;
    setup(&server).await;
    let (folder, _library) = seed_library_after_setup(&server).await;

    let asset = keeppix_db::AssetRepo::new(&server.db)
        .upsert_discovered(keeppix_domain::NewAsset {
            folder_id: folder,
            filename: AssetName::parse("suggestions.jpg").unwrap(),
            size_bytes: 10,
            mtime: chrono::Utc::now(),
            inode: Some(1),
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();

    let person = keeppix_db::PersonRepo::new(&server.db)
        .create(None)
        .await
        .unwrap();
    let face = keeppix_db::FaceRepo::new(&server.db)
        .insert_detected(keeppix_db::NewDetectedFace {
            asset_id: asset.id,
            bbox: keeppix_domain::FaceBBox {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.2,
            },
            landmarks: None,
            embedding: None,
            detect_score: 0.9,
            quality: Some(0.5),
            model_version: "yunet+sface".to_owned(),
        })
        .await
        .unwrap();

    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    let ticket = issued.json::<serde_json::Value>().await.unwrap()["ticket"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut ws = open_socket(&server, &ticket).await;
    wait_until_looping(&mut ws).await;

    keeppix_db::FaceRepo::new(&server.db)
        .propose(face.id, person.id, 0.6)
        .await
        .unwrap();

    let msg = recv_matching(&mut ws, "suggestions.changed").await;
    assert_eq!(msg["v"], 1);
    assert!(msg["payload"]["count"].as_i64().unwrap() >= 1);
}

/// A backup that finishes (`BackupRepo::complete_run`, already used by
/// the real `keeppix-jobs::backup::run_one` job) must come out as
/// `backup.finished` — without a push, the Settings page would only know
/// the outcome by reloading.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_finished_backup_run_is_pushed_as_backup_finished() {
    let server = TestServer::start().await;
    setup(&server).await;
    let _ctx = admin_ctx(&server).await;

    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    let ticket = issued.json::<serde_json::Value>().await.unwrap()["ticket"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut ws = open_socket(&server, &ticket).await;
    wait_until_looping(&mut ws).await;

    let backups = keeppix_db::BackupRepo::new(&server.db);
    let run = backups.start_run(None, "0.1.0").await.unwrap();
    backups
        .complete_run(run.id, 4096, "/tmp/keeppix-ws-test.kpxb")
        .await
        .unwrap();

    let msg = recv_matching(&mut ws, "backup.finished").await;
    assert_eq!(msg["payload"]["run_id"], run.id.to_string());
    assert_eq!(msg["payload"]["status"], "ok");
    assert_eq!(msg["payload"]["size_bytes"], 4096);
}

/// Video transcoding writes its outcome to the `jobs` queue like any
/// other job — the WebSocket's poll reads it from there
/// (`JobRepo::list_recently_done`) and translates it into
/// `asset.derivative.ready` only for assets visible to the caller.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_finished_video_transcode_is_pushed_as_asset_derivative_ready() {
    let server = TestServer::start().await;
    setup(&server).await;
    let (folder, _library) = seed_library_after_setup(&server).await;
    let asset = AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("clip.mp4").unwrap(),
            size_bytes: 10,
            mtime: chrono::Utc::now(),
            inode: None,
            kind: AssetKind::Video,
        })
        .await
        .unwrap()
        .unwrap();

    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    let ticket = issued.json::<serde_json::Value>().await.unwrap()["ticket"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut ws = open_socket(&server, &ticket).await;
    wait_until_looping(&mut ws).await;

    let jobs = keeppix_db::JobRepo::new(&server.db);
    let job = jobs
        .enqueue(
            keeppix_domain::JobKind::TranscodeVideo,
            json!({"asset_id": asset.id.to_string(), "save_bandwidth": false}),
            keeppix_domain::JobPriority::Interactive,
            Some("transcode:ws-test"),
        )
        .await
        .unwrap();
    jobs.claim(
        uuid::Uuid::now_v7(),
        keeppix_domain::JobPriority::Interactive,
    )
    .await
    .unwrap();
    jobs.complete(job.id).await.unwrap();

    let msg = recv_matching(&mut ws, "asset.derivative.ready").await;
    assert_eq!(msg["payload"]["asset_id"], asset.id.to_string());
}

/// `scan.progress` reads the same source already used by `GET
/// /libraries/{id}/scan` (`JobRepo::discover_status_for_library` +
/// `AssetRepo::count_in_library`), not a second, invented state — the two
/// surfaces can't tell different stories for the same library.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_library_scan_pushes_scan_progress() {
    const TOTAL: usize = 10;
    let server = TestServer::start().await;
    setup(&server).await;

    let root = server.photos_root.join("ws-scan-progress");
    std::fs::create_dir_all(&root).unwrap();
    for n in 0..TOTAL {
        std::fs::write(root.join(format!("{n:03}.jpg")), b"x").unwrap();
    }

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "WsScanProgress",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let library_id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    let ticket = issued.json::<serde_json::Value>().await.unwrap()["ticket"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut ws = open_socket(&server, &ticket).await;
    wait_until_looping(&mut ws).await;

    let scan = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{library_id}/scan")))
        .send()
        .await
        .unwrap();
    assert_eq!(scan.status(), 202);

    let _worker = harness::spawn_worker_pool(
        server.db.clone(),
        server.database_url.clone(),
        server.data_dir.join("ws-scan-progress-data"),
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(remaining > Duration::ZERO, "no scan.progress arrived");
        let msg = tokio::time::timeout(remaining, recv_json(&mut ws))
            .await
            .expect("timeout");
        if msg["type"] == "scan.progress" && msg["payload"]["library_id"] == library_id {
            assert!(msg["payload"]["asset_count"].as_i64().unwrap() >= 0);
            break;
        }
    }
}

/// `RegionView` already carries `downloaded_bytes`, `status`, and
/// `last_error`, but a map download's progress was never pushed — the
/// only way to know was polling `GET /regions` at intervals. Same source
/// of truth (`RegionRepo`), not a second, invented state.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_region_download_progress_is_pushed_as_region_progress() {
    let server = TestServer::start().await;
    setup(&server).await;
    let ctx = admin_ctx(&server).await;

    let regions = keeppix_db::RegionRepo::new(&server.db);
    let region = regions
        .begin_download(
            &ctx,
            keeppix_db::NewMapRegion {
                id: "alto-adige".to_owned(),
                label: "Alto Adige".to_owned(),
                size_bytes: 1000,
                version: "1".to_owned(),
                source_url: "https://example.com/alto-adige.pmtiles".to_owned(),
                checksum_sha256: "ab".repeat(32),
            },
        )
        .await
        .unwrap();

    let issued = server
        .client
        .post(server.url("/api/v1/ws/ticket"))
        .send()
        .await
        .unwrap();
    let ticket = issued.json::<serde_json::Value>().await.unwrap()["ticket"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut ws = open_socket(&server, &ticket).await;
    wait_until_looping(&mut ws).await;

    regions
        .record_progress(&region.id, region.download_generation, 400)
        .await
        .unwrap();

    let msg = recv_matching(&mut ws, "region.progress").await;
    assert_eq!(msg["payload"]["region_id"], "alto-adige");
    assert_eq!(msg["payload"]["status"], "downloading");
    assert_eq!(msg["payload"]["downloaded_bytes"], 400);
    assert_eq!(msg["payload"]["size_bytes"], 1000);
    assert!(msg["payload"]["last_error"].is_null());
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn admin_ctx(server: &TestServer) -> AuthContext {
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    AuthContext::user(user.id, SystemRole::Admin)
}

/// Reads until a message of the sought `type` arrives, ignoring
/// ping/pong and other events (`assets.upserted` for the library's own
/// insertion, `resync`, …) that can interleave on the same poll.
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn recv_matching(ws: &mut LiveSocket, kind: &str) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(remaining > Duration::ZERO, "timeout waiting for {kind}");
        let msg = tokio::time::timeout(remaining, recv_json(ws))
            .await
            .unwrap_or_else(|_| panic!("timeout waiting for {kind}"));
        if msg["type"] == kind {
            return msg;
        }
    }
}

#[allow(clippy::unwrap_used)]
async fn handshake(server: &TestServer, ticket: &str, origin: &str) -> u16 {
    server
        .client
        .get(server.url("/api/v1/ws"))
        .header("Origin", origin)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header(
            "Sec-WebSocket-Protocol",
            format!("keeppix.v1, ticket.{ticket}"),
        )
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[allow(clippy::unwrap_used)]
async fn setup(server: &TestServer) {
    server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_library_after_setup(
    server: &TestServer,
) -> (keeppix_domain::FolderId, keeppix_domain::LibraryId) {
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    let ctx = AuthContext::user(user.id, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: user.id,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    (folder.id, library.id)
}

type LiveSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn open_socket(server: &TestServer, ticket: &str) -> LiveSocket {
    let ws_url = server.url("/api/v1/ws").replacen("http", "ws", 1);
    let mut request = ws_url.into_client_request().unwrap();
    request.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_str(&server.base_url).unwrap(),
    );
    request.headers_mut().insert(
        header::SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_str(&format!("keeppix.v1, ticket.{ticket}")).unwrap(),
    );
    let (ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("websocket connect");
    ws
}

/// The first ping proves `socket_loop` has passed `head_seq`: inserting
/// before that point would advance the cursor *past* the new asset.
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn wait_until_looping(ws: &mut LiveSocket) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let frame = tokio::time::timeout(remaining, ws.next())
            .await
            .expect("timed out waiting for the socket loop")
            .expect("socket closed")
            .expect("websocket frame");
        match frame {
            tokio_tungstenite::tungstenite::Message::Ping(_)
            | tokio_tungstenite::tungstenite::Message::Pong(_) => return,
            _ => {}
        }
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn recv_json(ws: &mut LiveSocket) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let frame = tokio::time::timeout(remaining, ws.next())
            .await
            .expect("timed out waiting for assets.upserted")
            .expect("socket closed")
            .expect("websocket frame");
        let tokio_tungstenite::tungstenite::Message::Text(text) = frame else {
            continue;
        };
        return serde_json::from_str(text.as_str()).expect("json envelope");
    }
}
