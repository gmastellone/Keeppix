# Task 10 — «Preferito» — Report

## Comportamento implementato

1. Schema: nessuna nuova migrazione. `asset_flags.favorite` e l'indice
   parziale `asset_flags_favorite_idx (user_id, asset_id) WHERE favorite`
   erano già nella migrazione 0037 del Task 6 — verificato, non assunto.
2. `favorite: bool` in `AssetFlags` (dominio), `AssetFlagsBody`, scrittura
   singola (`PUT /assets/{id}/flags`) e di massa (`POST /flags/batch`, con
   l'involucro `BulkOutcome` del Task 1). Stesso contratto di rimpiazzo
   completo già in vigore per `rating`/`pick`/`color_label`.
3. `AssetView.favorite` additivo, per-chiamante. Risolto in `GET
   /assets/{id}` (via `FlagRepo::get`) e nelle due pagine di browse (`GET
   /timeline`, `POST /search`) via il nuovo `FlagRepo::favorites_among`
   (una query per pagina, non N). Gli altri consumatori di `AssetView`
   (cartelle, duplicati, album, stack, share pubblico) restano a `false`
   di default — non sono fra i sette punti della spec, e share pubblico
   non ha nemmeno un utente per cui "preferito" abbia senso.
4. Indipendenza favorite/pick verificata con un test dedicato: scartare
   nel culling non azzera il preferito, e viceversa. `EXPLAIN` sull'indice
   parziale era già coperto dal Task 6 (`favorite_search_uses_the_partial_index`,
   `favorite_filter_is_per_user_not_per_asset`) — riverificati verdi senza
   modifiche.

## Ruling principali (dettagliati nel ledger)

- Nessuna nuova migrazione: schema già pronto dal Task 6.
- `favorite` segue il rimpiazzo completo esistente, non un merge parziale
  tri-stato: un `PUT`/batch senza `favorite` lo azzera, come già succede
  per `rating`. Costo noto e documentato, non nuovo di questo task.
- `AssetView.favorite` risolto solo per timeline/ricerca (le due superfici
  che il brief chiede di documentare); altri consumatori restano a
  `false`, deferito nel ledger.

## Test

```
keeppix-domain flags        5/5  (+1: favorite default false)
keeppix-db      flags.rs   11/11 (+2: indipendenza, favorites_among isola per utente)
keeppix-db      search.rs  19/19 (invariato — riverifica Task 6)
keeppix-db      migrations 11/11, fase2_culling_1k 4/4
keeppix-jobs    xmp.rs      5/5
keeppix-api     flags.rs    6/6  (+1: discard in culling non azzera favorite)
keeppix-api     timeline.rs 22/22 (+3: GET singolo, pagina, default false)
keeppix-api     search.rs   4/4  (+2: risoluzione pagina, chip Preferiti)
keeppix-api     albums/duplicates/stacks/share_*  invariati, nessuna regressione
keeppix-api     openapi.rs  7/7  (snapshot rigenerato, 2 campi additivi)
```

`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` verdi. `./scripts/test.sh` completo non eseguito (stesso motivo
dei task precedenti).

## Non fatto (fuori scope)

- `AssetView.favorite` non risolto per folders/duplicates/albums/stacks/
  share pubblico (default `false`), deferito nel ledger.
- Nessun merge parziale tri-stato per `favorite`/`rating`/`color_label`.
- Task 11+ non iniziato.
