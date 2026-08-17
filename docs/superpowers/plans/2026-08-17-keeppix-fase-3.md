# Keeppix Fase 3 — Multiutente, condivisione e link pubblici

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Condividere una cartella con un utente o un gruppo e vedergli comparire esattamente quel sottoalbero; mandare un album a chi non ha un account con un link che scade, protetto da password, senza esporre le coordinate GPS di casa.

**Architecture:** Nessun motore nuovo. La funzione di visibilità della Fase 1a — la cui firma era stata congelata apposta — viene estesa con la tabella `permissions`, e **i chiamanti non cambiano**. Un link pubblico è un `AuthContext::ShareLink`: lo stesso motore con un contesto diverso, non una strada parallela con regole proprie.

**Spec:** [`../specs/fase-3-multiutente.md`](../specs/fase-3-multiutente.md) — leggerla prima; se piano e spec divergono, **vince la spec**
**Dipende da:** [`2026-08-17-keeppix-fase-2r.md`](2026-08-17-keeppix-fase-2r.md) — **da completare prima**. Senza gestione utenti e librerie da interfaccia, metà di questa fase non sarebbe collaudabile da una persona, e i test di viaggio della 2R sono la rete che serve qui.

---

## Perché questa fase è diversa dalle altre

Nelle fasi precedenti un difetto produceva una foto mancante o una scansione
lenta. **Qui un difetto mostra a qualcuno foto che non doveva vedere.**

Tre conseguenze pratiche su come va eseguita:

1. **Ogni test di permesso va scritto nella direzione del fallimento
   pericoloso.** Non «l'utente autorizzato vede», ma «l'utente non autorizzato
   **non** vede» — e provando a uscire dal perimetro, non solo verificando che
   il perimetro esista.
2. **Il fallimento di default è chiudersi, mai aprirsi.** Un errore nella
   risoluzione dei permessi deve produrre zero risultati, non tutti.
3. **La verifica è per `grep`, non per campione.** Il Task 6 non aggiunge
   funzioni: dimostra che non esiste un percorso che legga asset senza passare
   dal filtro.

---

## Il vincolo di leggerezza, applicato a questa fase

Bersaglio: **Raspberry Pi 5, 8 GB, 200.000 foto** (vedi `/AGENTS.md`).

Il filtro di visibilità sta sul **percorso più caldo del prodotto**: ogni
richiesta di timeline, ricerca, miniatura e messaggio WebSocket lo attraversa.
Una query di permessi lenta non rallenta una pagina: rallenta tutto.

- Il filtro deve restare **una clausola SQL con i suoi parametri**, valutata
  dentro la query principale — non una seconda query né un elenco di id
  materializzato in Rust.
- **Budget: `GET /timeline` sotto 300 ms con 50 permessi e 10.000 asset**
  (Task 1, Step 3). Se degrada, la mitigazione è cachare i **prefissi risolti**
  per utente con invalidazione su cambio permessi — **mai** materializzare una
  tabella di visibilità.
- Le pagine di amministrazione e la pagina pubblica vanno in **chunk lazy
  separati**. Il bundle d'ingresso resta sotto **150 KB gzip**.
- Nessuna dipendenza nuova senza ragione nel ledger. Il rate limiting (Task 9)
  si fa in-process con una struttura semplice: **niente Redis**, che
  contraddirebbe D5 dello spec generale.

---

## Global Constraints

Gli invarianti di [`/AGENTS.md`](../../../AGENTS.md), più quelli che questa fase
rende critici:

- **`Forbidden`, mai `NotFound`**, sugli oggetti altrui — anche se l'id non
  esiste. Qui smette di essere teorico: con la condivisione, gli id altrui
  circolano davvero.
- **Una sola funzione costruisce il filtro di visibilità.** Non deve esistere
  un metodo che legga asset senza `AuthContext`.
- **REST, WebSocket, ricerca, media e (in Fase 5) WebDAV condividono lo stesso
  controllo.** Un buco non può esistere in un solo canale.
- **Nessuna tabella di visibilità materializzata per utente.**
- **Solo-allow, nessun deny.** Vince il permesso più alto fra quelli
  applicabili. L'ereditarietà si interrompe con `inherit = false`, che è
  esplicito e visibile in interfaccia.
- **I gruppi non si trasportano nell'`AuthContext`**: si derivano da `user_id`
  con un join. Un elenco trasportato è un elenco che può essere stantio, e
  rimuovere qualcuno da un gruppo deve avere effetto immediato.
- Nuove migrazioni con **prefisso a quattro cifre**.

---

## I viaggi utente

| # | Viaggio | Task |
|---|---|---|
| **V5** | Condivido una cartella con un altro utente: lui la vede, e vede *solo* quella | 1, 4 |
| **V6** | Creo un gruppo «Famiglia», ci metto tre persone, condivido con il gruppo; aggiungo un quarto membro e vede tutto senza che io ricondivida | 1, 2 |
| **V7** | Creo un album con foto da cartelle diverse, lo condivido con un link protetto da password e con scadenza; un estraneo lo apre dal telefono | 3, 5, 10 |
| **V8** | Revoco il link: chi ce l'ha non entra più, **subito** | 5, 11 |
| **V9** | Un cliente carica le sue foto attraverso un link di upload; le trovo in una coda di revisione | 7 |

---

## Task 1: `permissions` e la visibilità ereditata

**Contribuisce a:** V5, V6 — **è il task su cui poggia tutta la fase**

**Files:**
- Create: `crates/keeppix-db/migrations/0015_permissions.sql`
- Create: `crates/keeppix-db/src/permissions.rs`, `crates/keeppix-db/tests/permissions.rs`
- Modify: `crates/keeppix-db/src/visibility.rs`

### La migrazione

```sql
CREATE TABLE permissions (
    id           uuid PRIMARY KEY,
    subject_type text NOT NULL CHECK (subject_type IN ('user','group')),
    subject_id   uuid NOT NULL,
    object_type  text NOT NULL CHECK (object_type IN ('folder','album','asset')),
    object_id    uuid NOT NULL,
    role         text NOT NULL CHECK (role IN ('viewer','editor')),
    -- false = l'ereditarietà si ferma qui: il sottoalbero sotto questo nodo
    -- NON riceve questo permesso.
    inherit      boolean NOT NULL DEFAULT true,
    granted_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- Un soggetto non può avere due permessi sullo stesso oggetto: si aggiorna
-- il ruolo, non se ne accumulano due.
CREATE UNIQUE INDEX permissions_unique_grant
    ON permissions (subject_type, subject_id, object_type, object_id);

-- Il verso caldo: "cosa posso vedere io" — risolto a ogni richiesta.
CREATE INDEX permissions_subject_idx ON permissions (subject_type, subject_id);

-- Il verso freddo: "chi vede questo" — pannello di condivisione.
CREATE INDEX permissions_object_idx ON permissions (object_type, object_id);
```

