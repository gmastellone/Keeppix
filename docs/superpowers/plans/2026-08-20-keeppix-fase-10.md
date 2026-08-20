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

## Task 2 — Endpoint di geometria della timeline

**Dove:** `crates/keeppix-api/src/routes/timeline.rs`, `crates/keeppix-db/src/timeline.rs`,
nuova migrazione.

1. `GET /api/v1/timeline/geometry` con gli stessi parametri di filtro e la stessa
   risoluzione di visibilità di `/timeline` (`VisibilityScope::resolve`).
2. Corpo binario, `application/octet-stream`, record da 22 byte
   (`uuid` 16 · `w` u16 · `h` u16 · `month` u16), little-endian. Intestazione di 8
   byte con versione di formato e numero di record.
3. `ETag` derivato dal massimo `updated_at` della vista + conteggio: permette
   `304 Not Modified` sul rientro nella stessa vista, che è il caso normale.
4. Migrazione con l'indice di copertura:
   ```sql
   CREATE INDEX assets_geometry_idx ON assets (folder_id, taken_at_utc DESC, id DESC)
       INCLUDE (width, height) WHERE status <> 'trashed';
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

**Verifica:** test che con `purged` e un chiamante non-owner l'intero lotto è
rifiutato **prima** di toccare qualunque file (autorizzazione prima
dell'esecuzione, non a metà); test di riuscita parziale con un file già assente.

---

## Task 5 — Album dinamici, condivisi, con copertina

**Dove:** nuova migrazione, `crates/keeppix-db/src/albums.rs`, `routes/albums.rs`.

1. Migrazione con `kind`, `rule jsonb`, `is_shared`, `cover_tint`, `monochrome` e il
   `CHECK` che lega `rule` a `kind='dynamic'` (spec §5.2).
2. `rule` è un `SearchNode` serializzato. La lettura passa dallo **stesso**
   `compile_for_sql`: nessun secondo compilatore.
3. `GET /albums` restituisce `member_count` e `date_range` (min/max `taken_at`) per
   entrambi i tipi. Per i dinamici entrambi sono calcolati; vanno in cache `moka`
   con invalidazione esplicita su: creazione/modifica di album, import, cestinamento,
   modifica di metadati che l'AST possa toccare.
4. Gli album dinamici **rifiutano** `POST /albums/{id}/assets` con `409`: non hanno
   membri espliciti.

**Verifica:** test che un album dinamico "preferiti del 2026" cambia membri quando
una foto viene marcata preferita, **senza** che nulla venga scritto in
`album_assets`; test che la cache viene invalidata su quella marcatura.

---

## Task 6 — Nuovi assi di `SearchNode`

**Dove:** `crates/keeppix-db/src/search.rs`, `routes/search.rs`, nuova migrazione.

1. Varianti nuove: `Rating{cmp,value}`, `Favorite`, `DateRange{from,to}`,
   `Day{value}`, `Month{value}`, `Country{value}`, `Aperture{cmp,value}`,
   `Shutter{cmp,value}`.
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
   Cerca sia la vista "Preferiti" sia gli album dinamici: **una sola
   implementazione per tre schermate.**

**Verifica:** test che `favorite` e `pick` sono indipendenti (scartare nel culling
non tocca il preferito); `EXPLAIN` sulla vista Preferiti deve usare l'indice
parziale.

---

## Task 11 — Gli aggregati per riga di elenco

**Dove:** `crates/keeppix-db/src/folders.rs`, `albums.rs`, `share.rs`, cache in
`crates/keeppix-db/src/lib.rs`.

Tre conteggi che l'interfaccia mostra accanto a **ogni riga** di un elenco, e che
diventano N+1 se scritti nel modo ovvio:

1. **foto per cartella** → `asset_count` in `FolderView`, per `/folders/tree`.
2. **membri per album** → già previsto dal Task 5, stessa tecnica.
3. **elementi per link pubblico** → in `/share/links`.

Regola unica: **un solo `GROUP BY` per elenco**, mai un `COUNT` per riga. Risultato
in cache `moka` con **invalidazione esplicita** agganciata a import, cestinamento e
spostamento — la cache di Fase 6 è senza TTL apposta, e qui un conteggio scaduto è
un numero sbagliato mostrato all'utente, non un rallentamento.

**Verifica:** un test che conta le query emesse (`sqlx` logging o un contatore) e
asserisce che una sidebar con 3 cartelle ne emette **una**, non tre; un test che
l'import di una foto invalida il conteggio della sua cartella.

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

## Task 14 — Chiudere l'OpenAPI e i client generati

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
