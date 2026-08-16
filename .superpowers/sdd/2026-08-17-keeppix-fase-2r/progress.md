# SDD ledger — plan: docs/superpowers/plans/2026-08-17-keeppix-fase-2r.md

Branch: `fase-2r`
Spec di riferimento pipeline: docs/superpowers/specs/fase-1b-ingestione.md

Ruling: si lavora in-place sul branch `fase-2r` da main aggiornato,
come chiesto dall'utente. Nessun push/PR/merge.


Ruling (Task 1): `filetime` come *dev-dependency* di `keeppix-jobs` —
serve a impostare `mtime` nel passato nei test di discovery; senza, non
si distingue Settled da InFlight. Nessun uso in produzione. Costo se
sbagliato: una dipendenza di test in più (CVE surface solo in CI).

Ruling (Task 1): la spec 1b §5.2 dice «due letture a 5 s di distanza»; il
piano 2R sostituisce il sonno nel ciclo con `freshness` (mtime ≥ 60 s →
Settled in un solo `stat`; altrimenti InFlight e job `discover_library`
con `run_after = now()+5s`). Vince l'intento della spec (non indicizzare
mezzi file) senza il costo che la rendeva inutilizzabile. Annotato perché
piano e spec divergono sulla meccanica, non sul risultato.

**RED osservato (Task 1):**
`cargo test -p keeppix-jobs --test discover_perf discovering_a_thousand…`
con `PRODUCTION_STABILITY_WAIT` (5 s): dopo **>60 s** il test era ancora
`running` (timeout del runner a ~70 s). Coerente con ~5 s × N file.

MEASUREMENT (Task 1): discovery di 1.000 file assestati =
**1.45–1.68 s** (debug, cloud VM). Prima: timeout >60 s (~5 s/file).

Task 1: complete (commit `d70f583`, test verdi)

Ruling (Task 2): il piano diceva «solo superficie HTTP», ma `LibraryRepo`
non aveva `update`/`delete` — aggiunti lì (non negli handler). Costo se
sbagliato: metodi repo in più; altrimenti PATCH/DELETE sarebbero impossibili
senza SQL negli handler.

Ruling (Task 2): nei test l'allowlist è `data_dir/photos`, non `/photos` —
stessa proprietà (canonicalize + `starts_with`), senza richiedere root FS.
Il caso letterale `/photos/../etc` resta coperto dalla stessa logica in
produzione. Costo se sbagliato: un test che non esercita `/photos` di sistema.

Ruling (Task 2): `Env::prefixed("KEEPPIX_").split(",")` così
`KEEPPIX_LIBRARY_ROOTS=/photos,/data/extra` è una lista senza JSON. Costo se
sbagliato: un valore KEEPPIX_* con virgola legittima si spezzerebbe
(oggi nessuno).

Task 2: complete (commit `e7d1111`, test verdi)

Ruling (Task 3): `keeppix-api` dipende da `keeppix-jobs` per chiamare
`enqueue_rescan` e tenere `LibraryWatchers` in `AppState` — altrimenti si
reimplementerebbe l'accodamento o i watcher resterebbero solo in `main`.
Costo se sbagliato: un arco di dipendenza in più (nessun ciclo).

Ruling (Task 3): `eta_seconds` è sempre `null` finché non ci sono misure
di throughput (Task 9). Meglio assente che inventato. Costo se sbagliato:
UI senza ETA finché non si riempie.

Task 3: complete (commit `7cf4b8d`, test verdi)

Ruling (Task 4): dopo disable/cambio password si fa `SessionCache::clear()`
oltre al revoke in DB — altrimenti un token revocato resta valido fino a
30s via cache in-process e il test (e la proprietà di sicurezza) fallirebbero.
Costo se sbagliato: un picco di autenticazioni DB subito dopo.

Ruling (Task 4): `map_unique_violation` distingue `users_username_key` vs
`users_email_key` via `constraint()` — debito Fase 0 saldato. Costo se
sbagliato: messaggio generico se Postgres cambia i nomi degli indici.

Task 4: complete (commit `68c60a5`, test verdi)

Ruling (Task 5): `POST /trash/empty` richiede admin o owner di almeno una
libreria (`libraries.owner_id`), non il solo ruolo Admin — un utente con
libreria propria può svuotare il proprio cestino; Mario senza librerie → 403.
Costo se sbagliato: un viewer condiviso (Fase 3) non potrà svuotare anche se
avesse visibilità sul cestino.

Ruling (Task 5): `GET /assets/{id}/stack` su asset fuori stack restituisce
`members: []` con `stack_id`/`primary_asset_id` null, non 404 — l'asset esiste
ed è visibile. Costo se sbagliato: il client deve distinguere «nessuno stack»
da «asset inesistente» (403).

Ruling (Task 5): `days_remaining` calcolato in HTTP da `TRASH_RETENTION_DAYS`
(30) e `deleted_at`, non in SQL — stessa formula della spec §6. Costo se
sbagliato: drift se un giorno la retention diventa configurabile per libreria.

Deferred (Task 5): `TrashRepo::cleanup_expired` resta non schedulato (debito
Fase 2); `empty` salta file non cancellabili come `cleanup_expired`.

Task 5: complete (commit `436f520`, test verdi)

