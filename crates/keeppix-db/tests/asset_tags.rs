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

// ---------------------------------------------------------------------------
// Fase 7 Task 9: la coda di revisione (confirm/reject, singoli e in blocco,
// list + count con visibilità).
// ---------------------------------------------------------------------------

async fn seed_proposed(test: &TestDb, asset: AssetId, tag: TagId, score: f32) {
    sqlx::query(
        "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) \
         VALUES ($1, $2, 'proposed', 'ai', $3)",
    )
    .bind(asset.as_uuid())
    .bind(tag.as_uuid())
    .bind(score)
    .execute(test.db().pool())
    .await
    .unwrap();
}

/// Libreria + cartella + tag "vuoto" (nessun embedding, non serve per questi
/// test) posseduti da `owner`, e un secondo utente qualsiasi senza permessi.
struct ReviewFixture {
    owner: UserId,
    stranger: UserId,
    tag: TagId,
    asset: AssetId,
}

async fn review_fixture(test: &TestDb) -> ReviewFixture {
    let owner = harness::seed_admin(test).await;
    let stranger = harness::seed_user(test, owner, "estraneo").await;
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    let library = seed_library(test, owner, "/mnt/review").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_asset(test, folder.id, "review.jpg").await;
    let tag = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Revisione".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap()
        .id;
    seed_proposed(test, asset, tag, 0.9).await;
    ReviewFixture {
        owner,
        stranger,
        tag,
        asset,
    }
}

#[tokio::test]
async fn confirm_transitions_proposed_to_confirmed_with_decider() {
    let test = TestDb::start().await;
    let fx = review_fixture(&test).await;
    let ctx = AuthContext::user(fx.owner, SystemRole::Admin);

    AssetTagRepo::new(test.db())
        .confirm(&ctx, fx.tag, fx.asset)
        .await
        .unwrap();

    let row: (String, Option<uuid::Uuid>, bool) = sqlx::query_as(
        "SELECT state, decided_by, decided_at IS NOT NULL FROM asset_tags \
         WHERE asset_id = $1 AND tag_id = $2",
    )
    .bind(fx.asset.as_uuid())
    .bind(fx.tag.as_uuid())
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(row.0, "confirmed");
    assert_eq!(row.1, Some(fx.owner.as_uuid()));
    assert!(row.2, "decided_at must be set");
}

#[tokio::test]
async fn confirm_is_idempotent_when_already_confirmed() {
    let test = TestDb::start().await;
    let fx = review_fixture(&test).await;
    let ctx = AuthContext::user(fx.owner, SystemRole::Admin);
    let repo = AssetTagRepo::new(test.db());

    repo.confirm(&ctx, fx.tag, fx.asset).await.unwrap();
    // Same decision again: no-op success, not an error.
    repo.confirm(&ctx, fx.tag, fx.asset).await.unwrap();
}

#[tokio::test]
async fn confirm_conflicts_when_already_rejected() {
    let test = TestDb::start().await;
    let fx = review_fixture(&test).await;
    let ctx = AuthContext::user(fx.owner, SystemRole::Admin);
    let repo = AssetTagRepo::new(test.db());

    repo.reject(&ctx, fx.tag, fx.asset).await.unwrap();
    let err = repo.confirm(&ctx, fx.tag, fx.asset).await.unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Conflict(_)));
}

#[tokio::test]
async fn reject_is_permanent_and_rematch_never_resurrects_it() {
    let test = TestDb::start().await;
    let fx = review_fixture(&test).await;
    let ctx = AuthContext::user(fx.owner, SystemRole::Admin);
    let repo = AssetTagRepo::new(test.db());

    repo.reject(&ctx, fx.tag, fx.asset).await.unwrap();

    // A rematch (as if the tag were re-created/patched) must not touch it.
    repo.propose_for_tag(fx.tag).await.unwrap();

    let row = fetch_assignment(&test, fx.asset, fx.tag).await.unwrap();
    assert_eq!(row.0, "rejected");
}

#[tokio::test]
async fn confirm_on_never_proposed_pair_is_not_found() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    let library = seed_library(&test, owner, "/mnt/review-nf").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_asset(&test, folder.id, "unmatched.jpg").await;
    let tag = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Mai proposto".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap()
        .id;

    let err = AssetTagRepo::new(test.db())
        .confirm(&ctx, tag, asset)
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::NotFound));
}

#[tokio::test]
async fn deciding_on_a_foreign_asset_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let fx = review_fixture(&test).await;
    let stranger_ctx = AuthContext::user(fx.stranger, SystemRole::User);

    let confirm_err = AssetTagRepo::new(test.db())
        .confirm(&stranger_ctx, fx.tag, fx.asset)
        .await
        .unwrap_err();
    assert!(matches!(confirm_err, keeppix_db::DbError::Forbidden));

    let reject_err = AssetTagRepo::new(test.db())
        .reject(&stranger_ctx, fx.tag, fx.asset)
        .await
        .unwrap_err();
    assert!(matches!(reject_err, keeppix_db::DbError::Forbidden));

    // The proposal must be untouched by the failed attempts.
    let row = fetch_assignment(&test, fx.asset, fx.tag).await.unwrap();
    assert_eq!(row.0, "proposed");
}

