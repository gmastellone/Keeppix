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

## Gruppo D — Task 9: `SearchNode::Person`/`PersonGroup`/`PersonCount`

`crates/keeppix-db/src/search.rs`: tre varianti nuove sull'AST esistente
(non un motore a parte — stesso approccio di Fase 7 Task 10 per
`Tag`/`Category`/`Semantic`), compilate da una nuova `compile_fase8_axis`
(stessa ragione di `compile_fase7_axis`: restare sotto il tetto di righe
per funzione di clippy).

- `Person { id }` → `EXISTS` su `faces.person_id = id`. Basta questa sola
  condizione: `FaceRepo::reject` pulisce `person_id` insieme a
  `rejected_at`, quindi un volto rifiutato non ha mai `person_id`
  valorizzato — non serve escludere esplicitamente i rifiutati come fa
  `list_for_asset` a livello applicativo.
- `PersonGroup { id }` → stesso `EXISTS`, con un `JOIN
  person_group_members` — "almeno una persona del gruppo" (spec §6).
- `PersonCount { cmp, value }` → `COUNT(DISTINCT fc.person_id)` sui volti
  dell'asset, non `COUNT(*)` sui volti: due volti della stessa persona
  nella stessa foto contano una volta sola. Riusa `IsoCmp`/`cmp_op`, stesso
  pattern di `Rating`/`Iso`.

Nessuna delle tre richiede una propria clausola di visibilità: `a` (la
tabella `assets` della query esterna) è già filtrata da `VisibilityScope`
in `run`, la stessa assunzione già usata da `Tag`/`Category`. Ruling:
**`compile_fase8_axis` resta `Result`-returning con
`#[allow(clippy::unnecessary_wraps)]`**, anche se nessuno dei tre nodi
fallisce mai — la firma uniforme con le funzioni sorelle (`compile_fase7_axis`,
`compile_search_axis`) evita di spezzare la simmetria del dispatch, dove il
chiamante propaga sempre con `?`. — *Costo se sbagliato*: nessuno, è solo
una scelta di stile.

Nessuna route/`SearchRequest` da toccare: `SearchRequest.ast` è già
documentato in OpenAPI come `#[schema(value_type = Object)]` (l'intero AST
non è enumerato per variante), quindi aggiungere varianti a `SearchNode`
non tocca `openapi.rs`/`docs/api/openapi.json` — verificato, non assunto.

