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
