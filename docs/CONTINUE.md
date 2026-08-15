# Prompt di continuazione — Keeppix

Incolla **tutto questo file** come primo messaggio di una sessione nuova
(Cursor, Codex, Claude Code, altro). Non riassumere: il modello deve avere
lo stesso contesto, non una versione diluita.

---

Sei un agente che riprende Keeppix. Non ricominciare la Fase 1. Non aprire la
Fase 2. Non fare merge su `main` e non fare push/PR se l'utente non lo chiede.

Keeppix è una galleria fotografica self-hosted (Rust + Vue). Il documento che
comanda il tuo comportamento è `AGENTS.md` nella root: **invarianti prima del
giudizio**. Se spec e piano divergono, vince la spec; annota il ruling nel
ledger.

## Snapshot (2026-08-14)

- **Branch di lavoro:** `fase-1` (traccia `origin/fase-1`). Non lavorare su `main`.
- **HEAD al handoff:** `ba52295` (`docs: record Fase 1c handoff`).
- **PR:** bozza https://github.com/gmastellone/Keeppix/pull/3 — esiste perché
  la CI giri. Resta draft.
- **Fase 0:** su `main`.
- **Fase 1a + 1b + 1c:** implementate su `fase-1`. 1c **chiusa** (11/11 task).
- **Fase 2+:** non iniziate. Vietato anticiparle «perché ci vuole poco».

## Cosa fare adesso (in quest'ordine)

1. `git checkout fase-1 && git pull` (no force-push). Se ci sono commit
   locali non pushati con i fix della review Opus 5 (cursore, bucket UTC,
   stream media, timeline client, …), restano sul working tree finché
   l'utente non chiede il commit.
2. Leggere, in quest'ordine:
   - `AGENTS.md`
   - `docs/superpowers/plans/2026-08-14-keeppix-fase-1c-STATO.md` ← **consegna
     corrente**
   - `docs/superpowers/plans/2026-08-13-keeppix-roadmap.md`
   - `docs/superpowers/specs/2026-08-13-keeppix-design.md`
   - spec della cosa che stai toccando
3. **Cancello di merge Fase 1** (non è un task 1c; è il passo successivo).
   Completarlo e annotare l'esito. Comandi:

   ```bash
   cd frontend && npm ci && npm run build
   cd .. && cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   ./scripts/test.sh
   cargo deny check advisories bans licenses
   docker build -t keeppix:dev .
   # poi compose bundled: health, setup, login, frontend dal binario
   # come in docs/superpowers/plans/2026-08-13-keeppix-fase-0-STATO.md
   # «Verifica Docker, eseguita»
   ```

   `--test-threads=1` e `--jobs 1` sono nello script. `frontend/dist` serve
   alla **compilazione** (`rust-embed`), non solo ai test. Lo script, anche
   se i test falliscono, fa `docker rm -f` dei testcontainers e `cargo clean`.
   Non usare `cargo test --workspace` a mano.
4. Se la suite è rossa: analizza con un modello più capace se l'utente lo
   chiede; **i fix** li fai tu, minimi, TDD. Non «sistemare» codice fuori
   dal fallimento.
5. Solo se l'utente lo chiede esplicitamente: togliere il draft dalla PR #3
   e mergiare su `main`. Altrimenti fermati.

**Non** aprire Fase 2 (RAW, XMP, culling, rating persistenti) finché il
cancello sopra non è chiuso e l'utente non lo chiede.

## Cosa c'è già (non rifarla)