### La query di visibilità

Estende quella della Fase 1a. **I chiamanti non devono cambiare**: se cambiano,
il contratto congelato è stato rotto e va corretto qui, non nei chiamanti.

```sql
WITH my_groups AS (
    SELECT group_id FROM group_members WHERE user_id = $1
),
granted AS (
    -- Le librerie che possiedo: tutto il loro albero
    SELECT f.path FROM folders f
      JOIN libraries l ON l.id = f.library_id
     WHERE l.owner_id = $1
    UNION
    -- I sottoalberi condivisi con me o con un mio gruppo
    SELECT f.path FROM permissions p
      JOIN folders f ON f.id = p.object_id
     WHERE p.object_type = 'folder'
       AND (   (p.subject_type = 'user'  AND p.subject_id = $1)
            OR (p.subject_type = 'group' AND p.subject_id IN (SELECT group_id FROM my_groups)))
)
SELECT ... FROM assets a JOIN folders f ON f.id = a.folder_id
 WHERE f.path <@ ANY(SELECT path FROM granted)
```

**Il punto difficile: l'interruzione dell'ereditarietà.** Non è un semplice
`path <@ prefisso`, perché un `inherit = false` su un nodo intermedio taglia il
sottoalbero da lì in giù.

Due strade da **misurare**, non da scegliere a intuito:

1. **CTE ricorsiva** che scende l'albero fermandosi ai nodi con `inherit = false`.
2. **`NOT EXISTS`**: il nodo è visibile se esiste un permesso su un antenato e
   **non** esiste un `inherit = false` fra quell'antenato e il nodo.

Costruire uno schema di prova con **3.000 cartelle e 50 permessi**, misurare
entrambe con `EXPLAIN ANALYZE`, e **registrare nel ledger la scelta con i
numeri**. Su un Pi la differenza fra le due può essere di un ordine di
grandezza.

- [ ] **Step 1: Scrivere i test che falliscono**

In quest'ordine di importanza. Il primo è il più importante di tutta la fase.

```rust
mod harness;

use harness::TestDb;
use keeppix_db::{AssetRepo, PermissionRepo, VisibilityScope};
use keeppix_domain::{AuthContext, SystemRole};

/// Il fallimento peggiore possibile di una funzione di visibilità è
/// APRIRSI invece di chiudersi. Va testato per primo, e su ogni repository.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_user_with_no_permissions_sees_zero_assets_everywhere() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let stranger = harness::seed_user(&test, admin, "estraneo").await;

    // L'admin indicizza 20 asset in una sua libreria.
    harness::seed_library_with_assets(&test, admin, 20).await;

    let ctx = AuthContext::user(stranger, SystemRole::User);

    assert_eq!(AssetRepo::new(test.db()).count_visible(&ctx).await.unwrap(), 0);
    assert!(TimelineRepo::new(test.db()).buckets(&ctx).await.unwrap().is_empty());
    assert!(SearchRepo::new(test.db()).run(&ctx, "*").await.unwrap().is_empty());
    assert_eq!(FolderRepo::new(test.db()).tree(&ctx).await.unwrap().len(), 0);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn sharing_a_folder_grants_its_whole_subtree() {
    // Condivido /2024, il destinatario vede /2024, /2024/Grecia,
    // /2024/Grecia/Santorini — e NON /2023.
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn inherit_false_stops_the_subtree_at_that_node() {
    // Permesso su /Foto con inherit=true, e un secondo permesso su
    // /Foto/Privato con inherit=false: /Foto/Privato e i suoi figli
    // NON sono visibili.
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_group_permission_applies_to_every_member() { /* … */ }

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn adding_a_member_grants_access_immediately() {
    // Senza toccare `permissions`: si aggiunge la riga in group_members
    // e l'accesso c'è alla richiesta successiva.
}

/// Smaschera un elenco di gruppi trasportato nel token invece che derivato.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn removing_a_member_revokes_access_immediately() {
    // Stesso AuthContext, stessa sessione: dopo la rimozione dal gruppo
    // il conteggio visibile torna a zero SENZA nuovo login.
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_highest_role_wins() {
    // viewer da gruppo + editor diretto = editor.
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_shared_folder_never_exposes_the_real_disk_path() {
    // La risposta contiene "Vacanze / 2024 / Grecia", mai "/mnt/nas/foto/...".
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn visibility_is_identical_on_every_channel() {
    // Stesso utente, stesso permesso: timeline, ricerca, cartelle e media
    // restituiscono lo stesso insieme. Se divergono, esistono due filtri.
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_an_asset_outside_the_scope_is_forbidden_not_not_found() {
    // E anche un id inesistente → Forbidden. Nessun oracolo.
}
```

- [ ] **Step 2: Eseguire, osservare i fallimenti, implementare**

- [ ] **Step 3: Misurare il budget — obbligatorio**

```rust
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_timeline_stays_under_budget_with_fifty_permissions() {
    let test = TestDb::start().await;
    harness::seed_scale(&test, /* cartelle */ 3_000, /* asset */ 10_000,
                        /* permessi */ 50).await;

    let start = std::time::Instant::now();
    let page = TimelineRepo::new(test.db()).page(&ctx, None, 200).await.unwrap();
    let elapsed = start.elapsed();

    assert!(!page.is_empty());
    assert!(
        elapsed < std::time::Duration::from_millis(300),
        "timeline con 50 permessi: {elapsed:?}. Il filtro di visibilità sta \
         sul percorso più caldo del prodotto: se è lento, tutto è lento."
    );
}
```

Registrare nel ledger il tempo reale e la strada scelta (CTE o `NOT EXISTS`)
con i numeri di `EXPLAIN ANALYZE`.

- [ ] **Step 4: Verificare e committare**

```bash
git commit -m "feat(db): extend visibility with inherited folder permissions"
```

---

## Task 2: Gruppi

