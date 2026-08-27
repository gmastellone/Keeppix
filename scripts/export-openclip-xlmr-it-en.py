#!/usr/bin/env python3
"""Exports OpenCLIP XLM-R ViT-B-32 (checkpoint laion5b_s13b_b90k) to ONNX,
pruned to the IT/EN vocabulary and quantized to int8 — replaces
MobileCLIP2-S2 (research-only, never fit for commercial use).

The only place in the pipeline where Python runs (an explicit project
constraint: "Python is allowed ONLY in the offline export script").
Everything else — loading, inference, IT/EN regression bench, RSS/ms
measurements — lives in Rust (`keeppix-media`, `ai_retrieval_bench`).

Requires network access to huggingface.co (the checkpoint is hosted ONLY
there, no mirror declared in `open_clip.pretrained.get_pretrained_cfg`), so
it cannot run in a network-restricted sandbox (blocked at the proxy level,
the same limitation previously hit for MobileCLIP2 — see
`models/README.md`). The two halves of the architecture were verified
separately offline before writing this file:
  - vision tower (ViT-B-32, no HF config needed): built with random weights
    via `open_clip.create_model_and_transforms`, exported to ONNX
    successfully, 512-d output confirmed.
  - text tower (XLMRobertaModel, known public architecture — vocab 250002,
    hidden 768, 12 layers, 12 heads, intermediate 3072): built with random
    weights via `transformers.XLMRobertaModel(XLMRobertaConfig(...))`,
    exported to ONNX successfully with current torch/transformers,
    **without needing** the `torch.backends.mha.set_fastpath_enabled(False)`
    workaround (likely because that symptom was specific to older
    torch/transformers versions — the flag below is still set defensively
    at zero cost, covering both cases anyway). Pooling/projection logic
    (masked `MeanPooler` + `nn.Linear(768, 512, bias=False)`) was read from
    the locally installed `open_clip/hf_model.py` and
    `open_clip/hf_configs.py` (`arch_dict['xlm-roberta']`) — source code,
    not weights, so no network was needed to read it.
  - `proj_type` of the real checkpoint: verified against a real CI run, it
    is 'mlp' (`nn.Sequential`, not `nn.Linear` as this docstring originally
    assumed — the `assert_real_proj_is_recognized` check below failed
    explicitly and informatively instead of silently exporting the wrong
    projection; it now recognizes both shapes).
  - The final L2 normalization is **not** in the exported graph, for the
    same reason as MobileCLIP2: `crates/keeppix-media/src/clip.rs` does it
    in Rust (`l2_normalize`) after extracting the tensor — verified by
    reading that file. The graph here exports the raw projection.

Usage:
  python3 scripts/export-openclip-xlmr-it-en.py --out models/openclip-xlmr-it-en

Required packages (deliberately not pinned to an exact version: the script
fails explicitly if an assumed API doesn't exist, instead of an obscure
error partway through — see the `hasattr`/`assert` checks below):
  torch, open_clip_torch, transformers, onnx, onnxruntime, wordfreq
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

CHECKPOINT_MODEL = "xlm-roberta-base-ViT-B-32"
CHECKPOINT_TAG = "laion5b_s13b_b90k"
EMBED_DIM = 512
IMAGE_SIZE = 224
# Token margin for each word in the frequency list: a real word often
# splits into multiple subword unigrams (prefixes/suffixes), not a single
# id. 20,000 common words per language is the same order of magnitude used
# to estimate practical coverage in other multilingual->bilingual vocabulary
# pruning work; not an arbitrary number, but not an exact science either —
# if the regression bench (comparing pre/post pruning) shows a recall drop,
# this is the first number to raise.
WORDFREQ_TOP_N = 20_000


def log(msg: str) -> None:
    print(f"→ {msg}", file=sys.stderr, flush=True)


def build_it_en_corpus(captions_path: Path) -> list[str]:
    """Corpus for vocabulary pruning: common IT/EN words (via `wordfreq`,
    data bundled in the package — zero network at runtime, unlike a
    Wikipedia dump) plus real sentences from the test bench
    (`captions.json`, already in the repo) — covers both the isolated
    vocabulary and real sentence segmentation, which for a SentencePiece
    tokenizer can split words differently than the lexicon alone.
    """
    from wordfreq import top_n_list

    words = set(top_n_list("en", WORDFREQ_TOP_N)) | set(top_n_list("it", WORDFREQ_TOP_N))
    sentences: list[str] = []
    if captions_path.is_file():
        bench = json.loads(captions_path.read_text(encoding="utf-8"))
        for pair in bench["pairs"]:
            sentences.append(pair["en"])
            sentences.append(pair["it"])
    else:
        log(f"WARNING: {captions_path} not found, corpus from wordfreq only (no real sentences)")
    return sorted(words) + sentences


def tokens_used_by_corpus(tokenizer, corpus: list[str]) -> set[int]:
    used: set[int] = set()
    for text in corpus:
        ids = tokenizer(text, add_special_tokens=True)["input_ids"]
        used.update(ids)
    return used


def prune_text_embedding(
    text_tower,
    used_ids: set[int],
    special_ids: set[int],
) -> tuple["torch.Tensor", dict[int, int]]:
    """Builds the pruned embedding matrix (rows: only the ids used by the
    IT/EN corpus plus the special ids) and the original-id -> new-index
    map. Ids that are NOT kept remain tokenizable (the tokenizer itself
    doesn't change: same segmentation for any language, not just IT/EN —
    consistent with the project constraint that "the other 107 languages
    are not a requirement: if pruning breaks them, that's fine"; here their
    words simply end up on the fallback), but at runtime they get remapped
    to the `<unk>` index — an explicit degradation, not a crash.
    """
    import torch

    embeddings = text_tower.embeddings.word_embeddings
    vocab_size, hidden = embeddings.weight.shape
    unk_id = text_tower.config.unk_token_id if hasattr(text_tower.config, "unk_token_id") else None
    keep = sorted(used_ids | special_ids)
    if unk_id is not None and unk_id not in keep:
        keep.append(unk_id)
        keep.sort()
    log(f"vocabulary: {vocab_size} -> {len(keep)} rows kept ({100 * len(keep) / vocab_size:.1f}%)")

    remap: dict[int, int] = {old: new for new, old in enumerate(keep)}
    unk_new = remap.get(unk_id, 0) if unk_id is not None else 0

    with torch.no_grad():
        pruned = embeddings.weight[keep].clone()

    return pruned, remap, unk_new, hidden


def apply_remap_for_export(input_ids, remap_tensor):
    """`remap_tensor` is a dense array [original_vocab_size] -> new index
    (or the `<unk>` index for ids that weren't kept), built once and
    carried into the ONNX graph as a constant — a `gather`, not a Python
    lookup: it must run inside `torch.onnx.export`, not before."""
    return remap_tensor[input_ids]


def assert_real_proj_is_recognized(text_tower) -> None:
    """Verified against the real checkpoint (no longer an assumption):
    `proj_type` is 'mlp' — `nn.Sequential(Linear(768, hidden, bias=False),
    GELU(), Linear(hidden, 512, bias=False))`, `hidden = (768+512)//2 = 640`
    per the formula in `open_clip/hf_model.py` — not 'linear' as this file
    originally assumed (it failed with an explicit error instead of
    silently exporting the wrong projection: `unexpected proj_type:
    Sequential`, verified against a real CI run). `TextTowerExport.forward`
    below simply calls `self.proj(pooled)`: it works identically for
    `nn.Linear` or `nn.Sequential`, so no change to the export mechanics
    was needed — only to this check, which had been too strict.
    """
    import torch.nn as nn

    proj = text_tower.proj
    if isinstance(proj, nn.Linear):
        if proj.bias is not None:
            raise SystemExit(
                "linear proj WITH bias — this file assumes bias=False. "
                "If the real checkpoint has a bias, add it to the export."
            )
        log(f"proj_type: linear ({proj.in_features}->{proj.out_features}, bias=False)")
        return
    if isinstance(proj, nn.Sequential):
        linears = [m for m in proj if isinstance(m, nn.Linear)]
        if len(linears) != 2 or any(lin.bias is not None for lin in linears):
            raise SystemExit(
                f"proj_type='mlp' but unexpected structure: {proj} — expected "
                "exactly 2 Linear layers without bias (plus an activation in "
                "between). Verify by hand before proceeding."
            )
        log(
            f"proj_type: mlp ({linears[0].in_features}->{linears[0].out_features}"
            f"->{linears[1].out_features}, bias=False)"
        )
        return
    raise SystemExit(
        f"unexpected proj_type: {type(proj).__name__} (expected nn.Linear or "
        "nn.Sequential/mlp). Verify model.text.proj of the real checkpoint "
        "before proceeding."
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path("models/openclip-xlmr-it-en"))
    parser.add_argument(
        "--captions",
        type=Path,
        default=Path("crates/keeppix-media/testdata/ai-bench/captions.json"),
    )
    parser.add_argument(
        "--opset", type=int, default=14, help="same opset already used for MobileCLIP2-S2"
    )
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    import torch
    import open_clip
    from onnxruntime.quantization import quantize_dynamic, QuantType
    from onnxruntime.quantization.shape_inference import quant_pre_process

    torch.backends.mha.set_fastpath_enabled(False)  # defensive, see docstring above

    log(f"loading {CHECKPOINT_MODEL} / {CHECKPOINT_TAG} from HuggingFace (requires network)")
    model, _, preprocess = open_clip.create_model_and_transforms(CHECKPOINT_MODEL, pretrained=CHECKPOINT_TAG)
    tokenizer_wrapper = open_clip.get_tokenizer(CHECKPOINT_MODEL)
    model.eval()

    # The REAL image normalization for this checkpoint, not assumed: the
    # Rust consumer (crates/keeppix-media) must apply the same mean/std
    # before feeding visual.onnx, and guessing it (even though "standard
    # CLIP" is a reasonable assumption, almost always true for LAION
    # checkpoints) would be exactly the kind of mistake
    # `assert_real_proj_is_recognized` above exists to avoid for the text
    # projection. Read from the real `torchvision.Compose` pipeline
    # returned by `create_model_and_transforms`, not from a constant.
    normalize_transforms = [t for t in preprocess.transforms if hasattr(t, "mean") and hasattr(t, "std")]
    if len(normalize_transforms) != 1:
        raise SystemExit(
            f"unexpected preprocessing pipeline: {len(normalize_transforms)} stages with "
            f"mean/std (expected exactly 1 — Normalize). Real pipeline: {preprocess}. "
            "Verify by hand before proceeding."
        )
    image_mean = tuple(float(v) for v in normalize_transforms[0].mean)
    image_std = tuple(float(v) for v in normalize_transforms[0].std)
    if len(image_mean) != 3 or len(image_std) != 3:
        raise SystemExit(f"mean/std not 3-channel: mean={image_mean} std={image_std}")
    log(f"real image normalization: mean={image_mean} std={image_std}")

    if model.visual.image_size not in ((IMAGE_SIZE, IMAGE_SIZE), IMAGE_SIZE):
        raise SystemExit(
            f"unexpected image_size: {model.visual.image_size} (expected {IMAGE_SIZE}) — "
            "check crates/keeppix-media/src/clip.rs (IMAGE_SIZE) before proceeding."
        )
    if model.text.output_dim != EMBED_DIM:
        raise SystemExit(
            f"unexpected embed_dim: {model.text.output_dim} (expected {EMBED_DIM}) — "
            "the project specifies 512-d, no schema migration is planned for "
            "a different embed_dim."
        )
    assert_real_proj_is_recognized(model.text)

    # --- Vision tower: no pruning, export as-is. ---
    log("ONNX export: vision tower")
    visual = model.visual
    dummy_img = torch.randn(1, 3, IMAGE_SIZE, IMAGE_SIZE)
    with torch.no_grad():
        visual_out = visual(dummy_img)
    if tuple(visual_out.shape) != (1, EMBED_DIM):
        raise SystemExit(f"unexpected vision tower output: {tuple(visual_out.shape)}")
    torch.onnx.export(
        visual,
        (dummy_img,),
        str(args.out / "visual_fp32.onnx"),
        opset_version=args.opset,
        input_names=["pixel_values"],
        output_names=["image_embeds"],
        dynamic_axes={"pixel_values": {0: "batch"}, "image_embeds": {0: "batch"}},
        dynamo=False,
    )

    # --- IT/EN corpus and text vocabulary pruning. ---
    log("building IT/EN corpus for vocabulary pruning")
    # `open_clip.get_tokenizer` for an HF-based model returns an
    # `HFTokenizer` (open_clip/tokenizer.py): `.tokenizer` is the real
    # `AutoTokenizer`, `.save_pretrained` delegates to it — read from the
    # installed source, not guessed.
    if not hasattr(tokenizer_wrapper, "tokenizer"):
        raise SystemExit(
            f"unexpected tokenizer_wrapper: {type(tokenizer_wrapper).__name__} "
            "without a `.tokenizer` attribute — open_clip.get_tokenizer did not "
            "return an HFTokenizer as expected. Verify by hand before "
            "proceeding (the API may have changed)."
        )
    hf_tokenizer = tokenizer_wrapper.tokenizer
    corpus = build_it_en_corpus(args.captions)
    used_ids = tokens_used_by_corpus(hf_tokenizer, corpus)
    special_ids = set(hf_tokenizer.all_special_ids)
    log(f"corpus: {len(corpus)} entries, {len(used_ids)} distinct token ids produced")

    pruned_weight, remap, unk_new, hidden = prune_text_embedding(
        model.text.transformer, used_ids, special_ids
    )
    original_vocab_size = model.text.transformer.embeddings.word_embeddings.weight.shape[0]

    # Dense remap tensor: original id -> new index (fallback to unk_new for
    # every id that wasn't kept). Becomes a constant in the ONNX graph — a
    # gather, not a Python dict — so it must exist BEFORE the export, not
    # be applied on the Rust side afterward.
    remap_tensor = torch.full((original_vocab_size,), unk_new, dtype=torch.long)
    for old_id, new_id in remap.items():
        remap_tensor[old_id] = new_id

    # Replaces the embedding table with the pruned one: from here on the
    # model expects input ids that are ALREADY remapped (0..len(remap)-1),
    # not the original XLM-R ids.
    with torch.no_grad():
        new_embedding = torch.nn.Embedding(len(remap), hidden, padding_idx=None)
        new_embedding.weight.copy_(pruned_weight)
    model.text.transformer.embeddings.word_embeddings = new_embedding
    model.text.transformer.config.vocab_size = len(remap)

    # --- Text tower: wrapper that applies the remap BEFORE the transformer,
    # then masked pooling + linear projection — an exact replica of
    # HFTextEncoder.forward (open_clip/hf_model.py), with the remap added
    # at the front. ---
    class TextTowerExport(torch.nn.Module):
        def __init__(self, text_tower, remap_tensor):
            super().__init__()
            self.transformer = text_tower.transformer
            self.pooler = text_tower.pooler
            self.proj = text_tower.proj
            self.pad_token_id = text_tower.transformer.config.pad_token_id
            self.register_buffer("remap", remap_tensor, persistent=False)

        def forward(self, input_ids_original, attention_mask):
            remapped = self.remap[input_ids_original]
            out = self.transformer(input_ids=remapped, attention_mask=attention_mask, use_cache=False, return_dict=False)
            last_hidden_state = out[0]

            class _Wrap:
                pass

            wrapped = _Wrap()
            wrapped.last_hidden_state = last_hidden_state
            pooled = self.pooler(wrapped, attention_mask)
            return self.proj(pooled)

    text_export = TextTowerExport(model.text, remap_tensor)
    text_export.eval()

    dummy_ids = torch.randint(0, original_vocab_size, (1, 32))
    dummy_mask = torch.ones(1, 32, dtype=torch.long)
    with torch.no_grad():
        text_out = text_export(dummy_ids, dummy_mask)
    if tuple(text_out.shape) != (1, EMBED_DIM):
        raise SystemExit(f"unexpected text tower output: {tuple(text_out.shape)}")

    log("ONNX export: text tower (pruned vocabulary)")
    torch.onnx.export(
        text_export,
        (dummy_ids, dummy_mask),
        str(args.out / "text_fp32.onnx"),
        opset_version=args.opset,
        input_names=["input_ids", "attention_mask"],
        output_names=["text_embeds"],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "sequence"},
            "attention_mask": {0: "batch", 1: "sequence"},
            "text_embeds": {0: "batch"},
        },
        dynamo=False,
    )

    # --- Dynamic int8 quantization (the candidate chosen by the bench).
    # Confirmed on real Rust numbers (an IT/EN same-harness bench against
    # MobileCLIP2-S2, after fixing a double vocabulary remap that had
    # skewed the first numbers): quality identical to MobileCLIP2-S2 (EN
    # r@1=1.00, IT r@1=0.95, the same single missed caption), ~3x faster.
    # An fp16 comparison of the visual tower alone was tried and discarded
    # afterward — an explicit decision to stick with int8 without further
    # iteration, not a technical problem with fp16 itself. Static QDQ
    # remains unexplored. ---
    # `quant_pre_process` (shape inference + constant folding) before the
    # actual quantization: recommended by the tool itself (otherwise a
    # runtime warning), not an optional step added arbitrarily. On the REAL
    # graph (not a synthetic one) `SymbolicShapeInference` fails with
    # `AssertionError: assert is_literal(shape_rank)` inside its own
    # `Reshape` node handler — a known limitation of the tool on graphs
    # with a dynamic batch axis (`dynamic_axes` above), not an error in the
    # exported graph. `skip_symbolic_shape=True` disables only that
    # advanced step, keeping the base ONNX shape inference and constant
    # folding — verified here for the first time against the real graph
    # (not testable earlier: it needed the real export to reproduce). If
    # even this failed, quantizing without preprocessing is preferred over
    # blocking the entire export for a step that is "recommended", not
    # "required" — to be revisited if post-quantization quality suffers
    # (measure it, don't assume).
    def preprocess_then_quantize(fp32_path: Path, preproc_path: Path, int8_path: Path) -> None:
        try:
            quant_pre_process(str(fp32_path), str(preproc_path), skip_symbolic_shape=True)
            source = preproc_path
        except Exception as e:  # pylint: disable=broad-except
            log(f"WARNING: quant_pre_process failed ({e!r}), quantizing without preprocessing")
            source = fp32_path
        quantize_dynamic(str(source), str(int8_path), weight_type=QuantType.QInt8)

    log("pre-processing + dynamic int8 quantization: visual")
    preprocess_then_quantize(
        args.out / "visual_fp32.onnx", args.out / "visual_preproc.onnx", args.out / "visual.onnx"
    )
    log("pre-processing + dynamic int8 quantization: text")
    preprocess_then_quantize(
        args.out / "text_fp32.onnx", args.out / "text_preproc.onnx", args.out / "text.onnx"
    )
    for tmp in ("visual_fp32.onnx", "text_fp32.onnx", "visual_preproc.onnx", "text_preproc.onnx"):
        path = args.out / tmp
        if path.is_file():
            path.unlink()

    # --- Tokenizer: NOT pruned (same segmentation for any input, see
    # docstring) — copied as-is, the Rust consumer uses it to tokenize,
    # then applies the remap itself before feeding text.onnx. The file
    # must be copied from the real HF tokenizer (`tokenizers`-compatible
    # format) — save_pretrained already writes it in that format. ---
    log("saving tokenizer (not pruned) and remap table")
    if hasattr(hf_tokenizer, "save_pretrained"):
        hf_tokenizer.save_pretrained(str(args.out / "tokenizer_hf"))
        tok_json = args.out / "tokenizer_hf" / "tokenizer.json"
        if tok_json.is_file():
            tok_json.rename(args.out / "tokenizer.json")
        else:
            raise SystemExit(
                f"{tok_json} not produced by save_pretrained — this checkpoint's "
                "HF tokenizer might not be based on `tokenizers` (the Rust "
                "`tokenizers` crate needs a fast tokenizer, not a slow/"
                "sentencepiece-only one): verify by hand before proceeding."
            )

    remap_out = {
        "original_vocab_size": original_vocab_size,
        "pruned_vocab_size": len(remap),
        "unk_new_index": unk_new,
        "pad_token_id": model.text.transformer.config.pad_token_id if hasattr(model.text.transformer.config, "pad_token_id") else None,
        # Sparse: only the ids that were KEPT. Any missing id = unk_new_index.
        # With roughly 10-20% of 250,002 rows kept (the real number is only
        # known after an actual run), a sparse dict is lighter than a dense
        # array of 250,002 integers — not a premature optimization, the
        # difference is orders of magnitude on a file that has to live in
        # git or the CI cache.
        "kept": {str(old): new for old, new in remap.items()},
    }
    (args.out / "id_remap.json").write_text(json.dumps(remap_out), encoding="utf-8")

    # Real `max_position_embeddings` of the text transformer: the position
    # table only exists up to that index, the ONNX graph has a dynamic
    # sequence axis (no limit imposed by the graph itself) but feeding it
    # beyond this number would make inference fail — the Rust consumer must
    # truncate tokenization to this value, read from the real config, not a
    # guessed constant.
    text_max_position_embeddings = model.text.transformer.config.max_position_embeddings

    manifest = {
        "checkpoint_model": CHECKPOINT_MODEL,
        "checkpoint_tag": CHECKPOINT_TAG,
        "embed_dim": EMBED_DIM,
        "image_size": IMAGE_SIZE,
        "image_mean": image_mean,
        "image_std": image_std,
        "text_max_position_embeddings": text_max_position_embeddings,
        "opset": args.opset,
        "quantization": "dynamic_int8",
    }
    (args.out / "export_manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    for name in ("visual.onnx", "text.onnx", "tokenizer.json", "id_remap.json"):
        path = args.out / name
        if not path.is_file():
            raise SystemExit(f"missing artifact after export: {path}")
        sha = hashlib.sha256(path.read_bytes()).hexdigest()
        log(f"{name}: {path.stat().st_size} bytes, sha256={sha}")

    log(f"✓ export complete in {args.out}")


if __name__ == "__main__":
    main()
