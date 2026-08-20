# Piano — Fase 10: Superficie API per l'interfaccia

**Specifica:** `docs/superpowers/specs/fase-10-api-interfaccia.md`
**Base:** tip di `origin/fase-6` (dopo il merge di Fase 5+6 su `main`).
**Branch:** `fase-10`.

---

## Cosa esiste già (verificato, non assunto)

Fatti raccolti leggendo il codice al tip di `origin/fase-6`. Servono a non
reimplementare cose che ci sono e a non rompere cose che funzionano.

- **`/timeline/buckets` risolve già la richiesta #3** del documento funzionale:
  `crates/keeppix-api/src/routes/timeline.rs:183` restituisce
  `[{month:"YYYY-MM", count}]`, con supporto `library` e `bbox`. **Non va toccato.**
- **`DiskAction` è già esattamente SP-18**: `crates/keeppix-domain/src/trash.rs:11`
  definisce `Kept` / `MovedToTrash` / `Purged`, il parametro è obbligatorio senza
  default, e `Purged` è ristretto a owner/admin. **Il Task 4 aggiunge solo la forma
  di massa: l'enum e la semantica non si toccano.**
- **Gli stack esistono**: tabella `stacks`, `GET /assets/{id}/stack`,
  `POST /assets/{id}/stack/primary` (`routes/stacks.rs`). Quello che manca è
  l'esposizione nelle viste di browse, non il modello.
- **`SearchNode`** (`crates/keeppix-db/src/search.rs:27`) è già un AST ricorsivo con
  `And/Or/Not` e un compilatore SQL parametrizzato (`compile_for_sql`) con guardia di
  profondità. Le varianti nuove si innestano lì: **non serve un secondo modello.**
- **`POST /viewport`** esiste già e serve proprio a promuovere la generazione delle
  miniature che l'utente sta guardando. Il frontend della Fase 11 lo userà: **non
  serve inventarne un altro.**
- **`crate::batch::reject_oversized_batch`** è già la guardia sui lotti troppo grandi,
  usata da `flags/batch`. L'involucro del Task 1 la riusa.
- **La cache in-process `moka`** è stata introdotta in Fase 6
  (`crates/keeppix-db/src/lib.rs:96`), senza TTL e con invalidazione esplicita
  tracciata su ogni mutazione. Gli aggregati degli album (Task 5) vanno lì, **con lo
  stesso patto di invalidazione esplicita**: una cache che scade da sola qui
  riporterebbe conteggi sbagliati.
- **`statvfs` è già usato** in `crates/keeppix-db/src/uploads.rs` — ed è oggi il punto
  che **non compila** (mismatch `u32`/`u64` fra piattaforme). Il Task 8 lo espone, ma
  **quel difetto va chiuso prima**, nel debito aperto della Fase 6.
- **L'OpenAPI generato copre 68 path su una superficie più larga**: mancano
  `albums`, `share`, `groups`, `permissions`, `audit`, `backup`, `restore`, `upload`,
  `health`. Il Task 10 lo chiude.

---

## Task 1 — L'involucro di riuscita parziale e la tassonomia degli errori

**Va per primo.** È la convenzione che tutti gli altri task (e le Fasi 7, 8, 9)
devono rispettare: se arriva dopo, va ritrattato tutto.

**Dove:** nuovo modulo `crates/keeppix-api/src/bulk.rs`; consumato da
`routes/flags.rs`, `routes/metadata.rs`, `routes/geotag.rs`.

1. Definire il tipo di risposta condiviso:
   ```rust
   pub struct BulkOutcome {
       pub succeeded: Vec<AssetId>,
       pub failed:    Vec<BulkFailure>,
       pub batch_id:  Option<BatchId>,
   }
   pub struct BulkFailure { pub id: AssetId, pub reason: FailureReason, pub detail: Option<String> }
   pub enum FailureReason { Unreachable, PermissionDenied, FileMissing, Timeout }
   ```
   `FailureReason` serializza in kebab-case (`permission-denied`) ed è un **insieme
   chiuso**: è ciò che permette al frontend di decidere se mostrare "Riprova".
2. Mappare `DbError` e gli errori di filesystem su `FailureReason`. Dove la natura
   non è distinguibile, **non inventarla**: serve un quinto valore interno
   `Unknown` che il frontend tratta come non-ritentabile. Meglio un caso onesto in
   più che quattro categorie di cui una mente.
3. Convertire i sei endpoint elencati nella spec §3.3 perché eseguano
   **elemento per elemento** e accumulino l'esito, invece di fallire in blocco.
4. `POST /flags/batch` passa da `204` a `200` + corpo. Gli altri cinque mantengono
   `batch_id` **dentro** l'involucro: nessun campo rimosso o rinominato, il
   contratto additivo di `/api/v1` regge.

**Verifica:** un test per endpoint che semina un lotto in cui **almeno un elemento
fallisce davvero** (non un mock: una cartella resa non scrivibile con `chmod`) e
asserisce che gli altri sono riusciti, che il fallito compare in `failed` con la
ragione giusta, e che `undo` sul `batch_id` annulla **solo** i riusciti.

