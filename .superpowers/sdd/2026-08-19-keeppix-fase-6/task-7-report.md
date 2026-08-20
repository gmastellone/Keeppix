# Task 7 — Idempotency-Key

## Summary

Implemented Task 7 on `fase-6` with:

- DB migration `0029_idempotency_keys.sql`
- new `keeppix-db` repository in `src/idempotency.rs`
- new API middleware in `crates/keeppix-api/src/idempotency.rs`
- router wiring so the middleware runs both in tests and in the embedded server

The middleware now:

- replays the cached response for the same authenticated user, same key, same
  request fingerprint
- returns `409 keeppix/idempotency-key-conflict` when the same key is reused for
  a different request fingerprint
- stores the response status, JSON body, and `Set-Cookie` header in the JSON
  envelope persisted under `response_body`

## Compatibility and scope

- `Idempotency-Key` is accepted immediately but remains optional for existing
  callers; requests without the header keep the pre-existing behavior.
- The middleware handles authenticated mutating requests with an empty body or
  `application/json` body.
- Streaming upload bodies are left on their protocol-native path instead of
  being buffered in middleware.

## Tests added or exercised

Added:

- `crates/keeppix-api/tests/idempotency.rs`
  - `replaying_the_same_mutation_key_and_body_returns_the_cached_response`
  - `reusing_the_same_mutation_key_with_a_different_body_is_a_conflict`

Exercised:

- `cargo test -p keeppix-api --test idempotency`
- `cargo test -p keeppix-api --test auth refresh`
- `cargo test -p keeppix-db --test migrations`
- `cargo clippy -p keeppix-db -p keeppix-api -p keeppix-server --all-targets -- -D warnings`
- `cargo fmt --check`

## Notes

- The plan's static migration numbers for future tasks must now shift to follow
  the real execution order on `fase-6`.
- The envelope inside `response_body` is a deliberate compromise to keep the
  SQL table shape frozen while still detecting request mismatches and replaying
  `Set-Cookie` for `/auth/refresh`.
