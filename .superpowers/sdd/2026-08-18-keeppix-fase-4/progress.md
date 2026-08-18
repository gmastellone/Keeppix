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

