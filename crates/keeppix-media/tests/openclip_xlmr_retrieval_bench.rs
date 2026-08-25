//! Task B (piano modelli IA, `docs/superpowers/plans/2026-08-22-keeppix-modelli-ai.md`):
//! banco di recupero IT/EN su `OpenCLIP` XLM-R IT/EN — il piano chiede
//! esplicitamente i numeri Rust sul target, non solo il round-trip Python
//! del 22 agosto. Sostituisce il bench `MobileCLIP2`-S2 (`ai_retrieval_bench.rs`,
//! rimosso: numeri reali confermati equivalenti-o-migliori, licenza
//! permissiva contro Apple ML Research Model License research-only).
//!
//! Richiede i pesi locali (prodotti da
//! `.github/workflows/export-openclip-xlmr.yml`, non ancora un vero
//! `scripts/download-*.sh`: non c'è un URL esterno stabile per questo
//! checkpoint potato+quantizzato, solo un export occasionale) e le foto del
//! banco (`./scripts/download-ai-bench.sh`, `captions.json` condiviso).
//! Senza di essi il test si salta.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use keeppix_media::current_rss_bytes;
use keeppix_media::decode_to_rgb8;
use keeppix_media::openclip_xlmr::{self, OpenClipXlmr};
use keeppix_media::retrieval::{RetrievalScore, cosine_similarity, score_retrieval};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BenchFile {
    pairs: Vec<BenchPair>,
}

#[derive(Debug, Deserialize)]
struct BenchPair {
    id: String,
    image: String,
    en: String,
    it: String,
}

fn bench_image_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("KEEPPIX_BENCH_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(dir) = std::env::var("KEEPPIX_MODELS_DIR") {
        return PathBuf::from(dir).join("bench-it-en");
    }
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(|p| p.join("models/bench-it-en"));
    workspace.unwrap_or_else(|| PathBuf::from("models/bench-it-en"))
}

fn captions_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/ai-bench/captions.json")
}

fn load_bench(images_dir: &Path) -> Option<(BenchFile, Vec<PathBuf>)> {
    let raw = std::fs::read_to_string(captions_path()).ok()?;
    let bench: BenchFile = serde_json::from_str(&raw).ok()?;
    let paths: Vec<PathBuf> = bench
        .pairs
        .iter()
        .map(|p| images_dir.join(&p.image))
        .collect();
    if paths.iter().any(|p| !p.is_file()) {
        return None;
    }
    Some((bench, paths))
}

fn embed_gallery(model: &mut OpenClipXlmr, paths: &[PathBuf]) -> (Vec<Vec<f32>>, f64) {
    let mut gallery = Vec::with_capacity(paths.len());
    let mut total_ms = 0.0_f64;
    for path in paths {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let (rgb, w, h) = decode_to_rgb8(&bytes).unwrap_or_else(|e| {
            panic!("decode {}: {e}", path.display());
        });
        assert_eq!(
            rgb.len(),
            (w as usize) * (h as usize) * 3,
            "decode length mismatch for {}",
            path.display()
        );
        let nchw = model
            .rgb_to_nchw(&rgb, w, h)
            .unwrap_or_else(|e| panic!("preprocess {}: {e}", path.display()));
        let started = Instant::now();
        let emb = model
            .embed_image_nchw(&nchw)
            .unwrap_or_else(|e| panic!("embed image: {e}"));
        total_ms += started.elapsed().as_secs_f64() * 1000.0;
        gallery.push(emb);
    }
    (gallery, total_ms / paths.len() as f64)
}

fn embed_queries(model: &mut OpenClipXlmr, texts: &[String]) -> (Vec<Vec<f32>>, f64) {
    let mut out = Vec::with_capacity(texts.len());
    let mut total_ms = 0.0_f64;
    for t in texts {
        let started = Instant::now();
        let emb = model
            .embed_text(t)
            .unwrap_or_else(|e| panic!("embed text '{t}': {e}"));
        total_ms += started.elapsed().as_secs_f64() * 1000.0;
        out.push(emb);
    }
    (out, total_ms / texts.len() as f64)
}

fn fmt_score(lang: &str, score: &RetrievalScore) -> String {
    format!(
        "{lang}: recall@1={:.2} recall@5={:.2} mrr={:.3} ranks={:?}",
        score.recall_at_1, score.recall_at_5, score.mrr, score.ranks
    )
}

/// Per ogni query: cosine similarity con l'immagine corretta (stesso indice)
/// vs. la migliore fra le immagini sbagliate. Serve a ricalibrare
/// `TAG_MATCH_BAND`/la soglia di default (0.75) su punteggi reali, non solo
/// ranghi — un rank=1 con margine minuscolo è un match fragile.
fn score_margins(queries: &[Vec<f32>], gallery: &[Vec<f32>]) -> Vec<(f32, f32)> {
    queries
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let correct = cosine_similarity(q, &gallery[i]);
            let best_wrong = gallery
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, g)| cosine_similarity(q, g))
                .fold(f32::NEG_INFINITY, f32::max);
            (correct, best_wrong)
        })
        .collect()
}

fn fmt_margins(lang: &str, margins: &[(f32, f32)]) -> String {
    let gaps: Vec<f32> = margins.iter().map(|&(c, w)| c - w).collect();
    let min_gap = gaps.iter().copied().fold(f32::INFINITY, f32::min);
    let avg_gap = gaps.iter().sum::<f32>() / gaps.len() as f32;
    let min_correct = margins
        .iter()
        .map(|&(c, _)| c)
        .fold(f32::INFINITY, f32::min);
    let max_wrong = margins
        .iter()
        .map(|&(_, w)| w)
        .fold(f32::NEG_INFINITY, f32::max);
    format!(
        "{lang}: correct_score min={min_correct:.3} | best_wrong_score max={max_wrong:.3} | gap min={min_gap:.3} avg={avg_gap:.3}"
    )
}