#[tokio::test]
async fn list_proposed_orders_by_score_descending() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    let library = seed_library(&test, owner, "/mnt/review-order").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let weak = seed_asset(&test, folder.id, "weak.jpg").await;
    let strong = seed_asset(&test, folder.id, "strong.jpg").await;
    let tag = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Ordinamento".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap()
        .id;
    seed_proposed(&test, weak, tag, 0.76).await;
    seed_proposed(&test, strong, tag, 0.98).await;

    let list = AssetTagRepo::new(test.db())
        .list_proposed(&ctx, None)
        .await
        .unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].asset_id, strong);
    assert_eq!(list[1].asset_id, weak);
    assert_eq!(list[0].tag_name, "Ordinamento");
}

#[tokio::test]
async fn list_proposed_filters_by_tag_id_and_hides_foreign_assets() {
    let test = TestDb::start().await;
    let fx = review_fixture(&test).await;
    let owner_ctx = AuthContext::user(fx.owner, SystemRole::Admin);
    let stranger_ctx = AuthContext::user(fx.stranger, SystemRole::User);

    let filtered = AssetTagRepo::new(test.db())
        .list_proposed(&owner_ctx, Some(fx.tag))
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].asset_id, fx.asset);

    let other_tag = TagId::new();
    let empty = AssetTagRepo::new(test.db())
        .list_proposed(&owner_ctx, Some(other_tag))
        .await
        .unwrap();
    assert!(empty.is_empty());

    let stranger_view = AssetTagRepo::new(test.db())
        .list_proposed(&stranger_ctx, None)
        .await
        .unwrap();
    assert!(
        stranger_view.is_empty(),
        "a user without a grant on the library must not see the proposal"
    );
}

#[tokio::test]
async fn confirm_all_for_tag_only_confirms_visible_proposed_rows() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    let library = seed_library(&test, owner, "/mnt/review-bulk-confirm").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let a = seed_asset(&test, folder.id, "a.jpg").await;
    let b = seed_asset(&test, folder.id, "b.jpg").await;
    let already_decided = seed_asset(&test, folder.id, "already.jpg").await;
    let tag = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Blocco".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap()
        .id;
    seed_proposed(&test, a, tag, 0.9).await;
    seed_proposed(&test, b, tag, 0.8).await;
    // Already decided (rejected) by a human: bulk confirm must not touch it.
    sqlx::query(
        "INSERT INTO asset_tags (asset_id, tag_id, state, source, score, decided_by, decided_at) \
         VALUES ($1, $2, 'rejected', 'user', 0.5, $3, now())",
    )
    .bind(already_decided.as_uuid())
    .bind(tag.as_uuid())
    .bind(owner.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    let confirmed = AssetTagRepo::new(test.db())
        .confirm_all_for_tag(&ctx, tag)
        .await
        .unwrap();
    let confirmed_set: std::collections::HashSet<AssetId> = confirmed.into_iter().collect();
    let expected: std::collections::HashSet<AssetId> = [a, b].into_iter().collect();
    assert_eq!(confirmed_set, expected);

    for asset in [a, b] {
        let row = fetch_assignment(&test, asset, tag).await.unwrap();
        assert_eq!(row.0, "confirmed");
    }
    let untouched = fetch_assignment(&test, already_decided, tag).await.unwrap();
    assert_eq!(
        untouched.0, "rejected",
        "bulk confirm must not flip an already-decided pair"
    );
}

#[tokio::test]
async fn reject_all_for_tag_excludes_assets_the_caller_cannot_see() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let stranger = harness::seed_user(&test, owner, "vicino").await;
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    let stranger_ctx = AuthContext::user(stranger, SystemRole::User);
    let library = seed_library(&test, owner, "/mnt/review-bulk-reject").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_asset(&test, folder.id, "hidden.jpg").await;
    let tag = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Nascosto".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap()
        .id;
    seed_proposed(&test, asset, tag, 0.9).await;

    // The stranger has no grant on `owner`'s library: bulk reject touches
    // nothing, and the proposal must survive untouched.
    let rejected = AssetTagRepo::new(test.db())
        .reject_all_for_tag(&stranger_ctx, tag)
        .await
        .unwrap();
    assert!(rejected.is_empty());
    let row = fetch_assignment(&test, asset, tag).await.unwrap();
    assert_eq!(row.0, "proposed");

    let rejected = AssetTagRepo::new(test.db())
        .reject_all_for_tag(&ctx, tag)
        .await
        .unwrap();
    assert_eq!(rejected, vec![asset]);
}

