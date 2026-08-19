# Fase 5 — WebDAV e upload riprendibili — Ledger

Branch: fase-5
Base commit (main before Fase 5): 7751eed
Plan: docs/superpowers/plans/2026-08-19-keeppix-fase-5.md
Spec: docs/superpowers/specs/fase-5-webdav-upload.md

## Decisioni e Ruling

Ruling: il campo di risposta di `POST /api/v1/upload/check` si chiama
`unknown_hashes`, non `known_hashes` come nell'illustrazione della spec — la
spec stessa (§1.2) e il caso di test pinnato descrivono il comportamento come
«47 hash di cui 12 sconosciuti → la risposta elenca esattamente quei 12»,
cioè gli sconosciuti, non i noti. L'illustrazione `{ known_hashes: [...] }`
è quindi un'etichetta fuorviante sullo stesso comportamento. Dato che è
un'API nuova (Task 1 di Fase 5, non ancora rilasciata), rinominare il campo
non rompe `/api/v1` congelato. Costo se sbagliato: un rename del campo JSON,
nessuna migrazione dati coinvolta.

Ruling: `POST /api/v1/upload/check`, `POST /api/v1/upload`, `HEAD` e `PATCH
/api/v1/upload/{id}` restano dietro il middleware CSRF standard di
`/api/v1` in questo task, nonostante un commento ormai obsoleto in
`csrf.rs` che ipotizzava un'esenzione per le rotte tus/WebDAV vivessero fuori
da `/api/v1`. Il piano di fase (Task 5) mette esplicitamente queste rotte
sotto `/api/v1/upload/*` e prevede l'esenzione CSRF come lavoro a parte più
avanti nella fase. Costo se sbagliato: un client tus reale senza
`x-keeppix-client` riceverebbe 403 finché il Task 5 non aggiunge l'esenzione.

Ruling: l'estrazione dei metadati (`JobKind::ExtractMetadata`) non viene
accodata alla finalizzazione dell'upload in questo task. L'asset creato da
`UploadSessionRepo::finalize` resta con lo stato di default
(`AssetStatus::Discovered`), come un asset scoperto dal walker prima
dell'indicizzazione. Il piano assegna esplicitamente l'enqueue con
`JobPriority::High` al Task 2. Costo se sbagliato: un asset caricato via tus
resta "discovered" finché non arriva la prossima scansione o il Task 2,
invece di essere indicizzato subito.

Ruling: nel fix del Task 1 (ordine `rename()`/commit in `finalize`), il ramo
del duplicato esatto (`SkippedDuplicate`) rimuove il temporaneo **prima**
del commit della `DELETE FROM upload_sessions`, non dopo come nel resto del
codice originale. Non tocca mai la cartella target, quindi non ricade
nell'invariante "mai un asset senza file" — ma se il commit fallisse dopo la
`remove_file_tolerant`, la sessione (rollback) resterebbe con un
`temp_path` che non esiste più: tollerato ovunque venga letto di nuovo
(`remove_file_tolerant` è già NotFound-tollerante), mai un rischio peggiore.
Costo se sbagliato: nessuno osservabile — è già lo scenario che il codice
gestisce per un temporaneo scaduto.

Ruling: il chunk massimo accettato da `PATCH /api/v1/upload/{id}` è fissato
a 64 MiB (`MAX_CHUNK_BYTES` in `routes/upload.rs`), non specificato dalla
spec §1.2. Un chunk più grande del minimo fra questo limite e i byte
rimanenti della sessione riceve `413`, non un `400` generico come prima
della revisione. Costo se sbagliato: un client che manda chunk più grandi di
64 MiB (adattivi, la spec §1.3 ne ipotizza fino a 16 MB) va aggiornato a
spezzarli, oppure il limite va reso configurabile.

## Task Log

Task 1 (Sessioni di upload tus — schema e protocollo): complete
(commit ea660f9, test verdi: 14 in `keeppix-db`, 9 in `keeppix-api`,
`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` puliti).

