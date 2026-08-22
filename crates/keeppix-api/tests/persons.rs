//! Fase 8 Task 6/7/8: rotte HTTP di persone/gruppi e coda di revisione volti.
//! Le regole di visibilità/centroide sono già coperte a fondo dai test di
//! `keeppix-db`; qui si verifica soprattutto il filo che le collega a HTTP —
//! percorsi montati, forma del JSON, codici di stato.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    create_library, create_user, login_as, scan_and_wait, setup_admin, tiny_fixture_path,
};
use keeppix_db::{FaceRepo, PersonRepo};
use keeppix_domain::{AssetId, AuthContext, FaceBBox, PersonName, SystemRole};
use serde_json::{Value, json};

async fn admin_id(server: &TestServer) -> keeppix_domain::UserId {
    keeppix_db::UserRepo::new(&server.db)
        .find_by_username(&keeppix_domain::Username::parse("giovanni").unwrap())
        .await
        .unwrap()
        .unwrap()
        .0
        .id
}

async fn seed_scanned_asset(server: &TestServer, tag: &str) -> AssetId {
    let root = server
        .photos_root
        .join(format!("persons-{tag}-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(tiny_fixture_path(), root.join("a.jpg")).unwrap();
    let library_id = create_library(server, &format!("Lib-{tag}"), &root).await;
    scan_and_wait(
        server,
        &library_id,
        1,
        Instant::now() + Duration::from_secs(90),
    )
    .await;

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
    let asset_id: AssetId = page["assets"][0]["id"].as_str().unwrap().parse().unwrap();
    asset_id
}

/// Inserisce e conferma un volto direttamente via repository (nessuna rotta
/// HTTP di rilevamento in questa fase: quella la guida la pipeline, non un
/// client) — mette la persona nello stato che le rotte HTTP devono poi
/// gestire correttamente.
async fn seed_confirmed_face(
    server: &TestServer,
    asset_id: AssetId,
    person_id: keeppix_domain::PersonId,
) -> keeppix_domain::FaceId {
    let admin = admin_id(server).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let face = FaceRepo::new(&server.db)
        .insert_detected(keeppix_db::NewDetectedFace {
            asset_id,
            bbox: FaceBBox {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.2,
            },
            landmarks: None,
            embedding: None,
            detect_score: 0.9,
            quality: Some(0.5),
            model_version: "scrfd-500mf+arcface".to_owned(),
        })
        .await
        .unwrap();
    FaceRepo::new(&server.db)
        .assign(&ctx, face.id, person_id)
        .await
        .unwrap();
    face.id
}

#[tokio::test]
async fn create_get_and_list_a_person() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let asset = seed_scanned_asset(&server, "cgl").await;

    let created: Value = server
        .client
        .post(server.url("/api/v1/persons"))
        .json(&json!({"name": "Marta"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(created["name"], "Marta");
    let person_id = created["id"].as_str().unwrap().to_owned();

    let got: Value = server
        .client
        .get(server.url(&format!("/api/v1/persons/{person_id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(got["id"], person_id);
    assert!(
        got.get("face_count").is_none(),
        "single-person response omits face_count"
    );

    seed_confirmed_face(&server, asset, person_id.parse().unwrap()).await;

    let list: Value = server
        .client
        .get(server.url("/api/v1/persons"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let entry = list
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == person_id)
        .expect("in list");
    assert_eq!(entry["face_count"], 1);
}

#[tokio::test]
async fn blank_name_is_rejected_with_422() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let resp = server
        .client
        .post(server.url("/api/v1/persons"))
        .json(&json!({"name": "   "}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn patch_renames_hides_and_sets_cover() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let asset = seed_scanned_asset(&server, "patch").await;

    let person = PersonRepo::new(&server.db).create(None).await.unwrap();
    let face_id = seed_confirmed_face(&server, asset, person.id).await;

    let updated: Value = server
        .client
        .patch(server.url(&format!("/api/v1/persons/{}", person.id)))
        .json(&json!({"name": "Elena", "hidden": true, "cover_face_id": face_id.to_string()}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(updated["name"], "Elena");
    assert_eq!(updated["hidden"], true);
    assert_eq!(updated["cover_face_id"], face_id.to_string());
}

#[tokio::test]
async fn a_person_invisible_to_an_outsider_returns_403() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let asset = seed_scanned_asset(&server, "outsider").await;
    let person = PersonRepo::new(&server.db).create(None).await.unwrap();
    seed_confirmed_face(&server, asset, person.id).await;

    create_user(&server, "outsider", "correct horse battery staple").await;
    let outsider_client = login_as(&server, "outsider", "correct horse battery staple").await;

    let resp = outsider_client
        .get(server.url(&format!("/api/v1/persons/{}", person.id)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);

    let list: Value = outsider_client
        .get(server.url("/api/v1/persons"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(list.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn merge_reassigns_faces_into_the_survivor() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let asset_a = seed_scanned_asset(&server, "merge-a").await;
    let asset_b = seed_scanned_asset(&server, "merge-b").await;

    let survivor = PersonRepo::new(&server.db).create(None).await.unwrap();
    let absorbed = PersonRepo::new(&server.db)
        .create(Some(PersonName::parse("Giovanni").unwrap()))
        .await
        .unwrap();
    seed_confirmed_face(&server, asset_a, survivor.id).await;
    seed_confirmed_face(&server, asset_b, absorbed.id).await;

    let merged: Value = server
        .client
        .post(server.url(&format!("/api/v1/persons/{}/merge", survivor.id)))
        .json(&json!({"absorbed": [absorbed.id.to_string()]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(merged["name"], "Giovanni");

    let list: Value = server
        .client
        .get(server.url("/api/v1/persons"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let entry = list
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == survivor.id.to_string())
        .expect("survivor listed");
    assert_eq!(entry["face_count"], 2);
}

#[tokio::test]
async fn separate_creates_a_new_person_via_http() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let asset = seed_scanned_asset(&server, "separate").await;

    let source = PersonRepo::new(&server.db)
        .create(Some(PersonName::parse("Gemelli").unwrap()))
        .await
        .unwrap();
    let face_id = seed_confirmed_face(&server, asset, source.id).await;

    let (status, body): (reqwest::StatusCode, Value) = {
        let resp = server
            .client
            .post(server.url(&format!("/api/v1/persons/{}/separate", source.id)))
            .json(&json!({"face_ids": [face_id.to_string()], "name": "Elena"}))
            .send()
            .await
            .unwrap();
        (resp.status(), resp.json().await.unwrap())
    };
    assert_eq!(status, 201);
    assert_eq!(body["name"], "Elena");
    assert_ne!(body["id"], source.id.to_string());
}

#[tokio::test]
async fn review_queue_confirm_and_reject_via_http() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let admin = admin_id(&server).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset_a = seed_scanned_asset(&server, "queue-a").await;
    let asset_b = seed_scanned_asset(&server, "queue-b").await;

    let candidate = PersonRepo::new(&server.db).create(None).await.unwrap();
    let face_repo = FaceRepo::new(&server.db);
    let face_a = face_repo
        .insert_detected(keeppix_db::NewDetectedFace {
            asset_id: asset_a,
            bbox: FaceBBox {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.2,
            },
            landmarks: None,
            embedding: None,
            detect_score: 0.9,
            quality: Some(0.5),
            model_version: "scrfd-500mf+arcface".to_owned(),
        })
        .await
        .unwrap();
    face_repo
        .propose(face_a.id, candidate.id, 0.55)
        .await
        .unwrap();
    let face_b = face_repo
        .insert_detected(keeppix_db::NewDetectedFace {
            asset_id: asset_b,
            bbox: FaceBBox {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.2,
            },
            landmarks: None,
            embedding: None,
            detect_score: 0.9,
            quality: Some(0.5),
            model_version: "scrfd-500mf+arcface".to_owned(),
        })
        .await
        .unwrap();
    face_repo
        .propose(face_b.id, candidate.id, 0.55)
        .await
        .unwrap();
    let _ = &ctx;

    let proposals: Value = server
        .client
        .get(server.url("/api/v1/faces/proposals"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(proposals.as_array().unwrap().len(), 2);

    // Conferma singola su face_a via /faces/{id}/confirm.
    let resp = server
        .client
        .post(server.url(&format!("/api/v1/faces/{}/confirm", face_a.id)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    // "Rifiuta tutti" per il candidato rifiuta ciò che resta (face_b).
    let outcome: Value = server
        .client
        .post(server.url(&format!(
            "/api/v1/persons/{}/proposals/reject",
            candidate.id
        )))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(outcome["succeeded"].as_array().unwrap().len(), 1);

    let remaining: Value = server
        .client
        .get(server.url("/api/v1/faces/proposals"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(remaining.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn assets_list_faces_and_manual_reject() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let asset = seed_scanned_asset(&server, "list-faces").await;
    let person = PersonRepo::new(&server.db).create(None).await.unwrap();
    let face_id = seed_confirmed_face(&server, asset, person.id).await;

    let faces: Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset}/faces")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(faces.as_array().unwrap().len(), 1);
    assert_eq!(faces[0]["id"], face_id.to_string());

    let resp = server
        .client
        .post(server.url(&format!("/api/v1/faces/{face_id}/reject")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let faces_after: Value = server
        .client
        .get(server.url(&format!("/api/v1/assets/{asset}/faces")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        faces_after.as_array().unwrap().is_empty(),
        "rejected faces are excluded"
    );
}

#[tokio::test]
async fn bootstrap_badge_counts_pending_face_proposals() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let asset = seed_scanned_asset(&server, "badge").await;
    let candidate = PersonRepo::new(&server.db).create(None).await.unwrap();
    let face = FaceRepo::new(&server.db)
        .insert_detected(keeppix_db::NewDetectedFace {
            asset_id: asset,
            bbox: FaceBBox {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.2,
            },
            landmarks: None,
            embedding: None,
            detect_score: 0.9,
            quality: Some(0.5),
            model_version: "scrfd-500mf+arcface".to_owned(),
        })
        .await
        .unwrap();
    FaceRepo::new(&server.db)
        .propose(face.id, candidate.id, 0.55)
        .await
        .unwrap();

    let bootstrap: Value = server
        .client
        .get(server.url("/api/v1/bootstrap"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(bootstrap["badges"]["revision"], 1);
}

// ---- Gruppi di persone ----

#[tokio::test]
async fn person_group_crud_and_membership() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let asset = seed_scanned_asset(&server, "group").await;
    let person = PersonRepo::new(&server.db).create(None).await.unwrap();
    seed_confirmed_face(&server, asset, person.id).await;

    let group: Value = server
        .client
        .post(server.url("/api/v1/person-groups"))
        .json(&json!({"name": "Famiglia"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let group_id = group["id"].as_str().unwrap().to_owned();

    let resp = server
        .client
        .post(server.url(&format!(
            "/api/v1/person-groups/{group_id}/members/{}",
            person.id
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let members: Value = server
        .client
        .get(server.url(&format!("/api/v1/person-groups/{group_id}/members")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(members, json!([person.id.to_string()]));

    let renamed: Value = server
        .client
        .patch(server.url(&format!("/api/v1/person-groups/{group_id}")))
        .json(&json!({"name": "Famiglia stretta"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(renamed["name"], "Famiglia stretta");

    let del = server
        .client
        .delete(server.url(&format!("/api/v1/person-groups/{group_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 204);
}