**Contribuisce a:** V6

**Files:**
- Create: `crates/keeppix-db/src/groups.rs`, `crates/keeppix-api/src/routes/groups.rs`
- Create: `crates/keeppix-db/tests/groups.rs`

Le tabelle `groups` e `group_members` **esistono dalla Fase 0** e sono vuote:
questo task le riempie di comportamento.

| Metodo | Percorso | Chi |
|---|---|---|
| `GET` | `/api/v1/groups` | admin |
| `POST` | `/api/v1/groups` | admin |
| `PATCH` | `/api/v1/groups/{id}` | admin |
| `DELETE` | `/api/v1/groups/{id}` | admin |
| `GET` | `/api/v1/groups/{id}/members` | admin |
| `POST` | `/api/v1/groups/{id}/members/{user_id}` | admin |
| `DELETE` | `/api/v1/groups/{id}/members/{user_id}` | admin |

- [ ] **Step 1: Test che falliscono**

```rust
#[tokio::test]
async fn deleting_a_group_with_active_permissions_is_refused_or_confirmed() {
    // Una cascata SILENZIOSA toglie accesso a persone senza che nessuno lo
    // sappia. O si rifiuta con 409 elencando i permessi, o si richiede un
    // parametro esplicito `?cascade=true`. Mai in silenzio.
}

#[tokio::test]
async fn a_plain_user_cannot_list_or_create_groups() { /* 403 */ }

#[tokio::test]
async fn group_names_are_unique_case_insensitively() { /* 409 */ }

#[tokio::test]
async fn removing_the_last_member_leaves_the_group_usable() { /* non lo cancella */ }
```

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 3: Album

**Contribuisce a:** V7

**Files:**
- Create: `crates/keeppix-db/migrations/0016_albums.sql`, `src/albums.rs`
- Create: `crates/keeppix-api/src/routes/albums.rs`, `crates/keeppix-db/tests/albums.rs`

```sql
CREATE TABLE albums (
    id             uuid PRIMARY KEY,
    name           text NOT NULL,
    description    text,
    owner_id       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cover_asset_id uuid REFERENCES assets(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE album_assets (
    album_id uuid NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    asset_id uuid NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    -- Ordinamento manuale. Interi con spazio fra loro per inserire senza
    -- rinumerare tutto.
    position bigint NOT NULL,
    added_by uuid REFERENCES users(id) ON DELETE SET NULL,
    added_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (album_id, asset_id)
);

CREATE INDEX album_assets_order_idx ON album_assets (album_id, position);
CREATE INDEX album_assets_asset_idx ON album_assets (asset_id);
CREATE INDEX albums_owner_idx ON albums (owner_id);
```

Album **virtuali**: nessuno storage, una foto in dieci album pesa una volta.

- [ ] **Step 1: Test che falliscono**

```rust
#[tokio::test]
async fn an_asset_can_live_in_many_albums() { /* … */ }

#[tokio::test]
async fn removing_from_an_album_does_not_delete_the_asset() { /* … */ }

#[tokio::test]
async fn reordering_survives_and_does_not_renumber_everything() {
    // Inserire fra due elementi non deve riscrivere 5.000 righe.
}

/// Il punto del modello: condividere un album concede accesso a QUELLE foto,
/// non alle loro cartelle né ai loro vicini.
#[tokio::test]
async fn sharing_an_album_grants_its_assets_but_not_their_folders() {
    // Il destinatario vede le 10 foto dell'album, e NON le altre 200 della
    // cartella che le contiene, e NON l'albero delle cartelle.
}

#[tokio::test]
async fn deleting_an_album_deletes_no_photo() { /* … */ }
```

Il quarto test è quello che definisce il modello: se fallisce, la condivisione
di un album è una scorciatoia per leggere una cartella intera.

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 4: Pannello permessi e `explain`

**Contribuisce a:** V5, V6

**Files:**
- Create: `crates/keeppix-api/src/routes/permissions.rs`
- Create: `frontend/src/components/SharePanel.vue`

| Metodo | Percorso | Note |
|---|---|---|
| `GET` | `/api/v1/permissions?object_type=&object_id=` | diretti **ed** ereditati, distinti |
| `POST` | `/api/v1/permissions` | concede a utente o gruppo |
| `PATCH` | `/api/v1/permissions/{id}` | cambia ruolo o `inherit` |
| `DELETE` | `/api/v1/permissions/{id}` | revoca |
| `GET` | `/api/v1/permissions/explain?object_type=&object_id=&user_id=` | **la catena del perché** |

`explain` è ciò che rende difendibile il modello solo-allow. Deve rispondere:

```json
{ "granted": true,
  "chain": [{ "subject": {"type":"group","name":"Famiglia"},
              "role": "viewer",
              "granted_on": {"type":"folder","name":"/Foto/Vacanze"},
              "inherited_to": {"type":"folder","name":"/2024/Grecia"} }] }
```

Senza, la domanda «perché vedo questa foto?» resta senza risposta, e il modello
diventa opaco quanto uno con i deny — cioè perde la ragione per cui è stato
scelto.

- [ ] **Step 1: Test che falliscono**

```rust
#[tokio::test]
async fn explain_names_the_group_and_the_node_that_granted_access() { /* … */ }

#[tokio::test]
async fn explain_says_no_when_there_is_no_access() {
    // granted:false con chain vuota — e NON un 403, perché la domanda
    // "perché non vedo questo?" è legittima e la fa il proprietario.
}

#[tokio::test]
async fn only_the_owner_or_an_admin_can_grant_permissions() {
    // Un editor NON può ricondividere. Regola dello spec §1.2.
}

#[tokio::test]
async fn listing_permissions_separates_direct_from_inherited() { /* … */ }
```

- [ ] **Step 2-4: Implementare, verificare, committare**

Il pannello frontend è **lo stesso componente** per foto, cartella, album e
selezione multipla. Se diventano quattro componenti, la coerenza si perde entro
una fase — e questo è esattamente il tipo di divergenza che la review finale
della Fase 0 ha già trovato altrove (`assert_security_headers` triplicato).

---

## Task 5: Link pubblici

**Contribuisce a:** V7, V8

**Files:**
- Create: `crates/keeppix-db/migrations/0017_share_links.sql`, `src/share_links.rs`
- Create: `crates/keeppix-api/src/routes/share.rs`
- Modify: `crates/keeppix-domain/src/auth.rs` — la variante `Actor::ShareLink`, **prevista dalla Fase 0 e mai implementata**

