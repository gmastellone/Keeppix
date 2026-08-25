//! `keeppix-db` duplica `MODEL_VERSION` per non dipendere da `keeppix-media`
//! (`deny.toml`). Le due costanti devono restare identiche: un mismatch
//! silenzioso renderebbe `SearchNode::Semantic` cieco a tutti gli embedding
//! già scritti (spec fase-7 §4.1, filtro `model_version = $1`).

#[test]
fn db_and_media_agree_on_the_embedding_model_version() {
    assert_eq!(
        keeppix_db::MODEL_VERSION,
        keeppix_media::openclip_xlmr::MODEL_VERSION,
        "keeppix-db::MODEL_VERSION è duplicato da keeppix-media::MODEL_VERSION: \
         se divergono, gli embedding scritti da keeppix-jobs (media) smettono \
         di essere trovati dalla ricerca (db)"
    );
}
