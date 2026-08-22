# Fase 8 — progress ledger

Branch: `fase-8` from `main` (post Fase 10 + Fase 7 merge).
Piano: `docs/superpowers/plans/2026-08-20-keeppix-fase-8.md` (11 task).
Spec: `docs/superpowers/specs/fase-8-volti.md`.
Sessione autonoma (nessun revisore esterno): il §10 di `docs/superpowers/PROSEGUI.md`
sostituisce la review prima del merge.

## Ambiente di questa sessione — leggere prima di continuare

Questa sandbox non ha `docker` (niente daemon): `./scripts/test.sh` e i test con
testcontainers non partono così come sono scritti. Due workaround, **locali a
questa sessione, non nel repository**:

1. **Postgres reale senza container**: `postgresql-16` + `postgresql-16-postgis-3`
   + `postgresql-16-pgvector` installati via `apt` (rete permessa verso
   `archive.ubuntu.com`), servizio avviato con `service postgresql start`,
   password `postgres` impostata. `crates/keeppix-db/tests/harness/mod.rs` usa
   **direttamente** questo server quando `KEEPPIX_TEST_DATABASE_URL` è
   impostata e il server offre `vector` — bypassa testcontainers per
   costruzione, non serve toccare il harness. Usato per **tutti** i test DB di
   questa fase.
   Nota: Postgres **16**, non 17 come l'immagine CI (`postgis/postgis:17-3.5`).
   Nessuna feature di questa fase dipende dalla versione minor di Postgres;
   la CI reale (17) resta il gate finale prima del merge.