| Pezzo | Dove / note |
|---|---|
| Auth, cookie `__Host-kpx_session` sempre `Secure` | Fase 0 |
| Librerie, cartelle `ltree`, asset `(folder_id, filename)` | 1a |
| EXIF immutabile, `asset_overrides` ancora assente in UI | 1a; override è modello, editing utente è Fase 2 |
| Job `SKIP LOCKED`, discover → EXIF → blake3 → thumb WebP lossless + thumbhash | 1b |
| ffmpeg in processo figlio con `rlimit` (niente seccomp) | 1b |
| Trigger `folder_month_counts`, `TimelineRepo` keyset | 1c |
| HTTP timeline, cartelle, media `/media/thumb\|preview/{hash}`, `/media/original/{id}` | 1c. Hash sconosciuto → **403** anche admin |
| `POST /viewport` promuove `derive:{hex}` a priorità Visible | 1c |
| Search AST (no SQL interpolato), saved searches, parser TS a mano | 1c. Niente Chevrotain, niente `rating:` |
| WS: `POST /api/v1/ws/ticket` 30 s monouso; `GET /api/v1/ws` con `keeppix.v1, ticket.<t>`; Origin; coda 256 → `resync` | Ticket consumato in `FromRequestParts` **prima** dell'upgrade. Fan-out dai worker **non** cablato: heartbeat + close se testo >64 KB |
| Cache sessioni 30 s, drop su logout/refresh | Gemelli di famiglia possono restare in cache fino al TTL |
| Frontend: timeline giustificata, thumbhash, densità 2–12, chip Tutti/Foto/Video, search, viewer (`i` info, niente rating finti), `/problems` | Album **assente**. Selezione multipla e pinch **non** fatti |
| `GET /problems`, `GET /duplicates` | Job failed **solo admin** |

Commit 1c rilevanti: `be786a8` … `ba52295`. Ledger:
`.superpowers/sdd/2026-08-14-keeppix-fase-1c/progress.md` (`git add -f`).

## Invarianti (difetto grave se li violi)

Sono in `AGENTS.md`. I più facili da rompere riprendendo a freddo:

- SQL solo in `crates/keeppix-db`. `keeppix-media` non conosce il DB.
- Ogni repo che legge dati utente: `AuthContext` primo parametro. Niente
  helper HTTP che fabbricano un `AuthContext`. Eccezioni già documentate
  nel codice (scanner, `JobRepo`, `count`, …): non aggiungerne senza doc
  comment con il motivo.
- Sondare un id altrui → `Forbidden`, mai `NotFound`.
- Query parametrizzate. sqlx solo `query` / `query_as` + `FromRow`. Mai
  macro `query!`, mai `.sqlx/`, mai `SQLX_OFFLINE`.
- Nessun `unwrap`/`expect` in produzione.
- Nessun percorso filesystem dal client.
- Errori RFC 9457 `application/problem+json`, `type` prefissato `keeppix/`.
  Backend non traduce.
- `/api/v1` solo aggiunte. `keeppix_api::Json<T>`, non `axum::Json`.
- `.fallback(...)` **prima** di `with_common_layers(...)`.
- Cookie di sessione: `Secure` incondizionato. Non reintrodurre logica
  sull'host (R7).
- Identità asset = `(folder_id, filename)`. `content_hash` non unico.
- Nessun path assoluto denormalizzato sugli asset.
- RAW non si riscrivono (quando arriveranno: sidecar `.xmp`).

## Metodo

TDD vero: test che fallisce → osservi il fail → minimo che lo fa passare.
Chiediti: *se rompo di proposito la cosa che questo test protegge, fallisce?*

Commit convenzionali **in inglese**, uno per unità logica. Il corpo spiega
**perché**. Non committare se l'utente non lo chiede, salvo che `AGENTS.md`
della fase in corso pretenda un commit per task — in quel caso sì, e in
inglese.

Ruling nel ledger se decidi qualcosa che il piano non specifica.

Fermati e chiedi solo per: azioni distruttive, push/merge/PR, o quando ogni
strada richiede un'informazione che solo l'utente ha.

## Fuori scope finché non te lo dicono

HLS, rating persistenti, preferiti, culling, RAW/XMP, album, sharing, mappe,
upload tus, WebDAV, service worker, moka oltre la cache sessioni, seccomp,
`permessage-deflate`, fan-out WS dai job.

## Dove sta il codice che toccherai nel cancello di merge

- Test workspace e clippy: tutto `crates/`
- Bundle ≤ 150 KB gzip del chunk **iniziale** (CI misura solo ciò che
  `dist/index.html` carica; i chunk lazy sono fuori budget). Oggi ~79 KB.
- Docker: `Dockerfile`, `compose.yaml`, `docs/DEPLOY.md`
- OpenAPI: `docs/api/openapi.json` — se aggiungi una rotta, `UPDATE_OPENAPI=1
  cargo test -p keeppix-api --test openapi …`

## Messaggio all'utente, quando hai finito il cancello

Di' se la suite è verde, cosa hai rimosso in Docker, e **non** mergiare.
Chiedi se vuole la PR pronta e il merge su `main`.