#[tokio::test]
async fn count_proposed_visible_counts_only_what_the_caller_can_see() {
    let test = TestDb::start().await;
    let fx = review_fixture(&test).await;
    let owner_ctx = AuthContext::user(fx.owner, SystemRole::Admin);
    let stranger_ctx = AuthContext::user(fx.stranger, SystemRole::User);

    let repo = AssetTagRepo::new(test.db());
    assert_eq!(repo.count_proposed_visible(&owner_ctx).await.unwrap(), 1);
    assert_eq!(repo.count_proposed_visible(&stranger_ctx).await.unwrap(), 0);

    repo.confirm(&owner_ctx, fx.tag, fx.asset).await.unwrap();
    assert_eq!(
        repo.count_proposed_visible(&owner_ctx).await.unwrap(),
        0,
        "a decided proposal must leave the queue"
    );
}

// Fase 11 Task 7 (§13.3 campo 5, "Aggiungi tag…"): un'aggiunta manuale è
// già una conferma, non passa dalla coda di revisione.

#[tokio::test]
async fn assign_inserts_a_confirmed_user_row_from_scratch() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    let library = seed_library(&test, owner, "/mnt/assign-fresh").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_asset(&test, folder.id, "fresh.jpg").await;
    let tag = create_tag_with_embedding(&test, &ctx, "A mano", 0.75, unit_axis(0), MODEL).await;

    AssetTagRepo::new(test.db())
        .assign(&ctx, tag, asset)
        .await
        .unwrap();

    let row = fetch_assignment(&test, asset, tag).await.unwrap();
    assert_eq!(row.0, "confirmed");
    assert_eq!(row.1, "user");
}

#[tokio::test]
async fn assign_overrides_an_existing_rejection_unlike_confirm() {
    let test = TestDb::start().await;
    let fx = review_fixture(&test).await;
    let ctx = AuthContext::user(fx.owner, SystemRole::Admin);
    let repo = AssetTagRepo::new(test.db());
    repo.reject(&ctx, fx.tag, fx.asset).await.unwrap();

    // `confirm` would conflict here (already tested above); `assign` is a
    // direct human decision made *now*, not a proposal resolution — it
    // must win over the earlier rejection instead of erroring.
    repo.assign(&ctx, fx.tag, fx.asset).await.unwrap();

    let row = fetch_assignment(&test, fx.asset, fx.tag).await.unwrap();
    assert_eq!(row.0, "confirmed");
    assert_eq!(row.1, "user");
}

#[tokio::test]
async fn assign_is_idempotent_and_forbidden_on_a_foreign_asset() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let stranger = harness::seed_user(&test, owner, "estraneo-assign").await;
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    let stranger_ctx = AuthContext::user(stranger, SystemRole::User);
    let library = seed_library(&test, owner, "/mnt/assign-forbidden").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_asset(&test, folder.id, "private.jpg").await;
    let tag = create_tag_with_embedding(&test, &ctx, "Privato", 0.75, unit_axis(0), MODEL).await;
    let repo = AssetTagRepo::new(test.db());

    assert!(repo.assign(&stranger_ctx, tag, asset).await.is_err());

    repo.assign(&ctx, tag, asset).await.unwrap();
    repo.assign(&ctx, tag, asset).await.unwrap();
    let row = fetch_assignment(&test, asset, tag).await.unwrap();
    assert_eq!(row.0, "confirmed");
}

#[tokio::test]
async fn confirmed_among_returns_only_confirmed_rows_grouped_by_asset() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    let library = seed_library(&test, owner, "/mnt/confirmed-among").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let confirmed_asset = seed_asset(&test, folder.id, "confirmed.jpg").await;
    let proposed_asset = seed_asset(&test, folder.id, "proposed.jpg").await;
    let untouched_asset = seed_asset(&test, folder.id, "untouched.jpg").await;
    let category = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Viaggi".to_owned(),
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
        .unwrap()
        .id;
    let tag = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Montagna".to_owned(),
                kind: TagKind::Tag,
                parent_id: Some(category),
                prompt: None,
                color: Some("#336699".to_owned()),
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap()
        .id;
    let repo = AssetTagRepo::new(test.db());
    repo.assign(&ctx, tag, confirmed_asset).await.unwrap();
    seed_proposed(&test, proposed_asset, tag, 0.9).await;

    let map = repo
        .confirmed_among(&[confirmed_asset, proposed_asset, untouched_asset])
        .await
        .unwrap();

    assert_eq!(map.len(), 1, "only the confirmed asset carries an entry");
    let badges = &map[&confirmed_asset];
    assert_eq!(badges.len(), 1);
    assert_eq!(badges[0].tag_id, tag);
    assert_eq!(badges[0].name, "Montagna");
    assert_eq!(badges[0].color.as_deref(), Some("#336699"));
    assert_eq!(
        badges[0].category_id,
        Some(category),
        "category_id is the tag's own parent_id, resolved in the same query"
    );
    assert!(!map.contains_key(&proposed_asset), "proposed, not confirmed");
    assert!(!map.contains_key(&untouched_asset));
}

#[tokio::test]
async fn confirmed_among_is_empty_for_an_empty_id_list() {
    let test = TestDb::start().await;
    let map = AssetTagRepo::new(test.db())
        .confirmed_among(&[])
        .await
        .unwrap();
    assert!(map.is_empty());
}
