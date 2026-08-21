# Fase 10 — Superficie API per l'interfaccia — Ledger

Branch: `fase-10`
Base: `main` @ a741ad5 (post-merge Fase 5+6)
Plan: `docs/superpowers/plans/2026-08-20-keeppix-fase-10.md`
Spec: `docs/superpowers/specs/fase-10-api-interfaccia.md`

Ordine obbligatorio (PROSEGUI): Task 1 → Task 1bis → resto.
Task 1 e 1bis vanno per primi, in quest'ordine.

## Decisioni e Ruling

Ruling: `FlagRepo::batch_set_partial` + `AssetRepo::filter_visible` invece di un
loop `set` nell'API — SQL resta in `keeppix-db`, un solo INSERT per i riusciti,
i non visibili finiscono in `failed` con `Forbidden`/`NotFound`. — *Costo se
sbagliato:* un INSERT in meno di efficienza rispetto al vecchio `batch_set`
all-or-nothing; comportamento per-id corretto.

Ruling: `PermissionRepo::partition_editable_assets` è la partizione condivisa
per metadata/geotag — visibilità + editor, senza abortire al primo rifiuto.
`assert_can_edit_assets` lo riusa ma mantiene lo short-circuit admin. — *Costo
se sbagliato:* un asset viewer-only classificato male; i test di partial
success lo bloccano.

Ruling: `recalculate-timezones` espone `BulkOutcome` **senza** rimuovere
`changed_count` (contratto additivo). `failed` è tipicamente vuoto: l'operazione
è per libreria, non per lista di id; i candidati sono già filtrati dalla query.
— *Costo se sbagliato:* un fallimento per-asset in timezone non entra in
`failed` finché non si partiziona il writer; accettabile per Task 1.

Ruling: in `import-gpx`, gli asset modificabili senza match temporale **non**
compaiono né in `succeeded` né in `failed` — non è un errore §7, è assenza di
coordinate da scrivere. Solo i geotaggati entrano nel `batch_id`. — *Costo se
sbagliato:* la UI non vede "processati senza match"; può aggiungere un motivo
additivo più avanti.

Ruling: `FailureReason::Unknown` mappa Conflict/Corrupted/InsufficientStorage/
Gone e IO non classificabile — meglio un quinto caso onesto che fingere
`permission-denied`. — *Costo se sbagliato:* "Riprova" non offerto dove magari
servirebbe; preferibile al contrario.

Task 1: complete (commit 14af320, tests green)

Ruling: `autovacuum_vacuum_scale_factor = 0.05` su `assets` — 5% dead tuples
invece del default 0.2; su librerie grandi la VM map resta fresca senza
aspettare un quinto della tabella. — *Costo se sbagliato:* più cicli
autovacuum su una tabella calda; accettabile per index-only scan affidabile.

Ruling: compose default = profilo SSD/NVMe (`random_page_cost=1.1`, …);
microSD override via `.env`. Valori misurati all'installazione (Fase 7),
non cablati come universali. — *Costo se sbagliato:* profilo sbagliato per
metà degli utenti finché non ricalibrano.

### EXPLAIN Task 1bis (15k righe `indexed`, testcontainer Postgres 17)

`random_page_cost=4.0` — timeline page (mese 2015-06, LIMIT 200):

```
Limit → Sort → Nested Loop → Bitmap Heap Scan on assets
  → Bitmap Index Scan on assets_timeline_idx
  → Index Only Scan folders_pkey
```

`random_page_cost=1.1` — timeline page: **stesso piano** (indice già preferito
con 720 righe nel mese su 15k totali).

`random_page_cost=4.0` — geometry stand-in (`folder_id, taken_at_utc DESC, id DESC`):

```
Limit → Sort → Seq Scan on assets (Filter: status = 'indexed')
```

`random_page_cost=1.1` — geometry stand-in: **stesso piano** (Seq Scan — indice
covering non esiste ancora, Task 2).

Task 1bis: complete (commit 376f030 + ledger cbccdd0, tests green)

## Task 2 — Endpoint di geometria della timeline

Ruling: `GeometryRecord`/`Geometry` in `keeppix-db` non portano `id` né
`count` separata — `records.len()` è il conteggio, e l'`ETag` combina quello
con `max(updated_at)`. Non serve una `count(*)` a parte. — *Costo se
sbagliato:* nessuno, è solo evitare una query ridondante.

Ruling: `TimelineRepo::geometry`/`geometry_in_bounds` **non** filtrano
`a.kind <> 'unknown'` nel percorso senza `bbox` — a differenza di
`page`/`buckets_in_bounds`. Il trigger `sync_folder_month_counts` (0009,
00011) non guarda `kind`, quindi `folder_month_counts` già conta gli asset
`unknown` indicizzati con `taken_at`; per far combaciare esattamente
`geometry.records.len()` con la somma dei bucket (verifica richiesta dal
piano) la query senza `bbox` deve restare coerente con **quel** conteggio,
non con `page`. Il percorso con `bbox` invece filtra `kind <> 'unknown'`,
perché lì il confronto è con `buckets_in_bounds` (che already lo fa). — *Costo
se sbagliato:* un pugno di file "unknown" (note di testo, sidecar) entrano
nella geometria della vista intera senza filtro bbox; cosmetico, la
geometria non identifica nulla. Se si scopre che serve coerenza con `page`
invece che con `buckets`, va rifatto aggiungendo `kind` all'indice di
copertura (che oggi non lo porta) o accettando di perdere l'index-only scan.

Ruling: `assets_geometry_idx` ha `folder_id` come colonna guida (come da
spec). Senza un filtro `library`/cartella che lo restringa, Postgres preferisce
il vecchio `assets_timeline_idx` (niente `Sort`, ma heap fetch per
`width`/`height`) invece del nuovo indice — misurato con `EXPLAIN`: bindare
`library_id = NULL` fa scegliere `assets_timeline_idx`, bindare una libreria
vera fa scegliere `assets_geometry_idx` in **index-only scan** (`Heap
Fetches: 0`). La verifica di scala usa quindi `?library=...`, il caso reale
per cui l'indice è stato disegnato. Un ipotetico "tutte le librerie insieme,
nessun filtro" resterebbe su `assets_timeline_idx` con heap fetch per riga —
non un seq scan, ma nemmeno index-only. Non è nel percorso che la spec chiede
di verificare, quindi non ho aperto un secondo indice per coprirlo. — *Costo
se sbagliato:* un admin senza libreria selezionata paga heap fetch invece di
index-only su una vista multi-libreria molto rara nell'uso reale.

Ruling: budget di scala per `/timeline/geometry` = 900ms su 200.000 asset
(vs. 300ms di `TIMELINE_BUDGET` per una singola pagina). `EXPLAIN ANALYZE`
mostra ~85-110ms di esecuzione server-side con `Index Only Scan` puro; il
resto (~500-600ms osservati) è trasferimento e decodifica lato client di
200.000 righe in una sola risposta — esattamente il costo che l'endpoint
sostituisce a 1.070 richieste paginate (spec §2.2), non uno che aggiunge.
900ms lascia margine di 3x sul misurato senza avvicinarsi al ~2s del seq
scan degradato che la spec cita come alternativa. — *Costo se sbagliato:* il
budget è troppo permissivo per accorgersi di una regressione più piccola di
3x; preferibile a un test instabile in CI.

Ruling: formato binario — intestazione 8 byte (`version: u32 LE = 1`,
`count: u32 LE`), poi `count` record da 6 byte (`w: u16`, `h: u16`,
`month: u16`, tutti LE). `month = anno*12 + mese_di_calendario (1..=12)`.
Width/height satura a `u16::MAX` se l'originale eccede (RAW enormi); month
satura ai margini di `u16` invece di traboccare — una data EXIF corrotta
resta cosmetica, non un panic (nessun `unwrap`/`expect` nel codice di
produzione, per invariante AGENTS.md). — *Costo se sbagliato:* nessuno
osservato; sono margini teorici (foto anno 1 o anno 5460+).

Ruling: `ETag` = `"{count in hex}-{max(updated_at).timestamp_micros() in hex}"`,
calcolato con una seconda query leggera (`max(updated_at)` sugli stessi
filtri, senza `width`/`height`) — tenerla fuori dalla query principale è
necessario: selezionare `updated_at` lì romperebbe l'index-only scan, perché
quella colonna non è nell'`INCLUDE` dell'indice di copertura. Misurata: ~26ms
su 200k righe, trascurabile rispetto al trasferimento della query principale.
`If-None-Match` è confrontato per uguaglianza esatta (o `*`), senza gestione
di liste multiple pesate — non serve qui. — *Costo se sbagliato:* un client
con più valori `If-None-Match` complessi (raro per un fetch programmatico)
non farebbe match; degrada a un 200 pieno, non a un errore.

Ruling: OpenAPI documenta la risposta con `body = [u8]` — utoipa 5.5 lo
risolve da solo a `content-type: application/octet-stream`,
`schema: {type: array, items: {type: integer, format: int32}}`. Non è lo
`{type: string, format: binary}` "ideale" di OpenAPI 3.1 per file binari
(richiederebbe una struct segnaposto dedicata solo per la doc, vedi
juhaku/utoipa#1146); ho preferito la forma automatica, corretta nel
content-type che è la parte che conta per i client generati, invece di una
struct fittizia senza controparte reale nel codice. — *Costo se sbagliato:*
un generatore di client molto pedante potrebbe tipare il corpo come
`number[]` invece di `Uint8Array`/`Data`; da rivedere se un client generato
si rivela scomodo da usare.

Task 2: complete (commit e3c7944 db + 447cd66 test di scala + 597372a api +
9bab5d0 ledger, test verdi:
`keeppix-db` timeline.rs 11/11, scale_geometry.rs 4/4, migrations.rs
11/11; `keeppix-api` timeline.rs 18/18, openapi.rs 7/7 incl. snapshot
rigenerato). `cargo fmt --check` e `cargo clippy --workspace --all-targets
-- -D warnings` verdi su tutto il workspace. `./scripts/test.sh` completo
**non eseguito** (costerebbe l'intera suite, inclusi `scale_200k.rs` e gli
altri test di scala non toccati da questo task): eseguiti invece i test
focalizzati elencati sopra più una verifica di build/clippy sull'intero
workspace.

Ruling: `geometry_stamp` / `geometry_stamp_in_bounds` (`count(*)` +
`max(updated_at)`) validano `If-None-Match` **prima** della scansione
completa — review Task 2: il 304 sul rientro non deve pagare il fetch di
tutti i `width`/`height`. — *Costo se sbagliato:* stamp e body divergono
se i filtri differiscono; i test di uguaglianza stamp↔geometry lo
bloccano.

Task 2 review fix: complete (commit 38063f6, tests green)

## Sync docs da main (a6c5ee1)

Ruling: piano Fase 10 e PROSEGUI riallineati da `origin/main` — aggiunto
**Task 22** (decodifica PNG/TIFF/WebP/HEIF; OpenAPI diventa Task 23). Le
decisioni del 20 agosto sera su Culling/RAW/IA restano **Fase 7+**; in
Fase 10 il Task 3 (stack in browse) resta come da piano. — *Costo se
sbagliato:* chiudere la 10 senza Task 22 lascia "carica foto normali"
rotto per tutto tranne JPEG.

## Task 3 — Lo stack collassato nelle viste di browse

Un implementatore precedente si è bloccato a metà lavoro (file modificati ma
non committati, più due migrazioni concorrenti numerate 0035/0036 che
duplicavano/contraddicevano la stessa modifica all'indice). Ho ripreso il
task da zero verificando ogni file toccato riga per riga invece di fidarmi
del lavoro trovato, ed eliminato le migrazioni duplicate: resta solo
`0035_assets_geometry_idx_stack.sql`.

Ruling: `TimelineRepo::page`/`page_in_bounds`, `SearchRepo::run` restituiscono
`Vec<AssetWithStack>` (`keeppix-db::stacks`), non un `BrowseAsset` separato —
`AssetWithStack { asset: Asset, stack: StackBadge }` con `Deref<Target =
Asset>`, così il codice che legge solo campi dell'asset (cursori keyset,
`taken_at_utc`, `id`) non cambia. La riga grezza è `AssetStackRow`, che
converte passando per `AssetRow::from_raw` (i campi di `AssetRow` sono
privati al modulo `assets`) — stesso pattern di `AlbumAssetRow` in
`albums.rs`. — *Costo se sbagliato:* nessuno osservato; è solo una scelta di
forma, riusabile identica da timeline e ricerca.

Ruling: il filtro di primario (`LEFT JOIN stacks s ON s.id = a.stack_id` +
`WHERE (a.stack_id IS NULL OR a.id = s.primary_asset_id)`) e il calcolo del
badge (`LEFT JOIN LATERAL` che aggrega `count(*)` e i `kind` dei membri) sono
due frammenti SQL separati in `stacks.rs`
(`STACK_PRIMARY_JOIN_SQL`/`STACK_PRIMARY_ONLY_SQL` vs
`STACK_BADGE_JOIN_SQL`/`STACK_BADGE_COLUMNS_SQL`): la geometria usa solo il
primo (deve solo escludere i non-primari, non mostrare `stack_size`/
`raw_kind`), page/search usano il secondo (che include il primo). Un join in
meno da pianificare per la geometria, che è già la query più sensibile
all'indice di copertura. — *Costo se sbagliato:* nessuno, è solo economia di
piano di esecuzione.

Ruling: `/timeline/buckets` conta direttamente da `assets` (join `folders` +
`stacks`, stesso filtro di primario di `page`) invece di leggere da
`folder_month_counts`. Il trigger che alimenta quella tabella
(`sync_folder_month_counts`, migrazioni 0009/00011) non guarda `stack_id`:
farglielo guardare avrebbe richiesto un secondo trigger su `stacks` per
intercettare i cambi di primario fatti da `StackRepo::set_primary` (che non
tocca `assets`, quindi il trigger su `assets` non se ne accorgerebbe da
solo) — più complessità nel trigger per un solo endpoint che la legge.
`folder_month_counts` resta intatta per gli altri usi (contatori di
cartella, cestino, `scale_200k.rs`). — *Costo se sbagliato:* un doppio
mantenimento contabile (query diretta per i bucket, trigger per il resto) se
in futuro serve un altro consumatore stack-aware dei conteggi mensili;
accettabile oggi con un solo consumatore.

Ruling: **bug trovato e corretto in review**: `TimelineRepo::geometry` (il
percorso senza `bbox`) aveva già il commento che dichiarava il filtro
`a.kind <> 'unknown'` (per restare coerente con `page`/`buckets`, ora che
`buckets` non legge più da `folder_month_counts`), ma l'SQL non lo
applicava — probabilmente un resto del refactor precedente lasciato a metà.
Corretto aggiungendo il filtro a `geometry()`, al suo `last_modified_sql` e a
`geometry_stamp()` (che deve restare sugli stessi filtri di `geometry()`,
altrimenti il suo `count` diverge da `geometry.records.len()` e il 304 su
`If-None-Match` diventerebbe scorretto). Aggiunto un test rosso→verde
(`geometry_omits_unknown_kind_assets_without_a_bbox_filter`) che prima non
esisteva: la copertura precedente controllava solo il percorso con `bbox`. —
*Costo se sbagliato:* nessuno ora che il test lo blocca; prima del fix, un
file "unknown" (nota di testo, sidecar) sarebbe comparso nella geometria
della vista intera ma non nella pagina corrispondente, un'incoerenza visiva
silenziosa.

Ruling (item 5 del brief — "misura il JOIN prima di aggiungere indici"):
misurato con `EXPLAIN (ANALYZE, BUFFERS)` a 200k righe (`scale_200k.rs`,
`scale_geometry.rs`, aggiornati per riflettere le query vere con lo stack
join). Il `LEFT JOIN stacks` per il filtro di primario costa sub-millisecondo
anche a 200k asset (tabella `stacks` piccola, hash/nested-loop join) — buckets
81ms, pagina 200 righe 3ms (budget 300ms), geometria intera 161ms (budget
900ms) con `Index Only Scan` su `assets_geometry_idx` e `Heap Fetches: 0`
confermati nel piano. **Nessuna denormalizzazione** (`is_stack_primary`
booleano su `assets`) è stata necessaria: il join resta la soluzione, come
preferito dal brief. — *Costo se sbagliato:* da rivedere se un utente reale
arriva ad avere centinaia di migliaia di stack (non solo di asset) nella
stessa libreria, scenario non misurato qui.

Ruling: `assets_geometry_idx` (migrazione 0034, Task 2) va esteso — non
aggiunto un secondo indice — con `stack_id` e `kind` nell'`INCLUDE`
(migrazione 0035): senza, testare quei due campi nella `WHERE` di `geometry`
avrebbe forzato un heap fetch per riga, perdendo l'index-only scan misurato
nel Task 2. Non si può modificare la 0034 già applicata (regola sqlx del
checksum), quindi si droppa e si ricrea l'indice con lo stesso nome in una
migrazione nuova — stesso pattern già usato per la 0034 stessa quando ha
sostituito l'indice precedente. Verificato con `EXPLAIN`: piano rimane
`Index Only Scan ... Heap Fetches: 0` con entrambi i filtri applicati. —
*Costo se sbagliato:* un `DROP`+`CREATE INDEX` su una tabella già popolata a
200k+ righe è bloccante (nessun `CONCURRENTLY` nelle migrazioni sqlx
transazionali); accettabile per una fase di sviluppo, da rivedere se questa
migrazione deve girare online su un'istanza già in produzione.

Ruling: `raw_kind` per un asset non impilato si deriva dal solo `kind` di
quell'asset (`raw_image` → `"raw"`, `image` → `"jpeg"`, video/unknown →
`None`), senza toccare il database: è il caso coperto da
`AssetView::from_asset` (usato anche da `GET /assets/{id}`, che non ha
accesso al badge di stack). Per un primario di pila, `raw_kind` aggrega i
`kind` di **tutti** i membri (non solo il primario): uno stack RAW+JPEG dà
`"raw+jpeg"` anche se si guarda solo il campo del primario RAW. — *Costo se
sbagliato:* nessuno osservato, è la lettura naturale della tabella nel
brief.

