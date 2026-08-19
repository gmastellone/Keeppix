# SDD ledger — plan: docs/superpowers/plans/2026-08-18-keeppix-fase-4.md

Spec: docs/superpowers/specs/fase-4-mappe.md (vince sul piano)
Branch: fase-4 (da origin/main @ b702c7a)

Ruling: `LocationSource` esiste già in `keeppix-domain::asset` (Fase 1, colonna
predisposta). Task 1 aggiunge `as_str()` su quell'enum e **non** crea
`geo.rs` — Place/TzBoundary arrivano nei task successivi. Costo se sbagliato:
il piano nomina `geo.rs` come NEW; chi cerca il file non lo trova finché
Task 2/7 non lo aprono per altro.

Ruling: l'ingest EXIF vive in `keeppix-jobs/src/metadata.rs` +
`AssetRepo::insert_exif`, non in `ingest.rs` (non esiste). SQL della
posizione resta in `keeppix-db`. Costo se sbagliato: un UPDATE in jobs non
compila (`sqlx` è solo in keeppix-db).

Minor (Task 1 review): closed in `eb320f4`. `signed_dms` rejects minutes/seconds
≥ 60; `gps_point` rejects |lat|>90 or |lon|>180. Tests in `exif_gps.rs`.

Task 1: complete (commits b702c7a..4ff8541, review clean after MakerNote
fixture + no SQL in jobs tests; bounds follow-up `eb320f4`)

Ruling: migration `places` is `0020_places.sql` — `0015` is already
permissions. Cost if wrong: sqlx checksum clash on an applied file.

Ruling: no `COPY FROM` in the migration. Testcontainers has no CSV on the
Postgres disk; import is `PlaceRepo` reading a file in the app image.
Cost if wrong: every `TestDb` fails to migrate.

Task 2: complete (commits 8e38016..7e31820, review clean)

Ruling: reverse fallback rows (`population = 0` region/country) are emitted
by `scripts/build-geonames.sh`, not only by tests. Cost if wrong: ocean
points 404 instead of "Campania"/"Italy".

Ruling: suggest uses pg_trgm `%` only — no `ILIKE '%'||q||'%'`, so `q=%%`
cannot dump the table.

Task 3: complete (commits b5b0b1a..794debe, review clean after fallback
rows + wildcard fix)

Ruling: Task 2 usa la migrazione schema-only `0020_places.sql`; il dump non
entra nella migrazione perché `TestDb` non monta artefatti GeoNames e le
migrazioni applicate non si modificano. Costo se sbagliato: l'import resta un
passo di bootstrap separato invece di essere atomico con la migrazione.

Ruling: l'artefatto `places.csv` è TSV senza header, risolve `admin1`/`admin2`
nel Docker stage e viene letto in streaming in batch da 1.000 dentro un'unica
transazione. Evita una nuova dipendenza CSV, non carica ~19 MB in memoria e
mantiene l'import idempotente tramite `ON CONFLICT (id)`. Costo se sbagliato:
il formato interno va versionato o convertito prima di aggiungere colonne.

Ruling: `serve` prova il seed dal percorso fisso
`/usr/share/keeppix/places.csv` solo quando `places` è vuota; file assente è
un no-op per `cargo run`, mentre file presente ma corrotto ferma il boot per
non lasciare un catalogo parziale. Costo se sbagliato: un aggiornamento del
dataset baked non rimpiazza automaticamente una tabella già popolata; servirà
un comando amministrativo esplicito.

Task 2: complete (commit `03d2e6d`, test verdi; Docker GeoNames stage verde,
235.408 righe normalizzate)

Ruling: la tabella `places` non ha un discriminante di feature. Per il fallback
Task 3 riserva `population = 0, admin1 IS NOT NULL, admin2 IS NULL` alle regioni
e `population = 0, admin1 IS NULL, admin2 IS NULL` alle nazioni; le località
`cities500` hanno popolazione positiva. Raggi fallback: 200 km regione, 1.000 km
nazione. Costo se sbagliato: una località GeoNames a popolazione zero viene
esclusa dal reverse/forward finché una migrazione non aggiunge il tipo feature.

Ruling: il boost personale del forward è binario entro 250 km dal centroide
delle ultime 50 righe `asset_overrides.location` scritte dall'utente, dopo la
similarità trigram e prima della popolazione. È stabile con cronologia rada e
non lascia che una differenza di pochi metri domini la popolazione. Costo se
sbagliato: il raggio va reso configurabile o sostituito con un decadimento
continuo, senza cambiare il contratto HTTP.

Task 3: complete (commit `f0813d4`, test verdi)

