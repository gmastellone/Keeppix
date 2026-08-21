//! Fase 7 Task 8: abbinamento tag↔foto via similarità CLIP → `asset_tags`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AssetTagRepo, EmbeddingRepo, FolderRepo, LibraryRepo, NewTag, TAG_MATCH_BAND, TagPatch, TagRepo,
};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, SystemRole, TagId,
    TagKind, UserId,
};

const MODEL: &str = "mobileclip2-s2";

/// Vettore unitario lungo l'asse `axis` (0..511).
fn unit_axis(axis: usize) -> Vec<f32> {
    let mut v = vec![0.0_f32; 512];
    v[axis] = 1.0;
    v
}

/// Due vettori unitari con cosine similarity ≈ `sim` (asse 0 e piano 0/1).
fn unit_with_similarity(sim: f32) -> Vec<f32> {
    let mut v = vec![0.0_f32; 512];
    let ortho = (1.0 - sim * sim).sqrt();
    v[0] = sim;
    v[1] = ortho;
    v
}

async fn seed_library(test: &TestDb, owner: UserId, path: &str) -> keeppix_domain::LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from(path),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria")
        .id
}

fn discovered(folder: FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
        size_bytes: 100,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

async fn seed_asset(test: &TestDb, folder: FolderId, filename: &str) -> AssetId {
    keeppix_db::AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, filename))
        .await
        .unwrap()
        .unwrap()
        .id
}

async fn create_tag_with_embedding(
    test: &TestDb,
    ctx: &AuthContext,
    name: &str,
    threshold: f32,
    embedding: Vec<f32>,
    model_version: &str,
) -> TagId {
    TagRepo::new(test.db())
        .create(
            ctx,
            NewTag {
                name: name.to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: Some(name.to_owned()),
                color: None,
                threshold: Some(threshold),
                embedding: Some(embedding),
                model_version: Some(model_version.to_owned()),
            },
        )
        .await
        .unwrap()
        .id
}

async fn fetch_assignment(
    test: &TestDb,
    asset_id: AssetId,
    tag_id: TagId,
) -> Option<(String, String, Option<f32>)> {
    sqlx::query_as::<_, (String, String, Option<f32>)>(
        "SELECT state, source, score FROM asset_tags \
         WHERE asset_id = $1 AND tag_id = $2",
    )
    .bind(asset_id.as_uuid())
    .bind(tag_id.as_uuid())
    .fetch_optional(test.db().pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn propose_for_tag_inserts_proposed_ai_above_threshold() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "/mnt/at-above").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_asset(&test, folder.id, "match.jpg").await;

    let tag_emb = unit_axis(0);
    let tag_id =
        create_tag_with_embedding(&test, &ctx, "Tramonti", 0.75, tag_emb.clone(), MODEL).await;
    EmbeddingRepo::new(test.db())
        .upsert(asset, &tag_emb, MODEL)
        .await
        .unwrap();

    let n = AssetTagRepo::new(test.db())
        .propose_for_tag(tag_id)
        .await
        .unwrap();
    assert_eq!(n, 1);

    let row = fetch_assignment(&test, asset, tag_id).await.expect("row");
    assert_eq!(row.0, "proposed");
    assert_eq!(row.1, "ai");
    assert!((row.2.unwrap() - 1.0).abs() < 1e-4);
}

#[tokio::test]
async fn propose_for_tag_includes_scores_inside_the_match_band() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "/mnt/at-band").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_asset(&test, folder.id, "weak.jpg").await;

    let threshold = 0.75_f32;
    // Mid-band: threshold − band/2 → still proposed, weaker score.
    let target_sim = threshold - TAG_MATCH_BAND / 2.0;
    let tag_id =
        create_tag_with_embedding(&test, &ctx, "Paesaggi", threshold, unit_axis(0), MODEL).await;
    EmbeddingRepo::new(test.db())
        .upsert(asset, &unit_with_similarity(target_sim), MODEL)
        .await
        .unwrap();

    let n = AssetTagRepo::new(test.db())
        .propose_for_tag(tag_id)
        .await
        .unwrap();
    assert_eq!(n, 1);

    let row = fetch_assignment(&test, asset, tag_id)
        .await
        .expect("in band");
    assert_eq!(row.0, "proposed");
    assert_eq!(row.1, "ai");
    let score = row.2.unwrap();
    assert!(
        (score - target_sim).abs() < 1e-3,
        "score={score} expected≈{target_sim}"
    );
    assert!(score < threshold);
    assert!(score >= threshold - TAG_MATCH_BAND);
}

#[tokio::test]
async fn propose_for_tag_skips_assets_below_threshold_minus_band() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "/mnt/at-below").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_asset(&test, folder.id, "nope.jpg").await;

    let threshold = 0.75_f32;
    let too_low = threshold - TAG_MATCH_BAND - 0.02; // 0.72
    let tag_id =
        create_tag_with_embedding(&test, &ctx, "Gruppo", threshold, unit_axis(0), MODEL).await;
    EmbeddingRepo::new(test.db())
        .upsert(asset, &unit_with_similarity(too_low), MODEL)
        .await
        .unwrap();

    let n = AssetTagRepo::new(test.db())
        .propose_for_tag(tag_id)
        .await
        .unwrap();
    assert_eq!(n, 0);
    assert!(fetch_assignment(&test, asset, tag_id).await.is_none());
}

