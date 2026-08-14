# Task 3 Report: Argon2id Password Hashing

## Summary
Task 3 implemented Argon2id password hashing in the `keeppix-domain` crate using test-driven development (TDD). All 7 new tests pass alongside 10 existing tests (17 total). Code passes Clippy with `-D warnings` and cargo fmt checks.

**Commit SHA:** `db82b8c`

---

## Implementation Steps

### Step 1: Add Dependencies
Executed:
```bash
cargo add argon2 --features std -p keeppix-domain
cargo add rand_core --features getrandom -p keeppix-domain
```

**Result:** 
- `argon2 v0.5.3` added with `std` feature (includes `rand`, `password-hash`, `alloc`)
- The `cargo add rand_core` command with `getrandom` feature failed because rand_core does not expose that feature. However, `getrandom v0.2.17` was automatically pulled as a transitive dependency of argon2, providing the necessary random salt generation capability via `rand_core::OsRng`.

**Updated Cargo.toml:**
```toml
argon2 = { version = "0.5.3", features = ["std"] }
```

### Step 2: Write Failing Tests
Created `crates/keeppix-domain/src/password.rs` with 7 test cases:
1. `password_rejects_short_input()` — verifies minimum length enforcement (< 10 chars fails)
2. `password_accepts_ten_characters()` — verifies exactly 10 chars is accepted
3. `hash_is_verifiable()` — hashes a password and verifies it matches
4. `hash_rejects_wrong_password()` — verifies wrong password fails verification
5. `same_password_produces_different_hashes()` — verifies random salt generation
6. `malformed_hash_returns_false_without_panicking()` — verifies graceful handling of corrupted hashes
7. `hash_is_argon2id_with_owasp_parameters()` — verifies hash format and OWASP parameters

All tests included `#[allow(clippy::unwrap_used)]` per project conventions.

### Step 3: Verify Test Failure (Red Phase)
Initial run confirmed module was not exported from lib.rs, tests did not execute.

### Step 4: Add Error Variant
Added to `crates/keeppix-domain/src/error.rs`:
```rust
#[error("password hashing failed: {0}")]
PasswordHashing(String),
```

This provides the third DomainError variant for password hashing failures (beyond existing InvalidUsername and InvalidPassword).

### Step 5: Implement Password Module
Created complete implementation in `crates/keeppix-domain/src/password.rs`:

**Types:**
- `Password(String)` — private wrapper for plaintext passwords
  - `parse(&str) -> Result<Self, DomainError>` — validates 10-1024 character range
  - `expose(&self) -> &[u8]` — internal method to access bytes
  - `Debug` impl returns `"Password(***)"` to prevent accidental secret leakage

- `PasswordHash(String)` — PHC-encoded hash ready for storage
  - `from_stored(String) -> Self` — creates from stored string
  - `as_str(&self) -> &str` — retrieves hash string

**Functions:**
- `hash_password(&Password) -> Result<PasswordHash, DomainError>` 
  - Generates random salt via `SaltString::generate(&mut OsRng)`
  - Hashes with Argon2id using OWASP parameters
  - Returns PHC-encoded hash string

- `verify_password(&Password, &PasswordHash) -> bool`
  - Returns `false` (never panics) if hash is malformed
  - Handles errors gracefully at two points: hash parsing and verifier construction

**Argon2 Configuration:**
```rust
const ARGON_M_COST: u32 = 19_456;     // OWASP: 19 MiB
const ARGON_T_COST: u32 = 2;          // OWASP: 2 iterations
const ARGON_P_COST: u32 = 1;          // OWASP: parallelism level 1
```

Instantiated with: `Argon2::new(Algorithm::Argon2id, Version::V0x13, params)`

### Step 6: Export from lib.rs
Updated `crates/keeppix-domain/src/lib.rs`:
```rust
pub mod password;
pub use password::{Password, PasswordHash, hash_password, verify_password};
```