**Test**: 3 nuovi in `crates/keeppix-db/tests/search.rs`
(`person_filter_matches_only_assigned_faces`,
`person_group_filter_matches_any_member`,
`person_count_filter_counts_distinct_persons_not_faces`), stesso stile dei
test `tag_filter_matches_only_confirmed_assignments`/
`category_filter_matches_confirmed_child_tags` già presenti. 28/28 verdi
in `search.rs` (25 preesistenti + 3 nuovi). `check-wired.py`: pulito (il
compilatore dell'AST è già raggiunto da `SearchRepo::run`, produzione
reale, non serve un'eccezione). `cargo fmt --all -- --check` e
`cargo clippy --workspace --all-targets -- -D warnings`: puliti.

Il chip «Persona» nel prototipo resta disabilitato fino alla Fase 11 (è
lì che si costruisce l'interfaccia che lo userebbe) — questo task sblocca
solo il lato server, come da piano.

## Task 9: complete

## Task 10 — Interruttore e cancellazione (la parte che mancava)

L'interruttore per libreria (`libraries.faces_enabled`, `PATCH
/libraries/{id}`) era già stato chiuso in un commit precedente insieme allo
schema (Task 3): colonna sulla stessa riga di `scan_enabled`, stesso
pattern, `list_pending_scan` lo rispetta. Qui si chiude la seconda metà
distinta dalla spec §7/domanda aperta n.6: **«Elimina tutti i dati dei
volti»**, un comando separato ed esplicito che cancella invece di
sospendere.

`FaceRepo::delete_all_data` (nuovo, `crates/keeppix-db/src/faces.rs`):
`DELETE FROM person_groups` (cascata su `person_group_members`), poi
`DELETE FROM persons` (cascata su `person_separations`, `SET NULL` su
`faces.person_id`/`proposed_person_id`), poi `DELETE FROM faces`
(embedding compresi), poi `DELETE FROM asset_face_scans` — dentro una
transazione, per non lasciare uno stato a metà se una delle quattro
cancellazioni fallisce.

Ruling: **azione globale, non per libreria** — a differenza
dell'interruttore, che la spec dichiara esplicitamente "per ogni
libreria". `PersonRepo::nearest_centroid` (Task 5) non ha mai filtrato per
libreria: una persona può avere volti in librerie diverse, quindi non
esiste un confine di libreria pulito per "cancella i dati di questa
persona" — cancellarla parzialmente (solo i volti di una libreria)
lascerebbe la persona in uno stato incoerente (centroide calcolato su
volti in parte spariti). La spec stessa non usa mai "di questa libreria"
per questo comando, a differenza del punto sopra sull'interruttore. —
*Costo se sbagliato*: se in futuro serve una cancellazione per libreria,
va decisa la semantica per le persone cross-libreria (dividerle? Lasciarle
intatte?) — non ovvio, rimandato a quando servirà davvero.

Ruling: **azzera anche `asset_face_scans`, non solo `faces`/`persons`/
`person_groups`** — la spec §7 dice "faccia piazza pulita di faces,
persons e gruppi" senza menzionare la tabella di tracciamento della Task
4/5, ma lasciarla intatta creerebbe uno stato peggiore del previsto: ogni
asset resterebbe segnato "già scansionato" per sempre, quindi una libreria
che riaccende `faces_enabled` dopo la cancellazione non ririleverebbe mai
nulla — il comando sarebbe una cancellazione permanente anche per chi
voleva solo "ricominciare da zero". — *Costo se sbagliato*: nessuno
osservato, è l'interpretazione più coerente con "elimina tutti i dati" e
non un'estensione arbitraria.

Ruling: **solo amministratori** (`ctx.is_admin()`), stessa soglia di
`LibraryRepo::delete` — altra azione distruttiva e irreversibile, e
questa è globale (non c'è un proprietario naturale come per una libreria
singola).

Rotta: `DELETE /api/v1/faces/data` (`crates/keeppix-api/src/routes/faces.rs`,
`delete_all_data`) — `/faces/data` invece di un bare `DELETE /faces`
perché non esiste una collection GET su `/faces` da cui questo sarebbe la
cancellazione naturale; il nome dice esplicitamente cosa cancella. Nessun
body, `204` in riuscita, `403` per chi non è admin. Aggiunta a
`openapi.rs`/`docs/api/openapi.json` (rigenerato via
`UPDATE_OPENAPI=1 cargo test`) e a `wired-exceptions.txt` (Rinvii,
fase-11 — stesso consumatore delle altre rotte volti/persone).

**Test**: `delete_all_data_requires_an_admin` e
`delete_all_data_wipes_faces_persons_groups_and_scan_state`
(`crates/keeppix-db/tests/faces.rs`); `delete_all_face_data_is_admin_only_and_wipes_persons`
end-to-end via HTTP (`crates/keeppix-api/tests/persons.rs`). Suite
rilevanti riverificate dopo l'aggiunta: `keeppix-db` faces/persons/lib
(19+12+altri lib, tutti verdi), `keeppix-api`
face_privacy/persons/openapi/libraries (3+11+8+14, tutti verdi).
`check-wired.py`: pulito dopo la voce in `wired-exceptions.txt`.
`cargo fmt --all -- --check` e `cargo clippy --workspace --all-targets
-- -D warnings`: puliti.

## Task 10: complete

## Task 11 — WebSocket, documenti, e il test del Task 1 che ora deve passare

`crates/keeppix-api/src/routes/ws.rs`: `suggestions.changed` — la Fase 10
(Task 19) aveva lasciato l'evento dichiarato ma non cablato,
esplicitamente per questa ragione: *"nessun codice di Fase 7/8 esiste da
cui leggerlo"* (Ruling Fase 10). Ora Fase 7 (tag) e Fase 8 (volti)
esistono entrambe, quindi `drain_suggestions` (nuovo) somma
`AssetTagRepo::count_proposed_visible` + `FaceRepo::count_proposed_visible`
— **la stessa identica somma** già usata dal badge `bootstrap.badges.revision`,
un solo canale perché il badge è già un conteggio combinato tag+volti, non
due badge distinti. Stesso disegno "magro" di `problems.changed`: il
numero viaggia come comodità, il contratto resta "ricarica il contatore"
(Ruling Fase 10 Task 19: "portare il numero resta ammesso come comodità,
mai come garanzia"), e stessa guardia "prima connessione a zero non
emette" per non gareggiare col Ping di apertura.

Ruling: **`socket_loop` supera il tetto di 100 righe di clippy con
l'ottavo `drain_*` — `#[allow(clippy::too_many_lines)]`, non un
refactor.** La funzione è un elenco piatto e ripetitivo di "prova a
leggere una fonte, esci se il socket è morto"; fattorizzarlo
richiederebbe chiusure eterogenee (ogni `drain_*` porta un tipo di stato
"visto" diverso: `HashMap`, `Option<String>`, `Option<i64>`, ...) solo per
stare sotto un limite di stile — esattamente l'astrazione-per-il-linter
che AGENTS.md scoraggia. Altre 9 funzioni nel codebase usano già lo stesso
`#[allow]` per lo stesso motivo. — *Costo se sbagliato*: nessuno, è pura
leggibilità.

**Test**: `a_proposed_face_is_pushed_as_suggestions_changed`
(`crates/keeppix-api/tests/ws.rs`) — stesso schema di
`an_offline_library_is_pushed_as_problems_changed` (apre il socket,
propone un volto via repository, aspetta l'evento). 10/10 verdi in
`ws.rs` (9 preesistenti + 1 nuovo).

**Il test del Task 1 (`face_privacy.rs`) resta verde** — riverificato qui
come condizione di chiusura esplicita del piano, non solo alle chiusure
intermedie di Task 6/8/10 già registrate sopra: 3/3 (`a_shared_folder_never_exposes_face_or_person_data`,
`a_shared_single_asset_never_exposes_face_or_person_data`,
`the_scanner_itself_catches_a_planted_leak`). Ispezione diretta di
`crates/keeppix-api/src/routes/share.rs`: zero occorrenze di
`face`/`Face`/`person`/`Person` in tutto il file — le rotte pubbliche non
toccano quei dati per costruzione, non solo perché il test non li trova.

Documenti: `docs/api/openapi.json` già tenuto sincrono a ogni task (Task
6/7/8/10 lo hanno rigenerato via `UPDATE_OPENAPI=1 cargo test`, non a
mano). Nessun altro documento di prodotto (README, `docs/CONTINUE.md`) va
toccato qui: si aggiornano al momento del merge, stesso schema delle
chiusure di Fase 7/10.

Suite riverificate un'ultima volta in blocco:
`cargo test -p keeppix-api --test face_privacy --test persons --test
openapi --test libraries --test ws` → 46/46 verdi. `cargo fmt --all --
--check` e `cargo clippy --workspace --all-targets -- -D warnings`:
puliti. `python3 scripts/check-wired.py`: pulito.

## Task 11: complete — Fase 8 pronta per la verifica di chiusura (PROSEGUI.md §10)

## Verifica di chiusura (PROSEGUI.md §10) — due difetti reali trovati dalla CI vera

Push del branch su `origin/fase-8` (oltre al branch di lavoro designato)
per ottenere CI reale: `.github/workflows/ci.yml` fa scattare i job solo su
push a `main`/`fase-*` (le PR di questo repo sono rotte lato GitHub,
commento nel file), quindi è l'unico modo per avere un giudizio CI prima
del merge — stesso meccanismo già usato per Fase 7/10. Due run reali,
entrambe rosse, **entrambe difetti veri**, non rumore:

**1. `libraries.faces_enabled` dentro il blocco gated da pgvector.**
`crates/keeppix-api/tests/albums.rs::refresh_returns_added_ids_as_succeeded_bulk_outcome`
falliva con `column "faces_enabled" does not exist`. Causa: il servizio
Postgres di default in CI (`postgis/postgis:17-3.5`, usato dai crate che
non hanno bisogno di IA) non ha pgvector **di proposito** — commento
esplicito in `ci.yml`: "resta senza vector per gli altri crate... quando
quell'URL non offre vector, i test AI ricadono su testcontainers
keeppix-db:dev". La migrazione 0046 mette `ALTER TABLE libraries ADD
COLUMN faces_enabled` **dentro** `DO $faces$ ... END $faces$`, che va in
no-op senza pgvector — ma `LibraryRepo` (core, non IA-gated) legge/scrive
quella colonna incondizionatamente in ogni `INSERT`/`SELECT` su
`libraries`. Fix: spostata `ALTER TABLE` fuori dal blocco, sempre
eseguita — stesso pattern di `scan_enabled`. Verificato **riproducendo il
bug in locale**: nascosto `vector.control` a livello di server (spostato
il file, non solo disabilitato per-db), creato un database senza pgvector
disponibile, rieseguito `cargo test -p keeppix-api --test albums` contro
quel server — falliva con lo stesso identico errore prima del fix, passa
dopo. `vector.control` ripristinato subito dopo.

**2. `bootstrap.rs`: il test del budget di query non contava la metà
"volti" del badge.** `bootstrap_emits_no_more_queries_than_individual_repos`
e `bootstrap_query_budget_still_holds_after_http_bootstrap_round_trip`
fallivano con `bootstrap=9 query, singoli=7: deve essere ≤` (deterministico,
non flaky — confermato con quattro run identici in fila). Causa: il Task 8
di questa fase ha aggiunto `FaceRepo::count_proposed_visible` a
`bootstrap::compose` (la metà "volti" del badge `revision`, accanto alla
metà "tag" già presente da Fase 7), ma **non** ha aggiornato
`load_individual_repos` nel test — che calcola il budget "quante query fa
`compose` rispetto alla somma dei singoli repository" — per contare anche
quella chiamata. `FaceRepo::count_proposed_visible` da sola costa 2-3
query (`probe_pgvector` + `VisibilityScope::resolve` + il conteggio),
tutte non contabilizzate sul lato "singoli". Fix: aggiunta la stessa
chiamata `FaceRepo::new(db).count_proposed_visible(ctx)` in
`load_individual_repos`, commento speculare a quello già presente per
`AssetTagRepo` (Fase 7 Task 9). Non è il difetto preesistente/flaky già
noto dal ledger di Fase 10 (quello si manifestava solo quando i due test
giravano insieme nello stesso binario ed era nato prima che Fase 7/8
esistessero) — è un gap nuovo, introdotto da questa fase, mai emerso prima
perché nessuna run precedente di `cargo test --workspace` in CI era
arrivata fin lì (le run precedenti si fermavano prima, sul difetto n.1 o
sul `check-wired.py`). Verificato con quattro run consecutive di
`cargo test -p keeppix-api --test bootstrap` dopo il fix: 3/3 verdi ogni
volta, nessuna variazione nel conteggio.

I primi due difetti sono esattamente il tipo che PROSEGUI.md §10 esiste per
intercettare prima del merge: un test verde in locale (con la variabile
d'ambiente che punta sempre a un server con pgvector, e senza mai far
girare `cargo test --workspace` fino in fondo per motivi di spazio disco
del sandbox) non è la controprova che il requisito regga in produzione.
Ogni push successivo ha rivelato il difetto *seguente* — `cargo test
--workspace` non ha `--no-fail-fast`, quindi ogni run reale si fermava al
primo binario che falliva, senza mai raggiungere il resto della suite.
Cinque cicli push→CI→diagnosi→fix in totale prima di un run interamente
verde:

**3. Due test HTTP creati in questa fase aprivano il server senza lo
schema volti.** `crates/keeppix-api/tests/face_privacy.rs` (i due test
DB-backed) falliva con `relation "persons" does not exist` — la stessa
causa di fondo del difetto 1 (server di default in CI senza pgvector),
ma qui il colpevole è `TestServer::start()` invece di
`TestServer::start_with_vector()`: il commento del harness lo dichiara
esplicitamente ("i test che [richiedono lo schema IA] usano
`start_with_vector`"), ma tutti i test di `face_privacy.rs`, `persons.rs`
(Task 6/7/8) e il test nuovo di `ws.rs` (Task 11,
`a_proposed_face_is_pushed_as_suggestions_changed`) sono stati scritti con
`start()` semplice — mai emerso prima perché il Postgres locale di questa
sandbox ha sempre pgvector installato, quindi `start()`/`start_with_vector()`
si comportano identici qui. Corretti tutti e undici i punti in `persons.rs`
più i due in `face_privacy.rs` più quello in `ws.rs`. Verificato
riproducendo di nuovo il server senza pgvector (stesso trucco del difetto
1); il fallback a testcontainers `keeppix-db:dev` non è verificabile in
questa sandbox (niente Docker), ma è lo stesso meccanismo già usato con
successo dai test IA di Fase 7 (`tags.rs`) in CI reale, non qualcosa
inventato qui.

**4. Flake preesistente di `bootstrap.rs`, root-causato invece di
lasciato aperto.** Dopo il fix del difetto 2,
`bootstrap_emits_no_more_queries_than_individual_repos` tornava a fallire
in modo non deterministico (`bootstrap=8 query, singoli=7` una volta,
verde la successiva) — esattamente il difetto già documentato nel ledger
di Fase 10 come *"fallisce quando gira insieme all'altro test dello
stesso binario, isolato passa sempre"*, mai chiuso allora. Causa reale:
`global_sql_capture` installa un subscriber di tracing **process-wide**
(necessario perché sqlx logga da un worker diverso dal task del test), e i
due test che leggono il conteggio si serializzano su `BUDGET_LOCK` — ma il
terzo test dello stesso file, `bootstrap_matches_individual_endpoints`,
gira concorrente e senza lock, e le sue query HTTP possono cadere dentro
la finestra di cattura di uno degli altri due, gonfiando il conteggio.
Fix: `BUDGET_LOCK` anche lì. Verificato con 8 run locali consecutivi
(prima del fix falliva in modo intermittente; dopo, 8/8 verdi), poi
confermato di nuovo in CI reale.

**5. `budgets.rs` — confermato rumore del runner, non un difetto.** La
prima run interamente verde di `cargo test --workspace` in CI ha
raggiunto, per la prima volta in questa fase, i test di performance
(`timeline_page_with_ten_thousand_assets_stays_within_budget` e
`timeline_buckets_with_ten_thousand_assets_stays_within_budget`), che
hanno sforato il budget (368ms/300ms, 276ms/200ms). Verificato **prima**
di trattarlo come rumore: `git diff main -- crates/keeppix-api/tests/budgets.rs
crates/keeppix-api/src/routes/timeline.rs` — zero differenze rispetto a
`main` su entrambi i file, quindi nulla in questa fase li tocca. Un solo
re-run mirato (`rerun_failed_jobs`, non un nuovo push) su un runner
diverso: verde. Non richiesto nessun fix.

Tutti i difetti reali (1-4) corretti, riverificati (suite mirate +
`fmt`/`clippy`/`check-wired.py` dopo ognuno, poi l'intera
`cargo test --workspace` in CI verde end-to-end, tutti i 130+ binari di
test del workspace confermati eseguiti — non solo "l'ultimo passo non ha
fallito"), e solo a quel punto si procede al merge.

## Fase 8: MERGIATA in `main`

`git merge --no-ff claude/keeppix-phases-8-11-shi9fl` (commit `e50b544`),
dopo `git merge-tree` a secco (nessun conflitto — la fase non tocca nulla
toccato da `main` nel frattempo) e CI reale verde sul branch. Le due cose
che il piano dichiara più importanti di tutto il resto sono state
riverificate un'ultima volta leggendo il codice reale sul branch appena
pushato, non il riassunto del ledger: `share.rs` — zero occorrenze di
`face`/`person` in tutto il file; `detect_faces.rs::group_face` —
`has_any_separation` blocca sempre l'assegnazione automatica certa prima
di qualunque confronto di similarità.

**CI sul commit di merge stesso: verde solo al terzo tentativo (run
32574907605), non in un colpo solo — documentato per intero, non
smussato, perché è successo sul commit che poi è rimasto su `main`.**
Un merge può introdurre problemi che nessuna delle due CI isolate (branch
e `main` pre-merge) avrebbe visto, quindi questa run separata non era una
formalità: è dove sono emersi due difetti apparenti in più.

- **Tentativo 1: FAILED** — `crates/keeppix-db/tests/scale_200k.rs`, due
  test oltre budget: `two_hundred_thousand_assets_keep_timeline_and_search_within_budget`
  (554ms contro 300ms) e `timeline_with_fifty_permissions_stays_under_budget_at_200k`
  (331ms).
- **Tentativo 2 (`rerun_failed_jobs`, nessun nuovo push): FAILED** — test
  **diverso**, `crates/keeppix-db/tests/scale_embeddings.rs::vector_search_stays_interactive_with_ivfflat`:
  la scansione IVFFlat grezza (`ORDER BY <=>` su 200.000 embedding seminati
  nello stesso test) ha misurato 643,2ms contro un budget di 500ms.
- **Verificato prima di trattarlo come rumore, non dopo:**
  `git diff e50b544~2 e50b544 -- crates/keeppix-db/tests/scale_200k.rs
  crates/keeppix-db/tests/scale_embeddings.rs crates/keeppix-api/src/routes/timeline.rs
  crates/keeppix-db/src/timeline.rs` — **zero differenze** su tutti e
  quattro i file rispetto a prima del merge. L'unico file toccato dalla
  fase vicino a questo codice, `crates/keeppix-db/src/search.rs`, aggiunge
  solo il dispatch di `SearchNode::Person/PersonGroup/PersonCount` — mai
  esercitato dal path SQL grezzo o da `SearchNode::Semantic` che questi
  due test misurano. `.github/workflows/ci.yml` (righe ~118-136) dichiara
  esplicitamente che `cargo test --workspace` in CI gira **senza**
  `--test-threads=1` (a differenza di `scripts/test.sh` in locale): più
  binari di test colpiscono lo stesso Postgres/`keeppix-db:dev` condiviso
  in concorrenza, per scelta — motivo strutturale, non specifico di questa
  fase, per cui i budget in millisecondi di `scale_200k.rs` e
  `scale_embeddings.rs` (dati da 200.000 righe, soglie strette) sono
  intrinsecamente sensibili al rumore del runner condiviso. Due test
  **diversi**, in due file **diversi**, entrambi senza alcuna differenza
  introdotta da questa fase, è più coerente con rumore generico del runner
  che con una regressione sistematica introdotta dal merge.
- **Tentativo 3 (`rerun_failed_jobs`, nessun nuovo push): SUCCESS** — tutti
  e 5 i job verdi, incluso `Test` (l'intera `cargo test --workspace`) e
  `La specifica OpenAPI è aggiornata`.

Ruling: **i budget di `scale_200k.rs`/`scale_embeddings.rs` sono rumore
del runner CI condiviso, non un difetto di Fase 8** — confermato con `git
diff` a zero su ogni file coinvolto in entrambi i tentativi falliti,
spiegato dal modello di esecuzione concorrente che `ci.yml` stesso
documenta, e risolto senza toccare codice o test. Costo se questo ruling
è sbagliato: un regresso di prestazioni reale nel path vettoriale/timeline
introdotto da questa fase passerebbe inosservato finché non degrada
abbastanza da fallire in modo deterministico — la mitigazione è che
questi due file restano test di scala già esistenti (non aggiunti da
Fase 8) e li si è visti fallire isolatamente anche in fasi precedenti
(vedi bug #5, `budgets.rs`, sopra), quindi il pattern di rumore era già
noto prima di questo merge, non inventato per giustificare una CI rossa.

Limite dichiarato e non risolto (Task 2): nessuna misura reale di
inferenza SCRFD/ArcFace su hardware vero — nessuna fonte verificata di
pesi ONNX raggiungibile da questa sessione. `ASSIGN_SIMILARITY`/
`PROPOSE_SIMILARITY` restano stime ragionate, non calibrate. Serve una
decisione umana su quale checkpoint usare prima che questi numeri possano
dirsi affidabili.

## 25 agosto: Task A del piano modelli IA — SCRFD/ArcFace → YuNet/SFace

Decisione presa il 22 agosto
(`docs/superpowers/plans/2026-08-22-keeppix-modelli-ai.md`): i pesi SCRFD/
ArcFace non sono mai stati scaricati (research-only, licenza incompatibile
con l'offerta commerciale pianificata di Keeppix) — sostituiti prima di
qualunque rilascio, non dopo. YuNet (MIT) + `SFace` (Apache 2.0), OpenCV
Zoo: liberi anche per uso commerciale, ~9,5 MB contro 16-264 MB di
qualunque variante InsightFace.

**A differenza dell'implementazione SCRFD, mai verificata contro pesi
reali**, questa volta i due file `.onnx` reali sono stati scaricati e
ispezionati per davvero, dentro questa sandbox — non dedotti dalla
documentazione:

- `curl` diretto a `media.githubusercontent.com/media/opencv/opencv_zoo/
  main/models/.../*.onnx` (non `raw.githubusercontent.com`, che per un
  percorso tracciato Git LFS torna il pointer testuale di ~130 byte, non
  il binario — scoperto e verificato con un confronto diretto fra i due
  host prima di scegliere quale usare nello script di download).
  `sha256sum` di entrambi i file scaricati identico byte-per-byte ai
  valori del piano (`321aa5a6…` YuNet, `2b0e941e…` `SFace`, dimensioni
  100.416 e 9.896.933 byte esatte).
- `onnx.load()` (pacchetto Python `onnx`, installato per l'occasione) sul
  grafo reale ha dato la risposta empirica a ogni ambiguità che la sola
  lettura della documentazione aveva lasciato aperta:
  - **YuNet**: input `[1, 3, 640, 640]` — dimensione **fissa** dichiarata
    dal grafo (non 256px come la precedente scelta SCRFD, non 320px come
    il default Python del modello *dinamico* più recente, che aveva
    inizialmente confuso la ricerca: quel default appartiene alla variante
    2026may, non alla `2023mar_int8` pinnata dal piano).
  - **`SFace`**: output `fc1: [1, 128]` — **128 dimensioni, non 512**
    come `ArcFace`. Confermato anche dal commento ufficiale OpenCV
    `samples/dnn/js_face_recognition.html` ("Get 128 floating points
    feature vector"). Il grafo dichiara ~144 input aggiuntivi oltre a
    `data` (parametri BatchNorm/PReLU): tutti con un initializer nel
    grafo stesso, quindi non vanno forniti a `Session::run` — comportamento
    ONNX standard, verificato programmaticamente (`data` è l'unico input
    del grafo privo di initializer).
- Formule di decodifica (nomi output `cls_{8,16,32}`/`obj_{8,16,32}`/
  `bbox_{8,16,32}`/`kps_{8,16,32}`, box in stile YOLO con
  `score=√(cls·obj)`, un rilevamento per cella — anchor-free, a differenza
  delle 2 ancore/cella di SCRFD) lette direttamente da
  `modules/objdetect/src/face_detect.cpp` di OpenCV, non da un riassunto.
  Soglie `confThreshold=0.6`/`nmsThreshold=0.3` dal wrapper Python
  ufficiale di OpenCV Zoo (`yunet.py`), non i valori SCRFD ereditati.
- **Ordine dei 5 landmark**: verificato che `FaceRecognizerSF::alignCrop`
  (`face_recognize.cpp`) — la funzione ufficiale che fa da ponte fra
  questi due modelli — legge i punti del rilevatore **in ordine stretto,
  senza permutazioni**, dentro lo stesso identico array di riferimento
  112×112 già usato per `ArcFace` (`{38.29,51.70},{73.53,51.50},…` —
  numericamente identico, verificato leggendo il sorgente C++: cambia il
  nome della costante — `align::SFACE_REFERENCE_112`, prima
  `ARCFACE_REFERENCE_112` — non i numeri). Nessun riordino necessario nel
  codice.
- **Canali RGB/BGR**: `blobFromImage` per YuNet chiama con `swapRB=false`
  su una sorgente BGR-nativa (`cv::imread`) — la rete riceve BGR. Il
  buffer di questo crate è RGB: il letterbox del rilevatore ora scambia
  R↔B esplicitamente (`letterbox_to_nchw_bgr`, con commento che ne spiega
  il motivo — il letterbox SCRFD precedente non lo faceva, ragionevole per
  la famiglia InsightFace ma non per YuNet). Per `SFace`,
  `swapRB=true` nel sorgente originale converte l'ingresso BGR-nativo di
  OpenCV in RGB per la rete: il nostro buffer è già RGB, quindi
  `embed_aligned` non scambia nulla — comportamento equivalente, non un
  bug per omissione.

**Migrazione di schema** (non nei 5 punti originali del piano — scoperta
necessaria durante l'implementazione, non assorbita in silenzio):
`0050_faces_embedding_dim_128.sql` porta `faces.embedding` e
`persons.centroid` da `vector(512)` a `vector(128)` (nessuna riga reale
esisteva a 512 dimensioni: i pesi precedenti non sono mai stati eseguiti,
in nessun ambiente). **Verificata per davvero**, non solo scritta: le
migrazioni 0001→0050 applicate in ordine numerico contro un Postgres 16 +
pgvector 0.6.0 reale installato in questa sandbox (`postgresql-16-pgvector`
via apt, dockerd non disponibile ma non serviva) — tutte le 51 vanno a
buon fine, `\d faces`/`\d persons` confermano `vector(128)` e l'indice
`faces_embedding_ivfflat_idx` ricreato correttamente.

Con lo stesso Postgres reale (auth `trust` locale, non solo peer),
`KEEPPIX_TEST_DATABASE_URL` puntato a quel server: **`cargo test -p
keeppix-db --test faces` (22/22 ok) e `--test persons` (12/12 ok) passano
per davvero**, non in teoria — la prima volta in questa sessione che del
codice toccato da un task sui volti gira contro un database reale con
schema a 128 dimensioni, non solo contro `cargo fmt`/lettura manuale.
`cargo check -p keeppix-domain --tests` e `-p keeppix-db --tests` puliti;
`cargo test -p keeppix-domain` 94/94 ok. `cargo fmt --all --check` pulito
sull'intero workspace dopo le modifiche.

`scripts/download-yunet-sface.sh` (nuovo, a differenza del template
MobileCLIP2 verifica lo sha256 — l'errore silenzioso dell'URL sbagliato
sopra è esattamente il motivo) **eseguito per davvero** contro la sorgente
reale, due volte: la prima scarica ed estrae, la seconda conferma il path
di cache (hash già corretto, nessun download ripetuto). Cache + step di
download aggiunti a `.github/workflows/ci.yml`, stesso pattern di
MobileCLIP2-S2.

**Non ancora verificabile in questa sandbox** (stesso limite dichiarato
da sempre: `ort-sys` non compila senza rete verso `cdn.pyke.io`, `CONNECT
proxy failed: 403`): che l'inferenza YuNet/`SFace` converga per davvero
dentro `cargo test` (il decode è verificato sul grafo ONNX reale e sul
sorgente C++ di OpenCV, non contro un'esecuzione reale del grafo stesso —
la differenza fra "le shape e le formule sono quelle giuste" e "il
risultato numerico è quello giusto" resta aperta), e la calibrazione di
`ASSIGN_SIMILARITY`/`PROPOSE_SIMILARITY` (Task A punto 4 del piano) — che
richiede proprio quell'inferenza reale per avere un numero da calibrare.
Prossimo passo: push su un branch con CI reale (rete completa), lettura
dell'esito di `detects_and_groups_faces_when_weights_are_present` (mai
girato, nemmeno adesso) e, se verde, misura ms/rilevamento,
ms/impronta, RSS a ledger.
