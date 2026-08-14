# Task 2 Report: Domain Types

## Summary
Successfully implemented pure domain types for Keeppix in the `keeppix-domain` crate. All 8 tests passing, clippy clean, formatting compliant. Commit: `f29394bcf5011ca060d81db54900d29d4dcfb885`

## Steps Executed

### Step 1: Add Dependencies
Modified `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/Cargo.toml` to add:
- `serde.workspace = true` (for serialization)
- `uuid.workspace = true` (for ID generation with v7)
- `chrono.workspace = true` (for DateTime<Utc>)

Note: `thiserror.workspace = true` was already present.

### Step 2: Create user.rs with Tests
Created `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/src/user.rs` with:
- `Username` type with `parse()` method that normalizes to lowercase and validates:
  - Length: 3-32 characters
  - Allowed chars: a-z, 0-9, dot, underscore, hyphen
- `SystemRole` enum with `Admin` and `User` variants and `is_admin()` method
- `User` struct with all required fields (id, username, email, display_name, role, locale, created_at, disabled_at)
- `User::is_active()` const method
- `NewUser` struct for creation
- 4 tests for Username validation and normalization

### Step 3: Test Failure Verification
Tests were not yet executable at this point as modules were not yet included in lib.rs.

### Step 4: Create error.rs
Created `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/src/error.rs` with:
- `DomainError` enum with `InvalidUsername` and `InvalidPassword` variants
- Derived: `Debug`, `Error`, `PartialEq`, `Eq`
- Error messages via `thiserror` derive

### Step 5: Create ids.rs
Created `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/src/ids.rs` with:
- Macro `id_type!` for generating ID wrappers
- `UserId` and `GroupId` types wrapping `Uuid`
- Methods: `new()`, `from_uuid()`, `as_uuid()`, `Default`, `Display`, `FromStr`
- Derived: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`
- 2 tests: time ordering (UUID v7) and string roundtrip

### Step 6: User Implementation
Already completed in Step 2 with full Username parsing implementation.

### Step 7: Create auth.rs
Created `/Users/giomanstellone/Documents/GitHub/Keeppix/crates/keeppix-domain/src/auth.rs` with:
- `Actor` enum with single variant `User { id: UserId, role: SystemRole }`
  - ShareLink variant intentionally omitted (arrives in Phase 3)
- `AuthContext` struct wrapping `Actor`
- `AuthContext::user()` const factory method
- `AuthContext::user_id()` const method returning `Option<UserId>`
  - Note: Returns Option to support ShareLink variant arrival; suppressed unnecessary_wraps lint
- `AuthContext::is_admin()` const method
- 2 tests: admin and regular user contexts

### Step 8: Create lib.rs
Created `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/src/lib.rs` with:
- Module declarations for `auth`, `error`, `ids`, `user`
- Public re-exports for: `Actor`, `AuthContext`, `DomainError`, `GroupId`, `UserId`, `NewUser`, `SystemRole`, `User`, `Username`

### Step 9: Test Verification (RED → GREEN)
**Initial run (Step 3 - expecting failure):** Not executed due to module inclusion being done later.

**Final run (Step 9):**
```
cargo test -p keeppix-domain
   Compiling keeppix-domain v0.1.0
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.25s
     Running unittests src/lib.rs

running 8 tests
test auth::tests::admin_context_reports_admin ... ok
test auth::tests::plain_user_context_is_not_admin ... ok
test ids::tests::ids_are_time_ordered ... ok
test ids::tests::id_roundtrips_through_string ... ok
test user::tests::username_rejects_too_short ... ok
test user::tests::username_rejects_invalid_characters ... ok
test user::tests::username_is_normalised_to_lowercase ... ok
test user::tests::username_accepts_allowed_punctuation ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Step 10: Clippy Verification
Encountered 3 clippy warnings during first check:
1. **`unnecessary_wraps`** in `auth.rs::user_id()` - Returned `Option<UserId>` always wrapped `Some`, but this is intentional to support future ShareLink variant. Added `#[allow(clippy::unnecessary_wraps)]`
2. **`unwrap_used`** in `ids.rs::tests::id_roundtrips_through_string()` - Added `#[allow(clippy::unwrap_used)]` to test method
3. **`unwrap_used`** in `user.rs::tests::username_is_normalised_to_lowercase()` - Added `#[allow(clippy::unwrap_used)]` to test method

Final run:
```
cargo clippy -p keeppix-domain --all-targets -- -D warnings
    Checking keeppix-domain v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s
```
✓ No warnings

### Format Verification
Initial format check found formatting issue in `auth.rs::user()`. Ran `cargo fmt` to fix.

Final check:
```
cargo fmt --check
(no output - formatting compliant)
```

### Step 11: Commit
```bash
git add crates/keeppix-domain
git commit -m "feat(domain): add user, id and auth context types"
```

**Commit SHA:** `f29394bcf5011ca060d81db54900d29d4dcfb885`