Ruling (Task 3 review): `build-geonames.sh` accetta una directory sorgente
opzionale come secondo argomento per testare il normalizzatore senza rete; senza
argomento conserva download e soglia produzione. `countryInfo.txt` non offre un
campo ASCII separato, quindi il nome paese alimenta sia `name` sia
`ascii_name`. Costo se sbagliato: i pochi nomi paese non ASCII richiederanno una
traslitterazione esplicita nel build stage.

Task 3 review fix: complete (commits `824c604`, `437bbd9`; DB 14/14, API 6/6,
fixture normalizer, fmt e clippy verdi)

Ruling: Tasks 4 and 6 both patch `crates/keeppix-api/src/lib.rs` and OpenAPI.
Run them sequentially (4 then 6), not as parallel implementers. Cost if wrong:
merge conflicts on the router and a torn OpenAPI snapshot.

Ruling: search and pin keep using `POST /metadata/batch`; the HTTP boundary
infers `user` from location+place and `map_pin` from a free coordinate, clearing
an omitted old place for pins. Copy and GPX use
`/metadata/batch/{copy-location,import-gpx}`. Cost if wrong: clients need an
explicit source field added compatibly to the existing request.

Ruling: `metadata_batches.previous` keeps its existing asset-id map shape and
adds backward-compatible fields (`had_override`, captured `location_source`) to
each stored value. This restores both overrides and source without a migration
and still reads old batches. Cost if wrong: a future batch payload version must
replace the internal JSON shape explicitly.

Ruling: EXIF ingest may now replace only `NULL`/`exif` sources, not merely guard
`user` and `map_pin`. Task 4 makes `copied` and `gpx` real assigned sources, and
a rescan must not relabel either as EXIF. Cost if wrong: newly extracted EXIF
coordinates remain in `assets.location` only after the user removes the
override.

Task 4: complete (commit `3fef1dd`, test verdi)

Ruling (Task 4 review): `location_source` entra nello snapshot di undo solo
quando il batch porta una sorgente esplicita o il patch tocca `location` /
`place_id`; i batch titolo/descrizione/orientamento e `shift_taken_at` non lo
catturano. Costo se sbagliato: aggiungere un nuovo writer di posizione che non
usa quei campi richiederà di passare esplicitamente la semantica allo snapshot.

Ruling (Task 4 review): il parser conserva ogni `trkseg` come sequenza
indipendente, anche fra `trk` separati. Nei vuoti sceglie l'estremo di segmento
temporalmente più vicino solo entro la tolleranza, senza interpolare fra
segmenti. Costo se sbagliato: tracce con segmenti temporalmente sovrapposti
usano il primo segmento del documento che copre l'istante.

Task 4 review fixes: complete (commit `961546d`, RED/GREEN e test richiesti
verdi; evidenza in `.superpowers/sdd/task-4-report.md`)

Task 4: complete (commits 3be62ad..397abcc, review clean after undo
location_source + GPX segment-boundary fixes)

Ruling: la cella della griglia misura `90 / 2^zoom` gradi (64 pixel su una
tile mondiale da 256 pixel), con zoom interno clampato a 30. Il test pinna
coordinate di snap esatte a zoom 10. Costo se sbagliato: cambia la densità dei
cluster, non il contratto HTTP né la visibilità.

Ruling: `saved_searches` conserva solo `query_text`, mentre il cluster riceve
solo `scope_id`; `SearchRepo::saved_query` interpreta quindi sul server la
stessa grammatica testuale già usata dal frontend e riusa il compilatore SQL
parametrizzato esistente. Costo se sbagliato: le due implementazioni del parser
possono divergere e andranno sostituite da un AST persistito in una migrazione
compatibile.

Ruling: zoom >= 15 prova al massimo 501 punti; fino a 500 restituisce punti
singoli, il 501esimo innesca la query aggregata. Così il controllo del cap non
materializza l'intero viewport. Costo se sbagliato: una seconda query solo nei
viewport densi ad alto zoom.

Task 6: complete (commit `a42e8cc`, fmt, clippy, DB e API verdi; evidenza in
`.superpowers/sdd/task-6-report.md`)

Ruling (Task 6 review): gli envelope vengono segmentati a massimo 90 gradi
prima del cast a `geography`. Il cast diretto dell'envelope mondo
`-180,-90,180,90` fallisce in PostGIS per un arco antipodale; la segmentazione
mantiene `a.location`/`o.location` nudi sul lato sinistro di `&&` e quindi
compatibili con GiST. Costo se sbagliato: qualche vertice in più nella
geography costante per query; il filtro può essere semplificato se PostGIS
espone in futuro un costruttore GIDX rettangolare pubblico.

