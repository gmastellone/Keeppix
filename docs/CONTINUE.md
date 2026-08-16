# Prompt di continuazione — Keeppix

Incolla **tutto questo file** come primo messaggio di una sessione nuova
(Cursor, Codex, Claude Code, altro). Non riassumere: il modello deve avere
lo stesso contesto, non una versione diluita.

---

Sei un agente che riprende Keeppix. Non aprire la Fase 3. Non fare merge su
`main` e non fare push/PR se l'utente non lo chiede (salvo aggiornare una
PR draft già richiesta).

Keeppix è una galleria fotografica self-hosted (Rust + Vue). Il documento che
comanda il tuo comportamento è `AGENTS.md` nella root: **invarianti prima del
giudizio**. Se spec e piano divergono, vince la spec; annota il ruling nel
ledger.

## Snapshot (2026-08-16)

- **Branch di lavoro:** `fase-2` (traccia `origin/fase-2`). Non lavorare su `main`.
- **HEAD al handoff:** vedi `git log -1` su `fase-2` (chiusura Fase 2 + fix
  `RLIMIT_AS` video `20eafc6`).
- **Fase 0 + 1:** su `main` (o integrate).
- **Fase 2:** implementata e **chiusa** sul branch `fase-2` (9/9 task + fix
  poster). Consegna:
  `docs/superpowers/plans/2026-08-15-keeppix-fase-2-STATO.md`
- **Fase 3+:** non iniziate. Vietato anticiparle.

## Cosa fare adesso (in quest'ordine)

1. `git checkout fase-2 && git pull` (no force-push).
2. Leggere, in quest'ordine:
   - `AGENTS.md`
   - `docs/superpowers/plans/2026-08-15-keeppix-fase-2-STATO.md` ← **consegna
     corrente**
   - `docs/superpowers/plans/2026-08-13-keeppix-roadmap.md`
   - `docs/superpowers/specs/2026-08-13-keeppix-design.md`
   - ledger `.superpowers/sdd/2026-08-15-keeppix-fase-2/progress.md`
3. Solo se l'utente lo chiede: review, PR verso `main`, CI, merge.
4. Se la suite è rossa: fix minimi TDD. Non «sistemare» fuori dal fallimento.

**Non** aprire Fase 3 (sharing, album, permessi) finché l'utente non lo chiede.

## Cosa c'è già in Fase 2 (non rifarla)

| Pezzo | Note |
|---|---|
| Preview RAW embedded (TIFF/BMFF, no rawler) | 1–6 ms; cascade ≥1440 senza demosaic |
| `derive_from_bytes` + job `DeriveRaw` | fallback `dcraw_emu` sandbox |
| `asset_overrides` + `asset_flags` | EXIF immutabile; rating/pick per utente |
| Sidecar XMP atomici (`quick-xml`) | campi estranei preservati |
| Stack RAW+JPEG (regola basename) | regola 2s/camera differita |
| Trash 3 opzioni + `.keeppix-trash/` escluso dallo scan | |
| Duplicati + batch flags/overrides | 5000 apply ~57 ms release |
| Frontend culling lazy `/culling` | ingresso da Timeline; ~80 KB gzip iniziale |
| ffmpeg poster sandbox | `RLIMIT_AS` **1 GiB** (512 era troppo basso su Ubuntu) |

Ledger: `.superpowers/sdd/2026-08-15-keeppix-fase-2/progress.md` (`git add -f`).

## Aperto / differito (vedi STATO)

- Stack rule 2; XMP sweep solo su override; trash cleanup non schedulato
- Culling senza UI cartella/selezione; zoom 1:1 RAW puro
- Conferma manuale ≥100 foto + hash RAW invariati; CI/PR; compose smoke
- WS nativo `Authorization` senza Origin (Fase app)

## Invarianti (difetto grave se li violi)

Sono in `AGENTS.md`. In più, specifici Fase 2: **RAW mai riscritti**;
`asset_exif` immutabile; scritture file atomiche; decoder C in sandbox.

## Metodo

TDD vero. Commit convenzionali **in inglese**. Ruling nel ledger.
Fermati per: azioni distruttive, push/merge/PR, info solo dell'utente.

## Fuori scope finché non te lo dicono

Sharing/album/permessi (Fase 3), mappa (Fase 4), tus/WebDAV (Fase 5),
video avanzato/backup (Fase 6), seccomp, fan-out WS dai job.