Task 1, fix di review (2 Critical + 1 Important + 1 Minor): complete.
Critical 1 — `finalize()` faceva il commit (riga `assets` creata, sessione
cancellata) **prima** del `rename()` del temporaneo alla destinazione:
un `rename()` fallito a quel punto lasciava un asset senza file e nessuna
sessione da cui recuperare il temporaneo. Invertito l'ordine: `rename()`
prima, poi insert + delete + commit nella stessa transazione — l'asimmetria
sicura è un file al posto giusto senza riga se il commit fallisce dopo
(la prossima scansione lo indicizza), mai il contrario. Aggiunto un test di
regressione (`finalize_leaves_no_asset_and_keeps_the_session_when_rename_fails`)
che fallisce deterministicamente sul codice pre-fix (verificato: FAILED con
`git checkout` del solo `uploads.rs`, poi ripristinato) forzando il
fallimento del `rename()` con la cartella target cancellata dal disco.
Critical 2 — mancava un test per "verifica di decodificabilità fallita
anche con hash corretto" (spec §1.2, caso limite pinnato). Aggiunto
`completing_with_undecodable_content_never_enters_the_library` in
`keeppix-api/tests/upload.rs`: hash blake3 corretto su un payload di
garbage senza alcun magic number riconosciuto da
`keeppix_media::detect_kind`, assert `422 keeppix/upload-undecodable`,
nessun asset, temporaneo e sessione ripuliti.
Important — il `PATCH` bufferizzava l'intero chunk in RAM con
`axum::body::to_bytes(body, cap)`, con `cap` fino ai byte rimanenti
dell'intera sessione (potenzialmente gigabyte). Riscritto `write_chunk_checked`
sullo stile di `write_body_capped` in `routes/share.rs`: streaming diretto
sul temporaneo in modalità append, hash blake3 incrementale, e un checksum
sbagliato tronca (`set_len`) il file alla lunghezza originale invece di non
scriverlo mai (stesso esito osservabile: il chunk non sopravvive). Nuovo
limite `MAX_CHUNK_BYTES = 64 MiB` (vedi Ruling sopra), verificato con un
nuovo test multi-chunk (`a_multi_chunk_upload_completes_across_two_patches`)
che due `PATCH` in sequenza si accodano correttamente sul disco.
Minor — aggiunto `client_mtime_is_preserved_on_the_finalized_asset`.
Test verdi: 15 in `keeppix-db` (14 + 1 nuovo), 12 in `keeppix-api` (9 + 3
nuovi: decodificabilità, `client_mtime`, multi-chunk). `cargo fmt --check`
e `cargo clippy --workspace --all-targets -- -D warnings` puliti. Nessuna
regressione nelle suite complete di `keeppix-db` e `keeppix-api`.


Task 1: complete (commits ea660f9..68f9a70, review clean after fix round)
- Critical fix: rename() before DB commit in finalize()
- Critical fix: decodability-failure test added
- Important fix: chunk streaming rewrite (write_chunk_checked, MAX_CHUNK_BYTES=64MiB)
- Minor noted: concurrent-finalize filename collision race (pre-existing, low-probability)

Ruling: la finalizzazione upload accoda `JobKind::ExtractMetadata` con
`JobPriority::High` e dedup key `meta:{asset_id}`; i duplicati esatti
(`SkippedDuplicate`) non accodano nulla perché l'asset esistente era già
indicizzato. Costo se sbagliato: job superflui su duplicati o asset nuovi in
stato `discovered` fino al prossimo rescan.

Ruling: aggiunta la dipendenza npm `hash-wasm` (^4.12.0) per calcolare
blake3 lato client — necessità reale, non opzionale: sia il pre-check
(`POST /upload/check`) sia `Upload-Checksum: blake3 <hex>` sul `PATCH`
richiedono esattamente quell'algoritmo, e `crypto.subtle` del browser non lo
implementa. Nessuna libreria blake3 esisteva già nel frontend. Per contenere
l'impatto sul budget di 150 KB gzip (§AGENTS.md), sia `hash-wasm` sia
`UploadPanel.vue` sono importati con `import()` dinamico — `UploadPanel` via
`defineAsyncComponent` in `App.vue`, `hash-wasm` dentro `hashBytes()` in
`api/upload.ts` — quindi finiscono in un chunk lazy separato
(`UploadPanel-*.js`, 8 KB / 3 KB gzip) verificato con `npm run build` a non
comparire nei chunk iniziali (`client-*.js`, `index-*.js`). Costo se
sbagliato: 3 KB gzip in più sul primo caricamento se il code-splitting
smettesse di funzionare — non l'intero pacchetto (9 KB gzip solo per
BLAKE3 secondo la tabella upstream).

