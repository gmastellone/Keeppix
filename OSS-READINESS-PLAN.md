# Keeppix OSS Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Keeppix repository presentable to an English-speaking public GitHub audience — clean branch model, a contributor guide instead of a phase-by-phase internal ledger, attractive READMEs, and a codebase whose comments read as intentional documentation rather than development-diary entries in Italian.

**Architecture:** Two independent phases. Phase A (structure and docs, Tasks 1-7) is fully specified below and safe to execute in one sitting: it touches branches, top-level docs, and READMEs — no application code. Phase B (codebase comment cleanup + translation, Tasks 8-10) is a large, mechanical, repeatable sweep across ~350 source files (226 Rust, 124 frontend) — Tasks 8/9 define the *procedure and verification gate*, not a pre-written diff per file, because the content of each file's comments isn't known until it's read. Phase B is designed to run as a series of small, independently-committed batches (one commit per crate / per frontend directory), so it can be paused and resumed across sessions without ever leaving the tree in a half-translated state within a single commit.

**Tech Stack:** No new dependencies. Git, Markdown, and the existing Rust/Vue toolchain (`cargo fmt`/`clippy`, `npm run build`) purely as verification after each batch.

**Spec:** This plan *is* the spec — it was scoped directly from the user's request, not a separate design doc. Original ask, verbatim intent: clean the repo to `dev`/`test`/`main` only; keep the superpowers-style working method but strip phase ledgers down to what helps external contributors; remove unnecessary comments and cruft from the code; rewrite the READMEs to be attractive technically, architecturally, and as a pitch; everything customer/contributor-facing on GitHub in English; when describing future work, say "website + docs" and "desktop and mobile clients" — never name specific tech (no Tauri, no Capacitor).

## Global Constraints