Files committed:
- `crates/keeppix-domain/Cargo.toml` - Added 3 dependencies
- `crates/keeppix-domain/src/auth.rs` - 56 lines
- `crates/keeppix-domain/src/error.rs` - 9 lines
- `crates/keeppix-domain/src/ids.rs` - 71 lines
- `crates/keeppix-domain/src/lib.rs` - 10 lines
- `crates/keeppix-domain/src/user.rs` - 117 lines

**Total:** 266 insertions in 6 files

## Decisions

1. **Optional wraps in `user_id()`**: Kept `Option<UserId>` return type as specified in brief, even though all Actor variants in this task always yield Some. This is future-proofing for ShareLink variant (Phase 3). Used `#[allow(clippy::unnecessary_wraps)]` to suppress the pedantic lint warning.

2. **Allow attributes in tests**: Used `#[allow(clippy::unwrap_used)]` for test methods that call `.unwrap()` on known-infallible operations, following standard Rust test patterns where assertions are acceptable.

3. **Username validation**: Implemented exactly as specified - lowercase normalization, 3-32 character bounds, and character set validation for `[a-z0-9._-]`.

4. **ID macro approach**: Used declarative macro to avoid code duplication for `UserId` and `GroupId` with consistent implementations.

5. **Const methods**: Kept `AuthContext::user()`, `user_id()`, and `is_admin()` as `const` as shown in brief, using match expressions that compile in const context.

## Concerns

None. All requirements met, all tests passing, all lints clean, formatting compliant.

## Files Modified

### Created:
- `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/src/auth.rs`
- `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/src/error.rs`
- `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/src/ids.rs`
- `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/src/user.rs`

### Modified:
- `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/Cargo.toml`
- `/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain/src/lib.rs`

## Verification Commands & Output

### Test Command
```bash
$ cargo test -p keeppix-domain
```
**Result:** ✓ 8 passed, 0 failed

### Clippy Command
```bash
$ cargo clippy -p keeppix-domain --all-targets -- -D warnings
```
**Result:** ✓ No warnings

### Format Check
```bash
$ cargo fmt --check
```
**Result:** ✓ Compliant

---

## Fix Round 1: Add Test Coverage for User::is_active()

### Issue
Code review identified that `User::is_active()` had zero test coverage. This method is bespoke logic (not macro-generated) and is named in the interface contract consumed by later tasks (Task 7 and Task 10). Both branches of the `disabled_at.is_none()` logic needed to be exercised.

### What Changed
Added two new test functions to `crates/keeppix-domain/src/user.rs` in the tests module:

1. **`user_is_active_when_disabled_at_is_none()`** - Verifies that `User::is_active()` returns `true` when `disabled_at: None`
   - Builds a complete `User` struct directly in the test
   - Uses `UserId::new()`, `Username::parse()`, `SystemRole::User`, and `Utc::now()`
   - Asserts `user.is_active()` is `true`

2. **`user_is_inactive_when_disabled_at_is_some()`** - Verifies that `User::is_active()` returns `false` when `disabled_at: Some(DateTime)`
   - Builds a complete `User` struct with `disabled_at: Some(Utc::now())`
   - Asserts `user.is_active()` is `false`

Both tests include `#[allow(clippy::unwrap_used)]` attributes for the `Username::parse().unwrap()` calls.

### Test Coverage
- File: `crates/keeppix-domain/src/user.rs`
- Test names:
  - `user::tests::user_is_active_when_disabled_at_is_none`
  - `user::tests::user_is_inactive_when_disabled_at_is_some`

### Verification

**Test Command:**
```bash
$ cargo test -p keeppix-domain
   Compiling keeppix-domain v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.62s
     Running unittests src/lib.rs (target/debug/deps/keeppix_domain-5da10da866182ed8)

running 10 tests
test auth::tests::plain_user_context_is_not_admin ... ok
test auth::tests::admin_context_reports_admin ... ok
test user::tests::username_accepts_allowed_punctuation ... ok
test user::tests::user_is_inactive_when_disabled_at_is_some ... ok
test ids::tests::id_roundtrips_through_string ... ok
test user::tests::user_is_active_when_disabled_at_is_none ... ok
test user::tests::username_is_normalised_to_lowercase ... ok
test ids::tests::ids_are_time_ordered ... ok
test user::tests::username_rejects_invalid_characters ... ok
test user::tests::username_rejects_too_short ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests keeppix_domain

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**Clippy Command:**
```bash
$ cargo clippy -p keeppix-domain --all-targets -- -D warnings
    Checking keeppix-domain v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
```

### Commit
```bash
git add crates/keeppix-domain
git commit -m "test(domain): add coverage for User::is_active() method"
```

**Commit SHA:** `c0382a0` (full: `c0382a05ca9f34ab95e1f3eab847fb96a5ca4c19`)

**Result:** ✓ 10 tests passed (8 original + 2 new), no warnings, formatting compliant