Ruling: `addFiles()` e `resume()`/`retry()` non avviano l'upload in modo
sincrono ma con `setTimeout(..., 0)` (`schedulePump`). Necessario perché lo
store deve poter esporre lo stato "queued" a chi osserva subito dopo
`await addFiles(...)` (spec del Task 3, test
`pre_check_skips_files_already_in_library`) prima che il ciclo di
concorrenza (max 3) faccia scattare "uploading". Costo se sbagliato: un
ritardo di un tick prima che un upload appena accodato parta davvero — mai
osservabile per l'utente, un frame a 0 ms.

Ruling: una sessione ripresa da `localStorage` (`initFromStorage`) perde
sempre l'oggetto `File` — non sopravvive a un refresh della pagina. Lo
store la segna "paused" con l'offset vero (da `HEAD`), ma non può riprendere
l'invio dei byte senza che l'utente riselezioni il file: `resume()` su una
sessione così imposta l'errore `upload.errors.missingFile` invece di
avviare un upload. Non è nella spec del Task 3 (i 4 test richiesti non lo
esercitano) — differito qui come limite noto, non silenziato: la UI per
"riseleziona il file per riprendere" è lavoro di un task successivo.

Task 3 (pannello di upload persistente, frontend): complete (vedi
`task-briefs/task-3-report.md` per l'elenco dei file e l'output di
verifica). 4 test Vitest nuovi in `UploadPanel.spec.ts`, tutti osservati
rossi contro uno stub prima dell'implementazione reale (TDD); `npm run
test` (88/88), `npx vue-tsc --noEmit` e `npm run build` puliti.

Task 3, fix di review (2 Important + 1 test gap, vedi
`task-briefs/task-3-report.md` sezione "Fix round"): complete (commit
98c60bf).
Important 1+2 — `runUpload` salvava `err.type` grezzo (es.
`keeppix/some-error`) in `session.error` per un `ApiProblem` generico nel
catch, invece di una chiave i18n come per gli altri errori strutturati
(`upload.errors.expired`, `upload.errors.missingFile`): una vista che
rendesse quel valore con `t()` avrebbe tentato di tradurre una stringa
senza corrispondenza in `en.json`/`it.json`. Normalizzato a
`upload.errors.unknown` (chiave già presente in entrambe le lingue, nessuna
aggiunta necessaria) — rimossa anche la dipendenza da `ApiProblem` in
`stores/upload.ts`, ora non più necessaria lì. `statusLabel()` in
`UploadPanel.vue` leggeva solo `session.status`, mai `session.error`: una
sessione `paused` che aveva perso il file (`session.error =
'upload.errors.missingFile'`) non mostrava alcun feedback.
Ruling: invece di scegliere *solo* fra le due opzioni proposte dalla review
(spostare `resume()` in "error" oppure mostrare l'errore anche su
"paused"), ho fatto entrambe: `resume()` ora imposta `session.status =
'error'` quando il `File` non è in memoria (non solo `session.error`), *e*
`statusLabel()` mostra `t(session.error)` anche per una sessione ancora
`paused` con un errore già presente, per difesa in profondità nel caso in
cui uno stato `paused`+`error` venga prodotto altrove in futuro. Costo se
sbagliato: nessuno osservabile, è un ramo extra nella UI che difficilmente
scatta dopo il fix di `resume()`.
Test gap — `UploadPanel.spec.ts` testava solo lo store, mai il componente
Vue montato: aggiunto un test che monta `UploadPanel.vue` con
`@vue/test-utils` e una sessione `status: 'error'` +
`error: 'upload.errors.missingFile'`, verificando che il testo renderizzato
contenga la traduzione specifica e non la label generica
`upload.status.error` ("Failed"/"Non riuscito").
Minor (fatto, non solo segnalato) — l'hash blake3 già calcolato da
`addFiles` per il pre-check ora è passato come `expected_hash` a
`createSession` (nuova mappa `expectedHashes` in `stores/upload.ts`,
tenuta in sincrono con `files` per id locale/remoto), invece di essere
scartato dopo il pre-check.
Verifica: `npm run test` 23 file / 89 test verdi, `npx vue-tsc --noEmit`
pulito, `npm run lint` 0 errori (9 warning pre-esistenti in
`SharesView.vue`, non toccato da questo fix).
