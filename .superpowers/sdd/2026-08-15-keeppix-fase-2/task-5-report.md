# Task 5 report: Sidecar XMP

**Status: DONE**

Branch `fase-2`. Quattro commit:

- `af00600` `feat(media): read and write xmp sidecars without losing foreign fields`
- `51127d7` `feat(domain): add JobKind::WriteSidecar`
- `6ee8f04` `feat(db): serve and enqueue pending xmp sidecar writes from OverrideRepo`
- `7dcb4e0` `feat(jobs): write xmp sidecars from pending overrides (JobKind::WriteSidecar)`

## 1. Cosa è stato fatto

- `crates/keeppix-media/src/xmp.rs`: `SidecarData { rating, description,
  title, tags, gps, taken_at, label }`, `XmpError`, `read_sidecar`,
  `write_sidecar`. Mappatura dei campi esattamente come da brief/spec
  §3.4 (`rating`→`xmp:Rating`, `description`→`dc:description`,
  `title`→`dc:title`, `tag`→`dc:subject`, GPS→`exif:GPSLatitude`/
  `exif:GPSLongitude`, `taken_at`→`exif:DateTimeOriginal`,
  `pick/reject`→`xmp:Label`). Implementazione a livello di stream di
  eventi `quick-xml` (non DOM): il file esistente viene riletto,
  attributi/elementi non gestiti restano bit-per-bit inalterati, solo
  gli attributi/elementi in `MANAGED_ATTRS`/gestiti vengono
  aggiunti/aggiornati/rimossi. Scrittura atomica: `.xmp.tmp` nella
  stessa cartella → `fsync` → rilettura e verifica → `rename()`. Nuova
  dipendenza `quick-xml = "0.41"` in `keeppix-media/Cargo.toml` (vedi
  Ruling nel ledger).
- `crates/keeppix-media/tests/xmp.rs`: 12 test — il critico
  `writing_preserves_fields_we_do_not_manage` (Lightroom
  `crs:Exposure2012`), lettura di un sidecar reale darktable, sidecar
  mancante → `Ok(None)`, XML malformato → `Err` non panic, UTF-8
  invalido → `Err` non panic, scrittura su sidecar malformato esistente
  → non lo tocca, creazione da zero quando non esiste, round-trip
  completo, azzeramento di tutti i campi gestiti, nessun `.tmp`
  residuo dopo una scrittura riuscita, cartella in sola lettura → `Err`
  senza corrompere nulla (Unix-only).
- `keeppix-domain::JobKind::WriteSidecar`: nuova variante, `as_str`/
  `parse`/round-trip test aggiornati. Nessuna migrazione: `jobs.kind`
  è `text` senza `CHECK` (confermato anche nel ledger di Task 3/4).
- `crates/keeppix-db/src/overrides.rs`:
  - `OverrideRepo::sidecar_source(asset_id)` — nessun `AuthContext`
    (stessa giustificazione di `pending_sidecars`, Task 4): effettivo
    (`COALESCE(override, exif)`) più `owner_rating`/`owner_pick` presi
    da `asset_flags` filtrato sul `owner_id` della libreria via join
    `assets → folders → libraries`.
  - `OverrideRepo::mark_sidecar_written(asset_id)` — imposta
    `xmp_written_at = now()`, da chiamare solo dopo la verifica della
    scrittura.
  - `SidecarSource` (pubblico, ri-esportato da `keeppix-db::lib.rs`):
    tipo di dominio che porta `Rating`/`Pick` — vive in `keeppix-db`,
    non in `keeppix-media`, perché quel crate non deve conoscere tipi
    di dominio legati al database.
  - `enqueue_sidecar_sweep`: chiamata da `apply` e `apply_batch` dopo
    la scrittura, accoda `JobKind::WriteSidecar` a priorità
    `Background` con dedup key fissa `"write_sidecar"`.
- `crates/keeppix-jobs/src/xmp.rs`: `run(db)` — rilegge
  `pending_sidecars(200)`, processa ogni asset (`write_one`), rimane
  ok anche se qualche asset è già stato cancellato (`NotFound` non è un
  fallimento del job), si ri-accoda da solo se il batch era pieno,
  ritorna `Err` con retry se anche un solo asset non è stato scritto
  (ma quelli riusciti restano marcati). `pick_label` converte
  `Pick::{None,Pick,Reject}` in `Option<String>` per `xmp:Label`
  (`None` → nessuna etichetta, non una stringa vuota).
- `crates/keeppix-jobs/src/dispatch.rs`: nuovo ramo per
  `JobKind::WriteSidecar → xmp_job::run(&self.db).await`.
- `crates/keeppix-jobs/tests/xmp.rs`: 2 test di integrazione end-to-end
  con `TestDb` reale — `apply` con voto del proprietario produce un
  sidecar nuovo corretto e l'asset esce da `pending_sidecars`; un
  sidecar Lightroom preesistente sopravvive a uno sweep innescato da un
  cambio di sola descrizione.

