## Task 1: Workspace e toolchain

**Files:**
- Create: `rust-toolchain.toml`, `Cargo.toml`, `rustfmt.toml`, `clippy.toml`, `.gitignore`
- Create: `crates/keeppix-domain/Cargo.toml`, `crates/keeppix-domain/src/lib.rs`
- Create: `crates/keeppix-db/Cargo.toml`, `crates/keeppix-db/src/lib.rs`
- Create: `crates/keeppix-media/Cargo.toml`, `crates/keeppix-media/src/lib.rs`
- Create: `crates/keeppix-jobs/Cargo.toml`, `crates/keeppix-jobs/src/lib.rs`
- Create: `crates/keeppix-dav/Cargo.toml`, `crates/keeppix-dav/src/lib.rs`
- Create: `crates/keeppix-api/Cargo.toml`, `crates/keeppix-api/src/lib.rs`
- Create: `crates/keeppix-server/Cargo.toml`, `crates/keeppix-server/src/main.rs`

**Interfaces:**
- Consumes: nulla.
- Produces: il workspace `keeppix` con 7 membri; il binario si chiama `keeppix` e vive in `keeppix-server`.

- [ ] **Step 1: Aggiornare la toolchain**

```bash
rustup update stable && rustc --version
```

Atteso: `1.85.0` o superiore. Se il comando non esiste, installare rustup da https://rustup.rs.

- [ ] **Step 2: Creare `rust-toolchain.toml`**

```toml
[toolchain]
channel = "1.85.0"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Creare il `Cargo.toml` del workspace**

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "AGPL-3.0-or-later"

[workspace.dependencies]
anyhow = "1"
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
uuid = { version = "1", features = ["v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }

[profile.release]
lto = true
codegen-units = 1
strip = true
```

- [ ] **Step 4: Creare i 7 crate**

```bash
cd Keeppix
for c in domain db media jobs dav api; do
  mkdir -p crates/keeppix-$c/src && touch crates/keeppix-$c/src/lib.rs
done
mkdir -p crates/keeppix-server/src
```

- [ ] **Step 5: Scrivere i `Cargo.toml` dei crate libreria**

Per ognuno di `domain`, `media`, `jobs`, `dav` (sostituire `NAME`):

```toml
[package]
name = "keeppix-NAME"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
thiserror.workspace = true
```

Per `keeppix-db` e `keeppix-api` le dipendenze arrivano nei task successivi: per ora identici ai precedenti più `keeppix-domain = { path = "../keeppix-domain" }`.

- [ ] **Step 6: Scrivere il `Cargo.toml` del binario**

```toml
[package]
name = "keeppix-server"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "keeppix"
path = "src/main.rs"

[dependencies]
keeppix-domain = { path = "../keeppix-domain" }
keeppix-db = { path = "../keeppix-db" }
keeppix-api = { path = "../keeppix-api" }
anyhow.workspace = true
tokio.workspace = true
tracing.workspace = true
```

- [ ] **Step 7: Scrivere un `main.rs` minimo**

```rust
fn main() {
    println!("keeppix {}", env!("CARGO_PKG_VERSION"));
}
```

- [ ] **Step 8: Configurare stile e lint**

`rustfmt.toml`:

```toml
edition = "2024"
max_width = 100
```

`clippy.toml`:

```toml
avoid-breaking-exported-api = false
```

Aggiungere in fondo al `Cargo.toml` del workspace:

```toml
[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "warn"
expect_used = "warn"
```

E in **ogni** `Cargo.toml` di crate:

```toml
[lints]
workspace = true
```

- [ ] **Step 9: Creare `.gitignore`**

```gitignore
/target
/data
/pgdata
node_modules
frontend/dist
.env
*.kpxb
```

- [ ] **Step 10: Verificare che tutto compili**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
Expected: build OK, nessun warning clippy, formattazione conforme.

- [ ] **Step 11: Verificare l'esecuzione del binario**

Run: `cargo run --bin keeppix`
Expected: stampa `keeppix 0.1.0`

- [ ] **Step 12: Commit**

```bash
git add -A
git commit -m "chore: scaffold cargo workspace with seven crates"
```

---

