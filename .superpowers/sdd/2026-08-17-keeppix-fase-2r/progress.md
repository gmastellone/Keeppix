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

Task 4: complete (commit `PENDING`, test verdi)