Task 6 review fixes: complete (commit `1578736`; RED/GREEN, geo 9/9, suite DB,
fmt e clippy verdi; evidenza in `.superpowers/sdd/task-6-report.md`)

Task 6: complete (commits 482a554..6fa7ae0, review clean after saved-search
quotes + geography GiST filter)

Ruling: la migrazione dei confini è `0021_tz_boundaries.sql` e resta
schema-only. Il dataset è un TSV interno generato dalla release pinned `2026c`
nel Debian build stage, copiato nel runtime distroless e importato dopo le
migrazioni solo quando la tabella è vuota. File assente è un no-op; file
presente corrotto ferma il boot e la transazione non lascia righe parziali.
Costo se sbagliato: aggiornare il dataset baked non sostituisce un catalogo già
popolato senza un futuro comando amministrativo/versionamento esplicito.

Ruling: il ricalcolo usa la posizione effettiva
`COALESCE(asset_overrides.location, assets.location)`, interpreta
`assets.taken_at_utc` come quadrante ingenuo tramite PostgreSQL `AT TIME ZONE`
e salta ogni asset con un override `taken_at` già presente. Il valore originale
resta immutabile. Costo se sbagliato: fotografie già corrette manualmente non
vengono rivalutate automaticamente quando cambia il dataset dei fusi.

Ruling: le assegnazioni `(asset_id, taken_at)` diverse entrano in un solo
`metadata_batches` usando lo snapshot e `undo_batch` esistenti; una lista vuota
non crea un batch. Costo se sbagliato: un futuro writer per-asset di altri campi
dovrà generalizzare questo metodo o aggiungerne uno parallelo.

Ruling: il lookup usa `ORDER BY tz_name LIMIT 1` oltre a `ST_Contains`. Un punto
su un bordo normalmente non appartiene a nessun poligono; un dataset corrotto
con sovrapposizioni produce comunque un solo risultato deterministico invece
di un errore. Costo se sbagliato: in una sovrapposizione reale vince il nome
IANA lessicograficamente primo finché il dataset non viene corretto.

Task 5: complete (commit `9f7b481`; fixture offline, fmt, clippy, DB, jobs e API
verdi; evidenza in `.superpowers/sdd/task-5-report.md`)


Ruling (Task 5 review): il writer timezone ricontrolla
`asset_overrides.taken_at IS NULL` nell'upsert e conserva nello snapshot undo
solo gli id effettivamente scritti. Candidati e scrittura ora vivono nella
stessa transazione; `enqueue_sidecar_sweep` resta volutamente dopo il commit,
come `apply_batch`. Costo se sbagliato: la coda sidecar può ancora fallire dopo
un commit riuscito, comportamento globale preesistente che questo fix non
ridefinisce.

Ruling (Task 5 review): il catalogo rifiuta durante il seed ogni nome assente
da `pg_timezone_names`; lookup singolo e LATERAL usano entrambi geography
`&&` + `ST_Covers` con la colonna GiST senza cast. Questo sostituisce il ruling
precedente su `ST_Contains`; `ORDER BY tz_name LIMIT 1` resta invariato. Costo
se sbagliato: un alias non esposto dal PostgreSQL installato ferma il boot e
richiede correggere il dataset normalizzato.

Task 5 review fixes: complete (commit `e9ae7f4`; RED/GREEN, fmt, clippy, DB,
jobs e API verdi; evidenza in `.superpowers/sdd/task-5-report.md`)

Task 5: complete (commits d73d6f0..6406358, review clean after atomic apply,
IANA seed check, geography lookup, and windowed preview)

Ruling: la migrazione regioni è `0022_map_regions.sql`; oltre alle colonne del
piano conserva `downloaded_bytes` e `last_error`, necessari per mostrare
avanzamento ed errore leggibile senza interrogare il filesystem dall'API. Costo
se sbagliato: due colonne globali in più e una futura migrazione per rimuoverle.

Ruling: il file `{data_dir}/maps/{id}.pmtiles.part` è la fonte esatta
dell'offset di ripresa dopo crash; `downloaded_bytes` ne è uno specchio
periodico per la UI. Così il resume non dipende dall'ultimo UPDATE riuscito.
Costo se sbagliato: la barra può restare indietro fino al prossimo checkpoint,
ma il byte range inviato al server resta esatto.

Ruling: le mutazioni del gestore vivono sotto `/api/v1/map/regions`; `file_path`
è sempre generato dal server da un id vincolato, mai accettato dal client.
Costo se sbagliato: il frontend Task 8 deve consumare questo percorso additivo,
senza cambiare lo schema delle richieste.

