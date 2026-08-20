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

