//! `keeppix-db` duplicates `MODEL_VERSION` so it doesn't have to depend on
//! `keeppix-media` (`deny.toml`). The two constants must stay identical: a
//! silent mismatch would make `SearchNode::Semantic` blind to every
//! embedding already written (the `model_version = $1` filter).

#[test]
fn db_and_media_agree_on_the_embedding_model_version() {
    assert_eq!(
        keeppix_db::MODEL_VERSION,
        keeppix_media::openclip_xlmr::MODEL_VERSION,
        "keeppix-db::MODEL_VERSION is duplicated from keeppix-media::MODEL_VERSION: \
         if they diverge, embeddings written by keeppix-jobs (media) stop \
         being found by search (db)"
    );
}
