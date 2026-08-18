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

