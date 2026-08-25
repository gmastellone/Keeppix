#!/usr/bin/env python3
"""Esporta OpenCLIP XLM-R ViT-B-32 (checkpoint laion5b_s13b_b90k) in ONNX,
potato al vocabolario IT/EN e quantizzato int8 — Task B del piano modelli IA
(docs/superpowers/plans/2026-08-22-keeppix-modelli-ai.md), sostituisce
MobileCLIP2-S2 (research-only, mai idoneo a un'offerta commerciale).

Unico punto della pipeline dove gira Python (vincolo esplicito del piano,
punto B: "Python è ammesso SOLO nello script di export offline"). Tutto il
resto — caricamento, inferenza, bench di regressione IT/EN, misure RSS/ms —
vive in Rust (`keeppix-media`, `ai_retrieval_bench`).

Richiede rete verso huggingface.co (il checkpoint è ospitato SOLO lì, nessun
mirror dichiarato in `open_clip.pretrained.get_pretrained_cfg`): non
eseguibile nella sandbox di sviluppo di questa sessione (bloccata a livello
di proxy, stesso limite già noto per MobileCLIP2 in Fase 7 — vedi
`models/README.md`). Le due metà dell'architettura sono state verificate
separatamente offline in quella sandbox, prima di scrivere questo file:
  - torre visione (ViT-B-32, nessuna config HF necessaria): costruita con
    pesi casuali via `open_clip.create_model_and_transforms`, esportata in
    ONNX con successo, output 512-d confermato.
  - torre testo (XLMRobertaModel, architettura pubblica nota — vocab 250002,
    hidden 768, 12 layer, 12 teste, intermediate 3072): costruita con pesi
    casuali via `transformers.XLMRobertaModel(XLMRobertaConfig(...))`,
    esportata in ONNX con successo con torch 2.13 / transformers correnti,
    **senza bisogno** del workaround `torch.backends.mha.set_fastpath_enabled
    (False)` documentato nel piano (probabile: quel sintomo era specifico
    delle versioni di torch/transformers del 22 agosto — qui sotto il flag
    resta comunque impostato in modo difensivo, costo zero, copre entrambi
    i casi). Logica di pooling/proiezione (`MeanPooler` mascherato +
    `nn.Linear(768, 512, bias=False)`) letta da `open_clip/hf_model.py` e
    `open_clip/hf_configs.py` (`arch_dict['xlm-roberta']`) installati
    localmente — codice sorgente, non pesi, quindi nessuna rete richiesta
    per leggerlo.
  - `proj_type` di QUESTO specifico checkpoint non verificato (il suo
    config.json è su HF, irraggiungibile qui): lo script legge il valore
    reale da `model.text.proj` a runtime e fallisce esplicitamente se non è
    `nn.Linear` — vedi `assert_real_proj_is_linear` sotto — invece di
    assumere in silenzio la forma sbagliata.
  - La normalizzazione L2 finale **non** è nel grafo esportato, per lo
    stesso motivo di MobileCLIP2: `crates/keeppix-media/src/clip.rs` la fa
    in Rust (`l2_normalize`) dopo l'estrazione del tensore — verificato
    leggendo quel file. Il grafo qui esporta la proiezione grezza.

Uso:
  python3 scripts/export-openclip-xlmr-it-en.py --out models/openclip-xlmr-it-en

Pacchetti richiesti (non pinnati a una versione esatta di proposito: lo
script fallisce esplicitamente se un'API assunta non esiste, invece di
un errore criptico a metà — vedi i controlli `hasattr`/`assert` sotto):
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
# Margine di token per ogni parola della lista di frequenza: una parola vera
# spesso si spezza in più subword unigram (prefissi/suffissi), non un solo
# id. 20.000 parole comuni per lingua è lo stesso ordine di grandezza usato
# per stimare la copertura pratica in altri lavori di potatura vocabolario
# multilingua->bilingue; non un numero preso a caso, ma nemmeno una scienza
# esatta — se il bench di regressione (Task B, confronto pre/post potatura)
# mostra un calo di recall, il primo numero da alzare è questo.
WORDFREQ_TOP_N = 20_000


def log(msg: str) -> None:
    print(f"→ {msg}", file=sys.stderr, flush=True)


def build_it_en_corpus(captions_path: Path) -> list[str]:
    """Corpus per la potatura del vocabolario: parole comuni IT/EN (via
    `wordfreq`, dati imbustati nel pacchetto — zero rete a runtime, a
    differenza di un dump Wikipedia) più le frasi reali del banco Task 2bis
    (`captions.json`, già nel repo) — copre sia il vocabolario isolato sia
    la segmentazione di frasi vere, che per un tokenizer SentencePiece può
    dividere le parole diversamente dal solo lessico.
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
        log(f"ATTENZIONE: {captions_path} non trovato, corpus solo da wordfreq (nessuna frase reale)")
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
    """Costruisce la matrice di embedding potata (righe: solo gli id usati
    dal corpus IT/EN più gli id speciali) e la mappa id-originale ->
    indice-nuovo. Gli id NON tenuti restano tokenizzabili (il tokenizer non
    cambia: stessa segmentazione per qualunque lingua, non solo IT/EN,
    coerente col vincolo A del piano — "le altre 107 lingue non sono un
    requisito: se la potatura le rompe, va bene", qui semplicemente le loro
    parole finiscono sul fallback), ma a runtime vengono rimappati
    sull'indice di `<unk>` — degradazione esplicita, non un crash.
    """
    import torch

    embeddings = text_tower.embeddings.word_embeddings
    vocab_size, hidden = embeddings.weight.shape
    unk_id = text_tower.config.unk_token_id if hasattr(text_tower.config, "unk_token_id") else None
    keep = sorted(used_ids | special_ids)
    if unk_id is not None and unk_id not in keep:
        keep.append(unk_id)
        keep.sort()
    log(f"vocabolario: {vocab_size} -> {len(keep)} righe tenute ({100 * len(keep) / vocab_size:.1f}%)")

    remap: dict[int, int] = {old: new for new, old in enumerate(keep)}
    unk_new = remap.get(unk_id, 0) if unk_id is not None else 0

    with torch.no_grad():
        pruned = embeddings.weight[keep].clone()

    return pruned, remap, unk_new, hidden


