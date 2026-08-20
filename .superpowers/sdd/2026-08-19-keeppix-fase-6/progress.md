# Fase 6 — Consolidamento — Ledger

Branch: fase-6
Base commit (fase-5 tip): c6b14f2
Plan: docs/superpowers/plans/2026-08-19-keeppix-fase-6.md
Spec: docs/superpowers/specs/fase-6-consolidamento.md

## Decisioni e Ruling

Ruling: il probe hardware video prova i backend in ordine SoC-aware ma ricade
sempre su `Software` senza errore se nessun encoder hardware è disponibile —
la fase richiede una misura reale, non il fallimento dell'istanza su host senza
accelerazione. Costo se sbagliato: un host CPU-only non partirebbe o
richiederebbe override manuale per una configurazione valida.

Ruling: HLS cache lives on disk under `{data_dir}/video-cache/{hash}/{profile}/`
with mtime touched on access — Task 8 will reap entries older than 90 days from
filesystem mtimes, no DB table in Task 2 — cost if wrong: duplicate cache
tracking later.

Ruling: `save_bandwidth` is an explicit query param on playback/HLS routes,
never inferred from User-Agent — spec §1 — cost if wrong: mobile users get
wrong quality.

Ruling: `/sync/delta` resta in `wired-exceptions.txt` finché Task 9 non
genera/integra un client mobile consumatore; è un endpoint per sync
incrementale, non per la SPA web. Costo se sbagliato: CI `check-wired` rossa o
un fake caller solo per soddisfare la guardia.

Ruling (review follow-up): `/sync/delta` is now consumed by
`SyncProbeView` at `/settings/sync` — removed from `wired-exceptions.txt`.
The SPA probe is intentional until a real mobile client ships; it is not a
fake string match. Cost if wrong: a throwaway settings page to delete later.

Ruling: la generazione client OpenAPI usa Docker (`openapitools/openapi-generator-cli`)
anziché una nuova dipendenza npm del workspace. I client sono artefatti
rigenerabili sotto `docs/api/clients/` e restano ignorati dal VCS; si
committano solo lo snapshot `docs/api/openapi.json` e lo script di
generazione. Cost if wrong: lock-in su un tool locale/non portabile.

Ruling (review follow-up): CI job `api-clients` runs
`scripts/generate-api-clients.sh` and `tsc --noEmit` on the generated
TypeScript client every PR; Swift is shape-checked via Package.swift. The
earlier “verified” claim without CI was false — this closes it. Cost if
wrong: slower CI / Docker pull on every PR.

Ruling: Task 7 prende la migrazione libera `0029_idempotency_keys.sql`; le
migrazioni numerate in anticipo nel piano per task futuri slittano in base
all'ordine reale di esecuzione sul branch, perché Postgres/sqlx richiedono un
ordine monotono effettivo e non è possibile inserire dopo una `0029` mancante.
Costo se sbagliato: i task futuri devono rinumerare le proprie migrazioni
rispetto al piano statico.

Ruling: `Idempotency-Key` è supportato da subito ma non ancora obbligatorio:
senza header la richiesta continua a comportarsi come prima, così i client
esistenti non si rompono mentre il mobile può adottarlo subito. Costo se
sbagliato: una finestra di compatibilità in cui i vecchi caller restano non
idempotenti finché non vengono aggiornati.

Ruling: la tabella congelata usa `response_body jsonb` come envelope
`{request,response}` — fingerprint della richiesta, body JSON e `Set-Cookie`
eventuale — invece di aggiungere una colonna per il request hash; il piano
congela le colonne, non la forma interna del JSON. Costo se sbagliato: una
futura migrazione dovrà separare metadata e payload in colonne dedicate.

## Task Log

Task 1: complete (commit 8305d90, probe hardware video misurato con test verdi)

Task 2: complete (commit c7f8e6f, HLS on-demand + player; media 9/9, jobs 2/2,
api 4/4, build frontend verde)

Task 6: complete (commits 7f0f807..be8bdbc, review clean — `/sync/delta` REST)

Task 9: complete (in progress commit range to be finalized in next commit; test
openapi 6/6 verdi, client TypeScript buildato, package Swift validato via
`swift package dump-package`)

Ruling: il service worker offline resta deliberatamente piccolo: precache della
shell (`/`, manifest, favicon, icone) + cache-first per `/media/thumb/*` e gli
asset statici hashed, senza introdurre una seconda logica di sincronizzazione
offline per le API `/api/v1`. Costo se sbagliato: offline limitato alla shell e
alle miniature già viste, ma nessuna cache stantia di payload auth/metadata.

Ruling: l'update del service worker non usa `skipWaiting()`: una nuova versione
resta in waiting finché le tab vecchie non si chiudono, così non sostituisce in
silenzio una shell durante upload o altre operazioni attive. Costo se sbagliato:
l'utente vede l'update al riavvio della PWA invece che immediatamente.

