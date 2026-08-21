# Task 13 — Problemi composti, non materia prima

## Cosa è stato fatto

- `keeppix-db`: `ProblemsRepo::composed`/`compose` compongono i tre secchi
  grezzi di `list` in un `Vec<ComposedProblem>` (id, gravità, titolo,
  descrizione in linguaggio naturale, libreria/cartella, azioni con
  etichetta), in italiano o inglese via `ProblemLanguage`. Due nature reali:
  libreria offline ("Riprova connessione" + "Dettagli") e sidecar XMP non
  scrivibile (`WriteSidecar` fallito per `PermissionDenied`, riconosciuto dal
  marcatore stabile `permission-denied:` in `last_error` — "Vedi N file..." +
  "Ignora"). Ogni altro fallimento resta un problema generico.
- `keeppix-jobs`: `xmp::write_one` tagga un `io::ErrorKind::PermissionDenied`
  con quel marcatore prima di propagarlo come `JobError::Worker`, così
  `keeppix-db` non deve indovinarlo dal testo libero di `io::Error` (che
  cambia formulazione a seconda della piattaforma).
- `keeppix-db::LibraryRepo::probe`: verifica `root_path.is_dir()` e aggiorna
  lo stato (Active/Offline); è l'azione dietro "Riprova connessione".
- `keeppix-api`: nuovo `POST /libraries/{id}/probe`; `GET /problems` resta
  additivo (contratto congelato) e aggiunge il campo `problems` con l'elenco
  composto — lingua da `?lang=`, poi `Accept-Language`, default italiano.
  OpenAPI aggiornata e snapshot rigenerata.

## Verifica chmod

`keeppix-jobs/tests/xmp.rs::a_readonly_folder_surfaces_as_a_composed_sidecar_problem`
rende una cartella `chmod 0o555`, fa fallire realisticamente il job
`WriteSidecar`, poi verifica via `ProblemsRepo::composed` gravità `Warning`,
testo con permessi/cartella corretti e azioni `view-files`/`ignore`.

## Test

TDD per ogni pezzo (RED osservato, poi verde). Suite complete a mano (crate
per crate, `--jobs 1 --test-threads=1`, non `./scripts/test.sh` per tempo):
`keeppix-db`, `keeppix-jobs`, `keeppix-api` (45 file di test), `keeppix-server`
tutti 100% verdi. `cargo fmt --check` e `cargo clippy --workspace
--all-targets -- -D warnings` verdi.

## Non fatto (fuori scope di questo task)

Frontend (`ProblemsView.vue`) non toccato: il piano affida il consumo della
UI a un task successivo; l'endpoint resta additivo apposta.

## Ledger

Rulings in `.superpowers/sdd/2026-08-20-keeppix-fase-10/progress.md`
(sezione "Task 13").

## Commit

`7af00e7`, `870b4f7`, `f01daea`, `e4949de`, `24ff805` su
`cursor/task-13-composed-problems-30e9` (branch da `fase-10`). Non pushato.