- All new and rewritten prose (READMEs, CONTRIBUTING, code comments) is in **English**. Commit messages are already English-only per existing convention — keep that.
- Never name a specific future-client technology (no "Tauri", no "Capacitor", no "Electron"). Say "a website with documentation" and "native desktop and mobile clients" instead.
- The phase-by-phase plans, specs, and ledgers are removed from the working tree entirely, not archived under a `docs/archive/` directory — the user explicitly doesn't want them kept around, even out of the way. `git log`/`git show` still has every one of them if ever needed; deleting from the working tree never deletes from history.
- What *does* survive, generalized: the superpowers working method itself (TDD discipline, verification-before-done, the invariants, the decision-writing habit) — as a framework for how contributors and their AI agents approach *future* work on Keeppix, not as a record of past phases. That's what `CONTRIBUTING.md` (Task 2) is for.
- Every commit must leave `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `npm run build` (frontend) green. Comment-only changes should never need this, but Phase B batches are exactly the kind of large mechanical edit where a stray syntax slip is easy to miss.
- No behavior change anywhere in this plan. If a Phase B pass reveals a real bug hiding in a comment's claim (e.g., a comment describing behavior the code no longer has), fix the comment to match the code — do not change the code to match a stale comment without flagging it separately.

---

## Phase A — Structure, contributor guide, and READMEs

### Task 1: Branch cleanup — only `dev`, `test`, `main` remain

**Files:** none (git refs only).

- [ ] **Step 1: Delete stale local phase branches**

These are leftover local refs from branches already deleted on `origin` earlier this session (`fase-0`, `fase-1`, `fase-2`, `fase-2r`, `fase-2r2`, `fase-2r3`, `fase-3`, `fase-4`, `fase-5`, `fase-6`, `fase-11`, `fase-11-geometry-pagination`, `cursor/ci-node24-actions-919e`, `cursor/cloud-agent-1786871000704-642yp`). Confirm each is fully merged into `main` before deleting (they all are — verified earlier this session), then delete:

```bash
git branch -D fase-0 fase-1 fase-2 fase-2r fase-2r2 fase-2r3 fase-3 fase-4 fase-5 fase-6 fase-11 fase-11-geometry-pagination cursor/ci-node24-actions-919e cursor/cloud-agent-1786871000704-642yp
```

- [ ] **Step 2: Create `dev` and `test` from current `main`**

```bash
git checkout main
git pull origin main
git checkout -b dev
git push -u origin dev
git checkout -b test
git push -u origin test
git checkout main
```

- [ ] **Step 3: Verify only the three branches exist remotely**

Run: `git branch -a`
Expected: local `main`, `dev`, `test` plus their `origin/*` counterparts — nothing else.

- [ ] **Step 4: Set `dev` as the repository's default branch on GitHub**

External contributors' PRs should target `dev`, not `main`. This is a GitHub setting, not a git operation:

```bash
gh repo edit gmastellone/Keeppix --default-branch dev
```

- [ ] **Step 5: Note the branch model in CONTRIBUTING.md (written in Task 2)**

`main` = last released/stable state. `dev` = integration branch, where feature PRs land. `test` = pre-release staging, promoted from `dev` periodically before a `main` release. Contributors branch off `dev` and PR back into `dev`.

### Task 2: Replace `AGENTS.md` with an English `CONTRIBUTING.md`

**Files:**
- Create: `CONTRIBUTING.md`
- Delete: `AGENTS.md` (content is superseded, folded into `CONTRIBUTING.md`)

**Interfaces:** None — this is a pure documentation task. `CONTRIBUTING.md` is the file GitHub auto-links from the "Contributing" prompt on new issues/PRs, so it belongs at the repo root.

- [ ] **Step 1: Write `CONTRIBUTING.md`**

Carry forward every invariant from `AGENTS.md` that is still true and still enforced — translated to English, with every reference to a specific closed phase, ledger file, or `Rn`/ruling-ID stripped (those are internal development history, deleted outright in Task 3, not something a new contributor needs to trace). Keep the reasoning ("why this rule exists"), drop the "which phase discovered it" citation.

```markdown
# Contributing to Keeppix

Keeppix targets a **Raspberry Pi 5, 8 GB RAM, NVMe disk** serving **200,000 photos** as the hardware floor, not an aspirational stretch goal. Weigh every change against that machine, not the one you're developing on. Extreme usability and extreme lightness are the same requirement seen from two sides, not a tradeoff.

- **No new dependency without a reason in the PR description.** Every crate is build time, CVE surface, and runtime RAM.
- **Nothing loads a full table into memory.** Paginate with keyset cursors; never a `SELECT` with no `LIMIT` on a table that grows.
- **The frontend only grows through lazy chunks.** The entry bundle stays under **150 KB gzip** (enforced in CI) — someone just browsing photos shouldn't pay for the admin pages.
- **Never `thread::sleep` in an async context.** It blocks a whole worker-pool thread, not just that job — on 4 cores that's a quarter of the pool's capacity.
- **Every hot-path operation has a budget, checked by a test.**
- **A function the user can't reach doesn't count as done.** A repository method with no route, or a route with no UI, is incomplete work — not finished work waiting on the rest. CI enforces this: `scripts/check-wired.py` fails the build if a public function or mounted route has no real production caller (see `scripts/wired-exceptions.txt` for the two legitimate reasons an exception is allowed: the consumer lands in a not-yet-built follow-up, or the function is deliberately kept as tested internal API with no HTTP surface).

## Invariants — violating one is a real defect, not a style choice

### Architecture

- **No SQL outside `crates/keeppix-db`.** HTTP handlers never write queries. This is enforced mechanically: `sqlx` is a dependency of `keeppix-db` alone, so a query written in a handler crate does not compile.
- **`keeppix-media` doesn't know about the database; `keeppix-db` doesn't know about images.** Enforced by a `[[bans.deny]]` rule in `deny.toml` — adding that edge fails `cargo deny check bans`.
- **Every repository method that reads a specific user's data takes an `AuthContext` as its first parameter.** The only exceptions are already documented in code with the reason in the doc comment (bootstrap/admin-creation paths, username lookup for login, and similar). Don't add a new one without the same explicit justification.
- **`Auth` is the only way an `AuthContext` enters the HTTP layer.** Don't write a helper that fabricates one.

### Security

- **Probing an id you don't own returns `Forbidden`, never `NotFound`.** Otherwise the endpoint becomes an existence oracle — you learn which ids exist by probing them. Applies to users, libraries, folders, assets, albums, everything.
- **Queries are always parameterized.** Never string-concatenate SQL. The only allowed interpolation is of code-level constants (column lists), never of anything that came from outside.
- **`sqlx` only in function form** (`sqlx::query`, `sqlx::query_as`) plus `#[derive(sqlx::FromRow)]` for mapping. Never the `query!` macros, never a `.sqlx/` directory, never `SQLX_OFFLINE`.
- **No `unwrap()` / `expect()` in production code.** Fine in tests, with a local `#[allow(clippy::unwrap_used)]` on the test function.
- **No filesystem path ever arrives from the client.** Media is addressed by id or content hash; the server resolves the real path from its own tree.
- **C decoders (ffmpeg, libraw) run in a disposable, separate process** with `rlimit` and seccomp. Never call them in-process.
- **The `__Host-kpx_session` cookie always carries `Secure`, unconditionally.** Don't reintroduce host-conditional logic here — it has been wrong once already.

### HTTP

- **Every error is RFC 9457** `application/problem+json` with a stable `type` field prefixed `keeppix/`. The backend never translates: `title` is English and exists for debugging; the frontend translates from the `type` code.
- **`/api/v1` is frozen**: additions only, never removals or meaning changes. A breaking change gets `/api/v2`.
- **`.fallback(...)` is registered *before* `with_common_layers(...)`.** In Axum 0.8, `Router::fallback` replaces the catch-all instead of merging with an already-wrapped one — registering it after means every 404 comes back without security headers. Applies to every mount point, including `embed::mount()`.
- **Axum rejections go through `keeppix_api::Json<T>`**, not `axum::Json`, so 415/400/422 stay in `problem+json`.

### Data

- **An asset's identity is `(folder_id, filename)`.** `content_hash` is indexed but **not** unique: the same photo in two folders is two assets with independent deletions. De-duplication is a presentation-layer choice, not an identity rule.
- **No denormalized absolute path on an asset.** It's rebuilt from the `ltree` tree; otherwise moving a folder with 40,000 photos becomes a 40,000-row `UPDATE`.
- **Original metadata is immutable.** `asset_exif` is never rewritten; user edits live in `asset_overrides`, and the displayed value is `COALESCE(override, exif)`.
- **No per-user materialized visibility table.** Changing a permission must take effect immediately.
- **A RAW file is never rewritten.** Metadata goes into an `.xmp` sidecar.

## Working method

### TDD, for real

1. Write the failing test.
2. **Run it and watch it fail.** Don't skip this — a test you haven't seen fail is a test you don't know proves anything.
3. Implement the minimum that makes it pass.
4. Run it again.

A test should fail if the behavior its name claims regresses. Ask yourself: *if I deliberately break the thing this test is supposed to protect, does it fail?*

### Verify before calling anything done

Before considering a task closed, run all of this and read the output:

\`\`\`bash
cd frontend && npm ci && npm run build   # required: the backend doesn't compile without dist/
cd .. && cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
./scripts/test.sh
\`\`\`

`frontend/dist` isn't a test prerequisite, it's a *compile* prerequisite: `rust-embed` bakes that directory in at compile time.

`./scripts/test.sh` runs `cargo test --workspace --jobs 1 -- --test-threads=1` and, even if tests fail, tears down testcontainers and runs `cargo clean`. Don't run `cargo test --workspace` by hand — without `--jobs 1` it starts one PostGIS instance per crate in parallel and `target/` grows to ~9 GB. `--test-threads=1` is required by `keeppix-server/tests/config.rs`, which manipulates process environment. Run clippy *before* the tests — the script deletes `target/` afterward.

Don't say "done" without having seen green output. If something is red, it's red.

### Commits

Conventional commits, one per logical unit of work — not one giant commit at the end of a task.

\`\`\`
feat(db): add library repository
fix(api): a database outage is a 503, not a session expiry
test(domain): add coverage for User::is_active()
docs: explain the migration checksum error
\`\`\`

The body explains **why**, not what — the diff already says what.

## Branches and pull requests

- `main` is the last released, stable state. `dev` is the integration branch — branch off it, and open your PR back into it. `test` is a pre-release staging branch, promoted from `dev` periodically before a `main` release; you shouldn't normally need to touch it directly.
- If a decision isn't fully specified by the issue or your own PR description, write down what you decided and why in the PR description — don't decide it silently. A reviewer (human or CI) should be able to see the reasoning without reconstructing it from the diff.

## What not to do

- Don't add a dependency without a real need — every dependency is CI build time and CVE surface. `cargo deny` checks licenses and advisories.
- Don't implement something from a future milestone "since it's barely more work." Scope creep skips review.
- Don't "fix" code outside your current task's scope. If you notice a real defect, open an issue for it instead.
- Don't modify a migration that has already shipped. From the first release onward, only new migration files are added — `sqlx` checks the checksum of applied migrations and refuses to start if one changed.

## Stack

- **Rust 1.88.0**, edition 2024. Axum 0.8, sqlx 0.8, PostgreSQL 17 + PostGIS 3.5 (pgvector on the same instance for AI).
- **Vue 3 + TypeScript + Vite + Tailwind v4 + Reka UI.** No Vuetify.
- Frontend initial bundle budget: **150 KB gzip**, checked in CI. Lazy per-route chunks are outside that budget.
- Docker image: **distroless**, no shell, non-root.
- Translations live in `frontend/src/i18n/{it,en}.json`. No hardcoded user-facing string in a component. Both locales must carry the same keys — CI checks this.
```

- [ ] **Step 2: Delete the superseded file**

```bash
git rm AGENTS.md
```

- [ ] **Step 3: Update every reference to `AGENTS.md` across the repo**

```bash
grep -rl "AGENTS.md" --include="*.md" --include="*.json" --include="*.yml" .
```

Expected hits: `README.md` (Documentation table — update in Task 4), `.github/workflows/*.yml` if any step references it (none currently, but check), and possibly `docs/superpowers/README.md` (removed in Task 3, so no fix needed there).

- [ ] **Step 4: Commit**

```bash
git add CONTRIBUTING.md
git rm AGENTS.md
git commit -m "docs: replace AGENTS.md with an English CONTRIBUTING.md for external contributors"
```

### Task 3: Remove the phase-by-phase ledgers, specs, and plans

**Files:**
- Delete: `docs/superpowers/plans/` (all, except this plan file itself — see Step 1)
- Delete: `docs/superpowers/specs/`
- Delete: `.superpowers/sdd/` (all phase ledgers)
- Delete: `docs/superpowers/PROSEGUI.md`, `docs/superpowers/README.md`, `docs/CONTINUE.md`, `docs/FIELD-TEST.md`
- Delete (now-empty dirs): `docs/superpowers/`, `.superpowers/`

**Interfaces:** None.

Not an archive — an outright removal, per explicit direction: the user doesn't want the phase-by-phase history kept around even out of the way. `git log`/`git show` still has every one of these files if ever needed; deleting from the working tree doesn't delete from history. What survives, generalized rather than historicized, is the *working method* those ledgers encoded — that's `CONTRIBUTING.md` (Task 2), which is a framework for future contributors and their AI agents, not a record of past phases.

- [ ] **Step 1: Move this very plan file out of the directory about to be deleted**

This plan currently lives at `docs/superpowers/plans/2026-08-27-keeppix-oss-readiness.md` — a path this task is about to delete out from under itself. Move it to the repo root first (a plain top-level file, deliberately not reusing the `docs/superpowers/` name), then continue the rest of this task from its new location:

```bash
git mv docs/superpowers/plans/2026-08-27-keeppix-oss-readiness.md ./OSS-READINESS-PLAN.md
```

- [ ] **Step 2: Delete everything else under `docs/superpowers/` and `.superpowers/`**

```bash
git rm -r docs/superpowers/plans docs/superpowers/specs docs/superpowers/PROSEGUI.md docs/superpowers/README.md
git rm -r .superpowers/sdd
git rm docs/CONTINUE.md docs/FIELD-TEST.md
rmdir docs/superpowers .superpowers 2>/dev/null || true
```

- [ ] **Step 3: Fix every reference to the deleted paths**

```bash
grep -rln "docs/superpowers/\|\.superpowers/sdd\|docs/CONTINUE.md\|docs/FIELD-TEST.md" --include="*.md" --include="*.rs" --include="*.ts" --include="*.vue" --include="*.py" --include="*.sql" .
```

Known hits to fix by hand — in every case, remove the reference (point at `CONTRIBUTING.md` if the surrounding sentence was about working method, or simply delete the sentence if it was purely a pointer to a now-gone historical doc):
- `README.md` — roadmap link, Documentation table (both rewritten in Task 4 anyway).
- `models/README.md` — points at `docs/superpowers/plans/2026-08-22-keeppix-modelli-ai.md` (rewritten in Task 6 — just describe the licensing decision inline, no link needed).
- `scripts/wired-exceptions.txt` — several comments cite plan paths; drop the path, keep the reasoning that follows it.
- `scripts/check-wired.py` — check its own comments/docstring for a path reference.
- Any lingering comment inside `crates/` or `frontend/src/` citing a `docs/superpowers/...` path — fixed for real as part of Phase B's sweep of that file, since it's exactly the kind of internal-development-diary comment Phase B removes anyway. For Phase A, it's enough that the grep above returns zero hits in `.md` files.

- [ ] **Step 4: Verify no remaining `.md` file references a deleted path**

```bash
grep -rln "docs/superpowers/\|\.superpowers/sdd\|docs/CONTINUE.md\|docs/FIELD-TEST.md" --include="*.md" .
```

Expected: empty (aside from `OSS-READINESS-PLAN.md` itself, which necessarily discusses the paths it's deleting — that's fine, it's gone by Task 10).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "docs: remove phase-by-phase plans, specs, and ledgers — superseded by CONTRIBUTING.md"
```

### Task 4: Rewrite the root `README.md`

**Files:** Modify: `README.md`

**Interfaces:** None.

The existing README (see current content) is already close to the bar — it just needs the future-work section genericized (no named client tech), the roadmap table's two "native app" rows collapsed into one honest line, and the Documentation table repointed at `CONTRIBUTING.md` instead of the removed `AGENTS.md`/`docs/superpowers/README.md`.

- [ ] **Step 1: Replace the "Where it's going" section**

Find:
```markdown
## Where it's going

The backend already speaks a versioned `/api/v1` REST API plus a WebSocket notification channel —
neither is tied to the web frontend. That's deliberate: **native mobile and desktop clients are on
the roadmap**, wrapping the same Vue frontend (Capacitor for mobile, Tauri for desktop) rather than
rewriting the UI three times.
```

Replace with:
```markdown
## Where it's going

The backend already speaks a versioned `/api/v1` REST API plus a WebSocket notification channel —
neither is tied to the web frontend. That's deliberate: **a public website with full documentation,
plus native desktop and mobile clients, are on the roadmap** — built to reuse this same API and
frontend rather than reinventing the UI for each platform.
```

- [ ] **Step 2: Fix the Features table's two roadmap rows**

Find:
```markdown
| Native mobile app (Capacitor) | 🗺️ roadmap |
| Native desktop app (Tauri) | 🗺️ roadmap |
```

Replace with:
```markdown
| Native desktop and mobile clients | 🗺️ roadmap |
| Public website and documentation | 🗺️ roadmap |
```

- [ ] **Step 3: Drop the dead roadmap link, since the doc it points at no longer exists**

Find:
```markdown
Full roadmap with frozen contracts and phase dependencies:
[`docs/superpowers/plans/2026-08-13-keeppix-roadmap.md`](docs/superpowers/plans/2026-08-13-keeppix-roadmap.md).
```

Replace with nothing — delete the line. The Features table above it already communicates roadmap status; a dangling link to a removed internal planning doc adds nothing for an external reader.

- [ ] **Step 4: Rewrite the Documentation table**

Find:
```markdown
## Documentation

| Doc | For |
|---|---|
| [`AGENTS.md`](AGENTS.md) | AI coding agents: invariants and method. Read before touching code. |
| [`docs/superpowers/PROSEGUI.md`](docs/superpowers/PROSEGUI.md) | Continuation prompt: phase order, decisions already made, where to stop and ask. |
| [`docs/superpowers/README.md`](docs/superpowers/README.md) | Index of specs, plans, ledgers. |
| [`docs/DEPLOY.md`](docs/DEPLOY.md) | Installation and operations. |
| [`docs/api/openapi.json`](docs/api/openapi.json) | The `/api/v1` HTTP contract (additive-only). |
```

Replace with:
```markdown
## Documentation

| Doc | For |
|---|---|
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | Contributing: invariants, working method, branch model. Read before opening a PR. |
| [`docs/DEPLOY.md`](docs/DEPLOY.md) | Installation and operations. |
| [`docs/api/openapi.json`](docs/api/openapi.json) | The `/api/v1` HTTP contract (additive-only). |
```

- [ ] **Step 5: Verify the README renders sensibly and every link resolves**

```bash
grep -oE '\]\(([^)]+\.md)\)' README.md | sed 's/](//; s/)//' | while read -r f; do [ -f "$f" ] || echo "BROKEN LINK: $f"; done
```

Expected: no output.

- [ ] **Step 6: Commit**

```bash
git add README.md
git commit -m "docs: genericize future-client wording in README, repoint docs table at CONTRIBUTING.md"
```

### Task 5: Replace the stale `frontend/README.md`

**Files:** Modify: `frontend/README.md`

The current file is the untouched Vite scaffold boilerplate ("This template should help get you started..."), never customized. Replace it with a short, real pointer — the root README already owns the full pitch and quick-start.

- [ ] **Step 1: Replace the whole file**

```markdown
# Keeppix frontend

Vue 3 + TypeScript + Vite. Built output is embedded into the Rust binary at compile time via
`rust-embed` — see the root [`README.md`](../README.md) for the full build and development flow.

```bash
npm ci
npm run dev     # dev server, proxies API calls to :5673
npm run build   # required before `cargo build`/`cargo run` on the backend
```

Initial bundle budget: 150 KB gzip, checked in CI (`npm run build` reports the size). Lazy
per-route chunks are outside that budget. See [`../CONTRIBUTING.md`](../CONTRIBUTING.md) for the
full frontend conventions (i18n key parity, no hardcoded user-facing strings, component structure).
```

- [ ] **Step 2: Commit**

```bash
git add frontend/README.md
git commit -m "docs: replace unused Vite scaffold README with a real one"
```

### Task 6: Translate and update `models/README.md`

**Files:** Modify: `models/README.md`

Currently Italian, and cites `docs/superpowers/plans/2026-08-22-keeppix-modelli-ai.md` and a ledger path — both deleted in Task 3.

- [ ] **Step 1: Read the current file in full before translating**

```bash
cat models/README.md
```

- [ ] **Step 2: Rewrite it in English, same structure, no dead links**

Translate every line, keeping the same information (download scripts, why OpenCLIP XLM-R has no download script and instead comes from the GitHub Actions export workflow, why MobileCLIP2-S2/InsightFace were never adopted, licensing reasoning). Drop the plan-path and ledger-path references entirely — fold the one sentence of reasoning that mattered from each (the licensing incompatibility, the int8-pruned-IT/EN choice) directly into this file's prose instead of pointing at a deleted doc. Do not summarize away the licensing explanation — that's the one part of this file a future contributor most needs to not accidentally re-break.

- [ ] **Step 3: Verify the referenced scripts still exist**

```bash
ls scripts/download-yunet-sface.sh scripts/download-ai-bench.sh .github/workflows/export-openclip-xlmr.yml
```

Expected: all three exist (confirms the rewrite didn't invent or drop a real reference).

- [ ] **Step 4: Commit**

```bash
git add models/README.md
git commit -m "docs: translate models/README.md to English, remove dead doc paths"
```

### Task 7 (Phase A close-out): Push branches, verify CI, remove this plan from the working tree

**Files:** none.

- [ ] **Step 1: Push `main`, open the mirror onto `dev` and `test`**

Phase A's commits so far were made on `main` directly per this session's established direct-merge pattern (see Task 1 for why: PRs have a known platform issue on this repo). Fast-forward `dev` and `test` to match, so all three branches agree going into Phase B:

```bash
git push origin main
git checkout dev && git merge --ff-only main && git push origin dev
git checkout test && git merge --ff-only main && git push origin test
git checkout main
```

- [ ] **Step 2: Confirm CI is green on `main` at this commit**

```bash
gh run list --branch main --limit 1
```

Expected: `success` on the latest run for the Phase A HEAD commit.

- [ ] **Step 3: Do not delete this plan file yet**

This plan now lives at `./OSS-READINESS-PLAN.md` (moved there by Task 3, Step 1, before its original directory was deleted). Leave it in place until Phase B is fully complete — Task 10 deletes it as the very last step.

---

## Phase B — Codebase-wide comment cleanup and translation

This phase touches ~350 files (226 Rust under `crates/`, 124 frontend under `frontend/src/`) that currently carry Italian-language, development-diary-style comments (`// Fase 9 Task 10: ...`, `// Ruling: ...`, references to specific commit SHAs and ledger entries). The goal per file:

1. **Translate** every remaining comment and doc comment to English.
2. **Remove** comments that only explain *what* the code does (redundant with well-named identifiers) or that narrate internal development history (`Fase N Task M`, `verificato riga per riga`, commit-SHA citations, "debt discovered on 27 August" style notes). Keep the file's substantive doc comments — architecture rationale (`//!` module docs explaining *why* a design was chosen), non-obvious invariants, and warnings about a specific past bug class — since those genuinely help a future contributor, translated but otherwise intact.
3. **Never** change behavior. If a comment turns out to describe behavior the code no longer has, fix the comment to match the code as it stands today, and call that out explicitly in the batch's commit message — don't silently "fix" the code instead.

Because the actual content of each file's comments isn't known ahead of time, this phase is a *procedure*, run in small batches, not a pre-written diff. Each batch is one crate (backend) or one top-level directory (frontend), each is its own commit, and each is verified before moving to the next — so the work can be paused and resumed across sessions without ever leaving an inconsistent state.

### Task 8: Backend sweep, one crate per batch

**Files:** every `.rs` file under each of, in this order (smallest/most-isolated first, so the procedure gets debugged on low-risk crates before the two biggest — `keeppix-db` and `keeppix-api` — carry more of the domain-specific narrative and deserve the most care):

1. `crates/keeppix-domain`
2. `crates/keeppix-test-support`
3. `crates/keeppix-dav`
4. `crates/keeppix-media`
5. `crates/keeppix-jobs`
6. `crates/keeppix-server`
7. `crates/keeppix-db`
8. `crates/keeppix-api`

For each crate, repeat this sub-procedure:

- [ ] **Step 1: List the crate's files with non-English comments**

```bash
grep -rl "// .*[àèìòù]\|//! .*[àèìòù]\|/// .*[àèìòù]" crates/<crate-name> --include="*.rs"
```

- [ ] **Step 2: Read and edit each listed file**

For each file: read it in full, translate every Italian comment to English, and delete any comment that is pure development-diary narrative (phase/task numbers, ledger citations, "verified on 27 August" style dating, commit SHAs) rather than a durable explanation of *why* the code is shaped the way it is. When a `//!` module doc explains a real architectural decision (e.g., "this repo never touches the filesystem directly because—"), keep it, translated, minus the phase citation.

- [ ] **Step 3: Verify the crate still compiles and lints clean**

```bash
cargo check -p <crate-name> --all-targets
cargo clippy -p <crate-name> --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: all three clean. A comment-only edit should never fail `check`, but a stray unclosed `/*` or a doc-comment on the wrong item (`///` vs `//`) will — this is the gate that catches it before commit.

- [ ] **Step 4: Confirm zero non-English comments remain in the crate**

```bash
grep -rl "// .*[àèìòù]\|//! .*[àèìòù]\|/// .*[àèìòù]" crates/<crate-name> --include="*.rs"
```

Expected: empty.

- [ ] **Step 5: Commit the crate as one unit**

```bash
git add crates/<crate-name>
git commit -m "docs(<crate-name>): translate comments to English, drop phase-tracking narrative"
```

- [ ] **Step 6: Repeat Steps 1-5 for the next crate in the list**

### Task 9: Frontend sweep, one directory per batch

**Files:** every `.vue`/`.ts` file (excluding `*.spec.ts`, which carry far less narrative and can be swept last as one combined batch) under, in this order:

1. `frontend/src/stores`
2. `frontend/src/api`
3. `frontend/src/composables` (if it exists — check first)
4. `frontend/src/components/ui`
5. `frontend/src/components`
6. `frontend/src/views/setup`
7. `frontend/src/views/settings`
8. `frontend/src/views`
9. `frontend/src/**/*.spec.ts` (all spec files, as one final batch)

- [ ] **Step 1: List the directory's files with non-English comments**

```bash
grep -rl "// .*[àèìòù]" frontend/src/<directory> --include="*.vue" --include="*.ts"
```

- [ ] **Step 2: Read and edit each listed file**

Same rule as Task 8 Step 2: translate, and drop phase/task/ledger narrative while keeping genuine "why" explanations — especially the ones documenting a deliberate deviation from a design mockup, or an accessibility fix, since those are exactly the kind of thing a new contributor would otherwise silently "fix" back to a worse state.

- [ ] **Step 3: Verify**

```bash
cd frontend && npx vue-tsc --noEmit && npx eslint <changed files> && cd ..
```

Expected: clean. Run the full suite once per directory batch, not per file:

```bash
cd frontend && npx vitest run && cd ..
```

- [ ] **Step 4: Confirm zero non-English comments remain in the directory**

```bash
grep -rl "// .*[àèìòù]" frontend/src/<directory> --include="*.vue" --include="*.ts"
```

Expected: empty.

- [ ] **Step 5: Commit the directory as one unit**

```bash
git add frontend/src/<directory>
git commit -m "docs(frontend): translate <directory> comments to English, drop phase-tracking narrative"
```

- [ ] **Step 6: Repeat Steps 1-5 for the next directory in the list**

### Task 10: Final repo-wide verification and cleanup

**Files:** delete `OSS-READINESS-PLAN.md` (this plan, at its Task-3-relocated path).

- [ ] **Step 1: Confirm zero Italian-language comments remain anywhere**

```bash
grep -rl "// .*[àèìòù]\|//! .*[àèìòù]\|/// .*[àèìòù]" crates --include="*.rs"
grep -rl "// .*[àèìòù]" frontend/src --include="*.vue" --include="*.ts"
```

Expected: both empty.

- [ ] **Step 2: Full workspace verification**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cd frontend && npm run build && npx vitest run && cd ..
```

Expected: all green.

- [ ] **Step 3: Merge to `dev` and `test`, push all three branches**

```bash
git checkout dev && git merge --ff-only main && git push origin dev
git checkout test && git merge --ff-only main && git push origin test
git checkout main && git push origin main
```

- [ ] **Step 4: Confirm CI green on `main`**

```bash
gh run list --branch main --limit 1
```

- [ ] **Step 5: Delete this plan file and commit**

```bash
git rm OSS-READINESS-PLAN.md
git commit -m "chore: remove the OSS-readiness plan now that it's fully executed"
git push origin main
```
