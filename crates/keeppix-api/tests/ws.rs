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
        "primo handshake deve consumare il ticket, got {first}"
    );

    let second = handshake(&server, ticket, &origin).await;
    assert_eq!(second, 403, "ticket già usato");
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

/// Il socket è un canale di notifica: un asset nuovo deve uscire come
/// `assets.upserted` senza che il client ricarichi la pagina.
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

    let msg = recv_json(&mut ws).await;
    assert_eq!(msg["v"], 1);
    assert_eq!(msg["type"], "assets.upserted");
    let ids = msg["payload"]["ids"].as_array().expect("ids");
    assert!(
        ids.iter()
            .any(|id| id.as_str() == Some(&asset.id.to_string())),
        "expected {} in {ids:?}",
        asset.id
    );
}

/// Verifica di Task 16: "l'avanzamento arriva anche se il client si
/// riconnette a metà operazione". Qui il client non si è mai connesso prima
/// — è la stessa proprietà: `operations` è la fonte di verità letta dal
/// poll, non uno stato che vive nella connessione, quindi una connessione
/// aperta a metà scansione vede comunque l'avanzamento corrente al primo
/// giro utile.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn operation_progress_arrives_over_a_connection_opened_mid_scan() {
    const TOTAL: usize = 40;
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
            "la scansione non ha fatto abbastanza progresso prima di connettersi"
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
            "nessun operation.progress arrivato dopo la connessione a metà scansione"
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

/// Il primo ping prova che `socket_loop` ha passato `head_seq`: inserire
/// prima di quel punto farebbe avanzare il cursore *oltre* l'asset nuovo.
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