```sql
CREATE TABLE share_links (
    id                 uuid PRIMARY KEY,
    -- SHA-256 del token. MAI il token in chiaro: un dump del database
    -- non deve aprire i link.
    token_hash         bytea NOT NULL,
    object_type        text NOT NULL CHECK (object_type IN ('asset','folder','album')),
    object_id          uuid NOT NULL,
    created_by         uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    password_hash      text,              -- argon2id, opzionale
    expires_at         timestamptz,
    max_views          int,
    view_count         int NOT NULL DEFAULT 0,
    allow_download     boolean NOT NULL DEFAULT true,
    allow_original     boolean NOT NULL DEFAULT false,
    allow_upload       boolean NOT NULL DEFAULT false,
    allow_cdn_cache    boolean NOT NULL DEFAULT false,
    -- Attivo di default sui link SENZA password: quando mandi la foto di
    -- casa a un conoscente non gli mandi anche le coordinate GPS di casa.
    hide_metadata      boolean NOT NULL DEFAULT true,
    upload_quota_bytes bigint,
    revoked_at         timestamptz,
    last_accessed_at   timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX share_links_token_hash_key ON share_links (token_hash);
CREATE INDEX share_links_creator_idx ON share_links (created_by);
CREATE INDEX share_links_object_idx ON share_links (object_type, object_id);
```

### Le proprietà di sicurezza, non negoziabili

- Token da **32 byte casuali** da CSPRNG, base64url. In database **solo
  l'hash**.
- **`X-Robots-Tag: noindex, nofollow`** e **`Referrer-Policy: no-referrer`** su
  tutte le pagine pubbliche. Il secondo impedisce che il token trapeli
  nell'header `Referer` quando l'ospite clicca un link.
- **Lookup a tempo costante**, nessuna enumerazione.
- Rate limiting per token e per IP sui tentativi di password (Task 9).
- `hide_metadata` toglie le coordinate **anche dalle risposte API**, non solo
  dall'interfaccia.

- [ ] **Step 1: Test che falliscono**

Il primo è il più importante: **provare a uscire dal perimetro**.

```rust
/// Un link non deve concedere più di ciò per cui è stato creato.
/// Si testa PROVANDO A USCIRE, non verificando che il dentro funzioni.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_link_on_an_album_cannot_reach_anything_else() {
    let test = TestServer::start().await;
    let (album, other_asset, containing_folder) = harness::seed_album_scenario(&test).await;
    let token = harness::create_share_link(&test, "album", album).await;

    // Dentro il perimetro: funziona.
    assert_eq!(get_shared(&test, &token, "/").await.status(), 200);

    // Fuori: tutto negato, allo stesso modo.
    assert_eq!(get_shared(&test, &token, &format!("/assets/{other_asset}")).await.status(), 403);
    assert_eq!(get_shared(&test, &token, &format!("/folders/{containing_folder}")).await.status(), 403);
    assert_eq!(get_shared(&test, &token, "/timeline").await.status(), 403);
    assert_eq!(get_shared(&test, &token, "/folders/tree").await.status(), 403);
    assert_eq!(get_shared(&test, &token, "/search").await.status(), 403);
}

#[tokio::test]
async fn an_expired_link_is_indistinguishable_from_a_nonexistent_one() {
    // Stesso status, stesso corpo: nessuna enumerazione.
}

#[tokio::test]
async fn a_wrong_password_is_indistinguishable_from_a_nonexistent_link() { /* … */ }

#[tokio::test]
async fn max_views_is_enforced() { /* la N+1 fallisce */ }

#[tokio::test]
async fn hide_metadata_strips_coordinates_from_the_api_too() {
    // Non basta che l'interfaccia non le mostri: la risposta JSON non
    // deve contenerle affatto.
}

#[tokio::test]
async fn the_token_never_appears_in_the_database() {
    // SELECT su tutte le colonne testuali: il token in chiaro non c'è.
}

#[tokio::test]
async fn public_pages_carry_noindex_and_no_referrer() { /* header */ }
```

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 6: `AuthContext::ShareLink` su tutti i canali

**Contribuisce a:** V7, V8 — **non aggiunge funzioni: dimostra che il motore è uno solo**

**Files:**
- Create: `crates/keeppix-api/tests/share_link_channels.rs`

È il task che impedisce la classe di difetto più grave della fase: un canale che
applica i permessi in modo diverso dagli altri.

- [ ] **Step 1: Test che falliscono**

```rust
/// Stesso token, ogni canale. Tutti devono negare allo stesso modo.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_share_link_is_confined_on_every_channel() {
    let test = TestServer::start().await;
    let (token, outside_asset, outside_hash) = harness::seed_confined_link(&test).await;

    // REST
    assert_eq!(get_shared(&test, &token, &format!("/assets/{outside_asset}")).await.status(), 403);
    // Ricerca
    assert_eq!(post_shared(&test, &token, "/search", json!({"q":"*"})).await.status(), 403);
    // Media per hash — il canale più facile da dimenticare
    assert_eq!(get_shared(&test, &token, &format!("/media/thumb/{outside_hash}")).await.status(), 403);
    assert_eq!(get_shared(&test, &token, &format!("/media/original/{outside_asset}")).await.status(), 403);
    // WebSocket: un ticket da share link non deve poter sottoscrivere
    // topic fuori dal perimetro
    assert!(ws_subscribe_fails(&test, &token, "timeline").await);
}
```

- [ ] **Step 2: Verificare per `grep` che non esistano scorciatoie**

```bash
# Ogni percorso che legge asset deve passare da visibility_scope.
grep -rn "FROM assets" crates/keeppix-db/src/ | wc -l
grep -rn "visibility\|VisibilityScope\|scope" crates/keeppix-db/src/ | wc -l
```

Ogni query su `assets` che **non** applica lo scope va giustificata nel doc
comment o è un difetto Critical. Registrare l'elenco nel ledger.

- [ ] **Step 3-4: Correggere se serve, committare**

---

## Task 7: Upload da ospite e coda di revisione

**Contribuisce a:** V9

**Files:**
- Modify: `crates/keeppix-api/src/routes/share.rs`
- Create: `crates/keeppix-db/src/guest_uploads.rs`

