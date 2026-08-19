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

## Review fix — RED

I test di regressione sono stati scritti prima delle correzioni. Il comando

```text
cargo test -p keeppix-jobs --jobs 1 --test regions repair_reenqueues_a_downloading_region_without_a_live_job -- --exact --test-threads=1
```

è fallito con `E0425`: `repair_interrupted_downloads` non esisteva. La nuova
copertura pinna inoltre:

- regione `downloading` senza job e job `running` stale → riparazione senza
  perdere l'offset né duplicare la `dedup_key`;
- body HTTP fermo dopo il primo chunk → cancel entro il polling, `.part`
  assente e lease del job rinnovato;
- cleanup del cancel fallito → stato ancora cancellabile e secondo cancel
  riuscito;
- ultimo tentativo HTTP fallito → regione `error`, file finale assente e nuovo
  download accodabile.

## Review fix — GREEN

Tutti con exit code 0:

```text
cargo fmt --check
cargo clippy -p keeppix-domain -p keeppix-db -p keeppix-jobs -p keeppix-api --all-targets -- -D warnings
cargo test -p keeppix-jobs --jobs 1 -- --test-threads=1
cargo test -p keeppix-db --jobs 1 -- --test-threads=1
cargo test -p keeppix-api --jobs 1 -- --test-threads=1
```

Il primo rerun completo di clippy ha esaurito il filesystem sugli artefatti
Cargo; dopo `cargo clean` lo stesso comando è stato rieseguito invariato ed è
verde. `./scripts/test.sh` non è stato eseguito.

## Review fix — ruling

- `cancel_requested` è una colonna aggiunta dalla migrazione `0023`: lo stato
  resta `downloading` finché la rimozione dei file non riesce, quindi un errore
  di cleanup non rende il cancel irripetibile.
- La stessa riparazione gira all'avvio e dal job `ReapStale`: prima rimette
  `pending` i lease oltre 600 s, poi accoda solo regioni senza job vivo.
- Il lease viene rinnovato nello stesso checkpoint che controlla il cancel,
  anche mentre il body HTTP non produce chunk e durante il checksum.

Task 7 review fix: complete (commit `12c97dd`, verifiche prescritte verdi).

## Remaining review findings — RED

Nuovi test di regressione osservati rossi prima delle correzioni:

1. `cancel_retires_running_job_before_a_distinct_download_can_start` non
   compilava (`E0599`: `JobRepo::retire_active` assente). Dopo il primo fix,
   impostando il vecchio job all'ultimo tentativo falliva ancora: il worker
   senza lease marcava `error` la nuova richiesta.
2. `startup_recovery_resets_a_running_region_job_immediately` non compilava
   (`E0425`: `recover_interrupted_downloads` assente); il solo percorso
   esistente applicava sempre la soglia stale di 600 secondi.
3. `failed_exhausted_cleanup_keeps_region_downloading_for_cancel_retry`
   falliva con stato osservato `Error` invece di `Downloading`, rendendo il
   cancel successivo un conflitto mentre il file non rimosso restava presente.

## Remaining review findings — GREEN mirati

Con exit code 0:

```text
cargo test -p keeppix-jobs cancel_retires_running_job_before_a_distinct_download_can_start -- --test-threads=1
cargo test -p keeppix-jobs --test regions startup_recovery_resets_a_running_region_job_immediately -- --test-threads=1
cargo test -p keeppix-jobs failed_exhausted_cleanup_keeps_region_downloading_for_cancel_retry -- --test-threads=1
```

Scelte di stato:

- il cancel ritira come `failed` il job `pending`/`running` prima di liberare
  la dedup key; un worker che perde il lease si ferma senza mutare la nuova
  regione, anche se era all'ultimo tentativo;
- al boot i soli `DownloadMapRegion` `running` tornano subito `pending`; il
  `ReapStale` generico resta schedulato ogni cinque minuti per gli altri job;
- se il cleanup finale fallisce, la regione resta `downloading` e quindi il
  cancel resta accettato e può ritentare la rimozione.

## Remaining review findings — verifica completa

Tutti con exit code 0:

```text
cargo fmt --check
cargo clippy -p keeppix-domain -p keeppix-db -p keeppix-jobs -p keeppix-api -p keeppix-server --all-targets -- -D warnings
cargo test -p keeppix-jobs --jobs 1 -- --test-threads=1
CARGO_INCREMENTAL=0 cargo test -p keeppix-db --jobs 1 -- --test-threads=1
CARGO_INCREMENTAL=0 cargo test -p keeppix-api --jobs 1 -- --test-threads=1
```

I primi tentativi DB/API hanno saturato il filesystem sugli artefatti Cargo
(`os error 28` / PostgreSQL `53100`), senza failure applicative. Dopo
`cargo clean` e la disabilitazione dell'incrementale, gli stessi test sono
stati rieseguiti integralmente e sono verdi. `./scripts/test.sh` non è stato
eseguito.

Task 7 remaining review fixes: complete (commit `25b65bc`).
