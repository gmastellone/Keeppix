# Keeppix Fase 1c — Timeline, API e frontend

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Navigare dal browser la libreria indicizzata: timeline a scrollbar esatta, ricerca, miniature dei bucket visibili per prime.

**Architecture:** `TimelineRepo` legge `folder_month_counts` (visibilità sui prefissi `ltree`) e pagina gli asset con keyset. Gli handler HTTP non scrivono SQL. Il WebSocket è solo notifica; la verità resta REST + `/sync/delta`. Il frontend chiede i bucket visibili; il backend promuove i job `derive:*` a priorità Visible.

**Tech Stack:** Rust 1.88 · Axum 0.8 · sqlx 0.8 · Vue 3 + Vite + Tailwind v4 · Reka UI · testcontainers

**Spec:** [`../specs/fase-1c-timeline.md`](../specs/fase-1c-timeline.md) — **vince sul piano**
**Design:** [`../specs/2026-08-13-keeppix-design.md`](../specs/2026-08-13-keeppix-design.md)
**1b STATO:** [`2026-08-14-keeppix-fase-1b-STATO.md`](2026-08-14-keeppix-fase-1b-STATO.md)
**PR:** bozza [#3](https://github.com/gmastellone/Keeppix/pull/3) — CI; merge solo a fine Fase 1

## Global Constraints

- SQL solo in `keeppix-db`. Handler → repository con `AuthContext` primo parametro.
- Sondare un id altrui → `Forbidden`, mai `NotFound`.
- Errori RFC 9457 `keeppix/…`. Backend non traduce.
- `/api/v1` solo aggiunte. `keeppix_api::Json<T>`, non `axum::Json`.
- `.fallback` **prima** di `with_common_layers`.
- Nessun percorso filesystem dal client: media per `id` o `content_hash`.
- Cookie `__Host-kpx_session` con `Secure` incondizionato.
- Bundle iniziale ≤ 150 KB gzip (chunk lazy fuori budget).
- i18n: stesse chiavi in `it.json` e `en.json`.
- TDD, clippy `-D warnings`, `fmt`, frontend build, `cargo test --workspace -- --test-threads=1`.
- Dopo i test con testcontainers: **spegnere e rimuovere i container**.
- Branch `fase-1`. Push ok. Merge su `main` **solo a fine Fase 1**, dopo i test complessivi (sotto).
- Niente Fase 2 (rating persistenti, RAW, culling) né 3–5.

## A fine Fase 1, prima del merge (come Fase 0)

Non è un task 1c: è il cancello di merge, da eseguire dopo il STATO 1c.

1. `cd frontend && npm ci && npm run build`
2. `cargo test --workspace -- --test-threads=1`
3. `cargo clippy --workspace --all-targets -- -D warnings`
4. `cargo fmt --check`
5. `cargo deny check advisories bans licenses`
6. `docker build` + `docker compose --profile bundled` + health, setup, login, frontend dal binario (stesso elenco del STATO Fase 0)
7. Spegnere/rimuovere i container di test

## Cosa NON è in 1c (ruling)

- **HLS** `/media/video/{id}/hls`: in 1b non c'è transcodifica all'ingest. L'endpoint non si aggiunge; si fa quando esiste un transcodificato (Fase 2 o on-demand).
- **`rating:` nella ricerca:** `asset_flags` è Fase 2.
- **Service worker** offline: se il budget bundle o il tempo lo escludono, si differisce nel STATO.
- **moka** per albero/conteggi: in 1c basta la cache sessioni in `Auth` (TTL 30 s o invalidazione su revoke). moka per il resto se un test di carico lo chiede, non prima.

---

## Task 1: Trigger `folder_month_counts`

La tabella esiste (1a, `0004`). Il commento dice «mantenuti da trigger in 1c». Senza trigger la scrollbar è vuota.

**Files:**
- Create: `crates/keeppix-db/migrations/0009_month_counts_trigger.sql`
- Create: `crates/keeppix-db/tests/month_counts.rs`
- Touch: `crates/keeppix-db/src/lib.rs` (sqlx::migrate!)

**Regole (spec §1.2):**
- Conta solo `status = 'indexed'` con `taken_at_utc IS NOT NULL`.
- `month = date_trunc('month', taken_at_utc)::date`.
- `INSERT` incrementa; `DELETE` decrementa (e cancella la riga a 0).
- `UPDATE` di `taken_at_utc`, `folder_id` o `status` sposta il conteggio.
- Test: dopo una sequenza insert/index/move/delete, `sum(asset_count) = count(*) FROM assets WHERE status='indexed' AND taken_at_utc IS NOT NULL`.

- [ ] **Commit** `feat(db): keep folder month counts in sync with assets`

---

## Task 2: `TimelineRepo`

**Files:**
- Create: `crates/keeppix-db/src/timeline.rs`
- Modify: `crates/keeppix-db/src/lib.rs` — `pub mod timeline`
- Create: `crates/keeppix-db/tests/timeline.rs`

**Interfaces:**
- `buckets(ctx, library_id: Option<LibraryId>) -> Vec<{month: NaiveDate, count: i64}>` — SQL spec §1, visibilità `VisibilityScope`. Sondare una library altrui → Forbidden.
- `page(ctx, bucket: NaiveDate, cursor: Option<(DateTime<Utc>, AssetId)>, limit: i64) -> Vec<Asset>` — keyset spec §1.1, `status='indexed'`, `ORDER BY taken_at_utc DESC, id DESC`, `LIMIT` clamp 1..=200.

Niente `OFFSET`.

- [ ] **Commit** `feat(db): add timeline repository with month buckets and keyset pages`

---

## Task 3: HTTP timeline + cartelle

**Files:**
- Create: `crates/keeppix-api/src/routes/timeline.rs`, `folders.rs`
- Modify: `routes/mod.rs`, `lib.rs` router
- Create: `crates/keeppix-api/tests/timeline.rs`
- OpenAPI: aggiornare `docs/api/openapi.json` nello stesso task (CI `git diff --exit-code`)

**Endpoints:**
- `GET /api/v1/timeline/buckets`
- `GET /api/v1/timeline?bucket=2024-07&cursor=`
- `GET /api/v1/folders/tree`
- `GET /api/v1/folders/{id}/children`

`Auth` extractor. problem+json. Forbidden-not-NotFound su id cartella.

- [ ] **Commit** `feat(api): serve timeline buckets and the folder tree`

---

## Task 4: Media + fallback SPA

**Files:**
- Create: `crates/keeppix-api/src/routes/media.rs`
- Modify: `crates/keeppix-server/src/embed.rs` — escludere `media/` e `dav/` dal fallback (spec §3.1)
- Modify: `keeppix-server` `AppState` / main se serve `data_dir` per i derivati
- Tests: hash sconosciuto → Forbidden (non oracolo); thumb esistente → `Cache-Control: public, max-age=31536000, immutable`; original per `id` con range.

`AppState` oggi ha solo `db` + `session_ttl`. Aggiungere `data_dir: PathBuf` (serve ai derivati). È il punto unico: non far arrivare path dal client.

- [ ] **Commit** `feat(api): stream thumbs and originals by hash or id`

---

## Task 5: Viewport → promote

**Files:**
- Create: `crates/keeppix-api/src/routes/viewport.rs`
- Hook: `JobRepo::promote` con chiavi `derive:{hex}` degli hash visibili, priorità `Visible` (2).
- `ActivityTracker::notify_authenticated_request` su ogni `Auth` riuscito (gancio 1b).

Test: enqueue derive background, POST viewport, il job ha `priority <= 2`.

- [ ] **Commit** `feat(api): promote visible derivative jobs from the viewport`

---

## Task 6: Ricerca

Parser **nel frontend** (AST). Il backend riceve JSON strutturato, **mai** la stringa interpolata in SQL.

**Files:**
- Create: `crates/keeppix-db/src/search.rs` — `SearchRepo::run(ctx, ast, cursor, limit)`
- Create: `crates/keeppix-api/src/routes/search.rs` — `POST /api/v1/search`
- Create: `frontend/src/search/parse.ts` + test vitest (and/or/not, `type:`, `camera:`, `iso:>`, anno, `has:gps`, `folder:`)
- `GET /api/v1/search/suggest?q=` — prefissi su `camera_model` / filename, visibili
- Migrazione `00010_saved_searches.sql` + `POST/GET /api/v1/saved-searches` minimi (lista + crea)

`pg_trgm`: se l'estensione non è in `0001`, aggiungerla in `00010` (CREATE EXTENSION IF NOT EXISTS). Verificare prima.

- [ ] **Commit** `feat(api): search from a structured ast without interpolating sql`

---

## Task 7: WebSocket

**Files:**
- Create: `crates/keeppix-api/src/routes/ws.rs`
- `POST /api/v1/ws/ticket` → ticket 30 s monouso (tabella o cache in-process)
- `GET /api/v1/ws` con `Sec-WebSocket-Protocol: keeppix.v1, ticket.<t>`
- Origin allowlist (config). Rifiuto senza Origin valido.
- Coda 256; overflow → un `resync`. Coalescing `scan.progress` 250 ms.
- Eventi filtrati con lo stesso `VisibilityScope`.
- Heartbeat 30 s. `permessage-deflate` off.
- Test: ticket riusato → 403; Origin sbagliato → close; overflow → `resync`.
- Nota nginx in `docs/DEPLOY.md` (blocco spec §4.7).

Il client mobile può usare `Authorization` — se in 1c non c'è client mobile, il test copre il cookie+ticket.

- [ ] **Commit** `feat(api): notify over websocket with a one-shot ticket`

---

## Task 8: Cache sessioni in `Auth`

`Auth::from_request_parts` oggi query per request. Cache con TTL 30 s **e** invalidazione in `revoke`/`rotate` (spec §3.3: non solo TTL).

Test: dopo revoke il cookie non passa più, anche se la cache aveva la sessione.

- [ ] **Commit** `feat(api): cache authenticated sessions and drop them on revoke`

---

## Task 9: Frontend — timeline

**Files:**
- `frontend/src/views/TimelineView.vue` (Home diventa la timeline)
- Griglia giustificata, header giorno/mese, thumbhash placeholder, chip Tutti|Foto|Video
- Scrubber: logica TS isolata + test (porte da urocissa, senza Vuetify)
- `POST /viewport` quando i bucket entrano nel viewport
- Densità 2–12, salvata in `localStorage`
- i18n it+en
- Distinguere 503 vs 401 nel bootstrap (spec §5.6) — oggi pagina bianca

Budget: misurare gzip del chunk iniziale.

- [ ] **Commit** `feat(web): render a justified timeline with thumbhash placeholders`

---

## Task 10: Frontend — ricerca, visualizzatore, problemi

- Barra ricerca + chip; parser del Task 6
- Visualizzatore fullscreen, swipe, `i` info; **solo** azioni che esistono (niente rating persistente). Preferito se `asset_flags` non c'è: non fingere, omettere.
- `GET /problems` + pagina: librerie offline, job `failed`, asset `error`
- `GET /duplicates` gruppi `content_hash` count>1 (spazio recuperabile = size*(n-1))
- Menu Album disabilitato o assente (Fase 3)

- [ ] **Commit** `feat(web): search, viewer, and a problems page`

---

## Task 11: Integrazione + STATO

- Test API: buckets di una libreria con 3 foto del fixture 1b (o seed) → scrollbar counts; page keyset.
- Spec 1c stato → chiusa.
- `docs/superpowers/plans/2026-08-14-keeppix-fase-1c-STATO.md`
- Ledger `.superpowers/sdd/2026-08-14-keeppix-fase-1c/progress.md`
- **Non mergiare.** I test complessivi Fase 0-style sono il passo successivo, dichiarato in questo piano.

- [ ] **Commit** `docs: record Fase 1c handoff`

---

## Criteri di completamento 1c

- [ ] Workspace verde, clippy, fmt, frontend build, deny bans.
- [ ] Timeline: buckets + keyset, somma conteggi = indexed.
- [ ] Media: thumb immutable; original per id; SPA non serve `index.html` su `/media/*`.
- [ ] Search POST da AST, zero interpolazione SQL.
- [ ] WS: ticket monouso, Origin, resync su overflow.
- [ ] Griglia nel browser con thumbhash, viewport promote.
- [ ] STATO.md. Niente merge su `main` in questo task.
