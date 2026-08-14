# Task 3 review: Tipi di dominio per librerie, cartelle e asset

**Base:** `5ad848f`  
**Head:** `0564f66` `feat(domain): add library, folder and asset types`  
**Verdict:** Approved

## Spec Compliance

**Compliant.** The diff produces the brief’s types, fields, enums, constructors, error variants, and re-exports. Controller ruling F3 is followed: `AssetStatus` is `Discovered | Indexed | Offline | Error | Trashed`, `LocationSource` is the plan enum, and `Asset` has no spec-§4.1 `active` / camera columns.

Checked against the produce list:

| Required | In diff |
|---|---|
| `LibraryId`, `FolderId`, `AssetId` via existing `id_type!` (UUID v7) | Yes — appended next to `UserId`/`GroupId` |
| `Library` fields exactly as listed, `LibraryStatus::{Active, Offline}`, `NewLibrary` | Yes |
| `Folder { id, library_id, parent_id, name, path: FolderPath, depth }` | Yes |
| `FolderPath::{root, child, as_str, depth, parse}` | Yes; also `is_descendant_of` required by the brief’s tests |
| `Asset` fields as listed, `AssetKind`, `AssetStatus`, `LocationSource`, `NewAsset` | Yes; `filename` is `AssetName` as in Step 6 |
| `DomainError::{InvalidFolderPath, InvalidAssetName}` added; existing variants kept | Yes |
| Pure domain: no I/O, no SQL, `keeppix-db` untouched | Yes — `PathBuf` is a value, not I/O |
| No `unwrap`/`expect` in production; test `unwrap` behind localized `#[allow]` | Yes (`folder.rs` tests only) |
| Conventional commit in English | Yes — matches the brief’s message |

`lib.rs` keeps the existing public surface (`GroupId`, `UserId`, auth/password/user types) and adds the brief’s exports in one merged `ids` re-export. That is the right merge, not extra scope.

## Strengths

- Implementation is the brief’s Step 3–7 code, not a parallel design. Names, variants, and field sets match.
- `FolderPath` keeps folder names out of the path; `parse` rejects empty / non-numeric labels / malformed separators; `is_descendant_of` uses the `ltree <@` self-inclusive rule and the `"{other}."` prefix form, so `"11"` is not a descendant of `"1"`.
- `AssetName` is the actual filename type (not a bare `String`) and rejects `/`, `\`, NUL, `.`, `..`, and empty.
- Existing `id_type!` is reused; no second ID abstraction.
- Nine planned tests are present (6 `FolderPath` + 3 `AssetName`). Asset tests do not use `unwrap`, so they correctly omit the clippy allow.

## Issues

### Critical

None.

### Important

None.

### Minor

1. **`FolderPath::root` / `child` can emit values `parse` rejects** — `crates/keeppix-domain/src/folder.rs` (`root`/`child`). `seq` is `i64`; `root(-1)` is `"-1"`, which `parse` rejects because `-` is not an ASCII digit. **Plan-mandated** (brief uses `i64` and digit-only `parse`). Real sequences from a DB will be non-negative; not a gate failure.

2. **`AssetName` NUL rejection is untested.** `parse` rejects `'\0'` (documented in `# Errors`); the three brief tests do not cover it. Residual test gap only.

## Assessment

**Approved.**

The task is spec-complete and well-built for a pure-types slice. Residual risks that are **not** blockers: `#[serde(transparent)]` on `FolderPath` and `AssetName` deserializes the inner `String` without `parse` — same crate convention as existing `Username`, and **plan-mandated**. `LocationSource` is exported but not a field on `Asset`, which matches the brief’s `Asset` shape (the enum is for later phases, per F3).