def apply_remap_for_export(input_ids, remap_tensor):
    """`remap_tensor` è un array denso [vocab_size_originale] -> nuovo
    indice (o l'indice di `<unk>` per gli id non tenuti), costruito una
    volta e portato dentro il grafo ONNX come costante — un `gather`, non un
    lookup Python: deve girare dentro `torch.onnx.export`, non prima."""
    return remap_tensor[input_ids]


def assert_real_proj_is_linear(text_tower) -> None:
    import torch.nn as nn

    proj = text_tower.proj
    if isinstance(proj, nn.Identity):
        raise SystemExit(
            "proj_type inatteso: nn.Identity (hidden_size == embed_dim?) — "
            "questo file assume una proiezione lineare 768->512. Verificare "
            "il config.json reale del checkpoint prima di procedere: la "
            "logica sotto va adattata, non ignorata."
        )
    if not isinstance(proj, nn.Linear):
        raise SystemExit(
            f"proj_type inatteso: {type(proj).__name__} (atteso nn.Linear). "
            "Verificare model.text.proj del checkpoint reale prima di procedere."
        )
    if proj.bias is not None:
        raise SystemExit(
            "proj lineare CON bias — questo file assume bias=False (osservato "
            "nella sorgente open_clip/hf_model.py per proj_type='linear'). "
            "Se il checkpoint reale ha un bias, aggiungerlo all'export sotto."
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
        "--opset", type=int, default=14, help="stesso opset già usato per MobileCLIP2-S2 (Fase 7)"
    )
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    import torch
    import open_clip
    from onnxruntime.quantization import quantize_dynamic, QuantType
    from onnxruntime.quantization.shape_inference import quant_pre_process

    torch.backends.mha.set_fastpath_enabled(False)  # difensivo, vedi docstring sopra

    log(f"caricamento {CHECKPOINT_MODEL} / {CHECKPOINT_TAG} da HuggingFace (richiede rete)")
    model, _, _ = open_clip.create_model_and_transforms(CHECKPOINT_MODEL, pretrained=CHECKPOINT_TAG)
    tokenizer_wrapper = open_clip.get_tokenizer(CHECKPOINT_MODEL)
    model.eval()

    if model.visual.image_size not in ((IMAGE_SIZE, IMAGE_SIZE), IMAGE_SIZE):
        raise SystemExit(
            f"image_size inatteso: {model.visual.image_size} (atteso {IMAGE_SIZE}) — "
            "verificare crates/keeppix-media/src/clip.rs (IMAGE_SIZE) prima di procedere."
        )
    if model.text.output_dim != EMBED_DIM:
        raise SystemExit(
            f"embed_dim inatteso: {model.text.output_dim} (atteso {EMBED_DIM}) — "
            "il piano dichiara 512-d, nessuna migrazione di schema prevista per "
            "un embed_dim diverso."
        )
    assert_real_proj_is_linear(model.text)

    # --- Torre visione: nessuna potatura, esporta così com'è. ---
    log("export ONNX: torre visione")
    visual = model.visual
    dummy_img = torch.randn(1, 3, IMAGE_SIZE, IMAGE_SIZE)
    with torch.no_grad():
        visual_out = visual(dummy_img)
    if tuple(visual_out.shape) != (1, EMBED_DIM):
        raise SystemExit(f"output torre visione inatteso: {tuple(visual_out.shape)}")
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

    # --- Corpus IT/EN e potatura vocabolario testo. ---
    log("costruzione corpus IT/EN per la potatura del vocabolario")
    # `open_clip.get_tokenizer` per un modello HF-based torna un `HFTokenizer`
    # (open_clip/tokenizer.py): `.tokenizer` è l'`AutoTokenizer` reale,
    # `.save_pretrained` delega a quello — letto dal sorgente installato,
    # non indovinato.
    if not hasattr(tokenizer_wrapper, "tokenizer"):
        raise SystemExit(
            f"tokenizer_wrapper inatteso: {type(tokenizer_wrapper).__name__} "
            "senza attributo `.tokenizer` — open_clip.get_tokenizer non ha "
            "tornato un HFTokenizer come atteso. Verificare a mano prima di "
            "procedere (l'API potrebbe essere cambiata)."
        )
    hf_tokenizer = tokenizer_wrapper.tokenizer
    corpus = build_it_en_corpus(args.captions)
    used_ids = tokens_used_by_corpus(hf_tokenizer, corpus)
    special_ids = set(hf_tokenizer.all_special_ids)
    log(f"corpus: {len(corpus)} voci, {len(used_ids)} id di token distinti prodotti")

    pruned_weight, remap, unk_new, hidden = prune_text_embedding(
        model.text.transformer, used_ids, special_ids
    )
    original_vocab_size = model.text.transformer.embeddings.word_embeddings.weight.shape[0]

    # Tensore di remap denso: id originale -> nuovo indice (fallback unk_new
    # per ogni id non tenuto). Diventa una costante nel grafo ONNX — un
    # gather, non un dizionario Python — quindi deve esistere PRIMA
    # dell'export, non essere applicato lato Rust dopo.
    remap_tensor = torch.full((original_vocab_size,), unk_new, dtype=torch.long)
    for old_id, new_id in remap.items():
        remap_tensor[old_id] = new_id

    # Sostituisce la tabella di embedding con quella potata: da qui in poi
    # il modello si aspetta id GIA' rimappati in ingresso (0..len(remap)-1),
    # non gli id originali XLM-R.
    with torch.no_grad():
        new_embedding = torch.nn.Embedding(len(remap), hidden, padding_idx=None)
        new_embedding.weight.copy_(pruned_weight)
    model.text.transformer.embeddings.word_embeddings = new_embedding
    model.text.transformer.config.vocab_size = len(remap)

    # --- Torre testo: wrapper che applica il remap PRIMA del transformer,
    # poi pooling mascherato + proiezione lineare — replica esatta di
    # HFTextEncoder.forward (open_clip/hf_model.py), con l'aggiunta del
    # remap in testa. ---
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
        raise SystemExit(f"output torre testo inatteso: {tuple(text_out.shape)}")

    log("export ONNX: torre testo (vocabolario potato)")
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

    # --- Quantizzazione int8 dinamica (il candidato scelto dal bench del
    # 22 agosto). QDQ statica / fp16 restano un confronto futuro (il piano
    # lo segnala esplicitamente: "provare QDQ statica o fp16 e scegliere
    # col numero") — richiede un giro di calibrazione con dati reali che
    # non ha senso scrivere alla cieca qui: si misura, poi si decide. ---
    # `quant_pre_process` (shape inference + constant folding) prima della
    # quantizzazione vera: raccomandato dallo strumento stesso (warning a
    # runtime altrimenti — verificato su un giro sintetico in questa
    # sessione), non un passo facoltativo aggiunto a caso.
    log("pre-processing + quantizzazione int8 dinamica: visual")
    quant_pre_process(str(args.out / "visual_fp32.onnx"), str(args.out / "visual_preproc.onnx"))
    quantize_dynamic(
        str(args.out / "visual_preproc.onnx"),
        str(args.out / "visual.onnx"),
        weight_type=QuantType.QInt8,
    )
    log("pre-processing + quantizzazione int8 dinamica: text")
    quant_pre_process(str(args.out / "text_fp32.onnx"), str(args.out / "text_preproc.onnx"))
    quantize_dynamic(
        str(args.out / "text_preproc.onnx"),
        str(args.out / "text.onnx"),
        weight_type=QuantType.QInt8,
    )
    for tmp in ("visual_fp32.onnx", "text_fp32.onnx", "visual_preproc.onnx", "text_preproc.onnx"):
        (args.out / tmp).unlink()

    # --- Tokenizer: NON potato (stessa segmentazione per qualunque input,
    # vedi docstring) — copiato così com'è, il consumatore Rust lo usa per
    # tokenizzare, poi applica lui stesso il remap prima di alimentare
    # text.onnx. Il file va copiato dal tokenizer HF reale (formato
    # `tokenizers`-compatibile) — save_pretrained lo scrive già in quel
    # formato. ---
    log("salvataggio tokenizer (non potato) e tabella di remap")
    if hasattr(hf_tokenizer, "save_pretrained"):
        hf_tokenizer.save_pretrained(str(args.out / "tokenizer_hf"))
        tok_json = args.out / "tokenizer_hf" / "tokenizer.json"
        if tok_json.is_file():
            tok_json.rename(args.out / "tokenizer.json")
        else:
            raise SystemExit(
                f"{tok_json} non prodotto da save_pretrained — il tokenizer "
                "HF di questo checkpoint potrebbe non essere basato su "
                "`tokenizers` (serve un fast tokenizer per il crate Rust "
                "`tokenizers`, non uno slow/sentencepiece-only): verificare "
                "a mano prima di procedere."
            )

    remap_out = {
        "original_vocab_size": original_vocab_size,
        "pruned_vocab_size": len(remap),
        "unk_new_index": unk_new,
        "pad_token_id": model.text.transformer.config.pad_token_id if hasattr(model.text.transformer.config, "pad_token_id") else None,
        # Sparso: solo gli id TENUTI. Qualunque id assente = unk_new_index.
        # Con ~10-20% di 250.002 righe tenute (numero reale solo dopo un
        # giro vero), un dizionario sparso pesa meno di un array denso da
        # 250.002 interi — non un'ottimizzazione prematura, la differenza è
        # ordini di grandezza sullo stesso file che deve stare in git o
        # nella cache CI.
        "kept": {str(old): new for old, new in remap.items()},
    }
    (args.out / "id_remap.json").write_text(json.dumps(remap_out), encoding="utf-8")

    manifest = {
        "checkpoint_model": CHECKPOINT_MODEL,
        "checkpoint_tag": CHECKPOINT_TAG,
        "embed_dim": EMBED_DIM,
        "image_size": IMAGE_SIZE,
        "opset": args.opset,
        "quantization": "dynamic_int8",
    }
    (args.out / "export_manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    for name in ("visual.onnx", "text.onnx", "tokenizer.json", "id_remap.json"):
        path = args.out / name
        if not path.is_file():
            raise SystemExit(f"artefatto mancante dopo l'export: {path}")
        sha = hashlib.sha256(path.read_bytes()).hexdigest()
        log(f"{name}: {path.stat().st_size} byte, sha256={sha}")

    log(f"✓ export completo in {args.out}")


if __name__ == "__main__":
    main()
