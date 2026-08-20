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

Ruling: la generazione client OpenAPI usa Docker (`openapitools/openapi-generator-cli`)
anziché una nuova dipendenza npm del workspace. I client sono artefatti
rigenerabili sotto `docs/api/clients/` e restano ignorati dal VCS; si committano
solo lo snapshot `docs/api/openapi.json` e lo script di generazione. Costo se
sbagliato: lock-in su un tool locale/non portabile o un diff enorme di codice
generato senza valore di review.

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