---

## Task 1bis — Tarare Postgres sull'hardware, **prima** di toccare gli indici

**Va fatto prima del Task 2 e del Task 12**, altrimenti li si misura in condizioni
che ne nascondono l'effetto.

`compose.yaml` avvia `postgis/postgis:17-3.5` **senza nessun parametro**: restano i
default di fabbrica. Il più dannoso è `random_page_cost = 4.0`, che dice al
pianificatore che una lettura casuale costa quattro volte una sequenziale — vero su
disco rotante, falso su SSD. **È il parametro che decide se Postgres userà gli
indici che questa fase aggiunge o preferirà una scansione sequenziale.**

| Parametro | Default | Bersaglio (Pi 5 / 8 GB, SSD) |
|---|---|---|
| `random_page_cost` | 4.0 | **1.1** |
| `shared_buffers` | 128 MB | ~2 GB |
| `effective_cache_size` | 4 GB | ~6 GB |
| `work_mem` | 4 MB | 32–64 MB |
| `max_connections` | 100 | 20 |

**Ruling: i valori si misurano all'installazione, non si cablano.** — Su microSD
`random_page_cost` resta alto e il profilo è opposto a quello su NVMe; un valore
fisso sarebbe giusto per una metà degli utenti e dannoso per l'altra. Il probe
hardware previsto dalla Fase 7 è il posto naturale dove misurarlo, ma per questa
fase basta renderli **configurabili** e documentare i due profili. — *Costo se
sbagliato:* si costruiscono cinque indici e li si vede ignorare, concludendo che
"gli indici non servono".

Aggiungere anche `autovacuum_vacuum_scale_factor` più aggressivo su `assets` e un
`VACUUM ANALYZE` a fine import massiccio (lo scheduler di manutenzione della Fase 6
è il posto giusto): l'index-only scan della geometria funziona **solo** se la mappa
di visibilità è aggiornata, altrimenti degrada in heap fetch senza dare errore.

**Verifica:** `EXPLAIN` della query di geometria e di timeline con i default e con i
valori tarati, entrambi nel ledger. Se il piano non cambia, il task ha comunque
prodotto il numero che serviva.

---

## Task 2 — Endpoint di geometria della timeline

**Dove:** `crates/keeppix-api/src/routes/timeline.rs`, `crates/keeppix-db/src/timeline.rs`,
nuova migrazione.

1. `GET /api/v1/timeline/geometry` con gli stessi parametri di filtro e la stessa
   risoluzione di visibilità di `/timeline` (`VisibilityScope::resolve`).
2. Corpo binario, `application/octet-stream`, record da **6 byte**
   (`w` u16 · `h` u16 · `month` u16), little-endian, **senza identificativo**.
   Intestazione di 8 byte con versione di formato e numero di record.

   **Niente uuid**: sono 16 byte casuali, quindi incomprimibili. Misurato su 214.000
   record realistici — con uuid 4,49 MB grezzi che restano **3,88 MB** dopo gzip;
   senza, 1,22 MB che diventano **0,44 MB**. Nove volte più piccolo sul filo. La
   riconciliazione non serve: la geometria **non identifica nulla, descrive
   altezze**, e sta nello stesso ordine delle pagine (`taken_at DESC, id DESC`) —
   le tessere vere arrivano dalle pagine, che gli uuid li portano già.
3. `ETag` derivato dal massimo `updated_at` della vista + conteggio: permette
   `304 Not Modified` sul rientro nella stessa vista, che è il caso normale.
4. Migrazione con l'indice di copertura:
   ```sql
   CREATE INDEX assets_geometry_idx ON assets (folder_id, taken_at_utc DESC, id DESC)
       INCLUDE (width, height) WHERE status = 'indexed';
   ```
5. Gli asset senza `width`/`height` noti (non ancora processati) **vanno inclusi**
   con `w=0,h=0`: il frontend li disegna con un rapporto predefinito 3:2 invece di
   non disegnarli affatto. Escluderli farebbe "saltare" il layout quando arrivano.

**Verifica:** test su 200k righe (esiste già `crates/keeppix-db/tests/scale_200k.rs`
come modello) che asserisce `EXPLAIN` = **index-only scan** e tempo sotto una soglia
esplicita; test che il conteggio dei record combacia esattamente con la somma dei
`count` di `/timeline/buckets` sugli stessi filtri.

---

## Task 3 — Lo stack collassato nelle viste di browse

**Dove:** `crates/keeppix-db/src/timeline.rs`, `routes/timeline.rs`, `routes/search.rs`.

1. Aggiungere ad `AssetView` due campi additivi: `stack_size: u16` (1 se non
   impilato) e `raw_kind: Option<String>` (`"raw"` / `"raw+jpeg"` / `"jpeg"`), il
   secondo per il badge SP-15.
