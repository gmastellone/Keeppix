//! Task B (piano modelli IA, `docs/superpowers/plans/2026-08-22-keeppix-modelli-ai.md`):
//! banco di recupero IT/EN su `OpenCLIP` XLM-R IT/EN, analogo a
//! `ai_retrieval_bench.rs` (MobileCLIP2-S2) — il piano chiede esplicitamente
//! i numeri Rust sul target, non solo il round-trip Python del 22 agosto.
//!
//! Richiede i pesi locali (prodotti da
//! `.github/workflows/export-openclip-xlmr.yml`, non ancora un vero
//! `scripts/download-*.sh`: non c'è un URL esterno stabile per questo
//! checkpoint potato+quantizzato, solo un export occasionale) e le stesse
//! foto del banco `MobileCLIP2` (`./scripts/download-ai-bench.sh`,
//! `captions.json` condiviso). Senza di essi il test si salta.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use keeppix_media::clip;
use keeppix_media::decode_to_rgb8;
use keeppix_media::openclip_xlmr::{self, OpenClipXlmr};
use keeppix_media::retrieval::{RetrievalScore, score_retrieval};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BenchFile {
    pairs: Vec<BenchPair>,
}

#[derive(Debug, Deserialize)]
struct BenchPair {
    // Non letto qui: la validazione della fixture (unicità/lunghezza degli
    // id) resta in ai_retrieval_bench.rs, non duplicata per ogni modello.
    #[allow(dead_code)]
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

    let rss_before = clip::current_rss_bytes();
    let mut model = OpenClipXlmr::load(&model_dir).expect("load OpenClipXlmr");
    let rss_after_load = clip::current_rss_bytes();

    let (gallery, ms_per_image) = embed_gallery(&mut model, &paths);
    let rss_peak_infer = clip::current_rss_bytes();

    let en_texts: Vec<String> = bench.pairs.iter().map(|p| p.en.clone()).collect();
    let it_texts: Vec<String> = bench.pairs.iter().map(|p| p.it.clone()).collect();
    let (en_q, ms_text_en) = embed_queries(&mut model, &en_texts);
    let (it_q, ms_text_it) = embed_queries(&mut model, &it_texts);

    let en = score_retrieval(&en_q, &gallery);
    let it = score_retrieval(&it_q, &gallery);

    // Sanity come nel bench MobileCLIP2: se l'inglese è sotto soglia,
    // preprocess/tokenizer/remap sono rotti — non un "gap linguistico".
    assert!(
        en.recall_at_1 >= 0.40,
        "EN recall@1 too low ({:.2}); harness or model broken. {}",
        en.recall_at_1,
        fmt_score("EN", &en)
    );

    drop(model);
    let rss_after_drop = clip::current_rss_bytes();

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

    // Stesso tetto morbido del bench MobileCLIP2 (Task 6, Fase 7): sotto 1
    // GiB mentre il modello gira.
    if let Some(peak) = rss_peak_infer.or(rss_after_load) {
        assert!(
            peak < 1_073_741_824,
            "RSS peak {peak} exceeds hard ceiling of 1 GiB while model runs"
        );
    }
}

/// Confronto int8 dinamico (default) vs `visual_fp16` sullo stesso banco —
/// il piano lo chiede esplicitamente ("l'int8 dinamico costa un colpo IT
/// sul visual: provare QDQ statica o fp16 e scegliere col numero").
///
/// Nessun codice Rust nuovo per caricare la variante: `visual_fp16.onnx`
/// ha lo stesso contratto I/O di `visual.onnx` (verificato offline in
/// `scripts/export-openclip-xlmr-it-en.py`, `keep_io_types=True`), quindi
/// basta una directory di confronto con gli stessi file ma
/// `visual_fp16.onnx` copiato come `visual.onnx`.
fn stage_fp16_visual_model_dir(source: &Path) -> Option<PathBuf> {
    let fp16_source = source.join("visual_fp16.onnx");
    if !fp16_source.is_file() {
        return None;
    }
    let dir = std::env::temp_dir().join(format!(
        "keeppix-openclip-xlmr-fp16-visual-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create fp16 comparison dir");
    std::fs::copy(&fp16_source, dir.join("visual.onnx")).expect("stage visual_fp16.onnx");
    for name in ["text.onnx", "tokenizer.json", "export_manifest.json"] {
        std::fs::copy(source.join(name), dir.join(name))
            .unwrap_or_else(|e| panic!("stage {name}: {e}"));
    }
    Some(dir)
}

#[test]
fn openclip_xlmr_visual_fp16_vs_int8_comparison() {
    let Some(model_dir) = openclip_xlmr::first_complete_model_dir() else {
        eprintln!("skipping: openclip-xlmr-it-en incomplete");
        return;
    };
    let Some(fp16_dir) = stage_fp16_visual_model_dir(&model_dir) else {
        eprintln!(
            "skipping: visual_fp16.onnx missing from {} (export cache predates the fp16 variant)",
            model_dir.display()
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

    let mut model = OpenClipXlmr::load(&fp16_dir).expect("load OpenClipXlmr (visual_fp16)");
    let (gallery, ms_per_image) = embed_gallery(&mut model, &paths);
    let en_texts: Vec<String> = bench.pairs.iter().map(|p| p.en.clone()).collect();
    let it_texts: Vec<String> = bench.pairs.iter().map(|p| p.it.clone()).collect();
    let (en_q, ms_text_en) = embed_queries(&mut model, &en_texts);
    let (it_q, ms_text_it) = embed_queries(&mut model, &it_texts);
    drop(model);
    let _ = std::fs::remove_dir_all(&fp16_dir);

    let en = score_retrieval(&en_q, &gallery);
    let it = score_retrieval(&it_q, &gallery);

    // Stessa sanity del bench principale: qui un EN basso significa che la
    // variante fp16 stessa (o lo staging della directory) è rotta, non un
    // "gap linguistico".
    assert!(
        en.recall_at_1 >= 0.40,
        "EN recall@1 too low ({:.2}) on visual_fp16 variant; harness or conversion broken. {}",
        en.recall_at_1,
        fmt_score("EN", &en)
    );

    eprintln!("MEASUREMENT OpenCLIP-XLMR-IT/EN retrieval (visual_fp16 variant):");
    eprintln!("  {}", fmt_score("EN", &en));
    eprintln!("  {}", fmt_score("IT", &it));
    eprintln!("  ms/photo (vision) ≈ {ms_per_image:.1}");
    eprintln!("  ms/text EN ≈ {ms_text_en:.1}, IT ≈ {ms_text_it:.1}");
    eprintln!(
        "  confronta con il blocco MEASUREMENT del default int8 sopra (stesso banco, stesso run)"
    );
}