## 2. Comportamenti pinnati (mappa test ↔ requisito)

| Requisito | Test |
|---|---|
| Leggi-modifica-riscrivi, mai da zero (campo Lightroom sconosciuto) | `writing_preserves_fields_we_do_not_manage` (media) |
| Idem, elemento figlio sconosciuto (darktable) | `writing_preserves_child_elements_we_do_not_manage` (media) |
| Lettura completa di un sidecar reale | `reading_a_real_darktable_sidecar` (media) |
| Sidecar mancante → `Ok(None)`, non errore | `reading_a_missing_sidecar_returns_none` (media) |
| XML malformato → `Err`, non panic | `a_malformed_sidecar_is_an_error_not_a_panic`, `invalid_utf8_is_an_error_not_a_panic` (media) |
| Scrivere su un sidecar malformato non lo tocca | `writing_to_a_malformed_existing_sidecar_leaves_it_untouched` (media) |
| Creazione da zero se il sidecar non esiste | `write_sidecar_creates_a_new_one_when_none_exists` (media) |
| Round-trip di tutti i campi | `round_trip_preserves_every_field` (media) |
| Azzeramento di tutti i campi gestiti | `writing_default_data_clears_every_managed_field` (media) |
| Nessuno stato parziale (niente `.tmp` residuo) | `no_tmp_file_survives_a_successful_write` (media) |
| Cartella in sola lettura → `Err`, nulla corrotto | `writing_to_a_read_only_directory_fails_without_corrupting_anything` (media, Unix-only) |
| `apply`/`apply_batch` accodano il sweep | `pending_sidecars_only_lists_updates_not_yet_written` (db, Task 4, riverificato) |
| Pipeline completa: voto proprietario → file scritto → non più pendente | `applying_an_override_writes_the_owners_rating_and_pick_to_a_new_sidecar` (jobs) |
| Il sweep non rigenera mai un sidecar esistente da zero | `the_sweep_never_regenerates_an_existing_sidecar_from_scratch` (jobs) |

## 3. TDD e mutation testing

### 3.1 keeppix-media

Tutti i 12 test di `tests/xmp.rs` scritti prima dell'implementazione
(seguendo il brief e la spec), osservati falliti (funzioni non
esistenti → errore di compilazione), poi resi verdi implementando
`xmp.rs`. Due mutazioni per confermare che i test critici catturano
davvero le regressioni che dichiarano di proteggere:

**Mutazione 1 — filtro dei campi gestiti rimosso in `apply`.**

Tolto il controllo `MANAGED_ATTRS.contains(&key.as_slice())` (tutti gli
attributi esistenti vengono scartati, non solo quelli sconosciuti):

```
test writing_preserves_fields_we_do_not_manage ... FAILED
assertion failed: after.contains("crs:Exposure2012")
```

Ripristinato → verde.

**Mutazione 2 — scrittura non atomica.**

`atomic_write` mutato per scrivere direttamente sul path finale invece
di passare da `.xmp.tmp` + `rename()`:

```
test writing_to_a_read_only_directory_fails_without_corrupting_anything ... FAILED
```

Il file esistente ha permessi normali (644) anche se la sua cartella è
in sola lettura (555): scrivendoci sopra direttamente l'operazione
*riesce* invece di fallire, dimostrando che il test dipende
effettivamente dalla disciplina tmp+rename e non da un dettaglio
incidentale del filesystem. Ripristinato → verde.

### 3.2 keeppix-jobs

I due test di `tests/xmp.rs` sono stati scritti e verificati verdi
dopo l'implementazione (la wiring del sweep era già definita dal
disegno emerso durante l'esplorazione — vedi i `Ruling` nel ledger).
Per non fidarmi di un verde che non ha mai visto un rosso, ho mutato
`write_one` per saltare `overrides.mark_sidecar_written(asset_id)`:

```
test applying_an_override_writes_the_owners_rating_and_pick_to_a_new_sidecar ... FAILED
thread '...' panicked: dopo la scrittura verificata l'asset non deve
più risultare pendente
```

Ripristinato → `test result: ok. 5 passed; 0 failed`.

## 4. Verifica finale (comandi eseguiti, output osservato)

```
$ export KEEPPIX_TEST_DATABASE_URL='postgres://postgres:postgres@127.0.0.1:5432/postgres'
$ cargo test -p keeppix-media --test xmp -- --test-threads=1
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test -p keeppix-jobs --test xmp -- --test-threads=1
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test -p keeppix-db --test overrides -- --test-threads=1
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo fmt --check
(nessun output — verde)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s)
(nessun warning — verde)
```