I file arrivano con flag `uploaded_by_guest` e finiscono in una coda che il
proprietario approva o scarta. **Nessuno riempie il disco a tua insaputa.**

- [ ] **Step 1: Test che falliscono**

```rust
/// La quota va applicata DURANTE il caricamento, non dopo: un file da
/// 10 GB su una quota da 1 GB va interrotto a 1 GB, non accettato e poi
/// rifiutato — su un Pi con disco piccolo la differenza è fatale.
#[tokio::test]
async fn the_quota_is_enforced_while_uploading_not_after() { /* … */ }

#[tokio::test]
async fn guest_uploads_do_not_appear_in_the_timeline_until_approved() { /* … */ }

#[tokio::test]
async fn a_guest_cannot_read_what_other_guests_uploaded() { /* … */ }

#[tokio::test]
async fn rejecting_an_upload_removes_the_file_from_disk() { /* … */ }
```

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 8: Audit log

**Contribuisce a:** tutti

**Files:**
- Create: `crates/keeppix-db/migrations/0018_audit_log.sql`, `src/audit.rs`

```sql
CREATE TABLE audit_log (
    id          bigserial PRIMARY KEY,
    actor_id    uuid REFERENCES users(id) ON DELETE SET NULL,
    actor_kind  text NOT NULL CHECK (actor_kind IN ('user','share_link','system')),
    action      text NOT NULL,
    object_type text,
    object_id   uuid,
    detail      jsonb,
    ip          inet,
    at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX audit_log_at_idx ON audit_log (at DESC);
CREATE INDEX audit_log_actor_idx ON audit_log (actor_id, at DESC);
CREATE INDEX audit_log_object_idx ON audit_log (object_type, object_id);
```

Registra: creazione e revoca di condivisioni e link, **accessi ai link
pubblici**, cancellazioni dal disco, cambi di ruolo, **accessi dell'admin a
contenuti altrui**, login falliti.

**Debito della Fase 0 da saldare o dichiarare ancora differito:**
`sessions.ip` non è mai popolata. Serve all'audit, e richiede una
configurazione «proxy fidati» per leggere `X-Forwarded-For` — popolarla con
l'IP del proxy sarebbe **peggio** che lasciarla vuota, perché sembrerebbe un
dato e non lo sarebbe.

- [ ] **Step 1: Test che falliscono**

```rust
#[tokio::test]
async fn an_admin_reading_someone_elses_photos_is_logged() { /* … */ }

#[tokio::test]
async fn the_audit_log_is_append_only() {
    // Nessuna rotta permette UPDATE o DELETE. Se un giorno servisse la
    // pulizia, è un job di manutenzione, non un endpoint.
}

#[tokio::test]
async fn a_plain_user_cannot_read_the_audit_log() { /* 403 */ }
```

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 9: Rate limiting

**Contribuisce a:** V7, V8

**Files:**
- Create: `crates/keeppix-api/src/ratelimit.rs`

**Debito della Fase 0**, differito qui con questa ragione: è **lo stesso
middleware** che serve ai link pubblici, e farlo prima significava scriverlo
due volte.

Copre: `/auth/login`, `/api/v1/setup`, i tentativi di password sui link
pubblici, e gli endpoint dei link in generale.

**Vincolo di leggerezza:** in-process, struttura semplice (finestra scorrevole
o token bucket su una `DashMap` con pulizia periodica). **Niente Redis**: su
nodo singolo contraddirebbe la decisione D5 dello spec generale, e aggiunge un
servizio da mantenere per un problema che non lo richiede.

- [ ] **Step 1: Test che falliscono**

```rust
#[tokio::test]
async fn the_limit_kicks_in_and_reopens_after_the_window() { /* … */ }

/// Per IP e per token SEPARATAMENTE: un attaccante con molti IP non deve
/// poter forzare un token, e un IP non deve poter esaurire il limite di tutti.
#[tokio::test]
async fn ip_and_token_are_limited_independently() { /* … */ }

#[tokio::test]
async fn an_authenticated_user_is_never_limited_on_the_normal_path() {
    // Sfogliare la timeline non deve mai incappare nel rate limit.
}

#[tokio::test]
async fn the_limiter_does_not_grow_without_bound() {
    // 100.000 IP distinti non devono far crescere la memoria all'infinito:
    // serve una pulizia. Su un Pi da 8 GB conta.
}
```

L'ultimo è il test di leggerezza: un rate limiter che non dimentica è una
perdita di memoria con un nome elegante.

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 10: Frontend — condivisione, gestione, pagina pubblica

**Contribuisce a:** V5, V6, V7

**Files:**
- Create: `frontend/src/views/SharesView.vue`, `GroupsView.vue`, `AlbumsView.vue`
- Create: `frontend/src/views/public/SharedView.vue`

La **pagina pubblica è un'applicazione a sé**: nessuna sessione, nessuna barra
di navigazione, nessun accesso al resto. Deve poter essere aperta dal telefono
di qualcuno che non ha mai sentito parlare di Keeppix.

Pagina **«Condivisioni»**: tutto ciò che esce di casa — chi vede cosa, tutti i
link attivi con ultimo accesso e conteggio visite, revoca di massa.

**Vincolo di leggerezza:** la pagina pubblica e le pagine di amministrazione
vanno in **chunk lazy separati**. Un ospite che apre un link non deve scaricare
il codice della gestione gruppi.

- [ ] **Step 1: Test vitest che falliscono**

```ts
it('la pagina pubblica non importa nulla dell’app autenticata', () => {
  // Verifica sul grafo dei chunk: SharedView non deve tirare dentro
  // il router autenticato, lo store di sessione, o la timeline.
})

it('mostra un messaggio chiaro quando il link è scaduto o revocato', () => {
  // Non una pagina bianca, non un 403 grezzo.
})

it('chiede la password senza rivelare se il link esiste', () => { /* … */ })
```

- [ ] **Step 2-4: Implementare, verificare, committare**

Verificare il bundle: `bundle d'ingresso < 150 KB gzip`, e la pagina pubblica
in un chunk a sé.

---

## Task 11: I viaggi V5-V9

**Files:**
- Modify: `crates/keeppix-api/tests/journeys.rs`

Cinque test end-to-end nella forma introdotta dalla Fase 2R.

Il più importante è **V8**: la revoca deve avere effetto **immediato**, e il
test deve provarlo **usando il link dopo la revoca**, non verificando che la
riga sia cambiata.

