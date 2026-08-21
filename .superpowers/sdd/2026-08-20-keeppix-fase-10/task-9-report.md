# Task 9 — User preferences — Report

**Branch:** `fase-10`  
**Commits:** `938f469` (db), `52fa867` (api)  
**Status:** complete, tests green, no push

## Delivered

- Migration `0039_user_preferences.sql`: `users.preferences jsonb NOT NULL DEFAULT '{}'`
- `PreferencesRepo`: `get` / `set`, merge via `UserPreferences::apply_patch`
- `GET` / `PATCH /api/v1/users/me/preferences` (session auth)

## Document shape

```json
{
  "theme": "chiaro|scuro|sistema",
  "grid_density": { "desktop": 2-12, "mobile": 2-6 },
  "notifications": { "digest": bool, "condivisioni": bool, "problemi": bool },
  "language": "it|en"
}
```

Defaults match UI mockup: chiaro, desktop 4 / mobile 3, all notifications true, `it`.

## Verification (TDD)

| Test | Result |
|------|--------|
| `keeppix-db` preferences (defaults, partial merge) | 2/2 |
| `keeppix-api` preferences (GET defaults, PATCH merge, unknown→400, 401) | 4/4 |
| `keeppix-api` openapi | 7/7 (89 ops, snapshot updated) |
| `keeppix-db` migrations | 11/11 |
| `cargo fmt --check`, clippy on touched crates | green |

## Rulings

- One jsonb column per user (spec §8.3 ruling)
- Notification keys kept as prototype Italian identifiers
- Unknown fields → `400 keeppix/unknown-field`, not silent accept

## Not done

- Task 10+ (favorite flag, bootstrap, etc.)
- `./scripts/test.sh` full workspace run
- Frontend consumer / `wired-exceptions` (Fase 11)