Ruling (Task 6): avanzamento scansione via **polling** ogni 2 s su
`GET /api/v1/libraries/{id}/scan`, non WebSocket — il piano cita WS ma il
task chiede polling per semplicità; WS non è cablato nel frontend. Costo
se sbagliato: più richieste HTTP durante il setup (accettabile, uso una
tantum).

Ruling (Task 6): dopo creazione admin si resta su `/setup` con stato
locale (`step`), senza cambiare il guard del router — un refresh a metà
wizard manda a `/` (istanza già `initialised`). Riprendere il wizard al
refresh richiederebbe guard + `GET /libraries` (fuori scope). Costo se
sbagliato: refresh durante setup salta passi 2–3.

Ruling (Task 6): navigazione automatica a `/` quando `phase === 'idle'`
e `asset_count > 0`; altrimenti pulsante «Vai alle foto». Costo se
sbagliato: libreria vuota resta su setup finché l'utente non continua.

Deferred (Task 6): refresh mid-wizard e gestione librerie post-setup da
UI admin (non in questa fase).

Task 6: complete (commit `9860dd4`, test verdi)

Ruling (Task 7): `wait_for_scan` combina polling HTTP su
`GET /libraries/{id}/scan` con `WorkerPool::step` (come `scan.rs`) finché
`phase == idle`, zero job pending, e conteggio `indexed` con `taken_at_utc`
nella libreria — i bucket timeline richiedono EXIF, non solo discover.
Costo se sbagliato: un po' di SQL nel harness oltre HTTP (stessa classe di
`scan.rs`).

Ruling (Task 7): tetto 60 s su V1 come `Instant` assoluto dall'avvio del
test, non solo su `wait_for_scan` — è il budget del viaggio completo che
avrebbe fatto fallire la build sul sonno per file. Costo se sbagliato:
flaky su runner lenti (oggi V1 ~1 s debug).

Ruling (Task 7): fixture sei foto in due cartelle sotto `photos_root`
(copie di `tiny.jpg`) — allowlist e EXIF reali senza inventare JPEG. Costo
se sbagliato: bucket vuoti se `tiny.jpg` perde EXIF.

MEASUREMENT (Task 7): V1 viaggio completo ~1,06 s (debug, cloud VM, 6 JPEG);
suite V1–V4 ~4,4 s; budget 60 s non toccato.

Task 7: complete (commit `ef05e3d`, test verdi)

Ruling (Task 8): `main.rs` usa già `PRODUCTION_SETTLED_AFTER` sul campo
`stability_wait` dell'`IngestHandler` (non `ZERO`, non
`PRODUCTION_STABILITY_WAIT` — quello resta solo per `run_after` su InFlight
in `discover.rs`). Costo se sbagliato: nessuno, è verifica.

Ruling (Task 8): budget RAW preview 50 ms in release, **100 ms in debug** —
`sample.cr3` supera 50 ms in build non ottimizzata (~76–80 ms misurati);
l'obiettivo Pi 5 è release. Costo se sbagliato: un falso positivo in CI
debug con soglia stretta.

Ruling (Task 8): seed 10k asset timeline via bulk SQL nel test API (come
`fase2_culling_1k`) — i trigger `folder_month_counts` devono essere
allineati; nessun worker ingest. Costo se sbagliato: test che misura solo
DB/HTTP, non la pipeline completa (accettabile per budget endpoint).

MEASUREMENT (Task 8, debug, cloud VM):
- discovery 1.000 file assestati: **1,41 s** (budget 30 s)
- handler produzione discover 200 file: **305 ms** (budget 5 s smoke)
- RAW preview: arw 3,6 ms, nef 2,6 ms, cr2 2,8 ms, cr3 76 ms, dng 0,6 ms
  (budget 100 ms debug / 50 ms release)
- `GET /timeline/buckets` (10k asset): **2,7 ms** (budget 200 ms)
- `GET /timeline` una pagina (10k asset): **13,5 ms** (budget 300 ms)
- `GET /libraries` (20 librerie): **2,5 ms** (budget 100 ms)

Task 8: complete (commit `370cc34`, test verdi)

Ruling (Task 9): in questo ambiente cloud non c'è daemon Docker né l'archivio
1.558 ARW — `field-test.sh` è stato adeguato e verificato che esce 1 senza
daemon; le misure sull'archivio reale restano da rilanciare dall'operatore
con `PHOTOS_PATH=… ./scripts/field-test.sh`. Costo se sbagliato: ledger senza
copertura RAW sui 1.558 finché non si rilancia.

MEASUREMENT (aggregato Fase 2R, cloud VM debug):
- discovery 1.000 file assestati: **1.41–1.68 s** (budget 30 s)
- RAW embedded preview: arw 3.6 / nef 2.6 / cr2 2.8 / cr3 76 / dng 0.6 ms
  (budget 50 ms release / 100 ms debug)
- timeline buckets 10k: **2.7 ms**; timeline page: **13.5 ms**; libraries×20: **2.5 ms**
- journey V1 (6 JPEG): **~1.06 s** (cap 60 s)
- bundle ingresso frontend: **~81 KB gzip** (budget 150)

Task 9: complete (commit 617a7e9, script aggiornato; field-test reale deferred)
