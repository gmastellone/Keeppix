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
