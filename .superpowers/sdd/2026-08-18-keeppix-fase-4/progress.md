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