Suite completa, un crate alla volta (`./scripts/test.sh` non eseguibile
in questo ambiente: la sua `cleanup_containers` assume che se il
comando `docker` esiste anche il demone sia raggiungibile — qui non lo
è, `docker ps` fallisce e `set -e`/`pipefail` interrompe lo script dopo
il primo crate; non modificato, fuori scope per questo task, coerente
con le istruzioni d'ambiente "No Docker — use
`KEEPPIX_TEST_DATABASE_URL`"):

```
keeppix-domain        → 42 passed (incluso job_kind_round_trips_snake_case con WriteSidecar)
keeppix-media         → tutti verdi tranne video::poster_extracts_one_frame (preesistente, vedi sotto)
keeppix-db            → tutti verdi (overrides 15, flags 9, + resto)
keeppix-jobs          → tutti verdi (10 binari + xmp 5/5)
keeppix-api           → tutti verdi (24+ test su auth/health/media/openapi/search/timeline/viewport/ws)
keeppix-server        → 9 (nessun test proprio ma compila e passa i suoi 9 unit test)
keeppix-dav           → 0 test, compila
keeppix-test-support  → 0 test, compila
```

`keeppix-media::video::poster_extracts_one_frame` è preesistente e
indipendente da questo task: verificato eseguendo lo stesso test dopo
aver riportato temporaneamente `crates/keeppix-media` al commit
precedente a Task 5 (`878418a`, prima di `af00600`) — fallisce
identicamente (`ffmpeg poster failed`), quindi non è una regressione
introdotta qui. Stessa causa già annotata nel ledger di Task 4
(limiti del sandbox verso il processo ffmpeg in questo ambiente).

`cargo deny check bans`: non verificato in questa sessione con lo
strumento CLI (non disponibile nell'immagine), ma nessun arco nuovo fra
`keeppix-media` e `keeppix-db` è stato introdotto — `SidecarSource`
(tipi `Rating`/`Pick`) vive in `keeppix-db`, `keeppix-media::xmp` non
importa nulla da `keeppix-db` né da `keeppix-domain` oltre `GeoPoint`
(già usato altrove nel crate).

## 5. Ledger

Aggiornato `.superpowers/sdd/2026-08-15-keeppix-fase-2/progress.md`:
tabella di avanzamento (Task 5 → complete), sezione narrativa con le
mutazioni sopra e quattro `Ruling`:

- `quick-xml = "0.41"` come unica dipendenza XML del workspace,
  giustificata dal bisogno di un'API a eventi (non DOM) per il
  leggi-modifica-riscrivi.
- Il job `WriteSidecar` è uno sweep su `pending_sidecars` (non un job
  per asset), con dedup key fissa e ri-accodamento se il batch è
  pieno.
- L'accodamento vive in `OverrideRepo::apply`/`apply_batch`
  (`keeppix-db`), non in `keeppix-jobs` — nessun reaper periodico
  esiste ancora nel codice per un pattern alternativo stile
  `ReapStale`.
- **Limite noto e differito**: un voto del proprietario (`FlagRepo::set`)
  isolato, senza alcun override di metadati, non fa comparire l'asset
  in `pending_sidecars` finché non arriva un altro cambiamento —
  `asset_flags` non ha un meccanismo di "pending" equivalente a
  `asset_overrides.updated_at`. Fuori dai confini scritti di Task 5
  (che parla di override, non di flag, come innesco); da rivedere se
  l'uso reale richiede propagazione immediata del solo rating.

## 6. Non fatto (fuori scope, rimandato)

- `tags`/`dc:subject`: la mappatura è implementata e testata a livello
  di `keeppix-media` (round-trip completo), ma `SidecarSource` scrive
  sempre `tags: Vec::new()` perché la tabella `tags` è una feature di
  Fase 3+ non ancora esistente — nessuna sorgente dati nel database per
  questo campo, come già annotato durante l'esplorazione.
- Propagazione immediata del solo rating/pick senza un override
  concomitante (vedi Ruling sul limite noto sopra).
- Nessun reaper periodico stile `ReapStale` per rieseguire lo sweep in
  assenza di nuovi override (es. dopo un riavvio con job falliti mai
  ritentati): l'accodamento diretto da `apply`/`apply_batch` copre il
  caso normale, un timer di sicurezza è fuori scope.

## 7. Non pushato

Nessun `git push` eseguito, come richiesto. `git log --oneline -6` sul
branch `fase-2`:

```
7dcb4e0 feat(jobs): write xmp sidecars from pending overrides (JobKind::WriteSidecar)
6ee8f04 feat(db): serve and enqueue pending xmp sidecar writes from OverrideRepo
51127d7 feat(domain): add JobKind::WriteSidecar
af00600 feat(media): read and write xmp sidecars without losing foreign fields
878418a docs(sdd): record Task 4 ledger and completion report
1949a5e test(db): pin undo restoring NULL on an existing override row
```