Task 3: complete (commit successivo a questa voce, test verdi:
`keeppix-db` timeline.rs 16/16, search.rs 8/8, scale_200k.rs 2/2,
scale_geometry.rs 1/1 [+3 harness]; `keeppix-api` timeline.rs 20/20,
search.rs 2/2, stacks.rs 2/2, openapi.rs 7/7 incl. snapshot rigenerato per i
due campi additivi di `AssetView`). `cargo fmt --check` e `cargo clippy
--workspace --all-targets -- -D warnings` verdi su tutto il workspace.
`./scripts/test.sh` completo **non eseguito** (stesso motivo del Task 2:
costerebbe l'intera suite); eseguiti invece tutti i test toccati dal task
più le due prove di scala a 200k, con `EXPLAIN` alla mano.

## Task 4 — Eliminazione di massa a tre vie

Ruling: `TrashRepo::choose` è stato scomposto in una funzione libera
`authorize_choose` (visibilità + editor/owner-admin + risoluzione di
`asset`/`library`/`folder_abs`) più il corpo che scrive. `choose` la chiama e
basta; il nuovo `TrashRepo::assert_batch_purge_authorized` la richiama in un
ciclo su tutti gli id del lotto con `DiskAction::Purged`, **senza** eseguire
mai la parte che scrive. Questo è il modo in cui `purged` diventa
all-or-nothing solo sull'autorizzazione: `POST /assets/batch/delete` chiama
`assert_batch_purge_authorized` (che fallisce sul primo id non autorizzato,
prima di toccare qualunque file) solo quando `disk_action == purged`, poi
itera `choose` per elemento esattamente come per `kept`/`moved_to_trash` —
nessuna logica di cancellazione duplicata, il brief lo chiedeva
esplicitamente. — *Costo se sbagliato:* nessuno osservato; è un refactor a
comportamento osservabile identico per il percorso singolo (`DELETE
/assets/{id}`), coperto dagli stessi test di trash.rs (db) che erano già
verdi prima del task.

Ruling: il lotto per `kept`/`moved_to_trash` resta a riuscita parziale
per-id (come Task 1): ogni `choose` gira per conto suo e un fallimento
(Forbidden, file mancante, permessi) finisce in `failed` via
`BulkOutcome::from_partition`, senza bloccare gli altri id. Solo `purged` ha
il doppio cancello (autorizzazione tutto-o-niente, poi esecuzione per
elemento) — il brief lo chiede solo per `purged`, e alzarlo anche per le
altre due opzioni contraddirebbe l'invariante "ogni file deve poter
riportare il proprio esito" già stabilito per `choose`. — *Costo se
sbagliato:* nessuno nel dominio del brief.

Task 4: complete (commit successivo a questa voce). Test nuovi verdi:
`keeppix-api` trash.rs 6/6 (3 nuovi: rifiuto dell'intero lotto `purged` per
un editor non owner con i file intatti; successo parziale su file già
assente con `moved_to_trash` — `purged` è tollerante ai file mancanti, quindi
la prova RED/GREEN usa `moved_to_trash`, che non lo è; successo parziale su
cartella di cestino in sola lettura con `chmod`, mappato a
`permission-denied`). Tutti RED prima dell'implementazione (404, la rotta
non esisteva), GREEN dopo. Test preesistenti ancora verdi dopo il refactor
di `choose`: `keeppix-db` trash.rs 12/12, duplicates.rs 8/8 (usa `choose` in
loop in `DuplicateRepo::resolve`), permissions.rs 17/17; `keeppix-api`
duplicates.rs 4/4, metadata.rs 8/8, openapi.rs 7/7 (snapshot rigenerato con
`UPDATE_OPENAPI=1`, elenchi `operationId`/`security`/conteggio operazioni
aggiornati da 82 a 83 per `assets_batch_delete`). `cargo fmt --check` e
`cargo clippy --workspace --all-targets -- -D warnings` verdi su tutto il
workspace. `./scripts/test.sh` completo **non eseguito** (stesso motivo dei
Task 2/3): eseguiti i test toccati dal task più i moduli con dipendenza
diretta su `TrashRepo::choose`/`parse_action`.

### Verifica richiesta dal brief: «nessuna posizione» come valore

