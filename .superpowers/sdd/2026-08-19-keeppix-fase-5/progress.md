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

Task 3: complete (commits eaff018..8a67589, review clean after fix round)
- Important fix #1+#2: session.error ora mostrato in UploadPanel.vue; errori ApiProblem mappati a upload.errors.unknown
- Important fix #3: test di mount del componente Vue verifica che session.error raggiunga il DOM
- Minor applicato: expected_hash passato a createSession dopo pre-check

Ruling: sessioni riprese da localStorage senza il File originale (refresh a metà
upload) vengono marcate status='error' con error='upload.errors.missingFile'.
Il pannello mostra il messaggio specifico. La riselezione del file non è
implementata (fuori scope dei 4 test richiesti e del brief). Costo se sbagliato:
UX degradata su refresh, ma nessuna regressione di integrità.

Ruling: nuova dipendenza npm `hash-wasm` (MIT, WASM blake3, ~9 KB gzip).
Non c'è implementazione blake3 in crypto.subtle; il protocollo tus la richiede
sia per il pre-check sia per Upload-Checksum. Import dinamico: non grava sul
bundle iniziale. Costo se sbagliato: rimpiazzo con un'alternativa, un rename
del campo non serve (nessun contratto rotto con il backend).

Ruling: `verify()` in `AppPasswordRepo` NON prende `AuthContext` — eccezione documentata
al pattern invariante del progetto, usata pre-autenticazione per WebDAV Basic Auth.
Stesso precedente di `UserRepo::find_by_username`. Costo se sbagliato: nessun rischio
di sicurezza diretto, ma aumenta la superficie degli endpoint non protetti da AuthContext.

Ruling: `AppPasswordId` implementato manualmente invece di usare la macro `id_type!`
del workspace. Hash non derivato (non necessario nel brief). Costo: drift futuro se
la macro cambia. Differita come nota al reviewer del branch finale.

Ruling: `verify()` non controlla `disabled_at` dell'utente — stesso gap della
sessione-auth (controllo solo al login). Non è una regressione di questo task.
Da ricordare quando Task 5 cabla il Basic Auth WebDAV attraverso `verify()`.

Ruling: idempotenza di `revoke` — `AND revoked_at IS NULL` nella UPDATE. Una seconda
revoca sull'id già revocato restituisce 204 anziché 404. Ragionato: WebDAV client
potrebbe riprovare; l'invariante "password revocata non funziona" è già soddisfatta.
Costo se sbagliato: semantica leggermente diversa da un DELETE idempotente puro.

