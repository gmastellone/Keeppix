# Task 7 report — Tile PMTiles e gestore regioni

## Esito

Implementato su `fase-4` nel commit `1aa2844`:

- migrazione schema-only `0022_map_regions.sql`;
- `RegionRepo` con stato, progresso, errori, cancel/delete e transizioni
  condizionate per evitare due writer;
- `DownloadMapRegion` nella coda esistente con dedup key `map-region:{id}`;
- downloader in-process `reqwest` streaming, resume da `.pmtiles.part`,
  checksum SHA-256 prima del rename e cleanup su cancel/checksum/I/O;
- allowlist compilata (`build.protomaps.com`), solo HTTPS, niente IP,
  localhost, userinfo o redirect; validazione sia all'enqueue sia subito prima
  della richiesta;
- serving autenticato e range-aware su
  `GET /api/v1/map/tiles/{region}/{z}/{x}/{y}`;
- API admin per list/download/cancel/delete sotto `/api/v1/map/regions`;
- contratto OpenAPI aggiornato con sole aggiunte.

Non sono stati implementati MapLibre UI né home geofence.

## TDD — RED

Osservati prima dell'implementazione:

1. `cargo test -p keeppix-domain map_region_download_kind_round_trips`
   falliva perché `JobKind::DownloadMapRegion` non esisteva.
2. Il test unitario jobs falliva su `host_allowed`, downloader e `RegionRepo`
   assenti.
3. `cargo test -p keeppix-db --test regions -- --test-threads=1` falliva per
   `NewMapRegion`/`RegionRepo`/`RegionStatus` assenti.
4. Il test API tile falliva perché repository e route regioni non esistevano.
5. Verifica per mutazione: rimossa temporaneamente la seconda validazione
   allowlist, `worker_revalidates_the_stored_url_before_any_request` falliva
   tentando davvero `https://127.0.0.1/private`; ripristinato il controllo, il
   test è verde.

## TDD — GREEN

I test Task 7 coprono:

- URL non allowlisted rifiutati all'enqueue;
- allowlist runtime ricontrollata prima della rete;
- `Range: bytes={offset}-` osservato da listener locale e resume esatto;
- checksum errato: stato `error`, mai `available`, parziale e finale assenti;
- doppio enqueue: un solo job/writer;
- range tile `206`, cancellazione regione e successivo `404 problem+json`;
- cancel: file parziale rimosso e errore leggibile;
- non-admin rifiutato sulle mutazioni.

## Verifica prescritta

Tutti con exit code 0:

```text
cargo fmt --check
cargo clippy -p keeppix-domain -p keeppix-db -p keeppix-jobs -p keeppix-api --all-targets -- -D warnings
cargo test -p keeppix-domain --jobs 1 -- --test-threads=1   # 48 test
cargo test -p keeppix-db --jobs 1 -- --test-threads=1
cargo test -p keeppix-jobs --jobs 1 -- --test-threads=1
cargo test -p keeppix-api --jobs 1 -- --test-threads=1
```

`./scripts/test.sh` non è stato eseguito, come richiesto. Il filesystem cloud
si è saturato due volte per gli artefatti `target/`; solo output Cargo generato
è stato rimosso con `cargo clean`, poi i comandi falliti per spazio sono stati
rieseguiti invariati fino a exit 0.

## Ruling e concern

- L'offset esatto persistito è la lunghezza del `.part`; il DB ne conserva uno
  specchio per la UI. Un crash tra due update non perde byte né corrompe il
  prossimo `Range`.
- `downloaded_bytes` e `last_error` estendono lo schema del brief perché
  avanzamento e messaggio leggibile non erano rappresentabili nelle sole
  colonne elencate.
- I test HTTP locali chiamano l'helper già oltre il confine di validazione:
  localhost non compare e non comparirà nell'allowlist di produzione.
- Nessun download reale Protomaps viene eseguito in CI; rete, resume e checksum
  usano fixture di pochi byte.
