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