```rust
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn v8_revoking_a_link_locks_out_whoever_holds_it() {
    let test = TestServer::start().await;
    let token = harness::seed_shared_album_link(&test).await;

    // Prima: funziona.
    assert_eq!(get_shared(&test, &token, "/").await.status(), 200);

    revoke_link(&test, &token).await;

    // Dopo: non funziona più. Subito, senza attendere una scadenza
    // né un riavvio, e su OGNI canale.
    assert_eq!(get_shared(&test, &token, "/").await.status(), 403);
    assert_eq!(get_shared(&test, &token, "/media/thumb/…").await.status(), 403);
}
```

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Task 12: i debiti scoperti dal field test della 2R2

Aggiunti dopo la chiusura della 2R2, analizzando i tre field test
sull'archivio reale. Nessuno è grande; insieme valgono un task.

### 12a — il thumbhash si perde sulle foto duplicate

**Osservazione.** Due esecuzioni identiche dello stesso field test, stesso
archivio, stesso commit, danno numeri **diversi**:

```
run 10:34   Con thumbhash | 707     RAW con preview | 707 / 779
run 11:46   Con thumbhash | 700     RAW con preview | 700 / 779
```

779 asset, ma solo **689 `content_hash` distinti**: 90 foto sono la stessa
immagine in due cartelle. Gli asset senza thumbhash (72–79) sono un
sottoinsieme di quei 90, e quali siano cambia da un'esecuzione all'altra.

**Causa.** In `crates/keeppix-jobs/src/raw.rs`, il guard di idempotenza:

```rust
let (thumb_path, _) = derivative_paths(data_dir, &hash);
if thumb_path.is_file() {
    return Ok(());          // ← esce senza propagare il thumbhash
}
```

e la propagazione avviene per `content_hash`
(`crates/keeppix-db/src/assets.rs:391`):

```sql
UPDATE assets SET thumbhash = $2, updated_at = now() WHERE content_hash = $1
```

La corsa è questa:

```
asset A e asset B sono la stessa foto (hash H) in cartelle diverse
  hash_job(A) → set_hash(A, H) → accoda derive_raw:H
    derive_raw(H) → deriva, scrive il file, UPDATE ... WHERE content_hash = H
                     → aggiorna solo A: B non ha ancora content_hash
  hash_job(B) → set_hash(B, H) → accoda di nuovo derive_raw:H
    derive_raw(H) → il file c'è già → return Ok(()) → B resta senza thumbhash
```

**Impatto: contenuto, ma reale.** Il derivato esiste su disco, quindi la foto
si vede; manca solo il placeholder sfocato del caricamento progressivo, su
circa il 10% degli asset. Non è un buco nero in timeline. È però un difetto
non deterministico, e in Fase 3 la condivisione aumenta il traffico di
lettura sulle stesse foto.

**Correzione.** Nel ramo di uscita anticipata, propagare il thumbhash già
noto agli asset che non ce l'hanno, senza rifare il demosaic:

```sql
UPDATE assets SET thumbhash = src.thumbhash, updated_at = now()
  FROM (SELECT thumbhash FROM assets
         WHERE content_hash = $1 AND thumbhash IS NOT NULL LIMIT 1) src
 WHERE content_hash = $1 AND thumbhash IS NULL
```

Una query, nessun ricalcolo, nessuna lettura di file.

**Test che deve fallire prima.** Due asset in cartelle diverse con lo stesso
contenuto; far completare `derive_raw` per il primo, poi assegnare
`content_hash` al secondo ed eseguire di nuovo `derive_raw` con lo stesso
hash. **Entrambi** devono avere `thumbhash` non nullo. Oggi il secondo resta
`NULL`.

### 12b — `TrashRepo::cleanup_expired` non ha chiamanti di produzione

**Osservazione.** Verificabile con un `grep`:

```
$ grep -rn "cleanup_expired" --include="*.rs" crates/
crates/keeppix-db/src/trash.rs:403:    pub async fn cleanup_expired(...)   ← definizione
crates/keeppix-db/tests/trash.rs:383,431                                    ← solo test
```

**Zero** chiamanti di produzione. Il cestino ha una rotta manuale
(`/trash/empty`) ma **la scadenza automatica non avviene mai**: le foto
cancellate restano su disco per sempre.

Su un Pi con storage limitato è una perdita di capacità silenziosa, e rompe
la promessa di conservazione a termine fatta all'utente.

**È la quarta occorrenza dello stesso difetto di processo** — funzione
scritta, testata, mai collegata. Le altre tre: `restat_if_stable` con lo
sleep, la scansione che richiedeva il riavvio, `detect_kind` mai chiamata.