/// Come `captions_fixture_has_twenty_distinguishable_it_en_pairs` in
/// `ai_retrieval_bench.rs` (`MobileCLIP2`, rimosso): la validazione della
/// fixture non dipende dal modello, ma vive qui ora che è l'unico bench.
#[test]
fn captions_fixture_has_twenty_distinguishable_it_en_pairs() {
    let raw = std::fs::read_to_string(captions_path()).expect("captions.json");
    let bench: BenchFile = serde_json::from_str(&raw).expect("parse captions");
    assert_eq!(bench.pairs.len(), 20, "Task 2bis asks for ~20 pairs");
    for p in &bench.pairs {
        assert!(!p.en.is_empty() && !p.it.is_empty());
        assert_ne!(p.en, p.it, "IT and EN captions must differ for {}", p.id);
        // Non banali: più di una parola.
        assert!(
            p.en.split_whitespace().count() >= 6,
            "EN too short: {}",
            p.en
        );
        assert!(
            p.it.split_whitespace().count() >= 6,
            "IT too short: {}",
            p.it
        );
    }
    let ens: Vec<_> = bench.pairs.iter().map(|p| p.en.as_str()).collect();
    let its: Vec<_> = bench.pairs.iter().map(|p| p.it.as_str()).collect();
    let mut ens_sorted = ens.clone();
    ens_sorted.sort_unstable();
    ens_sorted.dedup();
    assert_eq!(ens_sorted.len(), ens.len(), "duplicate EN captions");
    let mut its_sorted = its.clone();
    its_sorted.sort_unstable();
    its_sorted.dedup();
    assert_eq!(its_sorted.len(), its.len(), "duplicate IT captions");
}

#[test]
fn openclip_xlmr_it_en_retrieval_bench() {
    let Some(model_dir) = openclip_xlmr::first_complete_model_dir() else {
        eprintln!(
            "skipping: openclip-xlmr-it-en incomplete (girare .github/workflows/export-openclip-xlmr.yml)"
        );
        return;
    };
    let images_dir = bench_image_dir();
    let Some((bench, paths)) = load_bench(&images_dir) else {
        eprintln!(
            "skipping: bench images missing under {} (run scripts/download-ai-bench.sh)",
            images_dir.display()
        );
        return;
    };

    let rss_before = current_rss_bytes();
    let mut model = OpenClipXlmr::load(&model_dir).expect("load OpenClipXlmr");
    let rss_after_load = current_rss_bytes();

    let (gallery, ms_per_image) = embed_gallery(&mut model, &paths);
    let rss_peak_infer = current_rss_bytes();

    let en_texts: Vec<String> = bench.pairs.iter().map(|p| p.en.clone()).collect();
    let it_texts: Vec<String> = bench.pairs.iter().map(|p| p.it.clone()).collect();
    let (en_q, ms_text_en) = embed_queries(&mut model, &en_texts);
    let (it_q, ms_text_it) = embed_queries(&mut model, &it_texts);

    let en = score_retrieval(&en_q, &gallery);
    let it = score_retrieval(&it_q, &gallery);
    let en_margins = score_margins(&en_q, &gallery);
    let it_margins = score_margins(&it_q, &gallery);

    // Sanity come nel bench MobileCLIP2: se l'inglese è sotto soglia,
    // preprocess/tokenizer/remap sono rotti — non un "gap linguistico".
    assert!(
        en.recall_at_1 >= 0.40,
        "EN recall@1 too low ({:.2}); harness or model broken. {}",
        en.recall_at_1,
        fmt_score("EN", &en)
    );

    drop(model);
    let rss_after_drop = current_rss_bytes();

    eprintln!("MEASUREMENT OpenCLIP-XLMR-IT/EN retrieval (this host, debug+opt-level=2):");
    eprintln!("  {}", fmt_score("EN", &en));
    eprintln!("  {}", fmt_score("IT", &it));
    eprintln!(
        "  gap recall@1 = {:.2} (EN-IT), gap mrr = {:.3}",
        en.recall_at_1 - it.recall_at_1,
        en.mrr - it.mrr
    );
    eprintln!("  ms/photo (vision) ≈ {ms_per_image:.1}");
    eprintln!("  ms/text EN ≈ {ms_text_en:.1}, IT ≈ {ms_text_it:.1}");
    eprintln!(
        "  RSS bytes: before={rss_before:?} after_load={rss_after_load:?} peak_infer={rss_peak_infer:?} after_drop={rss_after_drop:?}"
    );
    // Punteggi coseno reali (non solo ranghi), per ricalibrare
    // TAG_MATCH_BAND/la soglia di default lato ricerca semantica.
    eprintln!("  {}", fmt_margins("EN", &en_margins));
    eprintln!("  {}", fmt_margins("IT", &it_margins));

    // Stesso tetto morbido del bench MobileCLIP2 (Task 6, Fase 7): sotto 1
    // GiB mentre il modello gira.
    if let Some(peak) = rss_peak_infer.or(rss_after_load) {
        assert!(
            peak < 1_073_741_824,
            "RSS peak {peak} exceeds hard ceiling of 1 GiB while model runs"
        );
    }
}