Task 10: complete (offline shell + cached thumbs + installable PNG icons;
frontend build verde, `node --check public/sw.js`, manifest JSON valido)

Task 7: complete (Idempotency-Key middleware + repo + migration; targeted tests
verdi)

Ruling: `moka` lives in `keeppix-db` (on `Db`), not in `keeppix-api` as the plan
sketched — `VisibilityScope::resolve` and `SettingsRepo` are called from jobs
and many repos, so only a cache owned by `Db` can be invalidated in the same
write path that mutates permissions/settings. Cost if wrong: an API-only cache
would leave jobs and deep repo reads stale or force duplicate invalidation logic.

Ruling: migration number is `0030_performance_indexes.sql` (next free on this
branch), not the plan's placeholder `0032` — same numbering rule as Task 7.
Cost if wrong: sqlx migrator would skip or collide.

Ruling: `FolderRepo::ensure_path` stays as-is. Measured on PostGIS testcontainer:
existing depth-20 path ≈ 3.9 ms/call (100×), 50 cold depth-8 trees ≈ 110 ms
total. Confined to ingest writes; rewrite risk outweighs gain. Cost if wrong:
very deep first-time imports stay N+1 until revisited.

Ruling: `PATCH /users/{id}` clears the API `SessionCache` on any profile update
so a role change is reflected on the same browser session without waiting the
30s TTL — the DB permission cache alone cannot fix `AuthContext.role` baked into
the session cache. Cost if wrong: unrelated profile patches also flush sessions
cache (cheap; re-auth from DB).

Task 12: complete (commits 82881b9, c2e35e3, test verdi)

Ruling: Password zeroize covers the owned serde→String→Password path via
`parse_owned` + `ZeroizeOnDrop`; axum/hyper request `Bytes` and internal
serde_json buffers remain outside our control without custom body middleware —
HTTP handlers use `parse_owned` on moved JSON fields. Cost if wrong: plaintext
may linger in the HTTP stack until pool reuse; the domain-owned copy is cleared.

Ruling: `users.locale` is the source of truth per spec §10.10 once a session
exists; `localStorage` is a first-paint/logged-out cache kept in sync by
`applyProfileLocale` and `setLocale`. Login-page language changes UI/cache only
until settings persist via `PATCH /users/{id}`. Cost if wrong: anonymous
language choice is not stored server-side until settings.

Task 11: complete (commit 2dacd25, targeted tests verdi)

Ruling: TOTP tables use migration `0031_totp.sql` (next free on this branch);
plan's `0029` was taken by Task 7 idempotency. `last_used_step` is added beyond
the plan's SQL sketch because reuse protection requires it. Cost if wrong:
another migration to add the column later.

Ruling: `users.totp_secret_enc` from 0001 stays unused; `totp_secrets` is the
source of truth. Altering 0001 is forbidden. Cost if wrong: a later cleanup
migration drops the dead column.

Ruling: login takes optional `totp_code` on the existing `POST /auth/login`
instead of a second challenge endpoint — smallest surface that is still real
2FA. Missing code with TOTP enabled → `401 keeppix/totp-required`. Cost if
wrong: clients must learn one new problem type; password correctness is
revealed when 2FA is on (intentional UX).

Ruling: recovery codes are hashed with blake3 keyed by `totp_key` (same key as
AES-GCM), not Argon2 — codes are high-entropy so slow hashing buys little and
would make regenerate/login heavier. Cost if wrong: a weaker KDF if the keyed
hash key leaks with the DB.

Task 5: complete (commit 58386d7, db totp 8/8, api totp 3/3, openapi 6/6, frontend build + i18n + check-wired verdi)

Ruling: backup migration is `0032_backup_config.sql` (next free on this
branch); plan's `0030` was taken by Task 12 indexes. Extra columns
`backup_runs.path` and `keeppix_version` are added beyond the plan sketch so
restore/proof can find the file and enforce the version gate without a second
table. Cost if wrong: a later migration to rename/split those columns.

Ruling: destination credentials are AES-GCM encrypted in `config` jsonb via
`backup_key` from SettingsRepo (same pattern as TOTP). Preferences including
the age passphrase live in `system_settings` key `backup_preferences`. Cost if
wrong: passphrase rotation needs a dedicated endpoint later.

Ruling: S3 upload is a minimal HTTP PUT with optional custom headers, not full
AWS SigV4 — enough for gateway/MinIO setups that terminate auth, recorded as a
known limit. SFTP upload shells out to `scp` in BatchMode; connection test is
TCP-only. Cost if wrong: operators needing native SigV4/SSH need a follow-up.

Ruling (review follow-up): S3 uses `aws-sigv4` path-style PUT/DELETE; SFTP
uses `russh` + `russh-sftp` with password or PEM private_key; `test_destination`
performs an authenticated write+delete probe. Cost if wrong: aws-lc / russh
build weight in the jobs crate.