**Correzione.** Schedulare la potatura come job periodico, con la stessa
disciplina degli altri job di manutenzione (priorità bassa, cadenza tarata
sul fatto che è un'operazione di pulizia, non interattiva). La finestra di
conservazione va letta dalla configurazione, non incisa nel codice.

**Test.** Una riga in cestino più vecchia della finestra sparisce senza
intervento manuale; una più recente resta.

### 12c — ritentativo dei job di derivazione falliti

Voce differita correttamente nel ledger della 2R2 («il ritentativo dei
derive falliti non passa dalla riscansione»), ma **nessun piano la possiede**.

Oggi un fallimento transitorio — disco momentaneamente occupato, processo di
demosaic ucciso dal gate della RAM — lascia la foto **senza miniatura per
sempre**: la riscansione non la ritenta, perché D2 salta correttamente gli
asset invariati.

Serve un ritentativo con backoff, limitato nel numero di tentativi, che non
passi dalla riscansione. La rotta `/problems` esiste già e mostra gli asset
in errore: il ritentativo va reso visibile lì.

### 12e — il WebSocket esiste nel backend e nessuno lo usa

**Osservazione.**

```
$ grep -rn "WebSocket" frontend/src/         → nessun risultato
$ grep -rn "ws::" crates/keeppix-api/src/lib.rs
  .route("/ws/ticket", post(routes::ws::ticket))
  .route("/ws",        get(routes::ws::connect))
```

Il backend ha l'implementazione **completa e montata**: ticket monouso
consumato prima dell'upgrade, validazione dell'`Origin`, backpressure con
`resync`, test in `crates/keeppix-api/tests/ws.rs`, voci in OpenAPI. Il
frontend non ci si collega **mai**.

Il ledger della 2R lo registra di sfuggita
(`fase-2r/progress.md:91`): «avanzamento scansione via **polling** ogni 2 s
[…] non WebSocket — il piano cita WS ma il task chiede polling per
semplicità; **WS non è cablato nel frontend**». La ripiegatura è stata
scritta; la lacuna che la rendeva necessaria no.

**Conseguenza.** La timeline **non si aggiorna in diretta**: mentre una
scansione lavora, le foto nuove non compaiono finché non si ricarica la
pagina. È esattamente ciò che il WebSocket doveva risolvere, ed è la ragione
per cui fu scelto rispetto a SSE.

**Quinta occorrenza** dello stesso schema — e la più istruttiva, perché la
guardia proposta in 12d **non l'avrebbe presa**: sul lato Rust i chiamanti
esistono (le rotte sono montate), la lacuna sta fra backend e frontend. La
guardia va quindi estesa: ogni rotta montata deve avere o un consumatore nel
frontend, o un'eccezione scritta con la fase che la userà.

**Correzione.** Cablare il client WebSocket nel frontend e usarlo per
l'aggiornamento in diretta della timeline. Il polling del wizard di setup può
restare — è un uso una tantum su una pagina che l'utente sta guardando — ma
va allora dichiarato come scelta, non come ripiego.

### 12d — la guardia in CI contro la quinta occorrenza

Le quattro occorrenze sopra si trovano tutte con lo stesso `grep`. Aggiungere
un controllo in CI che fallisce se una funzione pubblica di `keeppix-media` o
`keeppix-db` non ha **almeno un chiamante fuori dai test**.

Serve una lista di eccezioni dichiarate — una funzione può legittimamente
esistere in attesa della fase che la userà — ma l'eccezione va **scritta**,
con la fase che la consumerà. È esattamente la differenza fra una scelta e una
dimenticanza.

Costo: un `grep` in uno script. Copre la classe di difetto che ci è costata
tre field test.

**Estensione obbligatoria dopo 12e:** il controllo non può fermarsi al lato
Rust. Ogni rotta montata in `keeppix-api` deve avere un consumatore nel
frontend o un'eccezione dichiarata. Senza questa metà, la guardia avrebbe
lasciato passare proprio il WebSocket.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Task 14: i derivati sono senza perdita, e non dovrebbero esserlo

**Decisione richiesta all'utente prima di implementare** — è un compromesso
sulla qualità delle sue foto, non una scelta tecnica interna.

### Osservazione

Il field test misura **1,2 GB di derivati per 779 foto**: circa **1,54 MB per
foto** fra miniatura e anteprima. Il rapporto del 3,3% sul sorgente sembra
buono, ma il sorgente sono RAW da 46 MB: è il numero sbagliato da guardare.
I derivati hanno dimensioni **fisse** (240 px e 1440 px), quindi scalano col
**numero** di foto, non col peso degli originali.

Su 200.000 foto: **~308 GB di derivati**, indipendentemente dal fatto che gli
originali siano RAW o JPEG.

### Causa

`crates/keeppix-media/src/derive.rs:226` codifica con
`image_webp::WebPEncoder`. Il crate `image-webp 0.2.4` dichiara nel proprio
sorgente (`encoder.rs:631`):

```
/// Only supports "VP8L" lossless encoding.
```

e scrive chunk `VP8L`. **Ogni derivato è WebP senza perdita** — l'equivalente
di un PNG. Per un'anteprima da 1440 px destinata alla visualizzazione a
schermo, è la scelta più costosa possibile in spazio, e fra le più costose in
tempo di codifica.

Nessuno l'ha deciso: è la conseguenza non esaminata di quale crate è stato
scelto per scrivere WebP.

### Cosa si guadagna

Una codifica con perdita a qualità 80 su un'anteprima da 1440 px sta
tipicamente fra 150 e 250 KB, contro oltre 1 MB del lossless. L'ordine di
grandezza atteso è **7-8× in meno**:

| | Oggi | Con perdita |
|---|---|---|
| Per foto | ~1,54 MB | ~0,2 MB |
| Su 200.000 foto | **~308 GB** | **~40 GB** |
| Rapporto sul sorgente RAW | 3,3% | ~0,4% |

Su un Pi con storage limitato, 268 GB risparmiati non sono un dettaglio: sono
la differenza fra un disco che basta e uno che non basta.

**Probabile guadagno anche in tempo**, da misurare e non da dare per scontato:
la codifica lossless WebP fa trasformazioni ed entropy coding più costosi
della codifica con perdita. La derivazione dovrebbe accelerare, non
rallentare.

### Le strade, col loro costo

| Strada | Compressione | Costo |
|---|---|---|
| **WebP con perdita** via `webp` (binding libwebp) | migliore | dipendenza **C** nuova |
| **JPEG con perdita** via `jpeg-encoder` (Rust puro) | ~25-30% peggiore di WebP | nessuna dipendenza C |
| **AVIF** via `ravif` | migliore di tutte | codifica **lenta**: sbagliata per un Pi con 200.000 foto |

Sulla dipendenza C: la regola in `AGENTS.md` («i decoder scritti in C girano
in un processo separato con rlimit e seccomp») nasce dal **decodificare input
non fidato**. Qui si tratta di **codificare** un buffer RGB che abbiamo già
decodificato noi: profilo di rischio diverso. Se si sceglie libwebp, la
ragione va comunque scritta nel ledger, come chiede la regola sulle
dipendenze.

**Raccomandazione:** JPEG con perdita in Rust puro se si vuole restare senza
C; WebP con perdita se si accetta libwebp, che comprime meglio ed è
maturissimo. In entrambi i casi la qualità va resa configurabile, con un
default sensato, e **la miniatura da 240 px e l'anteprima da 1440 px possono
avere qualità diverse**.

### Test

1. Un'anteprima derivata da un'immagine di prova pesa **meno di un terzo**
   dell'equivalente lossless odierno (soglia larga di proposito: il test
   protegge dalla regressione a lossless, non certifica un rapporto esatto).
2. Il field test riporta un rapporto derivati/originali **sotto l'1%**.
3. La qualità è configurabile e il default è documentato in `DEPLOY.md`.

- [ ] **Decisione dell'utente su formato e qualità**
- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Task 13: la prova di scala

**Il piano, così com'è, non prova mai il vincolo che lo governa.**