#[tokio::test]
async fn rematch_does_not_overwrite_confirmed_or_rejected() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "/mnt/at-decided").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let confirmed_asset = seed_asset(&test, folder.id, "ok.jpg").await;
    let rejected_asset = seed_asset(&test, folder.id, "no.jpg").await;
    let proposed_asset = seed_asset(&test, folder.id, "maybe.jpg").await;

    let emb = unit_axis(0);
    let tag_id = create_tag_with_embedding(&test, &ctx, "Canidi", 0.5, emb.clone(), MODEL).await;
    for asset in [confirmed_asset, rejected_asset, proposed_asset] {
        EmbeddingRepo::new(test.db())
            .upsert(asset, &emb, MODEL)
            .await
            .unwrap();
    }

    sqlx::query(
        "INSERT INTO asset_tags (asset_id, tag_id, state, source, score, decided_by, decided_at) \
         VALUES ($1, $2, 'confirmed', 'user', 0.55, $3, now()), \
                ($4, $2, 'rejected', 'ai', 0.60, $3, now()), \
                ($5, $2, 'proposed', 'ai', 0.50, NULL, NULL)",
    )
    .bind(confirmed_asset.as_uuid())
    .bind(tag_id.as_uuid())
    .bind(admin.as_uuid())
    .bind(rejected_asset.as_uuid())
    .bind(proposed_asset.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    AssetTagRepo::new(test.db())
        .propose_for_tag(tag_id)
        .await
        .unwrap();

    let confirmed = fetch_assignment(&test, confirmed_asset, tag_id)
        .await
        .unwrap();
    assert_eq!(confirmed.0, "confirmed");
    assert_eq!(confirmed.1, "user");
    assert!((confirmed.2.unwrap() - 0.55).abs() < 1e-5);

    let rejected = fetch_assignment(&test, rejected_asset, tag_id)
        .await
        .unwrap();
    assert_eq!(rejected.0, "rejected");
    assert!((rejected.2.unwrap() - 0.60).abs() < 1e-5);

    let proposed = fetch_assignment(&test, proposed_asset, tag_id)
        .await
        .unwrap();
    assert_eq!(proposed.0, "proposed");
    assert_eq!(proposed.1, "ai");
    assert!(
        (proposed.2.unwrap() - 1.0).abs() < 1e-4,
        "proposed score must refresh on rematch"
    );
}

#[tokio::test]
async fn propose_for_tag_skips_mismatched_model_version() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "/mnt/at-mv").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_asset(&test, folder.id, "old.jpg").await;

    let emb = unit_axis(0);
    let tag_id = create_tag_with_embedding(&test, &ctx, "Vecchio", 0.5, emb.clone(), MODEL).await;
    EmbeddingRepo::new(test.db())
        .upsert(asset, &emb, "other-model-v1")
        .await
        .unwrap();

    let n = AssetTagRepo::new(test.db())
        .propose_for_tag(tag_id)
        .await
        .unwrap();
    assert_eq!(n, 0);
    assert!(fetch_assignment(&test, asset, tag_id).await.is_none());
}

#[tokio::test]
async fn threshold_only_change_does_not_require_rematch_api_contract() {
    // Documenta la regola di prodotto: cambiare solo `threshold` non ricalcola
    // le proposte (governa le analisi future). Qui verifichiamo che un update
    // solo-soglia non tocchi embedding e che le decisioni restino intatte
    // senza chiamare propose.
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "/mnt/at-thr").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_asset(&test, folder.id, "a.jpg").await;

    let emb = unit_axis(0);
    let tag_id = create_tag_with_embedding(&test, &ctx, "Soglia", 0.9, emb.clone(), MODEL).await;
    EmbeddingRepo::new(test.db())
        .upsert(asset, &emb, MODEL)
        .await
        .unwrap();
    // Prima proposta con soglia alta: sim=1 ≥ 0.9−band → riga.
    AssetTagRepo::new(test.db())
        .propose_for_tag(tag_id)
        .await
        .unwrap();
    assert!(fetch_assignment(&test, asset, tag_id).await.is_some());

    TagRepo::new(test.db())
        .update(
            &ctx,
            tag_id,
            TagPatch {
                name: None,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: Some(0.99),
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();

    // Senza propose_for_tag: la riga resta (threshold-only non rematch).
    let still = fetch_assignment(&test, asset, tag_id).await.expect("kept");
    assert_eq!(still.0, "proposed");
}

#[tokio::test]
async fn propose_for_assets_matches_all_tags_with_embeddings() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "/mnt/at-assets").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset_a = seed_asset(&test, folder.id, "a.jpg").await;
    let asset_b = seed_asset(&test, folder.id, "b.jpg").await;

    let emb = unit_axis(0);
    let tag1 = create_tag_with_embedding(&test, &ctx, "TagA", 0.5, emb.clone(), MODEL).await;
    let tag2 = create_tag_with_embedding(&test, &ctx, "TagB", 0.5, emb.clone(), MODEL).await;
    // Categoria senza embedding: non deve produrre proposte.
    TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Cat".to_owned(),
                kind: TagKind::Category,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();

    EmbeddingRepo::new(test.db())
        .upsert(asset_a, &emb, MODEL)
        .await
        .unwrap();
    EmbeddingRepo::new(test.db())
        .upsert(asset_b, &emb, MODEL)
        .await
        .unwrap();

    let n = AssetTagRepo::new(test.db())
        .propose_for_assets(&[asset_a])
        .await
        .unwrap();
    assert_eq!(n, 2, "one asset × two tags");

    assert!(fetch_assignment(&test, asset_a, tag1).await.is_some());
    assert!(fetch_assignment(&test, asset_a, tag2).await.is_some());
    assert!(
        fetch_assignment(&test, asset_b, tag1).await.is_none(),
        "asset_b not in the batch"
    );
}
