//! Fase 8 Task 1 — la regola che viene prima di tutto: i volti sono dati
//! biometrici e non compaiono **mai** su un link pubblico condiviso, in
//! nessun campo, su nessuna forma di oggetto condivisibile (foto, cartella,
//! album). Non è configurabile.
//!
//! Il test non ispeziona i campi noti di `ShareInfoResponse`/
//! `SharedContentResponse` uno per uno: cammina l'intero corpo JSON e
//! rifiuta qualunque chiave che somigli a "volto" o "persona", così un
//! futuro campo aggiunto per distrazione lo fa fallire comunque, non solo i
//! campi che qualcuno ha pensato di controllare oggi.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    create_library, create_share_link_from, scan_and_wait, setup_admin, share_client,
    tiny_fixture_path,
};
use keeppix_db::{FaceRepo, NewDetectedFace, PersonRepo};
use keeppix_domain::{AssetId, AuthContext, FaceBBox, PersonName, SystemRole, UserId};
use serde_json::Value;

/// Cammina ricorsivamente un `serde_json::Value` e raccoglie ogni chiave di
/// oggetto il cui nome, in minuscolo, contiene "face" o "person" — in
/// italiano od inglese ("volto"/"persona") non compaiono mai nei nomi di
/// campo del codice (le convenzioni del progetto sono in inglese), quindi
/// controllare le radici inglesi copre lo stesso rischio reale.
fn find_face_or_person_keys(value: &Value, path: &str, hits: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let lower = key.to_lowercase();
                if lower.contains("face") || lower.contains("person") {
                    hits.push(format!("{path}/{key}"));
                }
                find_face_or_person_keys(child, &format!("{path}/{key}"), hits);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                find_face_or_person_keys(item, &format!("{path}[{i}]"), hits);
            }
        }
        _ => {}
    }
}

fn assert_no_face_leak(body: &Value) {
    let mut hits = Vec::new();
    find_face_or_person_keys(body, "$", &mut hits);
    assert!(
        hits.is_empty(),
        "face/person data leaked through a public share response: {hits:?}\nbody: {body}"
    );
}

async fn admin_id(server: &TestServer) -> UserId {
    keeppix_db::UserRepo::new(&server.db)
        .find_by_username(&keeppix_domain::Username::parse("giovanni").unwrap())
        .await
        .unwrap()
        .unwrap()
        .0
        .id
}

/// Un asset reale (scansione vera, non un `INSERT` a mano) con un volto
/// confermato su una persona con nome — lo scenario più favorevole possibile
/// a una fuga di dati, per rendere il test onesto.
async fn seed_asset_with_confirmed_face(server: &TestServer) -> (AssetId, String) {
    let root = server
        .photos_root
        .join(format!("face-privacy-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(tiny_fixture_path(), root.join("with-face.jpg")).unwrap();

    let library_id = create_library(server, "FacePrivacy", &root).await;
    let deadline = Instant::now() + Duration::from_secs(90);
    scan_and_wait(server, &library_id, 1, deadline).await;

    let buckets: Value = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let month = buckets[0]["month"].as_str().unwrap();
    let page: Value = server
        .client
        .get(server.url(&format!("/api/v1/timeline?bucket={month}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let asset_row = page["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["filename"].as_str() == Some("with-face.jpg"))
        .expect("seeded asset");
    let asset_id: AssetId = asset_row["id"].as_str().unwrap().parse().unwrap();
    let folder_id = asset_row["folder_id"].as_str().unwrap().to_owned();

    let admin = admin_id(server).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let person = PersonRepo::new(&server.db)
        .create(Some(PersonName::parse("Giovanni Visibile").unwrap()))
        .await
        .unwrap();
    let face = FaceRepo::new(&server.db)
        .insert_detected(NewDetectedFace {
            asset_id,
            bbox: FaceBBox {
                x: 0.2,
                y: 0.2,
                w: 0.3,
                h: 0.3,
            },
            landmarks: Some(serde_json::json!({"left_eye": [10.0, 12.0]})),
            embedding: None,
            detect_score: 0.99,
            quality: Some(0.9),
            model_version: "scrfd-500mf+arcface".to_owned(),
        })
        .await
        .unwrap();
    FaceRepo::new(&server.db)
        .assign(&ctx, face.id, person.id)
        .await
        .unwrap();

    (asset_id, folder_id)
}

#[tokio::test]
async fn a_shared_folder_never_exposes_face_or_person_data() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let (_, folder_id) = seed_asset_with_confirmed_face(&server).await;

    let token = create_share_link_from(
        &server,
        serde_json::json!({
            "object_type": "folder",
            "object_id": folder_id,
            "hide_metadata": false,
        }),
    )
    .await;
    let guest = share_client(&token);

    let info: Value = server
        .client
        .get(server.url(&format!("/api/v1/share/{token}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_no_face_leak(&info);

    let assets: Value = guest
        .get(server.url(&format!("/api/v1/share/{token}/assets")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        assets["assets"].as_array().unwrap().len(),
        1,
        "sanity: the seeded asset must be present"
    );
    assert_no_face_leak(&assets);
}

#[tokio::test]
async fn a_shared_single_asset_never_exposes_face_or_person_data() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let (asset_id, _) = seed_asset_with_confirmed_face(&server).await;

    let token = create_share_link_from(
        &server,
        serde_json::json!({
            "object_type": "asset",
            "object_id": asset_id.to_string(),
            "hide_metadata": false,
        }),
    )
    .await;
    let guest = share_client(&token);

    let assets: Value = guest
        .get(server.url(&format!("/api/v1/share/{token}/assets")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        assets["assets"].as_array().unwrap().len(),
        1,
        "sanity: the seeded asset must be present"
    );
    assert_no_face_leak(&assets);
}

#[test]
fn the_scanner_itself_catches_a_planted_leak() {
    // Prova che `find_face_or_person_keys` funziona davvero: se il corpo
    // contenesse un campo come questo, il test lo troverebbe.
    let planted = serde_json::json!({
        "assets": [{"id": "x", "faces": [{"person_name": "Marta"}]}]
    });
    let mut hits = Vec::new();
    find_face_or_person_keys(&planted, "$", &mut hits);
    assert!(!hits.is_empty());
}