2. Timeline, ricerca e geometria restituiscono **solo il primario** di ogni pila:
   `WHERE (a.stack_id IS NULL OR a.id = s.primary_asset_id)`.
3. `/timeline/buckets` deve contare **le pile, non i file**, altrimenti il conteggio
   del mese e il numero di tessere divergono.
4. Estendere `assets_geometry_idx` (o affiancarne uno) perché copra la condizione di
   primario senza ricadere in un join costoso.

**Verifica:** semina una coppia RAW+JPEG impilata e asserisce: una sola tessera in
timeline, `stack_size == 2`, `raw_kind == "raw+jpeg"`, e `buckets` che conta 1.

---

## Task 4 — Eliminazione di massa a tre vie

**Dove:** `routes/trash.rs`, `crates/keeppix-db/src/trash.rs`.

1. `POST /api/v1/assets/batch/delete` con `{asset_ids, disk_action}`, risposta =
   involucro del Task 1.
2. `disk_action` **obbligatorio**, stessa validazione di `parse_action`, stessa
   restrizione owner/admin su `purged`.
3. Riusa `TrashRepo::choose` per elemento: nessuna nuova logica di eliminazione.
4. **`purged` è il caso che fallisce di più, non di meno.** Il documento lo marca
   come *«il buco più rilevante per il backend»*: l'eliminazione dal disco può
   fallire per permessi, file in uso o libreria offline, e il prototipo assume che
   riesca sempre. È il dialog più distruttivo dell'app ed è anche quello con più
   modi di non riuscire: ogni file deve poter riportare il proprio esito.