Task 7: complete (commit `1aa2844`; RED/GREEN, fmt, clippy, domain, DB, jobs e
API verdi; evidenza in `.superpowers/sdd/task-7-report.md`)

Ruling (Task 7 remaining review): il cancel ritira definitivamente il job
attivo tramite la dedup key prima di liberare la regione; il checkpoint
verifica il lease prima di scrivere progresso e `LeaseLost` non applica la
logica dell'ultimo retry alla richiesta successiva. Costo se sbagliato: il
worker ritirato produce un singolo errore `NotFound` quando il pool tenta di
chiuderlo, già innocuo per stato e deduplica.

Ruling (Task 7 remaining review): il recupero al boot resetta immediatamente
solo i `DownloadMapRegion` `running`, prima di avviare i worker; il reaper
generico a 600 secondi resta un job periodico ogni cinque minuti. Costo se
sbagliato: un crash può ripetere byte già scaricati solo secondo il normale
protocollo resume, mentre gli altri job conservano la protezione dai falsi
stale durante il processo vivo.

Ruling (Task 7 remaining review): ogni errore di cleanup lascia la regione
`downloading`; solo la rimozione riuscita consente `status = error`. Costo se
sbagliato: la UI continua a mostrare un download finché un cancel/retry non
riesce a rimuovere il residuo, invece di mostrare un errore non più
cancellabile.

Task 7 remaining review fixes: complete (commit `25b65bc`; RED/GREEN, fmt,
clippy, jobs, DB e API verdi; evidenza in
`.superpowers/sdd/task-7-report.md`).

Minor (Task 7, whole-branch): further cancel/finalize races were fenced with
per-generation file names, `may_finalize_download` before rename,
`mark_error` requiring `NOT cancel_requested`, and
`dedup_key=map-region:{id}:{generation}` (`03803e0`). Residual TOCTOU around
HTTP bodies is a known ceiling; upgrade is a single-owner actor per region.

Task 7: complete (commits 1354046..03803e0, review loops on resume/cancel/
checksum; allowlist and tile 404 in place)

Ruling: il click su un punto mappa richiede la vista completa già consumata da
`AssetViewer`, ma `/api/v1/assets/{id}` aveva solo `DELETE`. Task 8 aggiunge
`GET` sullo stesso path, restituisce l'`AssetView` pubblico esistente e pinna
l'assenza di `location`/`lat`/`lon`; le coordinate restano nel solo endpoint
metadata fino al Task 9. Costo se sbagliato: un'operazione additiva in più nel
contratto v1 e nello snapshot OpenAPI.

Ruling: il riquadro disegnato filtra sia bucket sia pagine timeline, non solo
l'URL. I due endpoint accettano `bbox` opzionale tramite lo stesso parser WGS84
dei cluster; senza bbox conservano le query materializzate preesistenti, con
bbox contano gli asset effettivi e applicano override posizione prima
dell'EXIF. Costo se sbagliato: la variante filtrata interroga `assets` invece
di `folder_month_counts`, limitatamente all'interazione esplicita della mappa.

Deferred (Task 8): il catalogo frontend minimo usa gli host
`build.protomaps.com` imposti dal brief, ma il servizio pubblica archivi planet
datati e non espone gli URL paese/manifest SHA-256 hardcoded dal contratto
attuale; gli URL paese campione restituiscono 404. UI, allowlist e protocollo
sono completi e testati con mock, ma prima del field test servono estratti
PMTiles reali e il relativo manifest. Costo: i download dal catalogo non
completano finché quei metadati non vengono sostituiti.

Task 8: complete (commit `9360d9f`; frontend 74/74, API timeline 12/12,
OpenAPI 6/6, fmt/clippy/build verdi; entry gzip 84.618 byte)

Task 8 review fixes: complete (commits `221545e`, `27475fe`; vitest 80/80,
entry gzip ~85 KB, Apply+Download banner; evidenza in
`.superpowers/sdd/task-8-report.md`)

Task 8: complete (commits 28fc6c4..27475fe, review clean)

Ruling: `AssetView` espone `location` e `place_id` con `skip_serializing_if`;
il geofence sui link pubblici usa la casa del proprietario della libreria via
`ST_DWithin` inclusivo; `hide_metadata` omette date e coordinate, non le zera
con `null`. `GET /api/v1/assets/{id}` autenticato mostra la posizione effettiva
senza geofence. Costo se sbagliato: un payload pubblico vicino a casa rivela
coordinate finché l'owner non configura `/users/me/home`.

Task 9: complete (commit `766312c`, share_geofence 6/6, openapi 6/6, db tests
verdi, fmt/clippy/build verdi)