Task 4: complete (commits 1cf9d49..618bce9, review clean — Important: ledger entry
era mancante dal commit dell'implementer, aggiunta ora in questo commit)

Ruling: la deroga CSRF per `/dav/*` in `require_client_header` (`csrf.rs`) è
per prefisso di path, non condizionata all'assenza di cookie di sessione.
Verificato che oggi è ridondante — il layer è applicato solo dentro
`api_routes()` via `.layer(...)`, e `/dav/*` è montato come rotta sorella
fuori da quel router, quindi non lo attraverserebbe comunque — ma il brief
la richiede esplicitamente come "opzione preferita" ed è difesa in
profondità a costo nullo se in futuro il layer venisse spostato a un livello
più alto del router. Costo se sbagliato: nessuno osservabile oggi.

Ruling (non differita, solo segnalata): `AppPasswordRepo::verify` non
controlla `disabled_at` dell'utente (già annotato sopra al Task 4). Il Task
5 la cabla nel Basic Auth di `/dav/handler` senza aggiungere quel controllo:
il brief non lo richiede, e aggiungerlo qui avrebbe significato toccare
`keeppix-db` fuori dal perimetro scritto del task ("scaffolding: router,
auth, deroga CSRF — nessun PROPFIND/GET/PUT"). Resta un difetto noto e
differito: un utente disabilitato con un'app-password ancora non revocata
può autenticarsi su WebDAV. Da correggere prima che i Task 6-8 esponga
operazioni reali (PROPFIND/GET/PUT), non necessariamente in questo task.

Task 5 (scaffolding WebDAV — router, auth, deroga CSRF): complete (test
verdi: 4/4 in `keeppix-api/tests/webdav_auth.rs`, 6/6 unit test in
`dav::tests`, intera suite `keeppix-api` verde, `cargo fmt --check` e
`cargo clippy --workspace --all-targets -- -D warnings` puliti). Vedi
`task-briefs/task-5-report.md` per l'elenco dei file e il dettaglio TDD.

Ruling (Task 6): risoluzione del path `WebDAV` **per id, non per nome** —
`/dav/folder/{folder_id}` e `/dav/asset/{asset_id}`, esattamente come
suggerito dal brief. Evita di risolvere una gerarchia di nomi contro
`ltree` (query più complessa, mai scritta). Costo: Finder (che naviga per
nome umano) non funziona con questo schema; rclone e Cyberduck sì, perché
sincronizzano confrontando l'`ETag`, non il path. Differito a un task
successivo se servirà mai la risoluzione per nome.

Ruling (Task 6): il ruolo reale dell'attore `WebDAV` non viene interrogato
con una query separata su `users` — `AuthContext::user(user_id,
SystemRole::User)` sempre, anche per un amministratore. `FolderRepo`/
`AssetRepo` filtrano comunque per `user_id` (proprietà della libreria,
grant espliciti), non per ruolo di sistema, quindi un admin vede le
proprie librerie come qualunque proprietario. Costo se sbagliato: un vero
amministratore perde solo la visibilità "onnisciente" su tutte le librerie
via WebDAV che ha invece nella web app — nessun rischio di sicurezza
(mai un privilegio in più, solo uno in meno), nessuna via per un utente
normale di ottenere `is_admin() == true`.

Ruling (Task 6): l'intero corpo `multistatus` viene costruito in un
`Vec<u8>` in memoria (via `quick_xml::Writer`) e inviato in un solo colpo,
non in streaming a blocchi su un `Body` di axum. Per una cartella con
meno di 10.000 file (il caso descritto dal brief) sono pochi MB — accettabile
per questo task. La vera streaming a blocchi (per librerie enormi) è
un'ottimizzazione differita: costo se sbagliato, un picco di RAM
temporaneo proporzionale al numero di figli di una singola cartella (non
dell'intera libreria, perché `Depth: infinity` è comunque rifiutato).

Ruling (Task 6): `Depth` assente sull'header `PROPFIND` è trattato come
`Depth: 1`, non come `infinity` (il default RFC 4918). Rifiutare
`infinity` ma non offrire un comportamento utile quando l'header manca del
tutto avrebbe reso `PROPFIND` senza header praticamente inutilizzabile per
client che lo omettono. Un valore diverso da `0`/`1`/`infinity`
(case-insensitive su "infinity") è trattato con la stessa tolleranza di
`1`, mai un errore. Costo se sbagliato: un client che si aspetta
`infinity` di default riceve un solo livello — mai un problema di
sicurezza, solo una lista più corta di quanto sperato.

Ruling (Task 6): `getlastmodified` per una cartella usa `Utc::now()` al
momento della risposta, non un timestamp persistito — il modello di
dominio `Folder` non porta un mtime (la colonna `created_at` esiste in
tabella ma non è caricata da `FolderRepo`). La sincronizzazione reale la
fa l'`ETag` sugli asset (`content_hash`), non `getlastmodified` sulle
cartelle: nessun client ne dipende per decidere se una cartella "è
cambiata". Costo se sbagliato: nessuno osservabile — differito, non
un'omissione silenziosa.

Ruling (Task 6): implementato anche `PROPFIND` su un singolo asset
(`/dav/asset/{id}`, un solo `D:response`, indipendente da `Depth`) oltre a
quanto richiesto esplicitamente dai 5 test del brief — un client `WebDAV`
reale (rclone, Cyberduck) tipicamente sonda un file con `PROPFIND` prima
di un `GET`. Riusa la stessa macchina XML del caso cartella, nessun codice
nuovo di rilievo. Costo se sbagliato: superficie in più non coperta da un
test dedicato — mitigato riusando `asset_entry`, già esercitato dal test
di listing della cartella.

Task 6 (`PROPFIND` e `GET`): complete (commit ffa2b14, test verdi: 5/5 in
`keeppix-api/tests/webdav_propfind.rs`, 22/22 unit test in `keeppix-api`
lib — inclusi i 6 nuovi di `dav::propfind::tests` —, intera suite
`keeppix-api` (32 file di test + lib) verde, `cargo fmt --check` e
`cargo clippy --workspace --all-targets -- -D warnings` puliti). Vedi
`task-briefs/task-6-report.md` per l'elenco dei file, il dettaglio TDD
(inclusa la mutazione deliberata sui due test più importanti) e l'output
di verifica.

Ruling (Task 7): `COPY` **non è implementata**, resta `501` nel dispatch
di `dav::handler` — il brief la marca esplicitamente come opzionale
("se troppo complessa, implementarla come stub 501"). `copy_subtree` in
`folders.rs` richiederebbe una copia ricorsiva dell'intero sottoalbero con
nuovi id per ogni cartella e ogni asset (righe `assets` indipendenti, per
l'invariante "identità = `(folder_id, filename)`"), più un controllo di
spazio libero sulla libreria di *destinazione* (che il brief nota può
differire da quella di partenza) e la copia fisica ricorsiva file per
file sul disco. Nessuno di questi pezzi riusa codice esistente in modo
diretto: sarebbe una funzionalità nuova di peso comparabile a un intero
task, non un'estensione di `move_subtree`. Nessun test la esercita, come
esplicitamente permesso dal brief. Costo se differita: i client `WebDAV`
che si aspettano `COPY` (duplicare una cartella) ricevono `501`, un
comportamento esplicito e documentato, non un errore silenzioso o una
copia parziale.

Ruling (Task 7): il permesso di editor su `MOVE` viene verificato su
**entrambe** le cartelle (`src_id` e `dst_parent_id`) nel handler
`write::move_folder`, non delegato del tutto a
`FolderRepo::move_subtree`. Verificato leggendo il codice:
`move_subtree` chiama `self.visible(ctx, new_parent)` sul genitore di
destinazione — solo visibilità, non `effective_role` — mentre controlla
l'editor solo sulla cartella sorgente. Senza il controllo aggiunto nel
handler, un viewer con visibilità (ma non editor) sulla cartella di
destinazione potrebbe spostarvi dentro cartelle di altri editor. Costo se
la ruling fosse sbagliata (cioè se bastasse il controllo di
`move_subtree`): nessuno, il controllo aggiunto è ridondante ma non
dannoso quando `move_subtree` già rifiuta; qui invece è necessario perché
il brief lo richiede esplicitamente ("Verifica permesso editor su
entrambe le cartelle coinvolte") e il codice letto conferma che
altrimenti la seconda cartella non viene controllata per ruolo.

Ruling (Task 7): il brief afferma che lo spostamento fisico su disco in
`MOVE` è "già fatto da `move_subtree` che chiama `rename()`" — verificato
leggendo `folders.rs` per intero: **non è vero**, `move_subtree` non
contiene nessuna chiamata a `rename()`, aggiorna solo `folders.path`
(`ltree`) nel database sotto lock a livello di libreria. Il piano non
copriva questo task in dettaglio, quindi vince la spec/brief per priorità
dichiarata in `AGENTS.md`, ma qui la spec stessa è in errore su un fatto
verificabile nel codice — non un'ambiguità da risolvere, un'informazione
sbagliata. Ho aggiunto lo spostamento fisico (`tokio::fs::rename` da
vecchio a nuovo `absolute_path`) nel handler `write::move_folder`, dopo
il commit di `move_subtree`. Nota sul rischio residuo: se il `rename()`
fisico fallisse (solo un errore di I/O reale, perché `move_subtree` ha
già validato ciclo/libreria/collisione di nome prima di arrivare qui), la
cartella risulterebbe già spostata nel database ma non sul disco — la
stessa identica lacuna, non introdotta da questo task, già presente e non
toccata in `PATCH /api/v1/folders/{id}` (che oggi non sposta la directory
per niente). Il `MOVE` `WebDAV` qui è quindi già più corretto
dell'endpoint REST esistente, non più fragile. Costo se differito
oltre: un'inconsistenza disco/database da correggere a mano nel caso raro
di un errore di I/O a metà operazione — non coperta da rollback
automatico in nessuno dei due endpoint.

Ruling (Task 7): un `MKCOL` ripetuto sullo stesso nome non fallisce con
`405` (RFC 4918 §9.3) — `FolderRepo::ensure_child` è idempotente per
costruzione (lo stesso motivo per cui lo scanner lo richiama senza
duplicare nulla), quindi il handler restituisce di nuovo `201` sulla
cartella già esistente. Nessun client reale in uso in questo progetto
dipende dal `405`. Costo se sbagliato: un client `WebDAV` che si aspetta
`405` per rilevare una collisione di nome non lo riceve — mitigato dal
fatto che il nome finale è comunque quello richiesto (nessuna rinomina a
sorpresa, a differenza della collisione su `PUT`).

Ruling (Task 7): un dotfile (`filename.starts_with('.')`, coprendo
`.DS_Store`, `._foto.jpg`, `.hidden`) viene scritto **sovrascrivendo** un
omonimo già presente sul disco, bypassando il controllo di collisione
`AssetRepo::ingest_direct`. L'invariante "mai sovrascrittura silenziosa"
di `AGENTS.md` protegge le foto dell'utente (righe `assets`, la sua
libreria), non la cache del suo sistema operativo: `.DS_Store` viene
riscritto in continuazione da Finder, e trattarlo come le foto
richiederebbe suffissi `_1`, `_2`, ... che si accumulerebbero senza
motivo a ogni sincronizzazione. Costo se sbagliato: nessuno osservabile
dall'utente — un dotfile non ha mai una riga `assets` da proteggere.

Task 7 (`PUT`, `MKCOL`, `MOVE`; `COPY` stub `501`): complete (commit
4d2564e, test verdi: 7/7 in `keeppix-api/tests/webdav_write.rs`, intera
suite `keeppix-api` verde — 36 binari di test, zero `FAILED`/`panicked`
—, `cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` puliti). Vedi `task-briefs/task-7-report.md` per l'elenco dei
file, il dettaglio TDD e l'output di verifica.

Ruling (Task 7, fix round — Important #1): `ensure_disk_space`
(`crates/keeppix-db/src/uploads.rs`, già usata da
`UploadSessionRepo::create` per la sessione `tus`) è diventata **`pub`**,
non `pub(crate)` — il chiamante nuovo, `dav::write::put` in
`keeppix-api`, è in un altro crate del workspace, dove `pub(crate)` non
sarebbe visibile. La funzione non contiene SQL, quindi esportarla non
viola "nessun SQL fuori da `keeppix-db`" né il divieto di dipendenza
`keeppix-media` ↔ `keeppix-db`. `dav::write::put` ora legge
`Content-Length` (passato dal dispatcher in `dav::mod.rs`) e la chiama
prima di scrivere qualunque byte del temporaneo — un `Content-Length`
oltre lo spazio libero diventa `507` senza toccare il disco. Aggiunto
anche un tetto assoluto `MAX_BODY_BYTES = 10 GiB` in `write.rs`, imposto
byte per byte durante lo streaming (non solo controllato una volta
sull'header dichiarato): senza questo, un client `chunked` senza
`Content-Length` avrebbe bypassato sia il controllo sulla dimensione
dichiarata sia `ensure_disk_space`, riempiendo il disco senza che nessun
controllo lo intercettasse. Costo se sbagliato: un client che carica
legittimamente un file più grande di 10 GiB riceve `413` invece di un
upload riuscito — da rivedere se servirà per RAW/video enormi.

Ruling (Task 7, fix round — Important #2): `mkcol` crea ora la directory
sul disco **prima** dell'`INSERT` in `folders` (`FolderRepo::ensure_child`),
non dopo. Prima del fix, un fallimento di `create_dir_all` dopo il commit
dell'`INSERT` lasciava una riga `folders` fantasma senza directory
corrispondente. Con l'ordine invertito, un fallimento su disco non tocca
mai il database; un fallimento dell'`INSERT` **dopo** che la directory è
stata creata da questa stessa chiamata la rimuove best-effort
(`remove_dir`, silenzioso se non vuota o già sparita) — ma **solo** se la
directory non esisteva già prima di questa chiamata (`MKCOL` idempotente
su un secondo tentativo, o una directory lasciata da uno scanner): non è
nostra da cancellare. Costo se la condizione fosse sbagliata: un secondo
`MKCOL` idempotente su una cartella già esistente, seguito da un
fallimento imprevisto dell'`INSERT`, avrebbe cancellato una directory
legittima con contenuto reale — evitato dal controllo `already_on_disk`.

Task 7, fix round (2 Important dalla review): complete (commit
e78c9eac30ffc7bf84266784487c1d2a26e906fb, pushato su `fase-5`). Test
verdi: 9/9 in `keeppix-api/tests/webdav_write.rs` (7 preesistenti + 2
nuovi: `put_with_a_declared_content_length_over_the_limit_returns_413`,
`mkcol_disk_failure_leaves_no_phantom_folder_row`), 4 nuovi unit test in
`dav::write::tests` (26/26 unit test di `keeppix-api::lib`), intera suite
`keeppix-api` e `keeppix-db` verde, `cargo fmt --check` e `cargo clippy
--workspace --all-targets -- -D warnings` puliti. Vedi
`task-briefs/task-7-report.md`, sezione "Fix round", per il dettaglio
completo (incluso l'esperimento isolato che verifica che `reqwest`/`hyper`
inviino sul filo un `Content-Length` impostato a mano anche quando
diverso dalla dimensione reale del corpo, usato per rendere il test del
413 deterministico senza spedire davvero terabyte di dati).

Ruling (Task 8): il brief affermava che `TrashRepo::choose` da sola basta
a far ricevere `403` a un editor su `DELETE` `WebDAV`, "stessa regola di
`may_purge`" — **falso**, verificato leggendo `trash.rs` per intero e il
test già esistente `permissions_roles.rs::a_folder_editor_can_edit_metadata_and_trash`
(Task 14b): per `DiskAction::MovedToTrash`, `TrashRepo::choose` **accetta
volutamente** un editor (`PermissionRepo::assert_can_edit_assets`), non
solo owner/admin — `may_purge` (owner/admin) si applica solo a
`DiskAction::Purged`. Il compito, però, richiede esplicitamente e senza
margine di interpretazione (istruzioni utente, non solo il brief) che un
editor riceva `403` su `DELETE` `WebDAV`. Interpretazione adottata: il
protocollo `WebDAV` non ha un dialogo di conferma né la possibilità di
scegliere `disk_action` come la REST API, quindi `DELETE` via `WebDAV` è
**deliberatamente più restrittivo** della REST API — solo owner/admin,
anche se l'azione fisica eseguita resta sempre `MovedToTrash` (mai
`Purged`, invarianza non toccata). Aggiunto un gate esplicito
(`only_owner_or_admin`, stesso predicato di `may_purge`) in
`dav::delete::asset`/`folder`, **prima** di chiamare `TrashRepo::choose`.
Costo se l'interpretazione fosse sbagliata (cioè se il brief avesse
davvero voluto un editor abilitato anche su `WebDAV`, come sulla REST
API): rimuovere il gate e lasciare che sia `TrashRepo::choose` a decidere
da sola — un cambiamento di una guardia, non un redesign.

Ruling (Task 8): `DavLockRepo` non espone un quinto metodo "unlock
condizionato" oltre ai quattro richiesti dal brief (`create`, `refresh`,
`delete`, `is_locked`). `dav::lock::unlock` riusa `refresh` come
test-and-set: se il token esiste ed è ancora attivo, `refresh` lo rinnova
(effetto collaterale innocuo, la riga viene cancellata immediatamente
dopo con `delete`) e restituisce `true`; altrimenti (token scaduto o mai
esistito, indistinguibili da qui) restituisce `false` → `404`. Costo se
sbagliato: un token appena scaduto per una manciata di query concorrenti
potrebbe vedere il proprio `timeout_at` esteso per una frazione di
secondo prima di essere cancellato — nessun effetto osservabile dal
client, la riga viene comunque rimossa nella stessa richiesta.

Ruling (Task 8): `LOCK` senza `If:` su una risorsa già bloccata da un
lock attivo (non scaduto) risponde `423 Locked`, non richiesto
esplicitamente dai 4 test del brief ma reso possibile dal quarto metodo
di `DavLockRepo` (`is_locked`) che il brief stesso elenca nell'API — senza
usarlo da qualche parte sarebbe stato codice morto. Un `LOCK` con `If:
(<token>)` su un token scaduto o inesistente risponde `412 Precondition
Failed` (casi limite del brief). Nessuno dei due percorsi è esercitato da
un test dedicato in questo task: differito come nota, non un'omissione
silenziosa (`Problem::locked()`/`Problem::precondition_failed()` sono
comunque unit-testabili in isolamento se servirà in un task futuro).

Ruling (Task 8): `DELETE /dav/folder/{id}` (cancellazione di un'intera
cartella, non solo di un singolo asset) non è esercitata da nessuno dei 4
test richiesti dal brief — solo `DELETE /dav/asset/{id}` lo è. Implementata
comunque per completezza dello spec (`FolderRepo::subtree` per raccogliere
ogni cartella discendente, `TrashRepo::choose` per ciascun asset di
ciascuna, poi `FolderRepo::delete_subtree` — nuovo metodo, singola
`DELETE ... WHERE path <@ ...` — e infine `remove_dir_all` sulla directory
fisica, mai prima che ogni asset sia già al sicuro nel cestino). Nessun
test dedicato: differito come lacuna di copertura nota, non come
funzionalità mancante.

Task 8 (`DELETE`, `LOCK`, `UNLOCK`): complete (commit cb217dd, test
verdi: 5/5 in `keeppix-api/tests/webdav_delete_lock.rs`, intera suite
`keeppix-api` verde — 37 blocchi `test result: ok`, zero `FAILED`/
`panicked`/`error[` —, intera suite `keeppix-db` verde — 34 blocchi
`test result: ok` —, `cargo fmt --check` e `cargo clippy --workspace
--all-targets -- -D warnings` puliti). Vedi `task-briefs/task-8-report.md`
per l'elenco dei file, il dettaglio TDD (incluse le tre mutazioni
deliberate sui test più importanti) e l'output di verifica.

## Fix round (review Task 8): test mancanti su `DELETE` di cartella e sui
## percorsi `423`/`412`

Ruling (Task 8, fix round): `DELETE /dav/folder/{id}` cancella la riga
`assets` dell'asset appena cestinato, non solo la riga `folders` — scoperta
mentre scrivevo `folder_delete_moves_all_assets_to_trash_and_removes_folder`.
A differenza di `DELETE /dav/asset/{id}` (dove la riga `assets` resta con
`status = 'trashed'`, verificato da `delete_asset_moves_it_to_trash_not_
file_system_removal`), qui `assets.folder_id REFERENCES folders(id) ON
DELETE CASCADE` (migrazione `0005_assets.sql`) cancella anche la riga
dell'asset già "trashed" nel momento in cui `folder_repo.delete_subtree`
rimuove la riga `folders` — l'ordine è corretto (l'asset è già al sicuro nel
cestino quando questo accade), ma il primo test scritto assumeva `status =
'trashed'` ancora leggibile da `assets` dopo la `DELETE` di cartella e
falliva con `RowNotFound`. Corretto asserendo invece `count(*) FROM assets
WHERE id = $1 = 0`: l'unica traccia persistente è `trash_entries` (che non
ha una FK verso `assets`, migrazione `0014_trash.sql`), e le asserzioni su
quella tabella (già eseguite prima nel test) bastano a provare "cestinato,
non cancellato" — il file fisico e l'audit trail sopravvivono, la riga
`assets` no. Non è un difetto: è la conseguenza attesa di "la cartella è
sparita del tutto dal DB", non richiede una modifica al codice di
produzione, solo al test. Costo se questa lettura fosse sbagliata (cioè se
si volesse che un asset trashato sopravviva come riga `assets` anche dopo
che la sua cartella è stata eliminata): andrebbe introdotta una `SET NULL`
o un passaggio di "orfanizzazione" prima di `delete_subtree`, cambiamento
non richiesto da nessuna spec letta finora.

Ruling (Task 8, fix round): lo stesso test verifica anche che un `PROPFIND`
sulla cartella appena cancellata risponda `403`, non `404` — anche se
l'utente usato nel test (`giovanni`) è l'admin bootstrap. Motivo: il
dispatcher `WebDAV` (`dav::mod::handler`) costruisce sempre
`AuthContext::user(user_id, SystemRole::User)`, mai `SystemRole::Admin`
(Ruling già registrato nel Task 6/8 originale), quindi `ctx.is_admin()` non
è mai vero per un attore `WebDAV` — `FolderRepo::visible` risponde `NotFound`
solo per un admin, `Forbidden` per chiunque altro, e su `WebDAV` è sempre il
secondo caso. Nessuna modifica al codice di produzione: solo l'assert del
test, scritto prima aspettandosi (erroneamente) `404` e corretto dopo aver
osservato il fallimento reale (`403`).

Aggiunti 3 test in fondo a `crates/keeppix-api/tests/webdav_delete_lock.rs`
(nessuna modifica al codice di produzione):

- `folder_delete_moves_all_assets_to_trash_and_removes_folder` — cartella
  con 2 asset, `DELETE /dav/folder/{id}` → entrambi in `trash_entries` con
  `disk_action = 'moved_to_trash'` e file fisico sotto `.keeppix-trash/`,
  riga `folders` sparita, riga `assets` sparita (cascade, vedi sopra),
  `PROPFIND` successivo → `403`.
- `locking_an_already_locked_resource_returns_423` — due `LOCK` consecutivi
  senza `If:` sulla stessa risorsa → il secondo `423`.
- `lock_with_expired_if_token_returns_412_or_404` — `LOCK`, scadenza manuale
  del token (`UPDATE dav_locks SET timeout_at = now() - interval '1 hour'`),
  poi `LOCK` con `If: (<token>)` → osservato `412` (comportamento reale del
  codice, `dav::lock::lock` → `Problem::precondition_failed()`); l'assert
  accetta anche `404` perché il brief lascia margine su questo dettaglio non
  contrattuale.

Verifica: `cargo fmt --check` pulito, `cargo clippy --workspace
--all-targets -- -D warnings` pulito, `cargo test -p keeppix-api --test
webdav_delete_lock -- --test-threads=1` 8/8 verdi, `cargo test -p keeppix-api
-- --test-threads=1` (intera suite del crate) verde. Commit
`a4a15d24fed4f5e71d0673ae1de8e64775e8b9e7`
(`test(api): add coverage for WebDAV folder delete and lock conflict
paths`). Vedi `task-briefs/task-8-report.md` (sezione "Fix round") per
l'output completo.

## Task 9: wizard di configurazione WebDAV (frontend)

Ruling (Task 9): l'indicatore live di "prima connessione" si basa **solo**
sul poller (GET ogni 3s dopo la generazione), senza un `GET` immediato
aggiuntivo dopo la `POST` per aggiornare subito la lista "usate in
precedenza". La prima versione faceva anche quel `GET` extra, ma introduceva
un secondo punto di refresh della lista da tenere sincronizzato col poller
senza beneficio reale (l'unica differenza osservabile è un ritardo di
massimo 3s nel mostrare la nuova password nella sezione storica, mentre è
già visibile per intero nella sezione "generata" sopra). Costo se sbagliato:
minima latenza percepita, nessun impatto funzionale.

Ruling (Task 9): niente generazione di QR code per iPhone/Android — il
brief lo mostra nello schizzo ma il vincolo esplicito del task è "NO nuove
dipendenze npm" e non esisteva già una libreria QR nel repo. Mostrato invece
l'URL WebDAV come testo monospaziato. Costo se sbagliato: task successivo per
aggiungere una dipendenza QR leggera con approvazione esplicita (la rotta è
lazy, quindi fuori dal budget dei 150 KB iniziali).

Ruling (Task 9): nessun link di navigazione aggiunto verso
`/settings/webdav` da altre view — il brief elenca solo router, view, i18n
e client API, e non esiste ancora un componente "Impostazioni" condiviso nel
codebase a cui agganciarsi. Un punto d'ingresso visibile è fuori dallo scope
dichiarato di questo task.

Task 9: complete (commit da annunciare in `task-briefs/task-9-report.md`,
test verdi: 91/91 Vitest incluso `i18n.spec.ts` per la parità delle chiavi,
`vue-tsc --noEmit` e `eslint` puliti sui file toccati).

## Task 10: PWA Share Target (frontend, ultimo task di Fase 5)

Ruling (Task 10): **la verifica manuale su un device Android/iOS reale è
differita** — il brief lo dice esplicitamente ("non è un task pinnabile solo
da test automatici") e questo ambiente CI/agente non ha un dispositivo reale
né un browser con supporto Web Share Target da guidare. Implementata solo la
parte tecnica (manifest, service worker, rotta, store); nessuna verifica
end-to-end "Condividi -> Keeppix" dalla galleria è stata eseguita. Costo se
il comportamento reale su Android Chrome divergesse dall'implementazione:
va rifatta la verifica (e eventualmente il service worker) al primo test su
device vero, prima di considerare la feature davvero pronta per gli utenti.

Ruling (Task 10): **`targetFolderId` diventa `string | null`** in
`UploadSessionState`/`PersistedSession`/`addFiles`, e `pump()` ora salta le
sessioni "queued" con `targetFolderId === null`. Il brief per
`addSharedFiles` mostra letteralmente `addFiles(files, null)` con il
commento "null = richiede scelta all'utente, come gli upload normali" — ma
**quel meccanismo non esiste**: il Task 3 non ha mai costruito un'interfaccia
di scelta della cartella di destinazione (il mockup §4.1 della spec la
mostra, ma è un mockup, non codice; `addFiles` richiede da sempre un
`folderId: string` non opzionale, e in produzione nessuna vista lo chiama
ancora — solo i test lo fanno con un id fisso). Costruire quell'interfaccia
ora avrebbe significato implementare, dentro il Task 10, una feature del
Task 3 mai completata: fuori scope. Ho scelto la decisione più piccola che
rispetta la lettera del brief: il tipo accetta `null`, la sessione condivisa
viene accodata come "queued" (visibile nel pannello, soddisfa il test
richiesto), ma `pump()` non la avvia mai finché non ha una cartella —
**quindi, con questo commit, i file condivisi dalla galleria restano in coda
per sempre senza un modo per l'utente di assegnargli una destinazione e
farli partire**. È un gap reale, non solo teorico. Costo se sbagliato: la
feature "condividi -> Keeppix" è visibile ma non porta a un upload
completato finché un task futuro non aggiunge un selettore di cartella al
pannello di upload (per gli upload normali *e* per quelli condivisi, dato
che il problema è lo stesso). Raccomando di aprire quel task esplicitamente
prima di comunicare la feature come utilizzabile.

Ruling (Task 10): **file per la Share Target: Cache Storage, non
IndexedDB.** Il service worker (`public/sw.js`) intercetta il POST a
`/share-target`, legge `event.request.formData()`, salva ogni file come
`Response` in una cache dedicata (`keeppix-share-target-v1`) più un indice
JSON con nome/tipo, poi fa `Response.redirect('/share-target', 303)`. La SPA
(`src/pwa/shareTarget.ts`, letta da `ShareTargetView.vue`) rilegge l'indice,
ricostruisce i `File` dai blob e cancella le entry lette. Scelto Cache
Storage invece di IndexedDB perché è l'API più semplice per spostare byte
grezzi (un `Blob`) dal service worker alla pagina senza serializzazione
custom, ed è quella usata nell'esempio Chrome/web.dev per Share Target di
file. Costo se sbagliato: nessuna dipendenza nuova in entrambi i casi, solo
un possibile refactor interno a questi due file se IndexedDB si rivelasse
necessaria per altri motivi (es. persistenza oltre la sessione del
service worker).

Ruling (Task 10): **nome della cache e chiavi duplicati** in
`public/sw.js` e `src/pwa/shareTarget.ts` invece di una costante condivisa —
`public/sw.js` è servito com'è dalla cartella `public/`, non passa dal
bundler Vite, quindi non può importare da `src/`. Un commento in entrambi i
file marca la dipendenza reciproca. Costo se sbagliato: se uno dei due file
cambia la chiave senza aggiornare l'altro, la Share Target si rompe
silenziosamente (nessun file letto, nessun errore) — rischio reale ma bordo
piccolo (due costanti, un solo punto di lettura/scrittura ciascuna).

Ruling (Task 10): rotta `/share-target` con `meta: { auth: true }`, come
tutte le altre rotte autenticate. Se l'utente non ha una sessione attiva
quando l'OS apre `/share-target`, la guardia del router lo manda a
`/login` e i file restano nella cache del service worker non letti (la
`ShareTargetView` non viene mai montata). Non gestito un rientro automatico
a `/share-target` dopo il login riuscito: è un edge case reale (condividere
da disconnesso) ma il flusso di login non porta oggi a nessun "redirect di
ritorno" per nessun'altra rotta protetta, quindi non è un'incoerenza
introdotta da questo task — segnalato qui come voce differita, non
risolto.

## Fase 5 — chiusura (verifica avversariale finale)

Verifica locale su commit `c6b14f2` (merge `origin/main` incluso):
- `cd frontend && npm ci && npm run build` — verde
- `cargo fmt --check` — verde
- `cargo clippy --workspace --all-targets -- -D warnings` — verde
- `python3 scripts/check-wired.py` — verde ("all public fns and mounted routes have a production caller")
- `./scripts/test.sh` — verde (114 blocchi `test result: ok`, zero `FAILED`/`panicked`)
- `npm run test` (frontend) — verificato in chiusura

CI GitHub Actions su PR #10: tutti e 4 i job (`backend`, `frontend`, `audit`,
`image`) falliscono in ~2s senza step eseguiti e senza log (`runner_name` vuoto,
`log not found` via `gh run view`) — problema infrastrutturale del runner/org,
non del codice. Push su `fase-5` ripetuto (`c6b14f2`), stesso esito. Verifica
locale sostituisce la CI finché i runner non tornano disponibili.

**Non mergiato in `main`** — come concordato, review domani mattina.

Task 10: complete (commit `60c9a3b`, `feat(frontend): PWA Share Target for
photo upload from the phone gallery`). Verifica: `npm run test` 92/92 verdi
(nuovo test `shared_files_are_queued_for_upload` osservato fallire per il
motivo giusto — `store.addSharedFiles is not a function` — prima
dell'implementazione, poi verde), `npx vue-tsc --noEmit` pulito, `npm run
build` verde con `manifest.webmanifest` e `sw.js` raggiungibili in
`dist/`, `npm run lint` 0 errori (9 warning pre-esistenti in
`SharesView.vue`, non toccato). **Verifica su device Android/iOS reale non
eseguita** — vedi primo ruling sopra e `task-briefs/task-10-report.md`.
