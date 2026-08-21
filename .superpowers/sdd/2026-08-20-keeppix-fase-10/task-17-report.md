# Task 17 — GET /bootstrap — Report

Branch: `fase-10`. Nessun push. Task 18+ non toccati.

## Consegnato

`GET /api/v1/bootstrap` (additivo) restituisce in un colpo:

- `user` — stesso payload di `GET /auth/me`
- `preferences` — stesso payload di `GET /users/me/preferences`
- `folders` — stesso payload di `GET /folders/tree` (`?roots=true` opzionale)
- `storage` — mappa `{library_id → {free_bytes, total_bytes}}` per ogni libreria
  visibile (stesso payload di `GET /libraries/{id}/storage`)
- `badges` — `{culling: 0, revision: 0}` finché Fasi 7/8/9 non espongono i
  singoli contatori

`routes/bootstrap.rs::compose` riusa gli stessi repository degli handler
singoli; nessun SQL proprio.

## Verifica

TDD: test scritti prima dell'implementazione (fallimento su route assente).

- `bootstrap_matches_individual_endpoints` — parità campo per campo con i
  quattro endpoint singoli.
- `bootstrap_emits_no_more_queries_than_individual_repos` — conta le query
  sqlx (stessa sequenza repo dei singoli, incluso `list` + `storage` per
  libreria) e asserisce `bootstrap ≤ somma`.

Mutazione osservata rossa: route rimossa → 404 su `bootstrap_matches`.

`cargo fmt --check`, `cargo clippy -p keeppix-api --all-targets -- -D warnings`,
`cargo test -p keeppix-api --test bootstrap` verdi. `./scripts/test.sh` completo
non eseguito (stesso motivo dei task precedenti).

Ledger aggiornato in `progress.md`.
