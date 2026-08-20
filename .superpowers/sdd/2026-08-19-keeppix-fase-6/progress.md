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