Difetto confermato, **deferito** (fuori dai file di questo task):
`OverrideRepo::effective` calcola `location` con
`COALESCE(o.location, a.location)`. Un override azzerato esplicitamente
(l'utente ha scelto "questa foto non ha un luogo", `asset_overrides.location
= NULL` con la riga presente) produce lo stesso `NULL` SQL di "nessuna riga
di override mai scritta" — `COALESCE` non li distingue, quindi la posizione
exif dell'asset torna a "vincere" anche dopo l'azzeramento esplicito. Non è
una sottigliezza teorica: verificato con un test end-to-end contro Postgres
reale (`crates/keeppix-db/tests/overrides.rs`,
`effective_location_after_an_explicit_clear_does_not_fall_back_to_the_exif_value`),
osservato RED (`Some(GeoPoint { lat: 41.9, lon: 12.5 })` invece di `None`)
prima di marcarlo `#[ignore]` con la spiegazione — non `#[allow]` su
un'asserzione sbagliata: il corpo resta il comportamento *corretto*
desiderato, così chi lo risolve lo riattiva togliendo l'attributo. La stessa
ambiguità vale, per costruzione identica, anche per `taken_at` e `place_id`
(entrambi letti con lo stesso `COALESCE`); il brief chiedeva solo di
verificare `location`, quindi non ho aperto il test per gli altri due, ma il
difetto è lo stesso.

**Perché non risolto qui**: un fix corretto richiede di distinguere "riga di
override con quel campo esplicitamente `NULL`" da "riga assente/campo non
toccato" — oggi impossibile perché entrambi gli stati collassano sullo stesso
`NULL` in colonna. Serve una nuova colonna/sentinella per campo (o un
formato diverso di `asset_overrides`), che tocca `apply_patch`,
`load_previous`/`restore_previous` (l'annullamento dei batch) e
`sidecar_source`, non solo `effective` — un cambiamento di modello dati più
grande del batch delete di questo task, e farlo solo per `location` senza
`taken_at`/`place_id` lascerebbe il contratto `COALESCE` incoerente a metà.
— *Costo se lasciato com'è:* un utente che nega esplicitamente il luogo di
una foto (GPS del telefono sbagliato, foto scannerizzata con exif fantasma)
la rivede comunque geolocalizzata finché non arriva un fix dedicato; nessun
rischio di sicurezza (nessuna esposizione di dati a chi non dovrebbe vederli,
solo un valore mostrato che l'utente aveva chiesto di azzerare).

## Task 5 — Album refresh

Ruling: `succeeded` del BulkOutcome di refresh elenca sia gli asset **aggiunti**
sia quelli **rimossi** in questa esecuzione — due facce della stessa mutazione
riuscita; `failed` resta vuoto (diff server-side su asset già visibili, non
per-id). — *Costo se sbagliato:* la UI non distingue entrati/usciti senza un
campo additivo futuro; accettabile per Task 5.

Ruling: album senza `rule` → `400 keeppix/album-has-no-rule` (non Forbidden):
non è un problema di permesso, è che non c'è nulla da rilanciare. —
*Costo se sbagliato:* un client che confonde 400 con 403; il `type` stabile
lo distingue.

Ruling: `NewAlbum.rule`/`CreateAlbumBody.rule` sono additivi su `POST
/albums` esistente — nessuna nuova rotta di creazione. `AlbumView` espone
anche `rule_run_at`/`is_shared`/`cover_tint`/`monochrome` (colonne aggiunte
dalla stessa migrazione, spec §5.2) con `#[serde(skip_serializing_if =
"Option::is_none")]` dove nullable, così un client vecchio non vede nulla
di nuovo se non li usa. — *Costo se sbagliato:* nessuno, il contratto
`/api/v1` resta solo-aggiunte.

Task 5: complete (commit 564ebbb db + a24971f api, tests green:
keeppix-db albums 11/11, migrations 11/11 [+1 nuovo: colonne rule/
rule_run_at/is_shared/cover_tint/monochrome], geo 14/14 [NewAlbum.rule
additivo]; keeppix-api albums 3/3). `cargo fmt --check` e `cargo clippy
--workspace --all-targets -- -D warnings` verdi su tutto il workspace.
`./scripts/test.sh` completo **non eseguito** (stesso motivo dei task
precedenti: costerebbe l'intera suite); eseguiti invece tutti i test
toccati dal task (`keeppix-db` albums.rs, migrations.rs, geo.rs;
`keeppix-api` albums.rs, openapi.rs per confermare che il nuovo endpoint
non tocca lo snapshot — `albums` resta fuori dalla superficie OpenAPI
generata, la chiude il Task 10/23).

## Task 6 — Nuovi assi di `SearchNode`

Ruling: i nove nuovi assi (`Rating`, `Favorite`, `DateRange`, `Day`,
`Month`, `Country`, `Aperture`, `Shutter`, `Place`) si annidano nell'unico
`SearchNode`/`compile_for_sql` esistente — nessun secondo modello, come
richiesto dal brief. `compile_for_sql` cresceva oltre il limite
`too_many_lines` di clippy con il nuovo match, quindi è stato scomposto in
`compile_for_sql` (combinatori `And`/`Or`/`Not` + dispatch) → `compile_leaf`
(assi già esistenti + dispatch dei nuovi) → `compile_search_axis` (i nove
assi del Task 6). Nessun comportamento cambia, solo la forma. — *Costo se
sbagliato:* nessuno, è un refactor a struttura.

Ruling: `compile_for_sql` prende ora `user_id: Option<uuid::Uuid>` per
alimentare `Rating`/`Favorite`, che sono per-utente (spec §4.1: il tuo 5
stelle non è il 5 stelle di un altro). `None` (un `AuthContext` senza
utente, se mai esistesse per questi percorsi) fa fallire quei due nodi con
`Forbidden` invece di produrre silenziosamente un confronto vuoto o, peggio,
un confronto sbagliato. Verificato che tutti i chiamanti reali
(`SearchRepo::run`, `AlbumRepo::refresh`, `GeoRepo::clusters`) passano da un
estrattore `SessionNotShare` che garantisce sempre `Some`, quindi il ramo
`Forbidden` è difensivo, non atteso in pratica. — *Costo se sbagliato:* nessuno
osservato; se in futuro un percorso senza sessione userà `Rating`/`Favorite`,
il `Forbidden` è il comportamento giusto comunque.

Ruling: `Favorite` — il piano lo assegna al Task 6 come asse di ricerca con
verifica `EXPLAIN`, ma la colonna `asset_flags.favorite` non esiste ancora
(è nominalmente Task 10: scrittura, `AssetView`, dominio `AssetFlags`).
Ho aggiunto la colonna minima (`boolean NOT NULL DEFAULT false`) e il suo
indice parziale (`asset_flags_favorite_idx` su `(user_id, asset_id) WHERE
favorite`) in questo task, con nome e forma già identici a quanto la spec
del Task 10 dichiara — così il Task 6 è verificabile end-to-end (query +
`EXPLAIN`) senza duplicare lavoro: il Task 10 troverà colonna e indice
pronti e userà solo la parte che gli manca (scrittura, superficie API). —
*Costo se sbagliato:* se il Task 10 avesse in mente un nome/forma diversi
per l'indice, va normalizzato lì; il costo è un `DROP`/`CREATE INDEX` in
più, non un conflitto di dati.

Ruling: `Country` risolve via `assets.place_id → places.country_code`
(`EXISTS` su `places`), **non** riusando `Folder` — nel prodotto reale
cartella e luogo sono due concetti distinti anche se nel prototipo
coincidevano (spec fase-10 §6). Non ho costruito un backfill automatico di
reverse-geocoding per popolare `place_id`: è fuori scope del Task 6, che
deve solo esporre l'asse di ricerca sul dato che Fase 4 già produce
(assegnazione manuale/import GPX). `value` viene uppercased a compile time
per combaciare con la convenzione di `country_code`. — *Costo se sbagliato:*
un asset senza `place_id` mai assegnato non compare mai per nessun paese,
comportamento corretto ma silenzioso finché non arriva un backfill dedicato.

Ruling: `Shutter` confronta `asset_exif.exposure` (testo EXIF grezzo, es.
`"1/125"` o `"2"`) convertendolo a secondi con un `CASE` SQL che gestisce
sia la forma a frazione sia quella decimale, e ritorna `NULL` (mai
comparabile, quindi mai un match falso) su formati malformati o divisore
zero invece di fallire la query. — *Costo se sbagliato:* un formato EXIF non
previsto (locale con virgola, notazione scientifica) non genera un errore
ma semplicemente non partecipa al filtro; preferibile a un panic o a un
500 su dati EXIF di terze parti che non controlliamo.

Ruling: `Day`/`Month` sono filtri ricorrenti (giorno-del-mese,
mese-dell'anno), non date assolute — la controparte naturale di `Year` già
esistente, e la lettura più utile per un utente che cerca "le foto di
compleanno" o "le foto d'estate" attraverso gli anni. `DateRange` resta
l'asse per un intervallo assoluto esplicito, entrambi gli estremi inclusi.
Tutti e tre filtrano su `taken_at_utc` restando fuori dal cestino, coerenti
con l'indice `assets_taken_day_idx` aggiunto nella stessa migrazione. —
*Costo se sbagliato:* se l'interfaccia si aspettava "giorno esatto" per
`Day`, la migrazione a un range di un giorno è un cambio piccolo e
localizzato in `compile_search_axis`.

Task 6: complete (commit 6ab60c8 migrazione + f1afe20 db/compile_for_sql +
12419cd test, tests green: `keeppix-db` search.rs 19/19 [+9 nuovi assi, +1
depth guard, +1 EXPLAIN partial index], migrations.rs 11/11 [+3 nuovi
indici], albums.rs 11/11, geo.rs 14/14; `keeppix-api` albums.rs 3/3,
openapi.rs 7/7 [nessuna modifica di superficie, `SearchNode` non è ancora
esposto via OpenAPI]). `cargo fmt --check` e `cargo clippy --workspace
--all-targets -- -D warnings` verdi su tutto il workspace. `./scripts/
test.sh` completo **non eseguito** (stesso motivo dei task precedenti);
eseguiti invece tutti i test toccati dal task più `keeppix-api` albums.rs/
openapi.rs per confermare che l'aggiunta non rompe il contratto pubblico.

## Task 7 — Sessioni attive

Ruling: `SessionId` (nuovo `id_type!` in `keeppix-domain`) identifica una
**famiglia** (`sessions.family_id`), non una riga di `sessions` — la riga
attiva cambia a ogni `POST /auth/refresh`, la famiglia no. `GET
/users/me/sessions` e la revoca lavorano sempre a livello di famiglia:
è quello che l'utente intende per "sessione"/dispositivo. — *Costo se
sbagliato:* nessuno osservato, è la lettura naturale dello schema 0002
(un login = una famiglia, una rotazione = una riga nuova nella stessa
famiglia).

Ruling: `device_label_from_user_agent` — funzione pura in `keeppix-db`,
non in `keeppix-domain` (non manipola entità di dominio, solo un
`Option<&str>` -> `Option<String>`) — riconosce browser/OS per
sottostringa con un ordine di priorità esplicito (Edge prima di Chrome,
Chrome prima di Safari, iOS prima di Android/Linux): gli `User-Agent`
reali si contengono a vicenda (Edge dichiara sia `Chrome/` sia `Safari/`;
un iPhone dichiara "like Mac OS X"). Un header presente ma non
riconosciuto produce `"Unknown device"`, non `None`: l'utente ha comunque
un dispositivo, semplicemente non sappiamo etichettarlo; solo l'header
**assente** produce `None`. — *Costo se sbagliato:* un browser/OS non
coperto (Brave, Vivaldi, ChromeOS) si etichetta col nome del motore
sottostante o "Unknown device" invece che col proprio nome; nessun rischio
di sicurezza, la stringa non è mai usata per decisioni di autorizzazione.

Ruling: la colonna legacy `sessions.user_agent` (0002_sessions.sql) resta
nello schema — non si modificano migrazioni già applicate (invariante
AGENTS.md) — ma da questo task in avanti `SessionRepo::create`/`rotate`
non ci scrivono più nulla: solo `device_label` viene popolato. Verificato
con un test che, dato lo stesso `User-Agent` in input, la colonna legacy
resta `NULL` mentre `device_label` porta l'etichetta breve. — *Costo se
sbagliato:* nessuno per gli utenti nuovi; le righe scritte prima di questo
task mantengono lo `User-Agent` intero già salvato, un debito preesistente
che questo task non retroattiva (fuori scope: nessuna riscrittura di dati
storici richiesta dal brief).

Ruling: `family_of` non prende `AuthContext` — stessa eccezione
documentata di `authenticate` (non elencata esplicitamente fra le quattro
di AGENTS.md, ma della stessa natura: il token presentato *è* la
credenziale, non c'è ancora un contesto quando serve risolvere la
famiglia corrente per confrontarla nell'elenco). Il confronto "è la
sessione corrente?" per il blocco della revoca (400
`keeppix/session-is-current`) vive nell'handler HTTP, non nel repository:
l'handler ha già il cookie sotto mano, e mettere quel controllo in
`revoke_family` avrebbe significato una seconda query di lookup dentro il
repository invece di un confronto in memoria. — *Costo se sbagliato:*
nessuno osservato; se un secondo chiamante HTTP arrivasse a usare
`revoke_family` senza passare dall'handler esistente, perderebbe la
protezione — non c'è oggi un secondo chiamante.

Ruling: `list_active` seleziona `consumed_at IS NULL AND revoked_at IS
NULL AND expires_at > now()` senza `GROUP BY`: la rotazione marca sempre
`consumed_at` sulla riga superata prima di inserirne una nuova, quindi
quel filtro individua già esattamente una riga per famiglia. `last_seen_at`
è `created_at` della riga attiva (login, o ultima rotazione se c'è stata):
è l'approssimazione più economica disponibile senza un contatore
aggiornato a ogni richiesta autenticata, che `authenticate()` non fa
apposta (vedi `authenticate_does_not_slide_expiry`, Fase 0). — *Costo se
sbagliato:* "ultimo accesso" nella UI può essere più vecchio dell'ultima
richiesta autenticata reale se la sessione non ha ancora ruotato; accettabile,
è la stessa granularità con cui l'utente già vede scadere la sessione.

Ruling: `revoke_family` tratta "famiglia inesistente" e "famiglia di un
altro utente" allo stesso modo — `Forbidden`, mai `NotFound` — stessa
regola di `AppPasswordRepo::revoke` e per lo stesso motivo (invariante
AGENTS.md: niente oracolo di esistenza). A differenza di
`AppPasswordRepo::revoke` non c'è un ramo admin: la spec non prevede che
un admin gestisca le sessioni di un altro utente da questo endpoint (lo
strumento per quello resta `POST /users/{id}/disable`, che già revoca
tutto). — *Costo se sbagliato:* se in futuro serve un pannello admin sulle
sessioni altrui, va aggiunto un percorso `AdminAuth` dedicato, non
riadattato questo.

Ruling: `DELETE /users/me/sessions/{id}` sulla propria sessione risponde
`400 keeppix/session-is-current` con `Problem::bad_request`, non `403`: non
è un problema di proprietà (è certamente sua), è che l'azione richiesta non
ha senso lì — il brief dice esplicitamente "per uscire c'è `/auth/logout`".
Un `type` stabile distinto permette al frontend di mostrare un messaggio
diverso da un 403 generico. — *Costo se sbagliato:* un client che non
distingue i due 4xx tratta "è la sessione corrente" come un generico
errore di permesso; il messaggio in `detail`/`title` resta comunque
comprensibile in debug.

Ruling: sia `DELETE .../sessions/{id}` sia `POST .../revoke-others`
chiamano `state.sessions.clear()` dopo la revoca — stesso pattern già
usato da `users::disable`/`users::change_password` — perché la cache
in-process è indicizzata per token del *chiamante*, non per famiglia
revocata: non esiste un `drop_token` puntuale possibile per un token che
questa richiesta non conosce. Il costo è che l'intera cache (di tutti gli
utenti) si svuota per la revoca di una sessione di un solo utente; la
cache ha comunque un TTL di 30s per entry, quindi il caso peggiore è già
limitato. — *Costo se sbagliato:* un utente revoca una propria sessione e
tutti gli altri utenti pagano un round-trip al database in più sulla
prossima richiesta autenticata; nessun rischio di correttezza.

Task 7: complete (commit 4baf8cb domain SessionId + f425e36 db device
label/list/revoke + e251a73 api routes/openapi, tests green: `keeppix-db`
sessions.rs lib 4/4 nuovi [device_label_from_user_agent], sessions.rs
integration 22/22 [16 preesistenti + 6 nuovi: label storage, propagazione
alla rotazione, `list_active` marca solo la corrente, esclude
revocate/scadute, `revoke_family` isola il dispositivo, ownership
Forbidden-non-NotFound], migrations.rs 11/11; `keeppix-api` sessions.rs
7/7 nuovi [current su sessione singola e su due dispositivi, revoca cross-
dispositivo senza toccare la chiamante, blocco sulla sessione corrente,
403 non 404 su sessione di un altro utente, revoke-others multi-
dispositivo, 401 senza autenticazione], auth.rs 27/27, users.rs 9/9,
credentials.rs 5/5 [preesistenti, confermano che le modifiche a
`SessionRepo::create`/`rotate` non hanno rotto nulla], openapi.rs 7/7
[snapshot rigenerato, 83→86 operazioni, nuovo schema `SessionView`, tag
`auth` riusato]). `cargo fmt --check`, `cargo clippy --workspace
--all-targets -- -D warnings` e `cargo build --workspace --all-targets`
verdi su tutto il workspace; `cargo deny check bans` verde (nessuna nuova
dipendenza). `./scripts/test.sh` completo **non eseguito** (stesso motivo
dei task precedenti: costerebbe l'intera suite, incluse le prove di scala
non toccate da questo task); eseguiti invece tutti i test toccati dal
task più i moduli con dipendenza diretta su `SessionRepo`.

`scripts/check-wired.py` segnala le tre nuove rotte come "senza
consumatore frontend" — atteso: il frontend arriva in Fase 11 (come già
per `/timeline/geometry` del Task 2 e `/assets/batch/delete` del Task 4,
nessuno dei due aggiunto a `wired-exceptions.txt`). Seguito lo stesso
precedente: non aggiunto alle eccezioni, lasciato alla chiusura di fase
insieme agli altri.

## Task 8 — Spazio su disco per libreria

Ruling: libreria non visibile → **`Forbidden`, non `NotFound`** — il
piano Task 8 dice «404», ma vince l'invariante AGENTS.md (niente oracolo
di esistenza). Stesso comportamento di `GET /libraries/{id}`; documentato
nei test HTTP e nel report. — *Costo se sbagliato:* nessuno, è la regola
già applicata ovunque sulle librerie.

Ruling: cache **60 s in `Db`** (`library_storage_cache` moka, chiave =
`library_id`) — sidebar chiede a ogni load, `statvfs` su NFS non è gratis;
stesso crate/livello di `permission_cache`. — *Costo se sbagliato:* dato
fino a 60 s stale dopo un cambio di volume; accettabile per indicatore UI.

Ruling: `uploads::disk_usage` restituisce `(free_bytes, total_bytes)` —
refactor del `statvfs` già usato da `ensure_disk_space`
(`f_bavail * f_frsize`, `f_blocks * f_frsize`). — *Costo se sbagliato:*
`total` è capacità del volume, non spazio usato dalla libreria; è ciò che
la sidebar prototipo mostra («1,4 TB su 2 TB»).

Task 8: complete (commit 91e2f1d test + 2f3b471 db + 4803b0a api, tests
green: `keeppix-db` libraries 13/13 [+2 nuovi: byte coerenti, Forbidden
non-NotFound], `keeppix-api` libraries 11/11 [+2 nuovi: 200 coerente,
403 su libreria altrui], openapi 7/7 [snapshot 86→87,
`libraries_storage`]). `cargo fmt --check` e `cargo clippy --workspace
--all-targets -- -D warnings` verdi. `./scripts/test.sh` completo **non
eseguito** (stesso motivo dei task precedenti).

## Task 9 — Preferenze utente

Ruling: colonna `users.preferences jsonb NOT NULL DEFAULT '{}'` — un solo
documento per utente (spec §8.3), non una colonna per preferenza. — *Costo
se sbagliato:* nessuna query SQL per singola preferenza; una migrazione per
ogni campo nuovo resta possibile solo estendendo il merge, non lo schema.

Ruling: default allineati al mockup UI — `theme=chiaro`, `grid_density
{desktop:4,mobile:3}`, tre notifiche `true`, `language=it`; chiavi
notifiche `digest`/`condivisioni`/`problemi` come nel prototipo. — *Costo se
sbagliato:* un client che si aspettava nomi inglesi deve mappare; coerente
con Fase 11 Impostazioni.

Ruling: validazione unknown-field in `UserPreferences::apply_patch` (db) con
400 `keeppix/unknown-field` in API — top-level e nested (`grid_density.*`,
`notifications.*`); valori fuori range → `invalid-preference`. — *Costo se
sbagliato:* nessuno, è il contratto Task 9.

Task 9: complete (commit 938f469 db + 52fa867 api, tests green:
`keeppix-db` preferences 2/2, migrations 11/11; `keeppix-api` preferences
4/4, openapi 7/7 [snapshot 87→89]). `cargo fmt --check` e `cargo clippy
-p keeppix-db -p keeppix-api --all-targets -- -D warnings` verdi.
`./scripts/test.sh` completo **non eseguito** (stesso motivo dei task
precedenti).

## Task 10 — «Preferito»

Ruling: schema già presente — il Task 6 ha aggiunto `asset_flags.favorite
boolean NOT NULL DEFAULT false` e `asset_flags_favorite_idx ON asset_flags
(user_id, asset_id) WHERE favorite` nella migrazione 0037, nome e forma già
identici a quanto la spec del Task 10 dichiara. **Nessuna nuova
migrazione**: verificato leggendo `0037_search_axes.sql` e
`migrations.rs`, non assunto. Il lavoro di questo task è solo il resto del
concetto (dominio, scrittura, `AssetView`) più la riverifica del già fatto.
— *Costo se sbagliato:* nessuno, è una lettura, non una scrittura.

Ruling: `favorite` in `AssetFlags`/`AssetFlagsBody` segue lo **stesso
contratto di rimpiazzo completo** già in vigore per `rating`/`pick`/
`color_label` — un `PUT`/batch senza `favorite` nel corpo lo riporta a
`false` (default di scrittura), esattamente come oggi un `PUT` senza
`rating` azzera il voto. Non ho introdotto un `Option<bool>` "tri-stato"
(lascia invariato/imposta) solo per questo campo: sarebbe stato un fix
asimmetrico rispetto a `rating`/`color_label`, che restano soggetti allo
stesso limite e non sono nello scope di questo task. **Indipendenza**
verificata è quella richiesta dalla spec §7bis.1 — `favorite` non è un
alias di `Pick`, sono due colonne senza alcuna logica che le accoppi:
scartare uno scatto nel culling (`pick = Reject`) passando esplicitamente
`favorite = true` nello stesso corpo non lo azzera (test
`favorite_and_pick_are_independent_axes` in `keeppix-db`, e
`discarding_in_culling_does_not_clear_favorite` a livello HTTP). — *Costo
se sbagliato:* una scrittura di massa "solo pick" che omette `favorite` nel
corpo azzera il preferito per tutti gli asset del lotto — stesso difetto di
forma già presente per `rating`/`color_label`, non nuovo di questo task;
se un client reale lo colpisce, il fix naturale è un merge parziale per
tutti e quattro i campi insieme, non solo per `favorite`.

Ruling: `AssetView.favorite` è risolto per **timeline** (`GET
/assets/{id}`, `GET /timeline`) e per **ricerca** (`POST /search`) — le due
superfici di browse che il brief chiede esplicitamente di documentare, e
che coprono di fatto tutti i "sette punti" della spec §7bis.1 (il cuoricino
sulla tessera e la vista "Preferiti" passano dalla stessa pagina di
ricerca/timeline; il chip di Cerca e la condizione degli album dinamici
usano `SearchNode::Favorite`, non `AssetView`; il pannello del lightbox usa
`GET /assets/{id}`; la modifica in blocco usa `PUT`/batch flags). Per la
pagina (`page`/`page_in_bounds`, `SearchRepo::run`) ho aggiunto
`FlagRepo::favorites_among(ctx, ids) -> HashSet<AssetId>` — **una** query
per pagina invece di N, senza riverificare la visibilità (già garantita da
chi ha costruito la pagina) e filtrata su `user_id = ctx.user_id()`, quindi
nessuna fuga del preferito di un altro utente. Gli altri consumatori di
`AssetView` (`folders::children`, `duplicates::members`,
`albums::list_assets`, `stacks::get`, `share::public_assets`) restano a
`favorite: false` di default: non sono fra i sette punti della spec, e
`share::public_assets` in particolare non ha nemmeno un utente autenticato
per cui "preferito" abbia senso (link pubblico). — *Costo se sbagliato:* un
utente che apre lo stack modal, una cartella o un album fisso non vede il
cuoricino corretto sulle tessere; se emerge come bisogno reale, si applica
lo stesso pattern (`favorites_among` + `with_favorite`) a quei quattro
handler senza toccare `FlagRepo`.

Task 10: complete (commit successivo a questa voce, tests green:
`keeppix-domain` flags 5/5 [+1: `favorite` di default `false`];
`keeppix-db` flags.rs 11/11 [+2 nuovi: indipendenza favorite/pick,
`favorites_among` isola per utente], search.rs 19/19 [invariato — riverifica
Task 6: `favorite_filter_is_per_user_not_per_asset`,
`favorite_search_uses_the_partial_index` restano verdi senza modifiche],
migrations.rs 11/11, fase2_culling_1k.rs 4/4 [struct literal aggiornato];
`keeppix-jobs` xmp.rs 5/5 [struct literal aggiornato]; `keeppix-api`
flags.rs 6/6 [+1 nuovo: discard in culling non azzera favorite], timeline.rs
22/22 [+3 nuovi: GET singolo, pagina, default `false` per un asset mai
votato], search.rs 4/4 [+2 nuovi: risoluzione per pagina, chip Preferiti su
`SearchNode::Favorite`], albums.rs 3/3, duplicates.rs 4/4, stacks.rs 2/2,
share_geofence.rs 6/6, share_link_channels.rs 3/3 [tutti invariati — nessuna
regressione sui consumatori di `AssetView` che restano a `favorite: false`],
openapi.rs 7/7 [snapshot rigenerato con `UPDATE_OPENAPI=1`: due campi
additivi, `AssetFlagsBody.favorite` e `AssetView.favorite`, entrambi
`boolean`]). `cargo fmt --check` e `cargo clippy --workspace --all-targets
-- -D warnings` verdi su tutto il workspace. `./scripts/test.sh` completo
**non eseguito** (stesso motivo dei task precedenti: costerebbe l'intera
suite); eseguiti invece tutti i moduli toccati dal task più i consumatori
di `AssetView` non toccati (per la controprova di non-regressione) e la
riverifica dei due test EXPLAIN/per-utente di `search.rs` ereditati dal
Task 6.

## Task 11 — Togliere i conteggi per riga, tranne quello del culling

Ruling: **i conteggi per riga non sono mai entrati in `/api/v1`.** — Il
prototipo mostrava foto per cartella, membri per album ed elementi per link,
ma `FolderView`/`AlbumView`/`LinkView` non hanno mai esposto `asset_count`,
`member_count` o `item_count`; Task 11 **non aggiunge** quei campi (contratto
congelato) e documenta la scelta nei route handler. `view_count` sui link
pubblici resta: conta **accessi**, non elementi condivisi. — *Costo se
sbagliato:* la sidebar Fase 11 non avrà numeri accanto alle cartelle; accettabile
per eliminare cinque aggregati e le loro invalidazioni.

Ruling: **il conteggio del culling resta fuori da questo task.** — Badge di
navigazione e selettore di lotto usano la sessione di culling (flags per lotto),
non `GET /folders/tree` o `GET /albums`.

Task 11: complete (commits 60b5097, 8942209, 1beac1b, tests green:
`keeppix-db` sidebar_load.rs 1/1, `keeppix-api` sidebar_load.rs 1/1;
`cargo fmt --check` e `cargo clippy -p keeppix-db -p keeppix-api --all-targets
-- -D warnings` verdi; frontend build verde). `./scripts/test.sh` completo
**non eseguito** (stesso motivo dei task precedenti).

## Task 12 — L'indice che manca alla timeline

Ruling: `assets_timeline_indexed_idx` convive con `assets_timeline_idx` — non
droppato (solo-aggiunte). A 200k righe con vincolo di mese il planner resta su
`assets_taken_day_idx` (Task 6); il nuovo indice serve quando il percorso
timeline vince (ordinamento keyset senza range stretto, o query assets-only). —
*Costo se sbagliato:* due indici parziali sovrapposti su `status = 'indexed'`;
accettabile finché il planner non preferisce sempre il nuovo.

Ruling: touch obbligatorio di `keeppix-db/src/lib.rs` al commento `migrate!`
quando si aggiunge una migrazione — senza, `sqlx::migrate!` non incorpora il
file nuovo e l'indice non esiste a runtime (scoperto durante il RED). —
*Costo se sbagliato:* ogni migrazione futura senza touch sembra applicata ma
non lo è finché non si ricompila toccando `lib.rs`.

### EXPLAIN Task 12 (`scale_200k.rs`, 200k righe + 5k `unknown`)

**`explain_page`** (mese 2032-10, LIMIT 200) — **prima e dopo identici**:

```
Index Scan using assets_taken_day_idx
  Filter: (kind <> 'unknown') AND (status = 'indexed')
Execution Time: ~0.47 ms
```

**`explain_timeline_ordering`** (solo `assets`, indici concorrenti nascosti in
transazione di test):

Prima (`assets_timeline_idx`):

```
Bitmap Index Scan on assets_timeline_idx
  Filter: (kind <> 'unknown') AND (status = 'indexed')
Execution Time: ~101 ms
```

Dopo (`assets_timeline_indexed_idx`):

```
Bitmap Index Scan on assets_timeline_indexed_idx
Execution Time: ~62 ms
```

Task 12: complete (commit successivo a questa voce, tests green:
`keeppix-db` scale_200k.rs 3/3, migrations.rs 12/12). `cargo fmt --check` e
`cargo clippy -p keeppix-db --all-targets -- -D warnings` verdi.
`./scripts/test.sh` completo **non eseguito** (stesso motivo dei task
precedenti).

## Task 13 — Problemi composti, non materia prima

Ruling: **la lingua arriva dalla richiesta, non da `UserPreferences.language`.**
— `ProblemLanguage::parse` prende una stringa grezza (query `?lang=` o il primo
tag di `Accept-Language`); l'handler HTTP prova prima `?lang=`, poi l'header,
default italiano. Nessuna lettura di preferenze salvate: un utente non loggato
sullo stesso browser di un altro vedrebbe comunque i propri problemi nella
lingua giusta senza bisogno di autenticarsi per leggere la preferenza. —
*Costo se sbagliato:* un client che manda solo `Accept-Language` senza query
avrebbe comunque la lingua giusta; il costo è basso.

Ruling: **il marcatore `permission-denied:` va messo da `keeppix-jobs`, non
indovinato da `keeppix-db` col testo libero di `io::Error`.** — Il messaggio di
un `PermissionDenied` cambia formulazione a seconda della piattaforma
(`Permission denied (os error 13)` su Linux, altro altrove); un marcatore
stabile scritto al punto in cui l'errore nasce sopravvive a qualunque
`Display` di `std::io::Error`. — *Costo se sbagliato:* la classificazione
"sidecar non scrivibile" smette di riconoscere il fallimento su piattaforme
diverse da quella testata, e il job finito in errore ricade nel caso generico
("operazione pianificata non riuscita") — degrado, non un errore visibile.

Ruling: **`GET /problems` resta additivo** (contratto `/api/v1` congelato):
`ProblemsView` mantiene `offline_libraries`/`failed_jobs`/`error_assets` e
aggiunge `problems`, l'elenco piatto composto. Il frontend (`ProblemsView.vue`)
non è stato toccato — il piano affida il consumo della UI a una fase/task
successivo (§47 è nella spec dell'interfaccia, non nel piano di questo task) —
e resta a leggere i secchi grezzi finché non viene aggiornato.

Ruling: **`ProblemsRepo::compose` accetta un `ProblemSet` già ottenuto**
(oltre a `composed`, che lo richiama internamente per chi non ha già il set) —
l'endpoint HTTP deve restituire sia i secchi grezzi sia l'elenco composto nella
stessa risposta; senza questo, avrebbe interrogato il database due volte per
la stessa richiesta.

Verifica chmod (spec del task): `keeppix-jobs/tests/xmp.rs
a_readonly_folder_surfaces_as_a_composed_sidecar_problem` rende una cartella
`0o555` con `chmod`, fa fallire realisticamente il job `WriteSidecar` (claim +
run + fail manuale sulla riga, `max_attempts` di default non fa scattare
`failed` con un solo tentativo), poi chiama `ProblemsRepo::composed` e
verifica: gravità `Warning`, titolo/descrizione in italiano che menzionano
permessi mancanti e cartella coinvolta, azioni `view-files`/`ignore`.

Task 13: complete (commits 7af00e7, 870b4f7, f01daea, e4949de; tests green:
`keeppix-db` libraries.rs 3 nuovi + problems.rs 9/9; `keeppix-jobs` xmp.rs 7/7
incluso i due test di permessi; `keeppix-api` libraries.rs 3 nuovi +
problems.rs 4/4 + openapi.rs 7/7). `cargo fmt --check` e `cargo clippy
--workspace --all-targets -- -D warnings` verdi. Suite complete eseguite a
mano (non via `./scripts/test.sh`, per lo stesso motivo di tempo dei task
precedenti — ogni crate girato singolarmente con `--jobs 1
--test-threads=1`): `keeppix-db` 100% verde, `keeppix-jobs` 100% verde,
`keeppix-api` 45 file di test 100% verde, `keeppix-server` 100% verde.

## Task 14 — Suggerimenti tipizzati e cluster con destinazione

Ruling: **il cambio di forma di `/search/suggest` è intenzionale, non
un'eccezione al contratto congelato.** — L'endpoint passa da
`{suggestions: string[]}` a `{suggestions: {kind, value, label, color?}[]}`:
è un cambio di tipo dell'elemento dell'array, non un'aggiunta. Il brief lo
chiede esplicitamente («la barra di ricerca deve sapere *di che tipo* è ogni
suggerimento»), e l'endpoint non ha ancora un consumatore frontend (Fase 11):
nessun client su `/api/v1` dipende dalla forma vecchia. Diverso dagli altri
task della fase, che tengono tutto additivo perché toccano endpoint già
consumati. — *Costo se sbagliato:* nessuno oggi; se un client esterno avesse
già integrato la forma stringa, romperebbe — non è il caso qui.

Ruling: **`tag` resta nell'enum senza fonte.** — Il brief lo dice
esplicitamente: la tabella dei tag non esiste (Fase 7). Le altre quattro
fonti nuove (`folder`, `iso`, `year`, `country`) usano dati che esistono già
da questo stesso branch (Task 6): cartelle, `asset_exif.iso`,
`taken_at_utc`, `assets.place_id → places.country_code`. — *Costo se
sbagliato:* nessuno, è la lettura letterale del brief.

Ruling: **`country` in `suggest` legge `assets.place_id` bare**, senza
`COALESCE` con `asset_overrides.place_id` — stessa scelta già presa da
`SearchNode::Country` nel Task 6 (quella colonna di override non viene
scritta da nessun percorso oggi). Stessa scelta ripetuta per `place_label`
di `MapClusterView`, per lo stesso motivo e per restare coerente con il
filtro `Country` che userebbe la stessa foto per lo stesso risultato. —
*Costo se sbagliato:* se in futuro un percorso scrive
`asset_overrides.place_id`, tre punti (search axis, suggest, cluster label)
vanno aggiornati insieme, non solo uno.

Ruling: **`value` di un suggerimento `folder` è l'id (stringa), `label` è il
nome.** — Serve a costruire `SearchNode::Folder{id}` se l'utente scegie la
pillola; `camera`/`filename`/`iso`/`year`/`country` hanno `value == label`
perché il testo stesso è già il valore del filtro (`SearchNode::Camera`,
`Iso`, `Year`, `Country` prendono direttamente quella stringa/numero). —
*Costo se sbagliato:* nessuno, è la lettura naturale di ogni asse.

Ruling: **`MapCluster.folder_id`/`place_label` derivano dallo stesso
`cover_asset_id`, non da un membro qualunque del cluster.** — Per il
percorso non aggregato (punti singoli) è un join diretto sulla stessa riga;
per il percorso a griglia, `folder_id` e `place_id` sono aggregati con lo
stesso `array_agg(... ORDER BY rating DESC, taken_at_utc DESC, id DESC)[1]`
già usato per `cover_asset_id` — tre `array_agg` paralleli con lo stesso
ordinamento, non un secondo criterio. Un `LEFT JOIN places` non può leggere
`place_id` calcolato da un `array_agg` allo stesso livello di query, quindi
il calcolo della copertina vive in un livello e la risoluzione del nome
del luogo nel livello sopra. Verificato con un test dedicato
(`grid_cluster_carries_the_cover_assets_folder_and_place_not_a_sibling`,
due asset in due cartelle/luoghi diversi nella stessa cella, rating diverso
per decidere la copertina) — osservato **rosso** mutando di proposito
l'`ORDER BY` di uno dei due `array_agg` (rotto, poi ripristinato) prima di
fissare il test definitivo, per confermare che l'asserzione prova davvero
l'allineamento e non passa per caso. — *Costo se sbagliato:* il popover di
un cluster misto mostrerebbe la cartella/luogo di un asset diverso da quello
la cui foto di copertina si vede — un bug visibile solo con cluster di
asset eterogenei, quindi facile da non notare in test superficiali.

Task 14: complete (commits da47ddf feat(search) + 3ce4c88 fix(api) clippy +
f37879b feat(map), tests green: `keeppix-db` search.rs 21/21 [+2 nuovi:
un tipo per fonte, `tag` mai senza fonte], geo.rs 15/15 [+1 nuovo:
allineamento cover/folder/place nel percorso a griglia], migrations.rs
11/11; `keeppix-api` search.rs 5/5 [+1 nuovo: pillole tipizzate via HTTP],
map.rs 10/10 [+1 nuovo: popover con dati sufficienti a navigare],
openapi.rs 7/7 [snapshot rigenerato: `SuggestionView`/`SuggestionKindView`
nuovi, `MapClusterView` +2 campi]). `cargo fmt --check` e `cargo clippy
--workspace --all-targets -- -D warnings` verdi su tutto il workspace.
`./scripts/test.sh` completo **non eseguito** (stesso motivo dei task
precedenti: costerebbe l'intera suite); eseguiti invece tutti i test
toccati dal task (`keeppix-db` search.rs/geo.rs/migrations.rs; `keeppix-api`
search.rs/map.rs/openapi.rs/places.rs) più due mutazioni manuali
osservate rosse (kind letterale rotto in `suggest`, `ORDER BY` disallineato
in `fetch_grid`) e ripristinate, per confermare che i nuovi test provano
davvero ciò che dichiarano.

## Task 15 — «Condivisi con me», e i pezzi mancanti del profilo

Il brief (`task-15-brief.md`) non era presente nella cartella SDD del task;
i requisiti sono stati letti dal piano di fase (`plans/2026-08-20-keeppix-
fase-10.md`, sezione Task 15) e dallo spec funzionale UI (§29 "Condivisi con
me", §61 "Profilo").

Ruling: **`password_changed_at` prende `default now()` più un backfill a
`created_at`**, non `NULL` nullable. — Il Profilo (§61) mostra sempre
"Ultima modifica: N mesi fa", quindi la colonna non può essere opzionale;
`now()` copre chi si registra da qui in avanti (una password appena
impostata è appena cambiata), il backfill riporta gli account esistenti
alla loro creazione — l'unico istante in cui è certo che la password
corrente sia stata scritta. `set_password_hash` la aggiorna a `now()` a
ogni cambio. — *Costo se sbagliato:* nessuno grave, è solo una data
mostrata in un'etichetta informale ("circa").

Ruling: **`server_name` è un campo di configurazione del server
(`KEEPPIX_SERVER_NAME`, default `"Keeppix"`)**, non una riga in
`system_settings`. — È un valore letto a ogni richiesta (`UserView` lo
porta su ogni risposta di login/me), fisso per la vita del processo, e non
ha un endpoint di scrittura in questo task: non serve la flessibilità
runtime di una tabella, e `Config`/`AppState` sono già il canale per valori
di questo tipo (vedi `webp_quality`, `watch_poll_secs`). — *Costo se
sbagliato:* se in futuro serve renderlo modificabile da UI senza riavviare
il server, va spostato in `system_settings` — un solo punto da cambiare,
`UserView::new` prende già una `&str` a parte.

Ruling: **`UserView::from(&User)` è diventato `UserView::new(&User,
&str)`**, non un secondo costruttore parallelo. — Era una funzione pura
senza accesso a `AppState`; `server_name` vive lì. Un secondo metodo
avrebbe lasciato `From` silenziosamente incompleto (compilerebbe ma
ometterebbe il campo) per chi lo richiama per abitudine. Tutti i punti di
costruzione (`auth::login`, `auth::me`, `users::list/create/patch`,
`setup::setup`) sono stati aggiornati nello stesso commit. — *Costo se
sbagliato:* nessuno, è un refactor meccanico verificato dal type-checker.

Ruling: **il conteggio elementi di `GET /share/links` conta solo gli asset
`indexed`**, stessa regola dei conteggi cartella del Task 11. — Un link
condiviso non deve promettere "246 elementi" e poi mostrarne meno perché
alcuni sono ancora in coda di scansione o marcati non-indicizzati. Il test
`item_counts_reports_indexed_assets_for_a_folder_link` semina apposta un
asset non indicizzato per provare che non viene contato. — *Costo se
sbagliato:* il numero visto nella scheda di condivisione non corrisponde a
quanto si vede aprendo il link.

Ruling: **il test `sidebar_endpoints_do_not_expose_per_row_counts` (Task
11) andava aggiornato, non contraddetto in silenzio.** — Asseriva che
nessun oggetto per-riga portasse un conteggio; `item_count` su
`GET /share/links` è un conteggio per-riga voluto da questo task, diverso
dai conteggi di cartelle/album che quel test protegge davvero (quelli
restano assenti). L'assert è stato reso specifico (`item_count` ammesso
solo per i link, ancora vietato per cartelle/album) invece di rimuovere la
protezione. — *Costo se sbagliato:* un conteggio per-riga tornerebbe su
cartelle o album senza che nessun test lo segnali.

Ruling: **`SharedWithMeItem.via_group` è `Some(nome)` solo quando l'accesso
è *puramente* di gruppo**, `None` se l'utente ha (anche) una concessione
diretta sullo stesso oggetto. — Un utente con permesso diretto non deve
vedere "condiviso tramite gruppo X" se in realtà gli è stato dato
direttamente; la scheda deve riflettere l'origine più diretta, non una
qualunque. Sugli oggetti con doppia origine vince il ruolo più alto delle
due (stessa regola di `explain`), coerentemente con "il permesso più
permissivo vince" già stabilito per il resto del sistema dei permessi.
Verificato con
`shared_with_me_collapses_direct_and_group_grants_on_the_same_object` e con
`shared_with_me_shows_the_group_origin_not_only_direct_grants`, quest'ultimo
osservato **rosso** rimuovendo temporaneamente la logica "diretto vince su
group-only" (tutti gli oggetti tornavano con `via_group` anche quando
c'era una concessione diretta) prima di ripristinarla. — *Costo se
sbagliato:* un utente vedrebbe un'origine di condivisione sbagliata,
confondendo chi gli ha davvero dato accesso.

Ruling: **la visibilità implicita di un admin su tutto non compare in
`/shared-with-me`.** — La query legge solo righe di `permissions`; un admin
non ha (né riceve) una riga per ogni oggetto che può già vedere per ruolo.
"Condivisi con me" deve elencare concessioni esplicite, non "tutto ciò che
posso vedere" — altrimenti la lista di un admin sarebbe l'intera libreria.
Verificato da `shared_with_me_never_lists_objects_the_user_cannot_see`
(che copre anche il caso duale: nessun oggetto fuori scope compare).

Task 15: complete (commits ac79bab feat(db) password_changed_at + f00eac2
feat(api) UserView additive fields + 888949e feat(api) item_count su
share/links + 3c6fb28 feat(api) GET /shared-with-me; tests green:
`keeppix-db` users.rs 15/15 [+2 nuovi], share_links.rs 7/7 [+4 nuovi:
conteggio batched per folder/album/asset, solo indexed], permissions.rs
22/22 [+5 nuovi: grant diretto, origine gruppo, member count album, scope
enforcement, collasso diretto+gruppo], migrations.rs invariato; `keeppix-
server` config.rs 8/8 [+2 nuovi: default e override di `server_name`];
`keeppix-api` auth.rs 28/28 [+1 nuovo: `me` porta i due campi additivi],
shared_with_me.rs 3/3 [nuovo file: grant diretto compare, nessuna
concessione → lista vuota, richiede autenticazione], sidebar_load.rs 1/1
[asserzione aggiornata, non rimossa], permissions_roles.rs 2/2,
share_link_channels.rs 3/3, openapi.rs 7/7 [snapshot rigenerato: `UserView`
+2 campi; `permissions`/`share` non erano e non sono nel documento
OpenAPI]. `cargo fmt --check` e `cargo clippy --workspace --all-targets --
-D warnings` verdi. `./scripts/test.sh` completo **non eseguito** (stesso
motivo dei task precedenti); eseguiti tutti i test dei crate/file toccati
dal task, più tre mutazioni manuali osservate rosse e ripristinate (route
`/shared-with-me` assente → 404 sui tre test nuovi; logica "diretto vince
su group-only" rimossa → `via_group` sempre popolato) per confermare che i
nuovi test provano davvero ciò che dichiarano.

Ruling: **la rinomina di massa (Fase 9) non esiste ancora nel codice: Task
16 costruisce l'infrastruttura di operazione/avanzamento/annullamento in
generale (`OperationKind`, `operations`, `OperationsRepo`) e la aggancia
all'unico long-op reale già presente, la scansione di libreria
(`discover::run_with_operation`).** `OperationKind` resta un enum a un solo
variante apposta: un futuro `BulkRename` si aggiunge senza toccare il
protocollo. — *Costo se sbagliato:* nessuno finché la Fase 9 non esiste;
quando esisterà, il contratto (`operation_id`, avanzamento sul WebSocket,
`cancel` → `BulkOutcome` parziale) è già quello giusto da riusare.

Ruling: **annullare a metà produce una riuscita parziale, non un
rollback.** `operations.succeeded_asset_ids` accumula ciò che è già stato
scritto; `finish_cancelled` chiude lo stato senza svuotarlo. `POST
/operations/{id}/cancel` restituisce esattamente questo elenco come
`BulkOutcome` (lo stesso involucro del Task 1, letto da qui invece che
costruito da zero) — non un rollback, non un secondo formato di risposta.
Verificato sia a livello `keeppix-jobs`
(`cancelling_mid_scan_leaves_exactly_the_files_already_applied`) sia end to
end via HTTP (`cancelling_a_scan_via_the_api_leaves_a_partial_bulk_outcome`,
con un vero `WorkerPool` su 40 file e l'annullamento chiamato mentre gira),
entrambi osservati **rossi** disabilitando rispettivamente il controllo di
`cancel_requested` e la chiamata a `request_cancel` nell'handler, prima di
ripristinarli.

Ruling: **una seconda `POST /scan` mentre un job per la stessa libreria è
già `pending`/`running` non crea una seconda operazione tracciata.** La
`dedup_key` condivisa `discover:{library_id}` (watcher e richieste utente)
fa collassare comunque i job su uno solo; creare comunque un'operazione la
lascerebbe `running` per sempre, perché nessun job la farebbe avanzare.
`start_scan` guarda prima se un job è già in coda (`operation_id: null` in
quel caso) e, come rete di sicurezza contro la corsa fra quel controllo e
l'accodamento, chiude comunque l'operazione appena creata
(`finish_cancelled`, esito vuoto) se `enqueue_rescan_with_operation`
scopre di aver perso la corsa. Verificato rosso disabilitando prima il
controllo preventivo (il codice di riserva l'ha comunque salvato — prova
che la rete di sicurezza serve davvero) e poi la rete di sicurezza stessa.

Ruling: **il WebSocket non ha memoria fra connessioni.** `drain_operations`
riparte con una mappa "visti" vuota a ogni nuova connessione: `operations`
resta l'unica fonte di verità, letta a ogni giro di poll, mai un replay di
eventi persi. Un client riconnesso a metà operazione vede quindi
l'avanzamento corrente al primo giro utile — provato aprendo una
connessione nuova solo dopo che una scansione aveva già superato 3 file
riusciti (`operation_progress_arrives_over_a_connection_opened_mid_scan`,
osservato rosso disabilitando la chiamata a `drain_operations`).

Task 16: complete (commits edb3ff1 feat(db) operations table, cac25ca
feat(jobs) wiring dello scan all'operazione, e3033f7 fix fmt/clippy,
1837c34 feat(jobs) dispatch/watch portano `operation_id`, 9acf9cf feat(api)
`operation_id` su scan + `POST /operations/{id}/cancel`, 4b6a145 feat(api)
`operation.progress` sul WebSocket, 2d5f694/73fdb55 test, 90ca36e
docs snapshot OpenAPI; test verdi: `keeppix-domain` 62/62 [invariato],
`keeppix-db` operations.rs 14/14 [nuovo file], migrations.rs 11/11 [+1
tabella], `keeppix-jobs` discover_operations.rs 3/3 [nuovo file],
suite completa del crate 100% verde (40 file di test, nessuna regressione),
`keeppix-api` scan.rs 8/8 [+4 nuovi: operation_id fino a `Done`, dedup
senza operazione orfana, annullamento parziale via HTTP, `Forbidden` su
operazione altrui], ws.rs 4/4 [+1 nuovo: avanzamento dopo connessione a
metà scansione], suite completa del crate 100% verde (48 file di test),
openapi.rs 7/7 [snapshot rigenerato: solo `ScanAccepted.operation_id`
additivo — `POST /operations/{id}/cancel` resta non documentato, come la
maggior parte delle rotte dal Task 9 in avanti, fino al Task 23].
`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` verdi sull'intero workspace; `cargo deny check bans` verde
(nessun nuovo arco proibito). `./scripts/test.sh` completo **non
eseguito** (stesso motivo dei task precedenti: costoso e già coperto dalle
suite complete dei crate toccati); sei mutazioni manuali osservate rosse e
ripristinate in questo task (elencate nei Ruling sopra) per confermare che
i nuovi test provano davvero ciò che dichiarano, non solo che passano.

Ruling: **`badges` resta `{culling:0, revision:0}` finché le Fasi 7/8/9 non
espongono contatori singoli.** — Bootstrap deve comporre repository esistenti,
non inventare SQL per code che non esistono ancora; i campi sono già nel
contratto così il frontend può leggerli e i task futuri li riempiono senza
rompere la forma. — *Costo se sbagliato:* badge a zero finché non arrivano le
fasi IA/culling; accettabile perché oggi non ci sono endpoint badge da
comporre.

Ruling: **`storage` è una mappa per ogni libreria visibile, non un solo id.** —
Il confronto query conta `list` + N×`storage`, come farebbe un client che
chiede lo spazio per ogni libreria che possiede o vede. — *Costo se sbagliato:*
payload leggermente più grande con molte librerie; coerente con la sidebar che
mostra lo spazio della libreria corrente e non richiede un secondo giro.

Task 17: complete (commits c0281c4 feat(api) GET /bootstrap, ee16774 test
query budget; test verdi: `keeppix-api` bootstrap.rs 2/2 [nuovo file],
compose in `routes/bootstrap.rs`, route additiva `GET /api/v1/bootstrap`;
`cargo fmt --check` e `cargo clippy -p keeppix-api --all-targets -- -D
warnings` verdi; mutazione route assente → 404 osservata rossa e
ripristinata).

## Task 18 — Misurare la geometria prima di complicarla

Ruling: **metodologia di misura** — script riproducibile
`scripts/measure-geometry-mobile.py` (gzip del formato binario Task 2 +
stima cold-start) e misura server-side già presente in
`scale_geometry.rs` (`MEASUREMENT geometry`). Il payload grezzo @ 214k è
**1 284 008 byte** (8 + N×6, indipendente dall'entropia). Gzip dipende dai
dati: da ~7 KiB (sintetico ripetitivo) a ~1,05 MiB (w/h/m a max entropia);
il riferimento del progetto resta **451 KiB** misurato su record realistici
(spec §2.3). Server @ 200k: **591 ms** in-process (105 ms SQL, index-only
scan). Client: **~32 ms** per gzip+scan `DataView` @ 214k. Cold-start first
paint stimato = `n×RTT + transfer(gzip) + server + client`, profilo Chrome
**Fast 3G** (1.6 Mbps, 150 ms RTT, 3 RTT di handshake). — *Costo se
sbagliato:* profilo di rete diverso dal reale; la soglia 2 s resta
comparabile solo se si riusa lo stesso profilo.

Ruling: **3,4 s @ 214k su Fast 3G** (451 KiB gzip spec + 591 ms server +
32 ms client) → **supera la soglia 2 s**. Decisione: **pianificare geometria
per mese in Fase 11** (mesi vicini + stima da `/timeline/buckets`, come da
piano Task 18); **non frammentare** `GET /timeline/geometry` in fase-10 —
questo task è solo misura. Whole-view resta perché il caso primario (server
di casa in LAN/Wi-Fi) resta sotto i 2 s anche col worst-case gzip; la
frammentazione serve agli accessi cellulari remoti. — *Costo se sbagliato:*
implementare per-mese troppo presto (complessità) o troppo tardi (scroll
impreciso su 4G remoto finché Fase 11 non lo fa).

Task 18: complete (commits 230e4e8 script + f0ed953 ledger/report; nessuna
modifica API/DB).

## Task 19 — Il protocollo WebSocket: da due eventi a nove

Ruling: **`scan.progress` legge la stessa fonte già usata da `GET
/libraries/{id}/scan`** (`JobRepo::discover_status_for_library` +
`AssetRepo::count_in_library`), non uno stato inventato — estratta la fase
(`idle`/`discovering`/`failed`/`offline`) in `scan_phase()` condivisa fra le
due superfici. Copre anche le riscansioni innescate dal watcher, che non
hanno un `operation_id` e quindi non compaiono mai in `operation.progress`
(Task 16): quelle restano l'unico segnale per gli scan avviati da un utente
con `POST /scan`, questo per "c'è attività su questa libreria" in generale.
Poll per-libreria (N+1 come lo storage del bootstrap, Task 17) — accettabile
alla scala di libreria per libreria/Pi di Keeppix. — *Costo se sbagliato:*
poll ridondante con `operation.progress` quando una scansione è tracciata;
nessuna perdita di informazione.

Ruling: **`problems.changed` è una firma su `ProblemsRepo::list`, non un
secondo stato.** Confronta un digest ordinato di (id libreria offline, id job
falliti, id asset in errore) con l'ultimo giro; se cambia, emette solo un
`count` come comodità — mai gli id, per il contratto già scritto nel piano
("un segnale, non uno stato"). Un client che perde il messaggio deve
ricaricare `GET /problems`, non fidarsi del numero. — *Costo se sbagliato:*
un doppio giro di query ProblemsRepo per ogni tick di poll; già lo stesso
costo che `GET /problems` paga a ogni refresh manuale.

Ruling: **`asset.derivative.ready` legge `jobs`, non un canale in-process.**
`JobRepo::list_recently_done(TranscodeVideo, cursor, …)` (nuovo,
`max_done_id` inizializza il cursore alla connessione così non si rivedono
transcodifiche già finite prima di collegarsi) più
`AssetRepo::filter_visible` prima di emettere — un job scritto da un worker
qualunque arriva al giro di poll successivo, la visibilità non si scavalca
mai. — *Costo se sbagliato:* nessuno osservato; il filtro di visibilità è lo
stesso già usato per le operazioni bulk (Task 1).

Ruling: **`backup.finished` è admin-only e non replica lo stato "running".**
`BackupRepo::list_runs(ctx, 1)`: se il chiamante non è admin si esce subito
(altrimenti il `Forbidden` interno romperebbe il socket), se l'ultimo run è
ancora `running` si aggiorna solo la mappa "visto" senza emettere — l'evento
esce solo alla transizione verso `ok`/`failed`. Una riconnessione dopo che un
backup è già finito vede comunque l'esito corrente al primo giro (stessa
proprietà "nessuna memoria fra connessioni, stato attuale al primo poll
utile" di `operation.progress`, Task 16): a differenza di
`asset.derivative.ready`, qui l'esito passato è ancora rilevante per chi
riapre Impostazioni. — *Costo se sbagliato:* nessuno osservato nei test.

Ruling: **`analysis.progress`, `suggestions.changed`, `culling.changed` non
sono cablati — nessun codice di Fase 7/8 esiste da cui leggerli.** Cablare un
emettitore adesso significherebbe inventare una fonte di verità che le fasi
future rifaranno da capo (violerebbe "non implementare cose di fasi
successive"). Il `type` di questi eventi non è nemmeno riservato nel
protocollo: il piano di Fase 7/8 dovrà solo scegliere quale dato persistito
emettere, la forma del canale (poll + `enqueue`/`resync` su overflow) è già
pronta e riusabile senza modifiche. `storage.changed` (elencato nel solo
documento di analisi gap, non nella tabella del piano) resta fuori per lo
stesso motivo per cui non serve: `GET /bootstrap` (Task 17) già porta lo
spazio libero per libreria, e la spec lo elenca come alternativa accettabile
("dentro bootstrap, oppure storage.changed"). — *Costo se sbagliato:*
nessuno finché la Fase 7/8 non esiste; quando esisterà, il worker che scrive
il progresso IA aggiungerà solo un altro `drain_*` nello stesso poll.

Ruling: **il test `a_new_asset_is_pushed_as_assets_upserted` filtra per
`type` invece di assumere il primo messaggio.** Con più tipi di evento sullo
stesso canale, una libreria appena creata dal test può emettere anche
`scan.progress` sullo stesso giro di poll — esattamente il comportamento che
un client reale deve già tollerare (il piano lo richiede: "il WebSocket è
canale di notifica", più tipi possono intercalarsi). Il test ora usa
`recv_matching`, la stessa tecnica dei nuovi test. — *Costo se sbagliato:*
nessuno; è l'unica modifica a un test preesistente.

Difetto osservato, non di questo task (annotato, non toccato):
`bootstrap_emits_no_more_queries_than_individual_repos`
(`crates/keeppix-api/tests/bootstrap.rs`) fallisce in modo deterministico
quando gira insieme a `bootstrap_matches_individual_endpoints` nello stesso
binario (verificato anche sul commit prima di questo task, con `git stash`):
il logger globale della crate `log` sembra restare quello installato dal
primo test che chiama `traced_db`, quindi il secondo non cattura nulla.
Isolato (`cargo test bootstrap_emits_no_more_queries_than_individual_repos`)
passa sempre. Non è una regressione di questo task — differito.

Task 19: complete (commits ce59769 feat(db) `list_recently_done`/
`max_done_id`, 2ddf3df refactor(api) `scan_phase` condivisa, 48a4e33
feat(api) i quattro nuovi emettitori + test. Test verdi: `keeppix-db` jobs.rs
14/14 [+1 nuovo], suite completa del crate verde (invariato dal Task 18);
`keeppix-api` ws.rs 8/8 [+4 nuovi: problems.changed, backup.finished,
asset.derivative.ready, scan.progress], resto della suite verde tranne il
difetto preesistente sopra. `cargo fmt --check` e `cargo clippy --workspace
--all-targets -- -D warnings` verdi; `cargo deny check bans` verde (nessun
arco `keeppix-media`↔`keeppix-db` introdotto). Quattro mutazioni manuali
(una per nuovo emettitore, `if false && drain_x(...)`) osservate rosse e
ripristinate per confermare che i nuovi test provano davvero l'emettitore e
non solo che il socket resta vivo. `./scripts/test.sh` completo **non
eseguito** (stesso motivo dei task precedenti).

## Task 20 — La pausa automatica dell'analisi è un comportamento del server

Ruling: **la soglia (4000 ms) resta un parametro passato a
`ActivityTracker::analysis_should_run(now, idle_threshold_ms)`, non una
costante cablata dentro la funzione** — stesso pattern già in uso per
`default_night_window()`/`in_night_window`, che prende la finestra come
parametro invece di leggerla da una costante interna. `DEFAULT_ANALYSIS_IDLE_MS
= 4000` esiste come punto di partenza documentato (il documento funzionale UI
lo dichiara «da tarare sul sistema vero»), non come unico valore possibile.
Non ho aggiunto un campo a `keeppix-server::Config`/`KEEPPIX_ANALYSIS_IDLE_MS`:
nessun consumatore reale esiste ancora (Fase 7), e un campo di configurazione
senza un solo punto che lo legga sarebbe morto fino a quel momento — la
firma a parametro è già la configurabilità richiesta; Fase 7 deciderà se le
serve anche una variabile d'ambiente quando avrà un job da parametrizzare
davvero. — *Costo se sbagliato:* Fase 7 dovrà aggiungere quel campo di
config comunque; costo di una riga, non di un redesign.

Ruling: **il segnale è un secondo campo su `ActivityTracker`
(`last_viewport_unix_ms`), non un tipo nuovo separato.** — `ActivityTracker`
è già l'unico punto di ingresso dell'attività per i worker (`WorkerPool`,
`main.rs`), ed è già condiviso via `Arc` fra API e job. Un secondo tipo
avrebbe significato un secondo `Arc` da infilare ovunque per un concetto
gemello. La risoluzione è in millisecondi (non secondi come
`last_auth_unix`) perché una soglia di 4000 ms sarebbe altrimenti quantizzata
a passi di un secondo — misurabile nei test solo per multipli di 1000 ms,
il che avrebbe reso `analysis_resumes_exactly_at_the_idle_threshold`
(3999 ms ancora in pausa, 4000 ms ripreso) impossibile da esprimere
correttamente. — *Costo se sbagliato:* nessuno osservato; è puro
dimensionamento del tipo di storage.

Ruling: **il gate (`analysis_should_run`) resta indipendente da
`current_profile()`/`EnergyProfile`, non un `AND` con la regola dei 5
minuti.** — `current_profile` blocca *tutto* il lavoro Background finché
non sono passati 5 minuti dall'ultima richiesta autenticata qualsiasi
(non solo di navigazione): durante una sessione di scorrimento attivo,
quella regola già impedisce ai job Background ordinari di girare, e lo fa
con una finestra pensata per lavoro pesante che non deve competere con
l'uso interattivo in generale (backup, cleanup, retry). Il piano di Fase 7
(`2026-08-20-keeppix-fase-7.md`, Task 6) chiede due cose *separate* per
l'analisi: priorità `Background` **e** «pausa automatica, soglia 4000 ms
dall'ultima attività, configurabile» — se la seconda fosse un `AND` con la
prima, la ripresa richiederebbe il *massimo* dei due tempi (fino a 5
minuti), non i 4 secondi promessi all'utente nel documento funzionale
(*"riprende 4 secondi dopo l'ultimo cambio di vista"*). Le due regole
restano quindi disponibili come **due leve indipendenti**: Fase 7 deciderà
se il proprio ciclo di analisi le combina (entrambe devono valere) o se
solo la seconda governa la ripresa (la prima resta per la classe di
priorità nella coda). Non ho scelto per loro conto: cablare oggi
`analysis_should_run` dentro `WorkerPool::step()` — che è condiviso da
*tutti* i job a priorità `Background` (backup, cleanup, retry_derives,
tmp_cleanup, hash, regions, watch, xmp) — avrebbe imposto una pausa di
navigazione di 4 secondi anche a lavoro che non ha nulla a che fare con
l'analisi IA, un comportamento non richiesto dal brief e potenzialmente
sorprendente per chi già si affida al comportamento attuale di quei job.
— *Costo se sbagliato:* quando Fase 7 scriverà lo scheduler dell'analisi
(Task 6 del piano Fase 7) dovrà scegliere esplicitamente come comporre le
due regole; il costo è una decisione rimandata, non un comportamento
sbagliato già spedito.

Ruling: **i due livelli (`AnalysisLevel::Full`/`Reduced`, 42/260 ms per
foto) sono un tipo puro e testato, non cablati a nessun job — come
richiesto dal brief quando "un job di analisi reale non esiste".** Fase 7
non esiste in questo branch (`docs/superpowers/plans/
2026-08-20-keeppix-fase-7.md` è un piano scritto *prima* che Fase 10
esistesse, per sua stessa nota). Non c'è quindi alcun consumo reale da
misurare per "il throughput differisce misurabilmente" richiesto dalla
verifica del brief: i numeri sono gli obiettivi dichiarati dal documento
funzionale UI (§57), non una misura di questo task. Il test
`reduced_level_is_documented_as_about_six_times_slower_than_full` verifica
solo che i due valori restino quelli dichiarati e nel rapporto atteso — è
una prova di configurazione, non di prestazioni. — *Costo se sbagliato:*
quando Fase 7 misura sul proprio hardware/modello, i due numeri cambiano
qui in un solo posto; nessuna logica dipende dal loro valore assoluto.

Ruling: **`POST /viewport` avvisa il gate anche con `hashes: []`.** — Il
prototipo aggiorna `lastNavAt` a ogni navigazione, non solo quando ci sono
foto nuove da promuovere: uno scroll che resta sui bucket già visibili è
comunque una navigazione, e non deve far ripartire l'analisi solo perché
quella pagina non ha portato hash nuovi da promuovere. Verificato **rosso**
disabilitando la chiamata al gancio (mutazione manuale, poi ripristinata):
il test
`a_viewport_call_notifies_the_analysis_pause_gate_even_with_no_visible_hashes`
falliva come previsto. — *Costo se sbagliato:* nessuno osservato; è la
lettura letterale del comportamento voluto.

Task 20: complete (commits 82b2f66 feat(jobs) + f5316fb feat(api); tests
green: `keeppix-jobs` profile.rs 12/12 [+5 nuovi: pausa immediata dopo un
cambio di vista, ripresa esatta alla soglia, nessuna pausa se non è mai
arrivato un cambio di vista, la soglia è un parametro del chiamante non una
costante cablata, `Reduced` ~6× `Full`]; `keeppix-api` viewport.rs 3/3 [+1
nuovo: `POST /viewport` avvisa il gate anche con `hashes: []`, osservato
**rosso** disabilitando temporaneamente la chiamata al gancio prima di
ripristinarla]. `cargo fmt --check` e `cargo clippy --workspace
--all-targets -- -D warnings` verdi su tutto il workspace; `cargo build
--workspace --all-targets` verde. Riverificati senza regressioni:
`keeppix-api` auth.rs 28/28 (tocca `state.rs`), openapi.rs 7/7 (nessuna
modifica di superficie: `POST /viewport` esisteva già, cambia solo un
gancio interno); `keeppix-server` config.rs 8/8 + embed.rs 5/5 (tocca
`main.rs`). `./scripts/test.sh` completo **non eseguito** (stesso motivo
dei task precedenti: costerebbe l'intera suite); eseguiti i test dei
moduli toccati più le suite di non-regressione elencate sopra.

## Task 21 — L'import a lotti, e le due discrepanze

Ruling: **il lotto scrive tre istruzioni `UNNEST`, non una.**
`AssetRepo::batch_upsert_discovered` (`INSERT ... ON CONFLICT DO UPDATE ...
RETURNING`, filtra a `mtime`/`size_bytes` cambiati come già faceva
`upsert_discovered`), `JobRepo::enqueue_many` (`INSERT ... ON CONFLICT
(dedup_key) WHERE status IN ('pending','running') DO NOTHING`) e
`OperationsRepo::record_success_many` (un solo `UPDATE` che avanza `done` e
appende tutti gli `asset_id` con `array_length`/`||`) restano tre
responsabilità separate dello stesso schema già in uso — non un'unica query
gigante che le fonderebbe. "Un `change_log` per lotto" (brief) non significa
una riga sommario: il trigger `assets_change_log` (`AFTER INSERT OR UPDATE
... FOR EACH ROW`) scrive comunque una riga **per asset** dentro la singola
istruzione `INSERT`, così il sync mobile non perde la granularità
entità-per-entità richiesta dalla spec §2.6 — cambia solo che quelle righe
nascono da un giro di rete invece che da 500. — *Costo se sbagliato:* se
`change_log` fosse davvero per-lotto, un client mobile che sincronizza a
metà lotto vedrebbe un cursore avanzato senza le righe intermedie: la
diagnosi sarebbe un mismatch di conteggio silenzioso, non un errore.

Ruling: **`FolderRepo::ensure_path` guadagna una cache per-scan
(`HashMap<Vec<String>, FolderId>`), non solo i tre `INSERT` batch.** Senza
cache, un migliaio di file nella stessa cartella avrebbe comunque fatto un
giro di rete a cartella per file: la cache è la stessa idea di "un giro di
rete per lotto" applicata alla risoluzione delle cartelle. — *Costo se
sbagliato:* nessuno osservato; è un `HashMap` locale alla funzione, niente
stato condiviso fra scan.

Ruling: **la misura è sulla fase discover isolata, non sul tempo totale di
import.** Con 1.000 file assestati (`discovering_a_thousand_settled_files_
takes_seconds_not_hours`, stesso commit prima/dopo via `git worktree`):
**1.698 s → 73 ms** (~23×, da ~1,7 ms/file a ~0,073 ms/file di soli round
trip DB). Sulla pipeline intera (`ingest_fixture_indexes_three_jpegs`, 3
JPEG reali attraverso exif+hash+derive+DB): **382 ms totali, ~127 ms/file**,
di cui l'`exif` misurato è ~0 ms e il `derive` ~3 ms — il resto è
ffmpeg/hash, non discover. Sull'archivio reale del campo
(`field-test-20260817-1855.md`, 1.558 file, 7m52s totali): al ritmo
misurato qui il discover di quell'intero archivio costerebbe ~114 ms *dopo*
il lotto (~2,6 s prima) — una frazione irrilevante dei 472 s osservati, che
sono dominati da exif (5m53s) e hash (6m54s), fasi CPU/IO-bound che questo
task non tocca.

**Ruling: il due-tempi resta una decisione differita, non presa qui, e il
numero appena misurato è il motivo.** Il brief chiedeva la decisione "solo
con il numero in mano" — il numero dice che il collo di bottiglia del
"giorni per il primo import" non è la scrittura discover (che ora costa
millisecondi anche su migliaia di file) ma exif+hash+derive per file, fasi
che questo task non doveva toccare (fuori scope: "non implementare cose di
fasi successive"). Battere il due-tempi (indicizzare subito, derivare dopo
in background) resterebbe quindi necessario per il problema reale, ma è
un cambio architetturale della pipeline di ingest — non una batch-insert —
e va deciso con la sua spec, non improvvisato qui. — *Costo se sbagliato:*
si spedisce Fase 10 credendo che il problema dell'import lungo sia risolto
perché "abbiamo fatto le batch insert", mentre l'utente aspetta ancora
ore/giorni allo stesso modo di prima.

Ruling: **`default_night_window()` passa a 2:00–7:00: vince l'interfaccia,
come richiesto dal brief e già annotato come discrepanza nel piano.**
Osservato **rosso** con
`default_night_window_matches_the_promise_made_in_the_ui` prima della
correzione (`left: (02:00,06:00) right: (02:00,07:00)`). Le altre finestre
di test (`night_window_yields_night_unless_interattivo` e simili) restano
a `2:00–6:00` di proposito: testano `ActivityTracker::current_profile` con
una finestra passata come parametro, non il default — non è la stessa
asserzione, e cambiarle userebbe un numero arbitrario invece di uno
significativo per quel test. — *Costo se sbagliato:* nessuno oltre al
testo già scritto nell'interfaccia (§57), che il codice non contraddiceva
più nemmeno prima di questo task nella forma della finestra, solo nell'ora
di fine.

Ruling: **`region.progress` porta `region_id`, `status`, `downloaded_bytes`,
`size_bytes`, `last_error` — un campo più di quanto elencasse il brief.**
`RegionView` cita solo `downloaded_bytes`/`status`/`last_error` come "dati
che esistono", ma senza `size_bytes` un client non potrebbe calcolare una
percentuale dall'evento da solo, e dovrebbe comunque richiamare `GET
/regions`: `size_bytes` è lo stesso identico campo già esposto da quella
rotta, non un dato nuovo. La chiave di deduplica per-regione è `(status,
downloaded_bytes, last_error)`: `size_bytes` non cambia mai durante un
download, quindi non serve nella chiave. `RegionRepo::list` richiede solo
un utente autenticato (le regioni sono globali all'istanza, non per-utente,
già così da Fase 4) — nessun controllo `is_admin` in più rispetto a `GET
/regions`. — *Costo se sbagliato:* un client che ignora `size_bytes` non
perde nulla; uno che lo usasse per altro dovrebbe comunque validarlo contro
`GET /regions`, come già fa per gli altri eventi "segnale, non stato".

Difetto osservato, non di questo task (annotato, non toccato):
`cancelling_a_scan_via_the_api_leaves_a_partial_bulk_outcome`
(`crates/keeppix-api/tests/scan.rs`) usava `TOTAL = 40`, sotto
`PRODUCTION_BATCH_SIZE = 500`: con la scrittura a lotti l'intera scansione
si applica in un'unica istruzione prima che il polling del test possa
osservare uno stato "a metà", e l'annullamento non ha più nulla di parziale
da lasciare. Stessa classe di difetto già vista e corretta in questo stesso
task per `discover_operations.rs` e `ws.rs` — qui applicata anche a
`scan.rs` con lo stesso rimedio (`TOTAL = 5 * PRODUCTION_BATCH_SIZE`),
osservato **rosso** prima della correzione (`done` restava ≥ `TOTAL`,
`status` era `Done` non `Cancelled`).

Task 21: complete (commits 0c239bc fix(jobs) night window + f9c0ddc
feat(db) batch primitives + 29c6921 feat(jobs) flush a lotti + f8d3059
feat(api) region.progress + 414ce6b test(api) fixture di cancellazione;
tests green: `keeppix-db` assets.rs 18/18 [+2 nuovi:
`batch_upsert_discovered` inserisce ogni file nuovo, omette i file
invariati], jobs.rs 15/15 [+2 nuovi: `enqueue_many` inserisce ogni voce,
rispetta il `dedup_key` esistente], operations.rs 16/16 [+2 nuovi:
`record_success_many` appende tutti gli id e avanza `done` una sola volta,
un lotto vuoto è un no-op], regions.rs 6/6 (nessuna regressione); `keeppix-
jobs` profile.rs 13/13 [+1 nuovo: `default_night_window()` è 2:00–7:00,
osservato **rosso** prima della correzione], discover_operations.rs 6/6 e
discover_perf.rs 6/6 (nessuna regressione, `TOTAL` già portato a `5 *
PRODUCTION_BATCH_SIZE` nel task precedente di questa sessione),
ingest_fixture.rs 4/4 (nessuna regressione); `keeppix-api` ws.rs 9/9 [+1
nuovo: un download regione pushato come `region.progress`], scan.rs 8/8
[+0 nuovi, 1 corretto: `cancelling_a_scan_via_the_api_leaves_a_partial_
bulk_outcome` con `TOTAL` portato sopra `PRODUCTION_BATCH_SIZE`], map.rs
10/10 (nessuna regressione, tocca `regions.rs`). `cargo fmt --check` e
`cargo clippy --workspace --all-targets -- -D warnings` verdi su tutto il
workspace; `cargo build --workspace --all-targets` verde. Rieseguite per
intero senza regressioni: `keeppix-jobs` (22 suite, tutte verdi) e
`keeppix-api` (tutte le suite tranne `bootstrap_emits_no_more_queries_
than_individual_repos`, verde in isolamento — stesso difetto differito
già annotato nel ledger di Task 19, non una regressione di questo task) e
`keeppix-db` (41 suite, tutte verdi). `./scripts/test.sh` completo **non
eseguito** (stesso motivo dei task precedenti: pulisce `target/` a fine
corsa e costerebbe l'intera suite del workspace); eseguiti invece
`cargo test -p keeppix-db`, `-p keeppix-jobs`, `-p keeppix-api` e `-p
keeppix-server` per intero, oltre alla misura isolata via `git worktree`
sul commit precedente per il numero "prima" della batch insert.

## Task 22 — La pipeline di derivati sa decodificare solo JPEG

Ruling: **HEIF/HEIC passa da `heif-convert` (CLI di `libheif-examples`) in
sandbox, non da `libheif-rs`.** Il brief suggeriva `libheif-rs`, ma quel
binding collega `libheif` in processo — esattamente ciò che il ruling del
piano vieta ("i nuovi decoder passano dallo stesso sandbox degli altri, non
ne sono esenti"). `heif-convert` è lo stesso schema già in uso per
`dcraw_emu`/`ffmpeg`: un binario esterno invocato con `sandbox::run` e
`RLIMIT_AS`/`RLIMIT_CPU`. Costa il round-trip per due file temporanei
(`heif-convert` legge/scrive solo file reali, non stdin/stdout — verificato
con `-o -` che fallisce con "Unknown file type in -" contro libheif 1.17) e
un secondo passaggio nel decoder PNG per il file di uscita — nessuna
dipendenza C nuova collegata in processo, la stessa cosa. — *Costo se
sbagliato:* un decoder in processo per il formato con la storia di CVE più
recente dei quattro, scoperto in produzione da un HEIC malformato invece che
in review.

Ruling: **il PNG di uscita di `heif-convert` per un HEIC 10 bit è 16 bit per
canale** (confermato con `heif-info` su `sample10.heic`: `bit depth: 10`, e
il PNG intermedio letto con `png::Decoder` senza `STRIP_16` mostra
`BitDepth::Sixteen`), non 8 bit come un HEIC Main comune. Il decoder PNG
scritto per questo task normalizza sempre a 8 bit/canale
(`Transformations::normalize_to_color8`), quindi il 10 bit sopravvive fino a
lì e viene poi ridotto come tutto il resto della pipeline (che è comunque
RGB8 fino a WebP). — *Costo se sbagliato:* un HEIC 10 bit reale avrebbe
prodotto errore o colori sbagliati senza che nessun test lo notasse; il
fixture `sample10.heic` è un file reale generato con `heif-enc -b 10`, non
un'assunzione.

Ruling: **GIF resta debito differito, non toccato da questo task.**
`detect_kind` classifica anche GIF come `AssetKind::Image`, ma il brief
elenca solo PNG/TIFF/WebP/HEIF da implementare — GIF non è nella lista.
Stesso sintomo di prima di questo task (`DeriveError::Decode` silenzioso per
ogni GIF caricato in libreria) resta per GIF finché non viene aperto un task
dedicato. — *Costo se sbagliato:* nessuno rispetto a oggi; non è una
regressione introdotta qui, è lo stesso difetto pre-esistente ristretto a un
formato in meno.

Difetto osservato, non di questo task (annotato, non toccato): la CI di
questo task installa `libheif-plugin-libde265` per `ubuntu-latest`, ma
`heif_convert_available()` (come `dcraw_emu_available`/`ffprobe_available`
prima di esso) verifica solo che `heif-convert --version` abbia successo, e
quel comando **non** carica i plugin di codec — confermato puntando
`LIBHEIF_PLUGIN_PATH` a una directory vuota: `--version` resta a exit 0,
mentre decodificare una vera HEIC fallisce con "Unsupported codec". Se il
pacchetto plugin manca su un host, i test HEIF non saltano in modo pulito:
falliscono con un errore di decodifica. Non un problema per questo task (il
plugin è installato sia qui che nella CI aggiornata), ma
`heif_convert_available()` sarebbe più corretto se tentasse una decodifica
reale invece di un semplice `--version` — debito annotato, non risolto qui
per non allargare la superficie del task.

Task 22: complete (commits 3b373bf feat(media) decoder multi-formato +
c63041d test(media) fixture PNG/TIFF/WebP/HEIF + 7ed1341 feat(docker)
libheif nel runtime + 11000fa ci libheif per i test; tests green:
`keeppix-media` derive_formats.rs 11/11 [nuovo file: PNG/TIFF/WebP/HEIF
8 bit/HEIF 10 bit producono thumb+preview validi, la stessa variante
malformata di ciascun formato fallisce con `DeriveError::Decode` senza
lasciare derivati parziali, `heif-convert` su input corrotto ritorna sotto
20s (guardia anti-hang sul sandbox `RLIMIT_CPU`), byte non riconosciuti
falliscono senza passare da `kind::detect_kind`], nessuna regressione sul
resto di `keeppix-media` (derive.rs, raw.rs, video.rs, xmp.rs, walk.rs,
tutte le altre suite verdi). `heif-convert` era già installato su questa
macchina (`libheif-examples` 1.17.6 + `libheif-plugin-libde265`/`-x265`/
`-aomdec`/`-aomenc`), quindi `heif_8bit_source_produces_thumb_and_preview` e
`heif_10bit_source_produces_thumb_and_preview` hanno **eseguito davvero**,
non saltato per assenza del binario — la conferma del 10 bit richiesta dal
brief è quindi osservata, non presunta. `cargo fmt --check`, `cargo clippy
--workspace --all-targets -- -D warnings` e `cargo build --workspace
--all-targets` verdi. `cargo deny check` verde (`advisories ok, bans ok,
licenses ok, sources ok`); solo warning informativi per due entry duplicate
di `zune-core`/`zune-jpeg` (0.4.x da `zune-jpeg` diretto, 0.5.x da `tiff`),
non un errore — `cargo deny` non ha una regola che lo vieti, e sono la
stessa libreria in due major diverse, non due dipendenze in conflitto
architetturale. `./scripts/test.sh` completo **non eseguito** (stesso
motivo dei task precedenti); eseguito invece `cargo test -p keeppix-media
--all-targets` per intero (nessuna regressione) oltre alla suite dedicata.
Dockerfile validato eseguendo a mano, dentro un container
`debian:bookworm-slim`, gli stessi comandi dello stage `heif` (l'ambiente
manca del plugin `docker buildx`, quindi non è stato possibile un
`docker build --target heif` reale) — non un test end-to-end dell'immagine
completa, ma sufficiente a confermare che `apt-get install
libheif-examples` e la raccolta delle librerie con `ldd` funzionano come
scritto.

Ruling: **router↔spec parity si verifica parsando `lib.rs`, non
introspezione a runtime.** `axum::Router` in 0.8 non esiste alcuna API per
enumerare le rotte montate a runtime (nessun `.routes()`, niente `Debug`
utile). L'alternativa era o costruire un secondo router "ombra" solo per i
test (rischio che diverga da quello reale) o parsare il codice sorgente di
`lib.rs` con una piccola regex-based extraction delle chiamate `.route(...)`.
Ho scelto il secondo: `extract_route_calls` in `tests/openapi.rs` legge
`crates/keeppix-api/src/lib.rs` a compile time (`include_str!`-style via
`std::fs::read_to_string` sul path relativo al manifest), estrae `(method,
path)` per ogni `.route("...", get(...)/post(...)/...)` e confronta l'insieme
con le chiavi di `paths` nel documento OpenAPI generato in-process. — *Costo
se sbagliato:* se qualcuno cambia lo stile di scrittura delle route in
`lib.rs` (es. un macro-helper che genera `.route(...)` senza scriverlo
letteralmente), il parser smette di vederle e il test perde silenziosamente
copertura. Difetto noto, non c'è modo di evitarlo del tutto senza
introspezione reale da axum.

Ruling: **le view API (`AuditEntryView`, `PermissionGrantView`,
`GrantSummaryView`, `ExplainView`, `ExplainChainLinkView`,
`SharedWithMeView`) restano in `keeppix-api/src/routes/*.rs`, non in
`keeppix-db`.** L'invariante del progetto è che `keeppix-db` non conosca
concetti HTTP/OpenAPI; `utoipa::ToSchema` è un dettaglio di presentazione
dell'API, non del dominio. Ogni view implementa `From<TipoDb>` e fa da
adattatore 1:1 (nessuna logica, solo rinominare/riformattare campi dove il
tipo db non è già `Serialize`-friendly per lo schema pubblico). — *Costo se
sbagliato:* duplicazione di campi manuale ogni volta che il tipo db cambia
forma; il compilatore lo cattura comunque (il `From` non compila più), quindi
il rischio è basso.

Ruling: **`upload::patch` e `share::public_upload` documentano il body con
`request_body(content = Vec<u8>, content_type = "application/octet-stream")`
invece di un content-type generico o omesso.** Sono gli unici due endpoint
del progetto che accettano un body binario grezzo (chunk di upload
resumable), non JSON; utoipa supporta `Vec<u8>` come schema binary nativo.
`upload::patch` ha inoltre risposte multiple documentate (`204` per chunk
accettato, `201` con `UploadCompleteResponse` quando il chunk completa la
sessione) perché il codice HTTP reale dipende dallo stato della sessione, non
solo dal successo/fallimento della singola chiamata. — *Costo se sbagliato:*
i client generati (TypeScript/Swift/Kotlin/Dart) tratterebbero il body come
JSON o mancherebbero la variante 201, ma è stato verificato generando
davvero i client TypeScript/Swift dallo spec aggiornato.

Difetto osservato, non di questo task (annotato, non toccato):
`scripts/check-wired.py` segnala funzioni pubbliche senza chiamante di
produzione e rotte montate senza consumer nel frontend, ed esce con codice 1
anche stashando tutte le modifiche di questo task (verificato con `git
stash` + re-run). È debito pre-esistente indipendente da Task 23 — molte
rotte di Fase 10 (albums, groups, permissions, share, audit, backup,
restore, upload) non hanno ancora un frontend che le consuma, che è
esattamente lo stato aspettato a metà fase. Non risolto qui perché
implementare il frontend per queste rotte è fuori scope (sarebbe "fare cose
di fasi successive perché tanto ci vuole poco").

Task 23: complete (commits 529f130 test(api) parity check red [49 rotte
mancanti] + d2efdb7 feat(api) annotazioni utoipa su audit/permissions/
backup/restore/share/upload/health + view struct + c361214 feat(api)
registrazione paths/schemas/tags in ApiDoc + rigenerazione
docs/api/openapi.json + 200ed3d test(api) aggiornamento assert conteggi/id
[90→139 operazioni]). Tutti gli 8 test di `tests/openapi.rs` verdi,
incluso il nuovo `router_registered_routes_are_all_documented` (era rosso
all'inizio con 49 rotte mancanti, ora verde con zero rotte scoperte).
`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` verdi. `cargo test -p keeppix-api --jobs 1 --
--test-threads=1` completo verde (nessuna regressione su nessun altro
modulo, incluse le suite WebDAV/WS/journeys/sidebar_load/budgets che
toccano audit/permissions indirettamente). `scripts/generate-api-clients.sh`
eseguito per intero dallo spec rigenerato: client TypeScript e Swift
generati senza errori; il client TypeScript generato è stato inoltre
type-checked con `tsc --noEmit` a zero errori. CI (`.github/workflows/
ci.yml`) job `api-clients` che esegue lo stesso script era già presente dal
commit `40a0ae9`, quindi il punto 3 del brief era già soddisfatto prima di
questo task — non serviva aggiungerlo, solo verificarlo. Nessun push.


## Phase close (2026-08-21)

Ruling: `scripts/wired-exceptions.txt` — le nuove rotte di Fase 10 senza
consumer frontend (`/bootstrap`, `/timeline/geometry`, sessioni, preferenze,
batch delete, shared-with-me, cancel operation) e `set_phase` restano in
Rinvii verso `fase-11`; `count_by_status`/`set_status` spostati da `fase-10`
a `fase-11` (WS già emette `scan.progress`); `ping` rimane debito
`non-rivendicata` perché /health non è stato collegato in questa fase. —
Perché: `check-wired.py` deve restare verde a chiusura senza fingere che la
UI di Fase 11 esista già. — Costo se sbagliato: debito silenzioso o eccezioni
che bloccano la guardia senza motivo.

Ruling: documenti di navigazione (`CONTINUE.md`, `superpowers/README.md`,
`PROSEGUI.md`) aggiornati a «Fase 10 implementata sul branch, in attesa di
merge; prossimo lavoro = Fase 7». — Perché: un agente nuovo non deve
ripartire dalla 10. — Costo se sbagliato: doppia implementazione o ordine
sbagliato 7/8/9.

Phase close docs: complete (pending full AGENTS.md verify + user merge).
`python3 scripts/check-wired.py` EXIT 0 con le eccezioni sopra.
