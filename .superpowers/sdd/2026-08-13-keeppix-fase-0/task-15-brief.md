## Task 15: Integrazione continua

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/release.yml`
- Create: `deny.toml`

**Interfaces:**
- Consumes: tutti i task precedenti.
- Produces: CI che blocca il merge su fmt, clippy, test, tipi frontend, budget bundle, compatibilità OpenAPI, audit delle dipendenze; e una pipeline di release che pubblica l'immagine multi-arch firmata.

- [ ] **Step 1: Scrivere `deny.toml`**

```toml
[advisories]
yanked = "deny"

[licenses]
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause",
         "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib", "MPL-2.0", "AGPL-3.0"]

[bans]
multiple-versions = "warn"
```

- [ ] **Step 2: Scrivere `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  push: { branches: [main] }
  pull_request:

env:
  CARGO_TERM_COLOR: always
  SQLX_OFFLINE: "true"

jobs:
  backend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.85.0
        with: { components: rustfmt, clippy }
      - uses: Swatinem/rust-cache@v2

      - name: Formattazione
        run: cargo fmt --all --check

      - name: Lint
        run: cargo clippy --workspace --all-targets -- -D warnings

      # I test di integrazione avviano Postgres via testcontainers: Docker è
      # già disponibile sui runner GitHub.
      - name: Test
        run: cargo test --workspace -- --test-threads=1

      - name: La specifica OpenAPI è aggiornata
        run: git diff --exit-code docs/api/openapi.json

  frontend:
    runs-on: ubuntu-latest
    defaults: { run: { working-directory: frontend } }
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: "24", cache: npm, cache-dependency-path: frontend/package-lock.json }

      - run: npm ci
      - name: Tipi
        run: npx vue-tsc --noEmit
      - name: Test
        run: npx vitest run
      - name: Build
        run: npm run build

      - name: Budget del bundle iniziale (150 KB gzip)
        run: |
          SIZE=$(find dist/assets -name '*.js' -exec gzip -c {} \; | wc -c)
          echo "bundle gzip: $SIZE byte"
          if [ "$SIZE" -gt 153600 ]; then
            echo "::error::bundle oltre il budget di 153600 byte"
            exit 1
          fi

  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
        with: { command: check advisories bans licenses }

  image:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - name: Build immagine (senza push)
        uses: docker/build-push-action@v6
        with:
          context: .
          push: false
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

- [ ] **Step 3: Scrivere `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags: ["v*"]
  schedule:
    # Ricostruzione settimanale: raccoglie le patch di sicurezza delle immagini
    # di base senza attendere una release.
    - cron: "0 4 * * 1"

permissions:
  contents: read
  packages: write
  id-token: write

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-qemu-action@v3
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - id: meta
        uses: docker/metadata-action@v5
        with:
          images: ghcr.io/${{ github.repository }}
          tags: |
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}
            type=raw,value=latest,enable={{is_default_branch}}

      - id: build
        uses: docker/build-push-action@v6
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          sbom: true
          provenance: mode=max

      - uses: sigstore/cosign-installer@v3
      - name: Firma l'immagine
        run: |
          cosign sign --yes \
            ghcr.io/${{ github.repository }}@${{ steps.build.outputs.digest }}
```

- [ ] **Step 4: Verificare i workflow in locale, per quanto possibile**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace -- --test-threads=1`
Expected: tutto verde. Poi `cd frontend && npx vue-tsc --noEmit && npx vitest run && npm run build`.

- [ ] **Step 5: Commit e push**

```bash
git add .github deny.toml
git commit -m "ci: add build, test, audit and release pipelines"
git push -u origin main
```

- [ ] **Step 6: Verificare che la CI passi su GitHub**

Aprire la pagina Actions del repository. Tutti e quattro i job (`backend`, `frontend`, `audit`, `image`) devono essere verdi. In caso di fallimento, correggere e ricommittare prima di considerare la Fase 0 conclusa.

---

## Criteri di completamento della Fase 0

La fase è chiusa quando **tutti** questi punti sono verificati:

- [ ] `cargo test --workspace -- --test-threads=1` è verde (≈40 test).
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` non produce warning.
- [ ] `cd frontend && npx vitest run && npx vue-tsc --noEmit` è verde.
- [ ] Il bundle iniziale del frontend è sotto 150 KB gzip.
- [ ] `docker compose --profile bundled up -d` avvia lo stack e l'healthcheck riporta `healthy`.
- [ ] Da browser: setup del primo admin, logout, login, ricarica pagina con sessione persistente.
- [ ] L'immagine non contiene shell (`docker run --entrypoint /bin/sh` fallisce).
- [ ] `docs/api/openapi.json` è committato ed elenca i 6 endpoint.
- [ ] La CI è verde su GitHub.
- [ ] Riavviando lo stack, i dati sopravvivono.

## Cosa NON è in Fase 0

Da non implementare, per quanto tentante: scansione di librerie, asset, miniature, EXIF, mappe, WebDAV, upload, condivisione, gruppi, 2FA, WebSocket, code di job. Ognuno ha la sua fase. L'obiettivo qui è avere fondamenta su cui il resto si appoggia senza dover essere riscritto.