`AGENTS.md` dichiara il bersaglio: **Pi 5, 8 GB, 200.000 foto**. Il criterio
di completamento più severo che abbiamo scritto finora è «`GET /timeline`
sotto 300 ms con 50 permessi e **10.000 asset**» — il 5% del bersaglio. E il
field test più grande mai eseguito ha **779 asset**: lo 0,4%.

779 foto provano la **correttezza**. Non dicono niente sulla **scala**.

**Cosa fare.** Una prova di scala **sintetica**: generare 200.000 righe in
`assets` con date, cartelle e permessi realistici, **senza file veri** — non
servono, perché ciò che va misurato sono le query, non l'I/O di ingestione.
Costa minuti, non ore, e si può rieseguire a ogni fase.

Misurare, con `EXPLAIN ANALYZE` nel ledger:

| Query | Budget |
|---|---|
| `GET /timeline` prima pagina | < 300 ms |
| `GET /timeline` pagina profonda (keyset, mesi indietro) | < 300 ms |
| Conteggi per mese (le intestazioni dei bucket) | < 300 ms |
| Ricerca testuale (`pg_trgm`) | < 500 ms |
| La query di visibilità del Task 1, con 50 permessi | < 300 ms |

**Perché qui e non più avanti.** Il Task 1 di questa fase introduce
l'ereditarietà dei permessi nella query più calda del prodotto. Se la strada
scelta (CTE o `NOT EXISTS`) non regge a 200.000 asset, va scoperto **mentre
la si scrive**, non in Fase 6 quando ci saranno sopra mappe, WebDAV e video.

E se un budget non è raggiungibile, la risposta giusta non è alzarlo in
silenzio: è scriverlo nel ledger con il numero misurato e la ragione.

**Nota onesta sui numeri che abbiamo.** Tutte le misure di prestazione
esistenti vengono da Docker Desktop su macOS, dove il bind mount passa da
virtiofs. La camminata dell'albero ha impiegato ~5 minuti per ~1.600 voci di
directory (~190 ms per `stat`): è il costo di virtiofs, non del codice — su
un filesystem nativo uno `stat` sta nei microsecondi. **Da quei numeri non si
può estrapolare il comportamento sul Pi.** Questa prova di scala misura le
query, che dipendono da Postgres e dagli indici, non dal filesystem: è
l'unica delle due che si trasferisce onestamente al bersaglio.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Criteri di completamento

Ognuno è **eseguibile**.

- [ ] `cargo test --workspace -- --test-threads=1` verde; clippy e fmt puliti.
- [ ] I viaggi **V5-V9** passano, oltre a V1-V4 della Fase 2R.
- [ ] **Budget**: `GET /timeline` sotto 300 ms con 50 permessi e 10.000 asset,
      misurato e registrato nel ledger insieme alla strada scelta per
      l'ereditarietà (CTE o `NOT EXISTS`) con i numeri di `EXPLAIN ANALYZE`.
- [ ] **Prova di scala (Task 13)**: gli stessi budget retti a **200.000
      asset** sintetici, con `EXPLAIN ANALYZE` nel ledger. È il bersaglio
      dichiarato in `AGENTS.md`; finora il test più grande ne aveva 779.
- [ ] **Zero funzioni pubbliche senza chiamante di produzione** in
      `keeppix-media` e `keeppix-db`, o eccezione scritta con la fase che la
      consumerà (Task 12d). Quattro difetti sono già usciti da qui.
- [ ] Un asset duplicato in due cartelle ha il thumbhash su **entrambi**
      (Task 12a), verificato su due esecuzioni consecutive del field test:
      il numero di `thumbhash IS NOT NULL` deve essere **identico** e pari al
      totale degli asset RAW derivati.
- [ ] Il cestino si svuota **da solo** oltre la finestra di conservazione
      (Task 12b).
- [ ] La timeline si aggiorna **in diretta** durante una scansione, senza
      ricaricare la pagina (Task 12e), e ogni rotta montata ha un consumatore
      nel frontend o un'eccezione scritta (Task 12d).
- [ ] Rapporto derivati/originali **sotto l'1%** nel field test (Task 14),
      contro il 3,3% odierno.
- [ ] **Nessun percorso legge asset senza passare da `visibility_scope`** —
      verificato per `grep`, con l'elenco nel ledger, non per campione.
- [ ] **Un utente senza permessi vede zero asset su ogni canale.**
- [ ] Un link pubblico revocato smette di funzionare **immediatamente**, su
      ogni canale.
- [ ] `hide_metadata` toglie le coordinate **anche dalle risposte API**.
- [ ] Il rate limiter non cresce senza limite.
- [ ] Bundle d'ingresso sotto 150 KB gzip; pagina pubblica in chunk separato.
- [ ] Provato a mano dal browser: condivisione di una cartella a un secondo
      utente, e apertura di un link pubblico da una finestra anonima.
- [ ] `scripts/field-test.sh` ancora verde entro i budget.
- [ ] CI verde sulla PR.

## Debiti saldati in questa fase

| Voce | Task |
|---|---|
| Rate limiting su login e setup | 9 |
| `sessions.ip` mai popolata | 8 — o dichiarata ancora differita con la ragione |
| `refresh`/`rotate` non ricontrollano `disabled_at` | 1, insieme ai permessi |
| `logout` risponde `204` anche se `revoke` fallisce | 4, con `/auth/devices` |
| Thumbhash perso sulle foto duplicate | 12a |
| `TrashRepo::cleanup_expired` mai chiamata in produzione | 12b |
| Ritentativo dei `derive_*` falliti (differito dalla 2R2) | 12c |
| WebSocket montato nel backend e mai usato dal frontend | 12e |
| Derivati in WebP **senza perdita**: ~308 GB su 200.000 foto | 14 |
| Nessuna prova al di sopra di 779 asset | 13 |

## Cosa NON è in Fase 3

Mappa e geocoding (Fase 4), WebDAV (Fase 5), video, backup, TOTP e sync delta
(Fase 6).

**Dichiarato esplicitamente, perché il silenzio non è una decisione:** la
**selezione collaborativa** sugli album condivisi — i pick di più utenti uniti
con l'avatar di chi li ha messi — è descritta nella spec della Fase 2 §4.1 e
richiede la condivisione, quindi *potrebbe* stare qui. **Non è in questa fase**:
va affrontata in Fase 6 insieme al resto delle funzioni collaborative, o
anticipata solo con una decisione scritta nel ledger.