**Verifica:** test che con `purged` e un chiamante non-owner l'intero lotto è
rifiutato **prima** di toccare qualunque file (autorizzazione prima
dell'esecuzione, non a metà); test di riuscita parziale con un file già assente;
test di riuscita parziale con una cartella non scrivibile (`chmod`), che è il caso
reale più probabile.

**Da verificare durante il task**, perché il documento lo segnala come sottigliezza:
«nessuna posizione» è un **valore**, non un'assenza — una foto può negare
esplicitamente il luogo *anche se la cartella ne avrebbe uno*. Il backend regge già
il tri-stato (`MetadataPatchRequest` usa `double_option`), ma va confermato che
`EffectiveMetadata` faccia vincere l'azzeramento esplicito sull'eredità della
cartella invece di confonderlo con "non impostata".

---

## Task 5 — Album: «Aggiorna album» al posto dei dinamici

**Dove:** nuova migrazione, `crates/keeppix-db/src/albums.rs`, `routes/albums.rs`.

Gli album dinamici **non si fanno**. Al loro posto un album normale che ricorda il
filtro con cui è nato e lo rilancia **quando l'utente lo chiede**.

1. Migrazione: `rule jsonb` (il `SearchNode` con cui l'album è stato creato),
   `rule_run_at`, `is_shared`, `cover_tint`, `monochrome`. **Niente `kind`**, niente
   vincolo `rule`↔`kind`.
2. `POST /api/v1/albums/{id}/refresh` riapplica `rule` usando lo **stesso**
   `compile_for_sql`, e restituisce l'**involucro di riuscita parziale** (Task 1) con
   le foto aggiunte e quelle rimosse. Aggiorna `rule_run_at`.
3. I membri stanno **sempre** in `album_assets`: il conteggio è una lettura banale,
   senza cache e senza invalidazioni.

**Ruling: il costo si paga quando l'utente lo chiede, non a ogni apertura.** — Un
album dinamico ricalcola i membri a ogni apertura della griglia: otto album sono
otto scansioni del catalogo, la query più cara dell'interfaccia. Spostare quel costo
su un'azione esplicita lo rende occasionale invece che continuo. Ne guadagna anche
l'utente: un album che cambia da solo può **perdere** una foto che voleva tenere.
— *Costo se sbagliato:* chi vuole una raccolta davvero viva usa un tag o una ricerca
salvata, che fanno la stessa cosa e costano una frazione.

**Verifica:** test che `refresh` elenca aggiunte e rimozioni; test che l'apertura
della griglia Album **non** esegue nessuna scansione del catalogo.

---

## Task 6 — Nuovi assi di `SearchNode`

**Dove:** `crates/keeppix-db/src/search.rs`, `routes/search.rs`, nuova migrazione.

1. Varianti nuove: `Rating{cmp,value}`, `Favorite`, `DateRange{from,to}`,
   `Day{value}`, `Month{value}`, `Country{value}`, `Aperture{cmp,value}`,
   `Shutter{cmp,value}`, **`Place{id}`**.

   `Place` non è `Folder`. Il documento lo dichiara esplicitamente: nel prototipo
   il chip «Luogo» del filtro rapido *«dice "Luogo" ma i valori sono le cartelle e
   il confronto è su `folderId`; nel mockup le tre cartelle coincidono con tre
   luoghi, quindi la finzione regge — nel prodotto reale sono due concetti
   diversi.»* I luoghi esistono già (tabella `places`, geocodifica inversa della
   Fase 4): manca solo l'asse di filtro.
2. Indici della spec §6.1, **tutti parziali** dove ha senso (favorite, rating>0).
3. `Country` richiede che la geocodifica inversa della Fase 4 abbia salvato il paese
   su `asset_exif` o su una tabella collegata: **verificare prima**; se non c'è, il
   task include il popolamento, altrimenti la variante mente come nel prototipo.

**Verifica:** un test per variante; un test che l'AST profondo resta sotto la guardia
esistente; `EXPLAIN` su `Favorite` deve usare l'indice parziale.

---

## Task 7 — Sessioni attive

**Dove:** `crates/keeppix-db/src/sessions.rs`, nuovo `routes/sessions.rs`.

1. Migrazione: `sessions.device_label text`, popolata al login derivandola dallo
   `User-Agent`. **Non conservare lo User-Agent completo**: si estrae l'etichetta
   ("Chrome su macOS") e si scarta il resto.
2. `GET /users/me/sessions`, `DELETE /users/me/sessions/{id}`,
   `POST /users/me/sessions/revoke-others`.
3. La sessione corrente è marcata `current: true` e **non è revocabile** dai primi
   due endpoint (per uscire c'è `/auth/logout`).

**Verifica:** test che `revoke-others` lascia viva esattamente la sessione chiamante;
test che il token di una sessione revocata non autentica più.

---

## Task 8 — Spazio su disco per libreria

**Dipende dalla chiusura del difetto di compilazione in `uploads.rs`.**

`GET /api/v1/libraries/{id}/storage` → `{free_bytes, total_bytes}`, riusando la
primitiva `statvfs` già presente. Il valore va in cache breve (60 s): la sidebar lo
chiede a ogni caricamento e `statvfs` su un volume di rete non è gratis.

**Verifica:** test che i due numeri sono coerenti fra loro e che l'endpoint risponde
`404` per una libreria non visibile al chiamante.

---

## Task 9 — Preferenze utente

`GET` / `PATCH /api/v1/users/me/preferences`, un solo documento `jsonb` per utente
(spec §8.3). Campi previsti dall'interfaccia: `theme` (`chiaro`/`scuro`/`sistema`),
`grid_density` (**due valori distinti**, desktop e mobile), tre booleani di notifica,
`language`.

`PATCH` è una fusione parziale, non una sostituzione: la UI salva una preferenza
alla volta.

**Verifica:** test che `PATCH` di un solo campo non azzera gli altri; test che un
campo sconosciuto viene rifiutato con `400` invece di essere accettato in silenzio.

---

## Task 10 — «Preferito», il concetto che manca

**Dove:** nuova migrazione, `crates/keeppix-domain/src/flags.rs`,
`crates/keeppix-db/src/flags.rs`, `routes/flags.rs`, `routes/timeline.rs`.

Il backend non ha nessuna nozione di «preferito»: zero occorrenze di `favorite`.
L'interfaccia lo usa in sette punti (spec §7bis.1). **Non è `Pick`**: quello è lo
stato dentro un lotto di culling, un asse indipendente.

1. Migrazione: `ALTER TABLE asset_flags ADD COLUMN favorite boolean NOT NULL DEFAULT false;`
   più `CREATE INDEX asset_flags_favorite_idx ON asset_flags (user_id, asset_id) WHERE favorite;`
   (parziale: i preferiti sono una minoranza, ~8% nel prototipo).
2. `favorite` entra in `AssetFlagsBody`, in `AssetFlags` di dominio, e in `AssetView`
   (additivo).
3. Scrittura singola (`PUT /assets/{id}/flags`) e di massa (`POST /flags/batch`, con
   l'involucro del Task 1).
4. `SearchNode::Favorite` — è la variante del Task 6 che alimenta sia il chip di
   Cerca sia la vista "Preferiti" sia il filtro dell'album: **una sola
   implementazione per tre schermate.**

**Verifica:** test che `favorite` e `pick` sono indipendenti (scartare nel culling
non tocca il preferito); `EXPLAIN` sulla vista Preferiti deve usare l'indice
parziale.

---

## Task 11 — Togliere i conteggi per riga, tranne quello del culling

**Dove:** `routes/folders.rs`, `routes/albums.rs`, `routes/share.rs`.

L'interfaccia mostrava sei conteggi accanto alle righe degli elenchi. **Ne resta uno.**

- **Tolti:** foto per cartella (sidebar), membri per album, elementi per link
  pubblico — e, quando arriveranno, foto per tag (Fase 7) e per persona (Fase 8).
- **Resta:** il conteggio del culling — badge di navigazione e selettore di lotto.

**Ruling: si toglie ciò che comunica peso, si tiene ciò su cui si decide.** —
«Urbino 556» contro «Urbino ~550» non cambia nessuna decisione: quel numero dà un
ordine di grandezza. «184 da vedere» invece è letteralmente la domanda che l'utente
si sta facendo mentre culla, e lì la precisione conta. In più il conteggio del
culling è per **lotto**, non per libreria: è anche il più economico dei sei.
— *Costo se sbagliato:* si perde un appiglio di orientamento nella sidebar; in
cambio spariscono cinque aggregati con le loro cache e le loro invalidazioni.

Con il Task 5 il conteggio dei membri di un album **non è più un aggregato**: i
membri stanno in `album_assets`, quindi è una lettura banale.

**Verifica:** un test che conta le query emesse dal caricamento della sidebar e
asserisce che **non** ne emette nessuna di aggregazione.

---

## Task 12 — L'indice che manca alla timeline

`TimelineRepo::page` (`crates/keeppix-db/src/timeline.rs:134`) filtra
`status = 'indexed' AND kind <> 'unknown'`, ma `assets_timeline_idx` copre solo
`(taken_at_utc DESC, id DESC)`: i due predicati restano filtri applicati dopo il
recupero dalla heap. E `assets_status_idx` è parziale su `('discovered','error')`,
cioè **l'insieme opposto**: non aiuta mai la timeline.

```sql
CREATE INDEX assets_timeline_indexed_idx ON assets (taken_at_utc DESC, id DESC)
    WHERE status = 'indexed' AND kind <> 'unknown';
```

Questo chiude anche la domanda rimasta aperta dalla Fase 6 sull'indice
`status <> 'trashed'`: il predicato reale non è una disuguaglianza ma
`= 'indexed'`, quindi l'indice giusto è parziale sul valore cercato.

**Verifica:** `EXPLAIN ANALYZE` prima e dopo su `crates/keeppix-db/tests/scale_200k.rs`,
con il numero nel ledger.

---

## Task 13 — Problemi composti, non materia prima

`ProblemsView` (`routes/problems.rs:30`) è `{offline_libraries, failed_jobs,
error_assets}`: tre secchi di materia prima. La §47 dell'interfaccia chiede un
**elenco piatto**, dove ogni problema ha: id, gravità (avviso / errore), titolo,
descrizione **già in linguaggio naturale**, cartella o libreria coinvolta, e
l'elenco delle **azioni proposte con la loro etichetta**.

1. Comporre i problemi lato server: è il server che sa perché un job è fallito, non
   il frontend.
2. Le due nature che il prototipo mostra devono esistere davvero: *"file con sidecar
   XMP non scrivibile"* (permessi) e *"libreria offline"* (percorso di rete).
3. L'azione **"Riprova connessione"** richiede un endpoint di verifica di
   raggiungibilità: `POST /libraries/{id}/probe`.

**Ruling: la descrizione in linguaggio naturale la produce il backend, non il
frontend.** — Il frontend non ha il contesto per trasformare `last_error` in *"permessi
di scrittura mancanti sulla cartella"*, e replicare quella traduzione in ogni client
(web, iOS, futuri) significa scriverla tre volte e sbagliarla due. — *Costo se
sbagliato:* i messaggi vanno tradotti lato server, quindi la lingua dell'utente deve
arrivare nella richiesta.

**Verifica:** test che una cartella resa non scrivibile con `chmod` produce un
problema con gravità, testo e azione corretti.

---

## Task 14 — Suggerimenti tipizzati e cluster con destinazione

**Dove:** `crates/keeppix-db/src/search.rs`, `routes/search.rs`, `routes/map.rs`.

1. **`/search/suggest` restituisce oggi `Vec<String>`** (`db/search.rs:107`): stringhe piatte,
   pescate solo da `camera_model` e `filename`. La barra di ricerca (§23) deve sapere **di che
   tipo** è ogni suggerimento per creare la pillola giusta — e per un tag deve avere anche il
   colore del pallino.
   ```
   { kind: "tag"|"camera"|"folder"|"iso"|"year"|"country"|"filename",
     value: "...", label: "...", color: "#…"? }
   ```
   Le fonti nuove (tag, cartelle, ISO, anno, paese) si aggiungono man mano che gli assi della
   §6 e della Fase 7 esistono: la **forma** però va fissata ora, altrimenti cambia due volte.
2. **`MapClusterView` è `{lat, lon, count, cover_asset_id, clustered}`.** Il popover (§27)
   chiede in più l'**etichetta leggibile del luogo** e l'**id di destinazione** con cui aprire
   la cartella. L'etichetta si ha già dalla geocodifica inversa della Fase 4.

**Verifica:** un test per `kind`; un test che il popover di un cluster ha abbastanza dati per
navigare senza una seconda richiesta.

---

## Task 15 — «Condivisi con me», e i pezzi mancanti del profilo

**Dove:** `routes/share.rs`, `routes/permissions.rs`, `routes/auth.rs`.

1. **`GET /api/v1/shared-with-me`** — non esiste. `permissions` è interrogabile solo *per
   oggetto* (`ListQuery{object_type, object_id}`); la scheda "Condivisi con me" (§29) chiede
   l'inverso: tutti gli oggetti condivisi **con l'utente corrente**, ciascuno con nome, tipo,
   numero di elementi, **proprietario** e **il mio ruolo**.
2. **Conteggio elementi** per ogni link pubblico in `GET /share/links` (§29 mostra
   *"246 elementi"*). Stessa regola del Task 11: un `GROUP BY`, non un `COUNT` per riga.
3. `UserView` (`auth.rs:20`) copre nome, email e ruolo. Mancano al §61: **nome del server** e
   **data dell'ultima modifica password** (*"Ultima modifica: 3 mesi fa"*). Il colore avatar
   arriva dalle preferenze (Task 9).

**Verifica:** test che un oggetto condiviso via **gruppo** compare in `/shared-with-me` con
l'origine dell'ereditarietà, non solo quelli condivisi direttamente.

---

## Task 16 — Avanzamento e annullamento delle operazioni lunghe

Il documento lo dichiara aperto (Parte XII §2.1): *"restano senza stato di avanzamento le
operazioni lunghe sul disco: rinomina di massa, spostamenti, scansioni. Lì non basta uno
scheletro — serve un avanzamento con una percentuale, e probabilmente la possibilità di
annullare a metà."*

**Il canale esiste già**: `/ws` è nato come canale di notifica, e il contratto congelato dice
che non è fonte di verità — esattamente il ruolo giusto per un avanzamento.

1. Le operazioni lunghe restituiscono subito un `operation_id` invece di bloccare.
2. Eventi di avanzamento sul WebSocket: `{operation_id, done, total, phase}`.
3. `POST /api/v1/operations/{id}/cancel`.

**Ruling: annullare a metà produce una riuscita parziale, non un rollback.** — Le rinomine e
gli spostamenti già eseguiti sul disco sono fatti; fingere di poterli disfare significherebbe
un secondo giro di operazioni sul filesystem che può a sua volta fallire. L'operazione annullata
restituisce lo stesso involucro del Task 1, con l'elenco di ciò che era già passato. — *Costo se
sbagliato:* l'utente vede "annullata" e trova metà lavoro fatto, quindi l'interfaccia **deve**
dirglielo con i numeri.

**Verifica:** test che annullare a metà di 100 rinomine lascia esattamente le prime N applicate e
le elenca; test che il progresso arriva anche se il client si riconnette a metà.

---

## Task 17 — `GET /bootstrap`: nove richieste diventano tre

Aprire la Timeline a freddo costa **nove richieste** prima del primo disegno utile
(utente, preferenze, cartelle+conteggi, spazio, buckets, geometria, prima pagina,
due badge), con tre catene di dipendenza vere. In LAN si nota poco; da fuori casa,
con 100 ms di andata e ritorno, è quasi un secondo di attesa.

`GET /api/v1/bootstrap` restituisce in un colpo: utente, preferenze, albero
cartelle con conteggi, spazio su disco, badge. Tutti dati piccoli, tutti richiesti
sempre, tutti già in cache lato server dopo i Task 9 e 11.

**Ruling: additivo, non sostitutivo.** — Le viste che cambiano un solo pezzo (le
preferenze da Impostazioni, i conteggi dopo un import) devono poterlo rileggere
senza riscaricare tutto, e i client non-web usano già i singoli endpoint.
`bootstrap` **compone gli stessi repository e non ha SQL proprio**, così non può
divergere. — *Costo se sbagliato:* due strade per lo stesso dato da tenere
coerenti.

**Verifica:** test che `bootstrap` e la somma dei singoli endpoint restituiscono
gli stessi valori; test che conta le query emesse e le confronta con la somma dei
singoli (deve essere ≤).

---

## Task 18 — Misurare la geometria prima di complicarla

La geometria intera pesa **4,7 MB** su 214.000 scatti (≈1,5 MB gzip). Su LAN è
nulla; su rete mobile è una pausa visibile a ogni avvio a freddo.

**Non ottimizzare in anticipo.** Questo task è una **misura**, con una soglia
dichiarata: se il primo disegno su rete mobile simulata supera i **2 secondi**, si
passa alla geometria **per mese** — solo i mesi vicini a quello guardato, con
l'altezza dei mesi non ancora scaricati **stimata** da `conteggio × rapporto
d'aspetto medio` (un numero che `/timeline/buckets` può restituire a costo zero) e
corretta quando arrivano i dati veri.

**Ruling: si parte dalla versione intera.** — È più semplice e rende lo scrubber
**esatto** invece che approssimato, e il documento chiede esplicitamente di
conoscere l'altezza dell'intera libreria in anticipo. Frammentare subito
significherebbe pagare complessità per un problema che sul caso d'uso primario —
un server di casa in LAN — non esiste. — *Costo se sbagliato:* si riscrive il
caricatore della geometria; layout e virtualizzatore restano identici.

**Verifica:** misura con throttling di rete, numero nel ledger, e la decisione
presa **in base a quel numero**, non a un'intuizione.

---

## Task 19 — Il protocollo WebSocket: da due eventi a nove

`routes/ws.rs` emette **solo** `assets.upserted` e `assets.deleted`. Il canale è
versionato (`v`) e ben fatto, ma è uno stub rispetto a ciò che l'interfaccia mostra
come **dato che cambia da solo**:

| Evento nuovo | Serve a |
|---|---|
| `analysis.progress` | §57 Analisi libreria — è una schermata di avanzamento **dal vivo** |
| `suggestions.changed` | badge Revisione (tag + volti) |
| `culling.changed` | badge Culling |
| `scan.progress` | import iniziale, Problemi |
| `operation.progress` | operazioni lunghe (Task 16) |
| `problems.changed` | §47 Problemi — un job che fallisce fa comparire una riga |
| `asset.derivative.ready` | transcodifica video completata (Fase 6) |
| `backup.finished` | esito backup (Fase 6) |

**Ruling: gli eventi sono magri — un segnale, non uno stato.** — Il contratto
congelato dice che *«il WebSocket è canale di notifica, non fonte di verità»*.
Quindi `suggestions.changed` dice "ricarica il contatore", non *quanto* vale: se
portasse il numero, un client che perde un messaggio resterebbe con un valore
sbagliato e nessun modo di accorgersene. Portare il numero resta ammesso **come
comodità**, mai come garanzia. — *Costo se sbagliato:* un giro in più di richiesta
per aggiornare un badge, che è esattamente il prezzo della correttezza.

**`analysis.progress` è il caso che giustifica il task**: senza push, §57 si
ridurrebbe a un'interrogazione a intervalli — cioè aggiungere carico a un Pi
proprio mentre l'analisi lo sta già usando.

**Verifica:** test che un client che si riconnette non perde lo stato (il primo
messaggio dopo la connessione è uno stato completo, non un delta); test che ogni
evento nuovo ha un consumatore reale nel frontend, altrimenti `check-wired.py` lo
segnala.

---

## Task 20 — La pausa automatica dell'analisi è un comportamento del server

Il documento fissa la soglia: **4000 ms — quattro secondi dall'ultimo cambio di
vista** — dopo i quali l'analisi riprende da sola. E dichiara la differenza di
velocità fra i livelli: **42 ms per foto in "Piena", 260 ms in "Ridotta"** — sei
volte più lenta, con l'interfaccia che dichiara all'utente la coda residua
ricalcolata.

Non è cosmesi dell'interfaccia: è **il server** che deve mettere in pausa e
riprendere, perché è il server a fare il lavoro. L'interfaccia si limita a dire
quando l'utente è attivo.

**Ruling: la soglia è configurabile, non cablata.** — Il documento la elenca fra le
decisioni aperte (*«i numeri esatti sono da tarare sul sistema vero»*): 4 secondi
vengono da un prototipo senza carico reale. — *Costo se sbagliato:* su hardware
lento l'analisi riparte troppo presto e rallenta la navigazione, che è il difetto
che la pausa esiste per evitare.

**Verifica:** test che l'analisi si ferma entro un tick dall'attività e riprende
dopo la soglia; test che i due livelli producono throughput misurabilmente diversi.

---

## Task 21 — L'import a lotti, e le due discrepanze

**Il numero che giustifica il task.** La prova sul campo
(`.superpowers/field-test-20260817-1855.md`) misura **1,65 asset/s** su Mac con NVMe.
Estrapolato a 200.000 asset: **~34 ore** su quell'hardware, **4–7 giorni** su un
Raspberry Pi 5, che è il bersaglio dichiarato.

La scomposizione dice dove attaccare: l'overhead **per file** misurato in Fase 1b è
~272 ms di coda e database, che a 200.000 asset fanno **~15 ore prima di
decodificare qualunque cosa**. È la voce più grossa, ed è anche l'unica fatta di
lavoro raggruppabile.

1. **Inserimento a lotti** invece che a file singolo: una transazione ogni N file,
   `COPY`/`INSERT` multi-riga, un solo `change_log` per lotto.
2. Misurare di nuovo, sullo stesso archivio, e mettere il numero nel ledger.
3. Se il tempo resta dell'ordine dei giorni, valutare l'**import in due tempi**
   (prima passata: albero + EXIF + thumbhash, libreria navigabile in un'ora;
   seconda, di notte: hash del contenuto e derivati). Il thumbhash rende le tessere
   già a colori, e la geometria è già nota: la libreria è *usabile* prima di essere
   completa.

**Ruling: la decisione sul due-tempi si prende con il numero in mano.** — Fra "34 ore"
e "7 giorni" cambia la risposta, e la prova disponibile è su 1.558 file su un Mac,
non su 200.000 su un Pi. — *Costo se sbagliato:* si consegna un prodotto in cui il
primo avvio chiede una settimana prima di mostrare qualcosa, cioè esattamente il
momento in cui un utente decide se tenerlo.

**Due discrepanze da chiudere nello stesso task**, entrambe piccole e reali:
- `default_night_window()` (`keeppix-jobs/src/profile.rs:29`) è **2:00–6:00**, ma
  l'interfaccia dichiara all'utente **2:00–7:00**. Vanno allineate — e siccome è un
  testo che l'utente legge come una promessa, vince l'interfaccia salvo ragioni.
- `RegionView` ha già `downloaded_bytes`, `status` e `last_error`: l'avanzamento del
  download delle mappe **esiste come dato** ma non viene mai spinto. Aggiungere
  `region.progress` agli eventi del Task 19.

**Verifica:** import dello stesso archivio prima e dopo, con i due numeri nel ledger.

---

## Task 22 — La pipeline di derivati sa decodificare solo JPEG: chiuderlo

**Debito trovato il 20 agosto 2026 su codice già in produzione (Fase 1b/2), non su una
fase futura.** `crates/keeppix-media/src/kind.rs::detect_kind` classifica correttamente
JPEG, PNG, GIF, WebP, TIFF (non-camera) e HEIC/HEIF/AVIF come `AssetKind::Image` — ma
`crates/keeppix-media/src/derive.rs::derive_from_bytes` chiama **solo**
`zune_jpeg::JpegDecoder`. Un TIFF, PNG, WebP-sorgente o HEIF importato oggi viene accettato
in libreria e poi **fallisce a generare miniatura e preview** (`DeriveError::Decode`),
verificato leggendo il codice, non ipotizzato.

**Contesto che rende questo task più urgente, non meno**: dopo la decisione del 20 agosto
sul Culling (`fase-7-ai-tag-scene.md` §B), i RAW entrano in Keeppix **solo** attraverso il
Culling — mai importati direttamente in libreria. Chi vuole una foto "normale" in libreria
la carica lei, e quella foto è quasi sempre uno di questi formati non-JPEG, non un RAW.
Senza questo fix, "carica foto normali" funziona bene solo per chi esporta sempre in JPEG.

1. **PNG**: crate `png` (o `image` con la sola feature PNG) — puro Rust, nessuna dipendenza
   C nuova.
2. **TIFF**: crate `tiff` — stesso decoder usato internamente da `image`-rs, puro Rust.
3. **WebP sorgente**: la libreria `webp` (già dipendenza, usata oggi solo per scrivere) sa
   anche leggere — verificare se serve solo collegarla nel dispatch o se manca un percorso.
4. **HEIF 8 e 10 bit**: `libheif-rs` (binding a `libheif`) — **l'unica vera dipendenza C
   nuova**, stessa categoria di LibRaw (già accettata nel progetto). Verificare esplicitamente
   che il build di libheif scelto supporti il 10 bit: non è garantito su tutte le
   distribuzioni pacchettizzate, e va confermato prima di dichiararlo supportato.

**Ruling: i nuovi decoder passano dallo stesso sandbox degli altri, non ne sono esenti.** —
`libheif`/HEVC hanno una storia reale di vulnerabilità nei parser su input non fidato: è
esattamente la classe di rischio per cui LibRaw e ffmpeg già girano dentro
`crates/keeppix-media/src/sandbox.rs` (`RLIMIT_AS`/`RLIMIT_CPU`). Un decoder nuovo che
processa file caricati da chiunque abbia accesso alla libreria non è meno pericoloso di un
RAW o un video — è più pericoloso: HEIF è il formato con la storia di CVE più recente dei
quattro. — *Costo se sbagliato:* un decoder fuori sandbox è il prossimo
`RLIMIT_AS`-troppo-basso della situazione, scoperto in produzione invece che in test.

**Verifica:** un file di prova per formato (incluso un HEIF 10 bit reale, non sintetico) che
produce miniatura e preview corrette; un test che un file malformato di ciascun formato fa
fallire il job in modo pulito (nessun crash, nessun processo orfano) invece di bloccare la
coda.

---

## Task 23 — Chiudere l'OpenAPI e i client generati

1. Annotare con `utoipa` gli otto gruppi oggi assenti dallo spec generato: `albums`,
   `share`, `groups`, `permissions`, `audit`, `backup`, `restore`, `upload`,
   più `health`.
2. Aggiungere in CI un controllo che **fallisce se una rotta registrata nel router
   non compare in `openapi.json`**: è l'unico modo perché il buco non si riapra.
3. Invocare `scripts/generate-api-clients.sh` in CI come passo build-and-discard,
   così la generazione è verificata davvero (debito aperto dalla Fase 6).

**Verifica:** il conteggio delle rotte del router e quello dei path dello spec
combaciano; il generatore gira in CI senza errori.

---

## Chiusura della fase

- `cargo fmt`, `clippy -D warnings`, `cargo deny`, `check-wired.py` puliti.
- `./scripts/test.sh` completo verde.
- CI reale verde sul branch (ora che la repo è pubblica e i minuti sono illimitati).
- `docs/superpowers/README.md`, `docs/CONTINUE.md`, il roadmap e
  `scripts/wired-exceptions.txt` aggiornati.
- Ledger `.superpowers/sdd/2026-08-20-keeppix-fase-10/progress.md` con un `Ruling:`
  per ogni decisione presa in corsa.
