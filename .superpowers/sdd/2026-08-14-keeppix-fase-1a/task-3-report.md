# Task 3 report: Tipi di dominio per librerie, cartelle e asset

**Status:** DONE  
**Branch:** `fase-1`  
**Commit:** `0564f66` `feat(domain): add library, folder and asset types`

## What I implemented

Pure domain types in `keeppix-domain`. No I/O, no SQL, `keeppix-db` untouched.

- IDs via existing `id_type!`: `LibraryId`, `FolderId`, `AssetId` (UUID v7), same as `UserId`.
- `FolderPath` newtype (`ltree` numeric labels): `root`, `child`, `parse`, `as_str`, `depth`, `is_descendant_of` (self included, same as `<@`).
- `Folder { id, library_id, parent_id, name, path, depth }`.
- `Library`, `LibraryStatus::{Active, Offline}`, `NewLibrary`.
- `Asset`, `AssetKind::{Image, RawImage, Video, Unknown}`, `AssetStatus::{Discovered, Indexed, Offline, Error, Trashed}`, `LocationSource::{Exif, User, MapPin, Copied, Gpx}`, `NewAsset`, `AssetName`.
- `DomainError::{InvalidFolderPath, InvalidAssetName}` added; existing username/password variants kept.
- `lib.rs` re-exports the brief list **in addition to** existing ones.

**F3:** `AssetStatus` as above. Camera fields are not on `Asset`.

## What you tested and test results

### Baseline (before any Task 3 code)

```
cargo test -p keeppix-domain -- --list
```

**22 tests** (unittests `src/lib.rs`). Matches the brief.

### Step 2 / TDD RED (FolderPath tests only, type not yet defined)

```
cargo test -p keeppix-domain folder
```

FAIL as expected — compile error, not a runtime fail:

```
error[E0433]: failed to resolve: use of undeclared type `FolderPath`
 --> crates/keeppix-domain/src/folder.rs:8:20
  |
8 |         let root = FolderPath::root(1);
  |                    ^^^^^^^^^^ use of undeclared type `FolderPath`

error: could not compile `keeppix-domain` (lib test) due to 10 previous errors
```

### Step 8 / TDD GREEN

```
cargo test -p keeppix-domain
```

PASS: **31 passed; 0 failed** (22 existing + 9 new).

New tests (all present):

| File | Tests |
|---|---|
| `folder.rs` | 6 (`root_path_is_a_single_label`, `children_extend_the_parent`, `parsing_accepts_a_numeric_path`, `parsing_rejects_non_numeric_labels`, `parsing_rejects_malformed_separators`, `a_path_is_its_own_ancestor_check`) |
| `asset.rs` | 3 (`asset_name_accepts_ordinary_filenames`, `asset_name_rejects_path_separators`, `asset_name_rejects_dot_entries_and_empty`) |

```
cargo clippy -p keeppix-domain --all-targets -- -D warnings
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

All PASS. `frontend/dist` was missing; built the frontend first (`npm run build` in `frontend/`). Dist is gitignored and was not committed.

## TDD Evidence

- **RED:** `cargo test -p keeppix-domain folder` → compile FAIL, `use of undeclared type FolderPath` (10 errors).
- **GREEN:** `cargo test -p keeppix-domain` → **31 passed** (baseline **22** → final **31**).

## Files changed

- Create: `crates/keeppix-domain/src/folder.rs`
- Create: `crates/keeppix-domain/src/library.rs`
- Create: `crates/keeppix-domain/src/asset.rs`
- Modify: `crates/keeppix-domain/src/ids.rs`
- Modify: `crates/keeppix-domain/src/error.rs`
- Modify: `crates/keeppix-domain/src/lib.rs`

Domain crate only. `.superpowers/` not committed.

## Controller rulings applied

- **F3:** `AssetStatus::{Discovered, Indexed, Offline, Error, Trashed}` and `LocationSource` as in the brief. No camera fields on `Asset`.
- No SQL, filesystem I/O, or repository code. `keeppix-db` not modified.
- `id_type!` reused for the three new IDs.
- `DomainError` variants **added**, existing ones kept.
- `lib.rs` re-exports include the brief list plus existing `Actor`, `AuthContext`, `DomainError`, `GroupId`, `UserId`, password/token/user types.

## Self-review findings

- Names, variants, and field sets match the brief verbatim (`Library`, `NewLibrary`, `Folder`, `FolderPath`, `Asset`, `NewAsset`, enums).
- `FolderPath::parse` rejects empty, non-numeric labels, and malformed separators (`""`, `"1..7"`, `".1"`, `"1."`).
- `is_descendant_of` treats a path as a descendant of itself (`ltree <@`).
- `AssetName::parse` rejects `/`, `\`, NUL, `.`, `..`, and empty; accepts spaces and unicode filenames.
- `Asset.filename` is `AssetName`, not `String`.
- `content_hash` is `Option<[u8; 32]>`; camera/EXIF live off `Asset` (Task 6).
- Re-export of IDs is one merged `pub use ids::{AssetId, FolderId, GroupId, LibraryId, UserId}` so existing `GroupId`/`UserId` remain public.

## Issues or concerns

None blocking.
