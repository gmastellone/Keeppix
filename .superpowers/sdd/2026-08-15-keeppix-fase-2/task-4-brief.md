## Task 4: `asset_overrides` e `asset_flags`

**Files:**
- Create: `crates/keeppix-db/migrations/0012_overrides_flags.sql`
- Create: `crates/keeppix-domain/src/flags.rs`, `overrides.rs`
- Create: `crates/keeppix-db/src/overrides.rs`, `flags.rs`
- Create: `crates/keeppix-db/tests/overrides.rs`, `flags.rs`

**Interfaces:**
- Produces:
  - `Rating(u8)` — 0..=5, `Rating::parse` rifiuta fuori range.
  - `Pick::{None, Pick, Reject}`
  - `AssetFlags { rating, pick, color_label }`
  - `OverridePatch { title, description, taken_at, location, place_id, orientation }` — ogni campo `Option<Option<T>>`: `None` = non toccare, `Some(None)` = azzera, `Some(Some(v))` = imposta.
  - `FlagRepo::{set, get, batch_set}` — tutti con `AuthContext`, tutti per utente.
  - `OverrideRepo::{apply, apply_batch, undo_batch, effective, pending_sidecars}`

**La migrazione:**

```sql
CREATE TABLE asset_overrides (
    asset_id       uuid PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    title          text,
    description    text,
    taken_at       timestamptz,
    location       geography(Point, 4326),
    place_id       bigint,
    orientation    smallint,
    updated_by     uuid REFERENCES users(id),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    -- NULL = mai scritto su file. Il job dei sidecar seleziona
    -- WHERE updated_at > COALESCE(xmp_written_at, '-infinity').
    xmp_written_at timestamptz
);

CREATE INDEX asset_overrides_pending_idx ON asset_overrides (updated_at)
    WHERE xmp_written_at IS NULL OR xmp_written_at < updated_at;

CREATE TABLE asset_flags (
    asset_id    uuid NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rating      smallint CHECK (rating BETWEEN 0 AND 5),
    pick        text CHECK (pick IN ('none','pick','reject')),
    color_label text,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (asset_id, user_id)
);

-- Il culling filtra per "gli scarti di questo utente": indice su (user_id, pick).
CREATE INDEX asset_flags_user_pick_idx ON asset_flags (user_id, pick)
    WHERE pick <> 'none';

-- Registro delle operazioni batch, per l'annullamento.
CREATE TABLE metadata_batches (
    id          uuid PRIMARY KEY,
    actor_id    uuid NOT NULL REFERENCES users(id),
    applied_at  timestamptz NOT NULL DEFAULT now(),
    undone_at   timestamptz,
    -- Valori precedenti, per asset. Serve solo all'annullamento.
    previous    jsonb NOT NULL
);
```

- [ ] **Step 1: Scrivere i test che falliscono**

Devono pinnare almeno:

- `effective()` restituisce `COALESCE(override, exif)` campo per campo — un override parziale non azzera i campi non toccati;
- `apply_batch` su 500 asset è **una** operazione, non 500 round-trip;
- `undo_batch` ripristina esattamente i valori precedenti, **anche quando il valore precedente era NULL**;
- `undo_batch` su un batch già annullato è idempotente, non raddoppia;
- il rating è **per utente**: due utenti sullo stesso asset non si sovrascrivono;
- un utente non proprietario riceve `Forbidden`, e su un id inesistente **anch'esso** `Forbidden`;
- `pending_sidecars` restituisce solo gli asset con `updated_at > xmp_written_at`.

Il test sull'`undo` con valore precedente `NULL` è quello che si dimentica: senza, «annulla» trasforma un campo mai valorizzato in stringa vuota.

- [ ] **Step 2-4: Fallimento, implementazione, verifica**

Run: `cargo test -p keeppix-db -- --test-threads=1`

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(db): add metadata overrides and per-user flags"
```

---