### Step 7: Test Green Phase
All 17 tests passed (10 existing + 7 new):
```
running 17 tests
test auth::tests::admin_context_reports_admin ... ok
test auth::tests::plain_user_context_is_not_admin ... ok
test ids::tests::id_roundtrips_through_string ... ok
test ids::tests::ids_are_time_ordered ... ok
test password::tests::hash_is_argon2id_with_owasp_parameters ... ok
test password::tests::hash_is_verifiable ... ok
test password::tests::hash_rejects_wrong_password ... ok
test password::tests::malformed_hash_returns_false_without_panicking ... ok
test password::tests::password_accepts_ten_characters ... ok
test password::tests::password_rejects_short_input ... ok
test password::tests::same_password_produces_different_hashes ... ok
test user::tests::user_is_active_when_disabled_at_is_none ... ok
test user::tests::user_is_inactive_when_disabled_at_is_some ... ok
test user::tests::username_accepts_allowed_punctuation ... ok
test user::tests::username_is_normalised_to_lowercase ... ok
test user::tests::username_rejects_invalid_characters ... ok
test user::tests::username_rejects_too_short ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.65s
```

Note: Hashing tests run slower (~0.6s total) due to Argon2 computational cost (OWASP parameters), but all complete within acceptable bounds.

### Step 8: Verify Linting
Clippy check with `-D warnings`:
```
    Checking keeppix-domain v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.13s
```
**Result:** No warnings or errors.

Formatting check:
```bash
cargo fmt --check
```
**Result:** No formatting issues (after fixing import line wrapping and export ordering).

### Step 9: Commit
Created conventional commit:
```
commit db82b8c
Author: Claude Opus 5
Date:   [automated]

    feat(domain): add argon2id password hashing
    
    Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

---

## Final Verification

### Test Results
```
$ cargo test -p keeppix-domain
running 17 tests
...
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.65s
```

### Clippy Results
```
$ cargo clippy -p keeppix-domain --all-targets -- -D warnings
    Checking keeppix-domain v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.28s
```
✓ Clean — no warnings

### Format Results
```
$ cargo fmt --check
```
✓ Clean — no formatting issues

---

## Decisions & Design Notes

1. **rand_core feature flag:** The brief specified `cargo add rand_core --features getrandom`, but rand_core v0.10.1 does not expose a `getrandom` feature. However, the necessary `OsRng` for random salt generation is available through argon2's transitive dependencies (`argon2` → `password-hash` → `rand_core`), and `getrandom v0.2.17` is available at the root level. No workaround was needed; the code compiles and runs correctly.

2. **verify_password resilience:** The function returns `false` at two error points:
   - If `PasswordHash::new()` fails to parse the stored hash
   - If `argon2()` instantiation fails
   
   This ensures a corrupted record safely denies access without crashing the login path.

3. **Password Debug impl:** Implemented to return `"Password(***)"` to prevent accidental leakage in logs or error messages. The plain text secret is never exposed.

4. **Test attribute pattern:** Followed existing project conventions by applying `#[allow(clippy::unwrap_used)]` to each test function that uses `.unwrap()`, rather than module-level suppression. This maintains explicit awareness of unwrap usage.

5. **Formatting adjustments:** Rust's formatter required:
   - Single-line import for the multi-item `password_hash` use
   - Alphabetical ordering of re-exports within each `pub use password::` block

---

## Files Modified

- **Created:** `crates/keeppix-domain/src/password.rs` (196 lines)
- **Modified:** `crates/keeppix-domain/src/error.rs` (added PasswordHashing variant)
- **Modified:** `crates/keeppix-domain/src/lib.rs` (added module and re-exports)
- **Modified:** `crates/keeppix-domain/Cargo.toml` (added argon2 dependency)

---

## Concerns

None. All requirements met:
- ✓ TDD: Tests written first, all passing
- ✓ Argon2id with exact OWASP parameters (m=19456, t=2, p=1)
- ✓ verify_password returns false on corrupted hash, never panics
- ✓ Password Debug impl hides secret
- ✓ 17 tests passing (10 existing + 7 new)
- ✓ Clippy clean with -D warnings
- ✓ Code formatted correctly
- ✓ Conventional commit created