2. **`ort` non scarica il runtime prebuilt** (`cdn.pyke.io` non è
   nell'allowlist di rete di questa sandbox: 403 sul CONNECT). Risolto
   installando `onnxruntime` via `pip install --target /tmp/ort-pip
   onnxruntime` (pypi.org è in allowlist) e puntando `ort-sys` alla sua
   `libonnxruntime.so` invece di scaricarla:
   `ORT_LIB_LOCATION=/root/ort-lib ORT_PREFER_DYNAMIC_LINK=1
   LD_LIBRARY_PATH=/root/ort-lib:$LD_LIBRARY_PATH` (con i symlink
   `libonnxruntime.so`/`libonnxruntime.so.1` → `libonnxruntime.so.1.29.0`
   dentro `/root/ort-lib`, SONAME compreso — senza il symlink versione-1 il
   linker dinamico non trova la libreria a runtime, solo a link time).
   Sblocca `cargo check/test/clippy` su `keeppix-media`/`keeppix-jobs`/
   `keeppix-api`/`keeppix-server`, che altrimenti non compilano affatto in
   questa sandbox (confermato: fallisce identico anche su `main`, non è un
   difetto introdotto da questa fase). **La CI reale ha rete verso
   `cdn.pyke.io`** (GitHub Actions), quindi questo non è un problema lì — è
   solo il modo in cui *questa sessione* può verificare localmente prima di
   spingere.

Senza pesi ONNX reali per SCRFD/ArcFace (nessuna rete verso HuggingFace da
questa sandbox — stesso limite già noto per MobileCLIP2-S2 in Fase 7, vedi
`models/README.md`), tutti i percorsi che richiedono i pesi restano nello
stato degradato esplicito (`inference_status = model_missing`), stesso
contratto già usato da `keeppix-media::ai::measure_image_inference` per CLIP.
La misura reale del Task 2 (ms di rilevamento/impronta) **non è ottenibile in
questa sandbox** — va presa nota qui come limite dichiarato, non finta.

## Divergenze dal piano scritto prima del codice reale — decise ora

Ruling: **niente crate `face_id`.** — Il piano (scritto prima di Fase 7)
proponeva il crate `face_id` "stesso autore e stesso stack `ort`" di CLIP.
Verificato: `face_id 0.4.4` pinna `ort = "2.0.0-rc.13"`, mentre
`keeppix-media` già naviga `ort` con un pin proprio risolto (in `Cargo.lock`)
a `2.0.0-rc.13` a sua volta — ma tramite un uso diretto del crate, non un
wrapper. Usare `face_id` aggiungerebbe una seconda dipendenza pesante
(`hdbscan`, `hf-hub`, `nalgebra`, `ndarray`, `bon`, ecc. — la maggior parte
mai usata perché disabilitiamo `default-features` per il vincolo "zero rete
esterna" di `hf-hub`) proprio nel nome di "un solo stack di inferenza", che è
esattamente ciò che si otterrebbe MEGLIO scrivendo SCRFD/ArcFace a mano con
lo stesso `ort` già in `keeppix-media`, stesso pattern (`clip.rs`/`ai.rs`).
— *Costo se sbagliato*: qualche settimana di lavoro in più per implementare
l'allineamento Umeyama a mano invece di riusarlo da `face_id`; accettabile,
è un algoritmo di ~40 righe, chiuso e ben noto.

Ruling: **IVFFlat su `faces.embedding`, non HNSW.** — La spec fase-8-volti.md
§3 scriveva `faces_hnsw`, ma è stata scritta prima che Fase 7 misurasse e
scegliesse esplicitamente IVFFlat per `asset_embeddings`
(`0045_asset_embeddings_ivfflat.sql`) **per la stessa ragione hardware** che
vale anche qui (build/RAM più leggeri sul Pi 8 GB) — la spec non è stata
aggiornata dopo quella scelta. Vince la decisione più recente e misurata
(AGENTS.md: "se spec e piano divergono, vince la spec" si applica quando la
spec è la fonte più aggiornata; qui la spec fase-8 precede la scelta di
Fase 7 su un vincolo hardware condiviso, quindi si riusa la scelta già presa
e misurata invece di introdurre una seconda politica di indicizzazione
vettoriale nello stesso progetto). — *Costo se sbagliato*: se in produzione
il recall IVFFlat risultasse insufficiente per il matching dei volti (a
differenza della ricerca semantica, qui serve un nearest-neighbour più
preciso), si ripesa con un numero reale davanti, non a priori.

Ruling: **`faces.proposed_person_id`/`proposed_score` aggiunti allo schema
(non nella spec §3).** — Senza un candidato scritto da qualche parte, la
coda di revisione (Task 8, "Questi volti sembrano Giovanni") non ha un nome
da proporre: la spec §4.1 descrive lo stato "proposto" ma non gli dà una
colonna. Due colonne nullable su `faces`, non una tabella `face_proposals`
separata — un volto ha *al massimo un* candidato alla volta (il centroide
più vicino), a differenza di `asset_tags` dove ogni (asset, tag) è una riga
propria perché un asset può avere N proposte di tag contemporaneamente. —
*Costo se sbagliato*: se in futuro serve mostrare più di un candidato per
volto, va comunque introdotta una tabella; nessuna migrazione già applicata
va toccata, si aggiunge.

Ruling: **`libraries.faces_enabled` di default `true`.** — Nessun precedente
esplicito nel codice reale (Fase 7 non ha introdotto un interruttore IA per
libreria, l'unico precedente è `scan_enabled` che è `true` di default). Non
essendoci un requisito esplicito "spento finché l'utente non lo accende" nella
spec §7 (che dice "disattivabile", non "disattivato di default"), si è scelta
coerenza con `scan_enabled` piuttosto che un opt-in silenzioso che
sorprenderebbe l'utente aspettandosi lo stesso comportamento delle altre
funzioni IA (Fase 7 CLIP è anch'essa attiva di default via `scan_enabled`).
— *Costo se sbagliato*: un utente che non vuole il riconoscimento facciale lo
scopre dopo, non prima; mitigato dal fatto che Task 10 rende l'interruttore
visibile e la cancellazione dei dati sempre disponibile.

## Gruppo A — Fondamenta

### Task 3 — Schema (fatto insieme al Task 1, vedi sotto)

Migrazione `0046_faces.sql`: `faces`, `persons`, `person_groups`,
`person_group_members`, `person_separations`, `libraries.faces_enabled`.
Stesso contratto no-op di 0043/0045 se pgvector non è installato. Ordine di
creazione: `persons` nasce senza vincolo FK su `cover_face_id` (colonna
nuda), `faces` la referenzia, poi un `ALTER TABLE persons ADD CONSTRAINT`
chiude il cerchio — le due tabelle sono mutuamente referenziate.

### Domain layer

`FaceId`/`PersonId`/`PersonGroupId` (`keeppix-domain/src/ids.rs`),
`Face`/`FaceBBox`/`Person`/`PersonName`/`PersonGroup`/`PersonSeparation`
(nuovo modulo `keeppix-domain/src/face.rs`). `PersonName::parse` rifiuta il
vuoto (Task 6: "il prototipo non lo controlla, è un difetto"). Aggiunta
`OperationKind::FaceDetection` — stesso involucro `Operation` che
`AiAnalysis` già usa per la Fase 7 (nessun nuovo evento WS, `drain_operations`
è già generico su `OperationKind`).

### DB layer — `FaceRepo`, `PersonRepo`, `PersonGroupRepo`

`FaceRepo`: `insert_detected` (pipeline), `list_for_asset`/`assign`/`reject`
(con `AuthContext`, visibilità via `AssetRepo::assert_visible`),
`auto_assign`/`propose` (pipeline, raggruppamento incrementale),
`list_proposed`/`confirm_proposal`/`count_proposed_visible` (coda di
revisione, stessa forma di `AssetTagRepo` per i tag). Un volto assegnato a
mano (`assigned_by` impostato) non è mai più toccato da `auto_assign`
(verificato da un test: `a_human_assigned_face_is_never_touched_by_auto_assign`).

`PersonRepo`: la visibilità di una persona è **transitiva** — non ha una
cartella propria, quindi `find_by_id`/`list_visible` verificano che almeno un
suo volto confermato sia visibile al chiamante secondo la stessa
`VisibilityScope` usata ovunque nel progetto. Un `AuthContext::ShareLink`
(nessun `user_id`) non vede mai nessuna persona — verificato da un test
dedicato (`a_share_link_never_sees_any_person`), coerente con la regola
Task 1. `merge`/`separate` implementano spec §4.2: unire riassegna i volti e
tiene il nome della persona con nome (sopravvissuta o assorbita), separare
crea una persona nuova **senza ripristinare uno stato precedente** (domanda
aperta n.5) e scrive `person_separations`.

Ruling: **la separazione blocca l'automatismo così — la persona che ha
almeno una separazione registrata va sempre in "proposto", mai in
"assegnato automaticamente".** — Il piano dice solo "person_separations
blocca permanentemente il riaccorpamento automatico", senza specificare il
meccanismo esatto: implementare una soglia di margine fra il primo e il
secondo centroide più vicino richiederebbe un secondo confronto pgvector per
ogni volto nuovo, solo per le persone con uno storico di separazioni (rare).
La regola più semplice — "chi è stato separato almeno una volta passa sempre
dalla revisione umana per ogni assegnazione automatica successiva" — non
causa mai un'assegnazione automatica silenziosa sbagliata, al costo di
qualche voce in più nella coda di revisione. `PersonRepo::has_any_separation`
espone il booleano; il consumo è nel raggruppamento incrementale (Task 5,
sotto). — *Costo se sbagliato*: più coda di revisione del necessario per le
persone con uno storico di separazioni; mai un errore silenzioso.

**Difetto trovato dal test, non dalla revisione**: la prima stesura di
`assign`/`reject`/`auto_assign` non richiamava mai
`PersonRepo::recompute_centroid` — solo `merge`/`separate` lo facevano. Il
test `centroid_is_the_average_of_confirmed_embeddings` in
`persons.rs` falliva (`centroid` restava `NULL` dopo un'assegnazione
manuale semplice). Corretto: ogni punto che cambia la composizione di una
persona (`assign`, `reject`, `auto_assign`, `confirm_proposal`, oltre a
`merge`/`separate` che già lo facevano) ricalcola i centroidi delle persone
coinvolte (`FaceRepo::recompute_affected_centroids`). Esattamente il tipo di
difetto che PROSEGUI.md §10 chiede di cercare — qui l'ha trovato un test
scritto per un'altra ragione (verificare la media), non una lettura del
codice a posteriori.

`PersonGroupRepo`: CRUD + membership, nessun calcolo (spec §5.1). Distinto
dai `groups` di Fase 3 (permessi utenti) — tabelle separate, stesso schema
del piano.

**Test**: 24 test di integrazione (`faces.rs`, `persons.rs`) contro Postgres
reale (vedi "Ambiente" sopra). `cargo clippy -p keeppix-db -p keeppix-domain
--all-targets -- -D warnings`: pulito. `cargo fmt --check`: pulito dopo
`cargo fmt`.

## Gruppo A — Task 1: il test della regola, verificato subito

**Ruling: Task 1 e Task 3 eseguiti insieme, non in sequenza rigida come
scritto nel piano.** — Il piano ordina "Task 1 prima di tutto, fallirà per
assenza di volti: va bene". In pratica, costruire la foto di test con un
volto confermato richiede *qualcosa* con cui inserire un volto — schema e
repository minimi. Si è scelto di far nascere schema+repo e test della
regola nello stesso arco di lavoro, invece di lasciare il test rosso per
compilazione fino a Task 3: il rischio che il piano voleva evitare (scrivere
la regola *dopo* codice che potrebbe già violarla) è comunque coperto, perché
il codice di `share.rs` (route pubbliche) non è stato toccato prima che
questo test esistesse e passasse. — *Costo se sbagliato*: nessuno osservato;
il test è verde ed è lo scenario più favorevole a una fuga (persona con nome,
volto confermato, sia condivisione per cartella sia per singolo asset).

`crates/keeppix-api/tests/face_privacy.rs`: cammina ricorsivamente l'intero
corpo JSON di `GET /api/v1/share/{token}` e `GET /api/v1/share/{token}/assets`
(via un vero scan, un vero `PersonRepo`/`FaceRepo::assign` confermato, un vero
link pubblico) e rifiuta qualunque chiave contenga "face" o "person" — non
solo i campi noti oggi, così un campo aggiunto per distrazione in futuro fa
fallire il test comunque. Un terzo test (`the_scanner_itself_catches_a_planted_leak`)
prova che lo scanner di chiavi funziona davvero, piantando una fuga finta.
**3/3 verdi.** Nessun percorso in `share.rs`/`AssetView` tocca `faces` oggi,
quindi il test parte già verde — resta la condizione di chiusura della fase
(Task 11): deve **restare** verde attraverso tutti i task successivi che
toccano volti/persone.

## Task 1: complete (schema+repo+test, commit su `fase-8`, 24+3 test verdi)
## Task 3: complete (stesso commit)

## Task 2 — Modelli e misura: SBLOCCATO parzialmente, misura reale NON ottenuta

Ambiente sbloccato: `ort` collegato a `libonnxruntime.so` locale (pip
onnxruntime, vedi "Ambiente" in cima) — `cargo check/test/clippy` girano su
`keeppix-media`/`keeppix-jobs`/`keeppix-api`/`keeppix-server` in questa
sandbox, cosa non scontata (senza il workaround falliscono identico anche su
`main`, confermato).

`crates/keeppix-media/src/align.rs`: allineamento per similarità 2D
(rotazione+scala uniforme, mai riflessione) via proiezione ai minimi
quadrati su numeri complessi — soluzione chiusa esatta per una trasformazione
a 4 gradi di libertà, non una SVD generica. 9 test (traslazione/rotazione/
scala note, immunità a riflessione, warp bilineare).

`crates/keeppix-media/src/face.rs`: `FaceModels::load` (`detect.onnx` +
`embed.onnx` sotto `models/scrfd-arcface/`), `detect()` (decodifica SCRFD a
3 stride, output letti per **nome** — `score_{8,16,32}`/`bbox_{8,16,32}`/
`kps_{8,16,32}` — non per indice posizionale, così un export con ordine
diverso fallisce in modo esplicito), `embed_face()`/`embed_aligned()`
(`ArcFace`, **non** L2-normalizzato dalla funzione: la normalizzazione non
serve prima di `pgvector`, che calcola la distanza coseno sui vettori grezzi
— vedi Task 5). 15 test sulla parte pura (anchor, `distance2bbox`/`kps`,
NMS, letterbox) — nessuno tocca `ort`.

**Limite non risolto, da segnalare esplicitamente**: a differenza di
MobileCLIP2-S2 (Fase 7), per SCRFD/ArcFace **non esiste in questo
repository uno script di download né una fonte verificata di pesi ONNX
reali** (`scripts/download-mobileclip2-s2.sh` ha un URL HuggingFace preciso,
scelto e verificato durante la Fase 7; qui non c'è l'equivalente). Non ho
inventato/indovinato un URL di un export SCRFD/`ArcFace` da terzi senza
poterlo verificare — sarebbe più rischioso di lasciare il gap dichiarato.
**Conseguenza**: la misura reale richiesta da Task 2 ("misurare su hardware
vero, mettere il numero nel ledger") **non è stata ottenuta, né qui né
sarebbe ottenuta in CI** (CI non ha uno script equivalente a
`download-mobileclip2-s2.sh` per i volti). `ASSIGN_SIMILARITY`/
`PROPOSE_SIMILARITY` in `detect_faces.rs` (Task 5) sono stime di partenza
ragionevoli per una similarità coseno `ArcFace`, non calibrate su dati
reali — dichiarato nei commenti del codice. **Serve una decisione umana**:
quale checkpoint SCRFD-500MF/`ArcFace` ONNX usare, e uno
`scripts/download-scrfd-arcface.sh` equivalente — fuori da quello che questo
agente può decidere in autonomia senza rete verificabile.

Osservazione collaterale (non un difetto introdotto qui): il test Fase 7
`embed_job::backfill_schedule_is_background_and_deduped`
(`crates/keeppix-jobs/tests/embed.rs`) non ha la guardia
`first_complete_model_dir().is_none() → skip` che hanno gli altri test dello
stesso file, quindi **fallisce in questa sandbox** (pesi CLIP assenti) pur
passando in CI (che li scarica). Non toccato: fuori dallo scope di Fase 8,
e "non sistemare codice fuori dal task corrente" (AGENTS.md).

## Task 2: parziale — codice e misura-quando-presente pronti; numero reale non ottenibile qui

## Task 4/5 — Rilevamento + raggruppamento incrementale

`crates/keeppix-jobs/src/detect_faces.rs`, `JobKind::DetectFaces`,
`OperationKind::FaceDetection`. Stesso scheletro di `embed.rs`: una sessione
ONNX per finestra, lotti da `DEFAULT_BATCH_SIZE=16`, gate di pausa fra un
lotto e l'altro, `Operation` con progress/cancel.

Pipeline per asset: rilevamento sulla miniatura 240px → per ogni volto
abbastanza grande (`MIN_FACE_SIZE_REL = 0.03` del lato corto — sotto,
**volutamente non riconosciuto**, spec Task 4 punto 4), impronta sulla
preview 2048px → `FaceRepo::insert_detected` → raggruppamento incrementale
→ `FaceRepo::mark_scanned` sempre, anche a zero volti o su fallimento
(miniatura assente, decodifica fallita): un asset irraggiungibile non deve
bloccare la coda, stesso spirito di `embed::ThumbLoadError::Missing`.

Ruling: **`PersonRepo::nearest_centroid` usa un k-NN pgvector su
`persons.centroid`** (non su `faces.embedding`, che pure ha l'indice
IVFFlat): il numero di persone è tipicamente ordini di grandezza sotto il
numero di volti, quindi una scansione sequenziale delle persone è già
veloce e non serve un secondo indice per questa query — l'IVFFlat su
`faces.embedding` resta per un eventuale «trova volti simili» futuro, non
consumato da questa fase. — *Costo se sbagliato*: con centinaia di migliaia
di persone (scenario non realistico per una libreria personale) servirebbe
un indice anche lì.

Ruling (già in `detect_faces.rs`): una persona con almeno una separazione
registrata non riceve mai un'assegnazione automatica certa — sempre
proposta. Vedi commento nel codice per il perché (evitare un secondo
confronto pgvector per volto).

9 test in `crates/keeppix-jobs/tests/detect_faces.rs`: coda vuota senza
richiedere pesi, errore esplicito a coda piena senza modello (lavoro non
sparisce dalla coda), validazione `limit_from_payload`, e un test end-to-end
che salta senza pesi reali (stesso pattern dichiarato sopra).

## Task 4: complete (pipeline; misura ms reale non ottenuta, vedi Task 2)
## Task 5: complete (raggruppamento incrementale; soglie da ricalibrare quando ci saranno pesi reali)

## Gruppo B/C — Task 6/7/8: API pannello foto, CRUD persone/gruppi, coda di revisione

`crates/keeppix-api/src/routes/faces.rs` (Task 6/8): `GET /assets/{id}/faces`,
`POST /faces/{id}/assign|reject`, `GET /faces/proposals`,
`POST /faces/{id}/confirm`, `POST /persons/{id}/proposals/confirm|reject`
(azioni in blocco — «conferma/rifiuta tutti» per una persona candidata,
involucro `FaceBulkOutcome` nuovo in `bulk.rs`, stesso disegno di
`BulkOutcome` già usato per i tag ma non generico su di esso: `BulkOutcome`
è tipizzato su `AssetId` in 10 punti già esistenti, un refactor a generico
avrebbe toccato codice fuori scope per un guadagno solo cosmetico qui).

`crates/keeppix-api/src/routes/persons.rs` (Task 6/7): CRUD persone
(`list`/`create`/`get`/`patch`/`delete`), `merge`/`separate`, CRUD gruppi di
persone e membership — 15 route in più. `PersonView` ha `face_count`
opzionale: presente in `GET /persons` (via `PersonSummary`, un giro di
query in più già pagato dal repository), assente nelle risposte di singola
persona per non pagarlo due volte.

Ruling: **«nuova persona» dalla coda di revisione non ha una route
dedicata** — il client fa `POST /persons` (nome vuoto o dato) poi
`POST /faces/{id}/assign` con l'id ottenuto. Una route unica
`POST /faces/{id}/assign-new-person` risparmierebbe un giro di rete alla UI
Fase 11, ma introdurrebbe un secondo percorso per la stessa operazione
(creare+assegnare) che il resto dell'API non ha in nessun altro punto —
tag e cartelle seguono lo stesso pattern «crea, poi referenzia». — *Costo
se sbagliato*: un giro di rete in più lato client, nessun costo lato
server/dati.

`crates/keeppix-api/src/routes/bootstrap.rs`: il badge `revision` ora somma
`AssetTagRepo::count_proposed_visible` (Fase 7) e
`FaceRepo::count_proposed_visible` (qui) — stesso contratto "quante cose
aspettano una decisione dell'utente" della Fase 7, un solo numero invece di
due badge separati (la spec non distingue "proposte tag" da "proposte
volti" nel badge globale).

**Test**: `crates/keeppix-api/tests/persons.rs`, 10 test end-to-end via
router reale (CRUD, merge, separate, coda di revisione, badge, 403/422).
`face_privacy.rs` (Task 1) resta 3/3 verde dopo queste route — verificato
di nuovo qui, non solo alla chiusura (Task 11 lo riverifica un'ultima
volta). `openapi.rs`: 8/8 verdi dopo aver aggiornato i due array
hardcoded (paths, operation_ids) e il conteggio totale via lo script di
rigenerazione della suite stessa (`UPDATE_OPENAPI=1 cargo test`), non a
mano.

## Task 6: complete (pannello dettagli foto: elenco/assegna/rifiuta)
## Task 7: complete (CRUD persone/gruppi, merge/separate)
## Task 8: complete (coda di revisione, azioni in blocco, badge bootstrap)

## Bonifica `scripts/check-wired.py` dopo Task 6/7/8

Il guardiano ha segnalato 4 funzioni morte e 14 route senza consumatore
frontend. Le 14 route sono attese (Fase 11 le consuma, vedi
`scripts/wired-exceptions.txt` — voce nuova, stessa forma delle voci
`/tags*` già lì per lo stesso motivo in Fase 7). Le 4 funzioni erano un
segnale reale, non falso positivo:

Ruling: **`embedding_of`/`list_unassigned_with_embedding` non erano morte
per errore di scope — indicavano un buco di correttezza vero.**
`FaceRepo::mark_scanned` viene chiamato incondizionatamente per ogni asset
(anche quando `group_face` fallisce a metà: un volto già inserito con
un'impronta calcolata, ma mai confrontato via pgvector e mai assegnato/
proposto) — un volto così resta orfano per sempre, perché il prossimo giro
di `process_pending` non lo rivede più (l'asset non è più "in coda", lo
scan è segnato fatto). Corretto aggiungendo
`detect_faces::regroup_stragglers`, chiamato all'inizio di `run()`: rilegge
i volti con impronta ma senza `person_id`/`proposed_person_id` (tramite le
due funzioni che il guardiano segnalava) e li fa ripassare da
`group_face`. Lotto piccolo (`STRAGGLER_BATCH_LIMIT = 200`): l'evento è
raro (solo un fallimento a metà pipeline), non un volume paragonabile alla
coda principale. — *Costo se sbagliato*: senza questo fix, un volto
orfano non compare mai né come assegnato né in coda di revisione — invisibile,
non solo lento; con `STRAGGLER_BATCH_LIMIT` troppo basso, si accumula un
arretrato che si smaltisce in più finestre invece che in una sola (mai un
dato perso, solo più lento a comparire).

Ruling: **`find_by_id_for_pipeline` (`faces.rs`) e `is_separated`
(`persons.rs`) rimossi, non messi in `wired-exceptions.txt`.** — Nessun
consumatore legittimo a breve termine: l'unico uso plausibile di
`find_by_id_for_pipeline` sarebbe stata una route HTTP, ma bypassa
`assert_face_visible`/`AuthContext` per costruzione (il nome dice "pipeline",
cioè pensata per codice interno senza contesto utente) — usarla in una
route violerebbe l'invariante "ogni metodo repository esposto a HTTP
verifica la visibilità" (AGENTS.md). `is_separated` non aveva alcun
chiamante nemmeno ipotizzabile: `has_any_separation` (usato da Task 5) fa
lo stesso lavoro con un nome più preciso (booleano "è mai stata separata",
non "è separata da chi"). Riscritti 9 punti di test (8 in `faces.rs` via un
nuovo helper `fetch_face_state`/SQL grezzo, 1 in `persons.rs`) per leggere
lo stato via SQL diretto invece che tramite queste funzioni — i test
restano equivalenti (stessa asserzione, stessa colonna), solo il percorso
per leggerla cambia. — *Costo se sbagliato*: se in futuro serve davvero un
"leggi un volto per id senza contesto utente" lato pipeline, va reintrodotta
con quel nome e quel confine chiaro, non resuscitata con lo stesso nome
generico che invitava all'uso sbagliato.

`python3 scripts/check-wired.py`: pulito dopo questi due interventi + le
voci nuove in `wired-exceptions.txt`. `cargo fmt --all -- --check`: pulito.
`cargo clippy --workspace --all-targets -- -D warnings`: pulito (due errori
nuovi risolti in questo giro: un tipo di riga SQL a 5 colonne in un test
estratto in un `type` alias invece di essere inline — `clippy::type_complexity`
sul tuple letterale; un commento `///` che era scivolato per sbaglio da
`run()` sopra `STRAGGLER_BATCH_LIMIT` durante l'inserimento del fix
straggler, lasciando `run()` senza `# Errors` — spostato al posto giusto).
