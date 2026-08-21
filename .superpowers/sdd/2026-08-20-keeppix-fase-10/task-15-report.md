# Task 15 — Report

Brief assente in cartella SDD; requisiti letti da `plans/2026-08-20-keeppix-
fase-10.md` (sez. Task 15) e dallo spec UI (§29, §61).

## Consegnato

1. **`GET /api/v1/shared-with-me`** — `PermissionRepo::list_shared_with_me`
   aggrega le righe di `permissions` (diretta o via gruppo) su
   folder/album per l'utente corrente: nome, tipo, `item_count` (batched
   `GROUP BY`, riusa i counter di `share_links`), owner, ruolo effettivo
   (max tra origini), `via_group` solo se l'accesso è *puramente* di
   gruppo. L'admin non compare per visibilità implicita: solo grant
   espliciti.
2. **`item_count` su `GET /share/links`** — batched `GROUP BY` per link
   folder/album (solo asset `indexed`), 1 per link asset. Ha richiesto di
   aggiornare (non rimuovere) l'assert del Task 11 in `sidebar_load.rs`.
3. **`UserView` additivo**: `server_name` (config `KEEPPIX_SERVER_NAME`,
   default `"Keeppix"`) e `password_changed_at` (colonna nuova, backfill a
   `created_at`, aggiornata a ogni cambio password).

## Verifica

`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` verdi. `./scripts/test.sh` completo non eseguito (costo tempo);
eseguiti a mano tutti i test dei crate/file toccati, tutti verdi:
`keeppix-db` users.rs 15/15, share_links.rs 7/7, permissions.rs 22/22,
migrations.rs 11/11 (+1 ignored preesistente); `keeppix-server` config.rs
8/8; `keeppix-api` auth.rs 28/28, shared_with_me.rs 3/3 (nuovo),
sidebar_load.rs 1/1, permissions_roles.rs 2/2, share_link_channels.rs 3/3,
openapi.rs 7/7 (snapshot rigenerato), users.rs 9/9.

Tre mutazioni osservate rosse e ripristinate: route assente → 404 sui test
`shared_with_me.rs`; logica "diretto vince su group-only" rimossa →
`via_group` sempre popolato; (sessione precedente) update
`password_changed_at` rimosso → test bump fallisce.

## Commit

- `ac79bab` feat(db): track password_changed_at on users
- `f00eac2` feat(api): surface server_name and password_changed_at on UserView
- `888949e` feat(api): add item_count to GET /share/links
- `3c6fb28` feat(api): add GET /api/v1/shared-with-me (Task 15)
- `be0b1e7` docs(sdd): Task 15 ledger entry

Ledger aggiornato in `progress.md`. Nessun push. Task 16 non iniziato.