Ruling: destination free-space is estimated (`pg_database_size` + maps +
overhead) and checked **before** staging; a post-pack re-check uses the real
size. Test hook: destination path ending in `keeppix-nospace` reports 0 bytes.
Cost if wrong: estimate under-shoots and the post-pack check still aborts late.

Ruling (review follow-up): Idempotency middleware looks up by key when session
auth fails, so `POST /auth/refresh` retries with a consumed cookie replay the
cached Set-Cookie instead of 401.

Ruling (review follow-up): every OpenAPI operation has an explicit English
`summary=`; CI test fails if any summary contains `# Errors`.

Ruling (review follow-up, Task 12): EXPLAIN ANALYZE on 20k assets showed
`status <> 'trashed'` and `status IN (discovered,indexed,error)` both use
`assets_folder_filename_key` with a residual filter (~4ms). Keep `<>` — the
partial status indexes are irrelevant once `folder_id` prunes; rewriting the
predicate is churn without a plan win. Cost if wrong: a future status value
outside the IN-list would need a migration either way.

Ruling: integrity scrub and VACUUM/backup dump are scheduled only inside the
default night window from `main.rs`, while lighter cleanups (sessions, done
jobs, idempotency, transcode cache) run on the 24h timer. All use
`JobPriority::Background` so `EnergyProfile::Interactive` never claims them.
Cost if wrong: night-only heavy work may wait up to ~1h after window open.

Ruling: `pg_restore` for full database restore is not transactional across the
whole dump — documented in the restore handler, not pretended solved. Safety
dump is written first. Hot maps-only restore skips the dump path entirely.

Ruling: `RegionRepo::list_all_for_jobs` and `BackupRepo::list_enabled_for_jobs`
are documented AuthContext exceptions (system inventory / pipeline), same class
as `purge_expired`. Cost if wrong: a future permission model for map regions
would need to revisit the job path.

Task 3: complete (commit a78f602 + 6be23b3 + eb3d8d0; db backup 8/8, kpxb
format + CLI age/zstd/tar round-trip, BackupView with originals warning)

Task 4: complete (same commits; restore rejects newer backup, maps-only hot
path, monthly restore-proof job + destinations test path)

Task 8: complete (same commits; maintenance scheduler wired; purge_expired
removed from wired-exceptions; EnergyProfile Background respect tested)

Ruling (CI unblock / public repo fix pass): cast `statvfs` `f_bavail`/`f_frsize`
to `u64` before multiply — native field widths differ (macOS vs Linux).

Ruling: hoist `quick-xml` 0.41 to workspace deps (align api with media;
RUSTSEC-2026-0195).

Ruling: bump `russh` 0.55 → 0.60.3 for advisory-clean SSH/SFTP path.

Ruling: stay on `age` 0.11 and ignore RUSTSEC-2026-0173 (proc-macro-error2
unmaintained via age→i18n-embed-fl). age 0.12 removes that crate but fails to
compile today (ml-kem vs dual crypto-common). Cost if wrong: we keep an
unmaintained *build-time* transitive until age/ml-kem ships a fix.

Ruling: remote backup destinations skip pre-write free-space checks by design
(no portable quota API); failures surface on upload. Documented in
`ensure_destination_space`.

Ruling (CI): install `ffmpeg` in the backend job — video tests need ffprobe;
without it the seed falls back to a fake file and `probe_stream` → 404.

Ruling: test harnesses DROP each `keeppix_test_*` DB on Drop with
`WITH (FORCE)`. Without this, KEEPPIX_TEST_DATABASE_URL left 10k+ DBs /
~176 GB on the shared Postgres (and CI WAL thrash). Cost if wrong: Drop
races a still-running pool; FORCE is the escape hatch.

Ruling: `ChangeLogRepo::since` forces cursor forward on a full page when
`safe_cursor` is stuck behind cluster-global `pg_snapshot_xmin` (shared CI
Postgres + parallel tests). Prevents infinite `/sync/delta` pagination.
Overlap safety still covered when `has_more` is false / safe retracts between
polls. Cost if wrong: under extreme xmin holdback a full page could skip an
in-flight lower seq — acceptable vs. client spin; dedicated Postgres is the
real fix for perfect overlap under load.

Ruling: upload finalize rename test sorts filenames in Rust — Postgres
`ORDER BY filename` under en_US/ICU can put `foto_1.jpg` before `foto.jpg`.

Ruling: CI installs `age` + `zstd` alongside ffmpeg — kpxb external CLI
round-trip test requires them; spawn-`age -d -p` was a false check that
panicked on NotFound before asserting magic.

Ruling: sandboxed HLS ffmpeg uses `-threads 1` / `-filter_threads 1`, 2 GiB
RLIMIT_AS, optional audio map, and one EAGAIN retry — CI parallel load was
failing scale with "Resource temporarily unavailable".

