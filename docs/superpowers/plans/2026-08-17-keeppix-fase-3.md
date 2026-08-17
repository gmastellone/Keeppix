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
| **V10** | Da amministratore creo un secondo utente e lo disabilito, tutto dal browser | 12a |
| **V11** | Torno da un servizio: sfoglio la cartella, seleziono 200 scatti, sposto le date perché la fotocamera era avanti di un'ora, e annullo quando sbaglio | 12c, 12e |
| **V12** | Cancello per errore, apro il cestino e ripristino | 12d |

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

## Task 11: I viaggi V5-V12

**Files:**
- Modify: `crates/keeppix-api/tests/journeys.rs`

Otto test end-to-end nella forma introdotta dalla Fase 2R: V5-V9 sulla
condivisione, V10-V12 sulle interfacce del Task 12.

**V10-V12 vanno scritti dopo il Task 12**, e devono attraversare l'interfaccia
come la attraverserebbe una persona: se passano chiamando l'API senza che
esista un modo di arrivarci dal browser, non provano ciò che il loro nome
afferma — è il difetto che ha prodotto cinque occorrenze in questo progetto.

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

## Task 12: le interfacce mancanti — i debiti che la guardia ha scoperto

La guardia della Fase 2R3 (`scripts/check-wired.py`) ha scoperto **17 voci**
spedite e mai raggiungibili dall'utente. Vengono **tutte** pagate qui.

Non sono rifiniture. Sono la differenza fra un'API e un prodotto: oggi
l'amministratore non può creare un utente, nessuno può sfogliare per cartelle,
il cestino non si apre, i metadati non si correggono in blocco e la sessione
scade senza rinnovo — mentre il backend, per ognuna di queste, funziona già ed
è testato.

È lo stesso principio scritto in `AGENTS.md` e violato quattro volte:
**una funzione che l'utente non può raggiungere non esiste.**

**Vincolo di leggerezza, valido per tutto il task:** ogni pannello qui sotto è
un **chunk lazy separato**. Chi guarda le foto non paga per la gestione degli
account, per il cestino o per l'editor in blocco. Il bundle d'ingresso resta
sotto **150 KB gzip**, verificato in CI.

### 12a — Gestione utenti

**Rotte già spedite, senza consumatore:** `/users`, `/users/me/password`,
`/users/{id}`, `/users/{id}/disable`.

La 2R le ha costruite e testate, e nessuna interfaccia le chiama: la gestione
utenti si fa oggi solo con `curl`. In una fase che si chiama «multiutente» è
il primo debito da saldare.

Serve: elenco utenti con ruolo e stato, creazione, cambio ruolo,
disabilitazione, e cambio della propria password. Disabilitare un utente deve
**revocargli le sessioni**, non solo impedirgli il prossimo accesso.

**Test.**
1. Un amministratore crea un utente, ne cambia il ruolo, lo disabilita, e le
   sessioni di quell'utente smettono di funzionare **subito**.
2. Un utente non amministratore che apre la pagina riceve `Forbidden`, e la
   voce non compare nemmeno nel menu — nascondere il link non basta, ma
   mostrarlo a chi non può usarlo è un difetto di interfaccia.
3. Un utente cambia la propria password e le **altre** sue sessioni cadono,
   quella corrente no.

### 12b — Rinnovo della sessione

**Rotta già spedita, senza consumatore:** `/auth/refresh`.

Oggi la sessione ha un TTL assoluto e la SPA non la rinnova mai: l'utente viene
espulso a orologio, nel mezzo di quello che sta facendo. Su una sessione di
culling di due ore è inaccettabile.

Il Task 1 tocca già `refresh`/`rotate` per il ricontrollo di `disabled_at`: il
cablaggio va fatto **insieme**, non dopo.

Serve un rinnovo silenzioso prima della scadenza, che non parta se la scheda è
in background e inattiva — un watchdog che rinnova a vuoto tutta la notte è
esattamente il consumo continuo che il bersaglio Pi vieta.

**Test.** Con un TTL abbreviato nei test, una sessione attiva **non** cade alla
scadenza; una scheda lasciata inattiva oltre la finestra **cade**, e il ritorno
mostra il login senza schermate rotte.

### 12c — Navigazione e riorganizzazione delle cartelle

**Rotte e funzioni già spedite, senza consumatore:** `/folders/tree`,
`/folders/{id}/children`, `fn move_subtree`, `fn regroup_folder`.

L'albero `ltree` esiste dalla Fase 1a, l'API dalla 1c, e non c'è modo di
sfogliarlo. Per chi organizza le foto in cartelle sul disco — il modello che
hai scelto contro quello a soli album — è la vista principale che manca.

Serve: albero navigabile con conteggi, apertura di una cartella in timeline
filtrata, e spostamento di un sottoalbero.

**Attenzione al peso:** l'albero **non si carica tutto**. Si espande un livello
alla volta con `/folders/{id}/children`; `/folders/tree` serve solo la radice.
Su 200.000 foto e migliaia di cartelle, un albero completo in una risposta sola
è esattamente ciò che `AGENTS.md` vieta.

**Test.**
1. L'albero mostra i figli solo quando si espande un nodo, non prima —
   verificato contando le richieste.
2. Spostare una cartella con molte foto **non** riscrive le righe degli asset
   (l'invariante dell'`ltree`: si sposta il sottoalbero, non i figli).
3. Un utente vede solo le cartelle che i suoi permessi gli concedono — questo
   task arriva **dopo** il Task 1, e ne usa la funzione di visibilità.

### 12d — Cestino

**Rotte già spedite, senza consumatore:** `/trash`, `/trash/empty`.

La Fase 2 ha costruito il cestino con conservazione a termine, la 2R3 ne ha
schedulato la potatura automatica, e l'utente **non può vederlo**. Cancella
foto e non ha modo di recuperarle né di liberare spazio prima della scadenza.

Serve: elenco degli elementi in cestino con quando scadono, ripristino
selettivo, svuotamento immediato con conferma.

**Lo svuotamento è distruttivo e irreversibile**: la conferma deve dire quanti
file e quanto spazio, non un «sei sicuro?» generico.

**Test.** Una foto cancellata compare in cestino con la sua data di scadenza; il
ripristino la rimette nella timeline; lo svuotamento chiede conferma con i
numeri e poi cancella davvero.

### 12e — Modifica di metadati e flag in blocco

**Rotte già spedite, senza consumatore:** `/metadata/batch`,
`/metadata/batch/shift-taken-at`, `/metadata/batch/{batch_id}/undo`,
`/flags/batch`.

Questa è la funzione del **professionista** descritto nella spec: torna da un
servizio con centinaia di scatti, la fotocamera aveva l'ora sbagliata, e deve
spostare le date di tutti e mettere una didascalia comune. Il backend lo fa
già, incluso l'**annullamento** di un'operazione in blocco.

Serve: selezione multipla in timeline, pannello di modifica applicato alla
selezione, spostamento delle date con anteprima del risultato, e un annulla
raggiungibile **dopo** l'operazione — un `undo` che esiste nell'API ma che
l'utente non trova non lo salva da niente.

**Test.**
1. Selezionate N foto e spostate le date, tutte cambiano e l'annulla le
   riporta esattamente com'erano.
2. L'annulla resta raggiungibile finché l'operazione è annullabile, e sparisce
   quando non lo è più — senza lasciare un pulsante che fallisce.
3. I metadati originali **non** vengono riscritti: la modifica vive in
   `asset_overrides` e il valore mostrato è `COALESCE(override, exif)`
   (invariante di `AGENTS.md`).

### 12f — Suggerimenti e ricerche salvate

**Rotte già spedite, senza consumatore:** `/search/suggest`, `/saved-searches`.

Serve: suggerimenti mentre si digita, e salvataggio di una ricerca con
richiamo dalla barra laterale.

**I suggerimenti sono su un percorso caldo**: vanno con debounce e con la
richiesta precedente annullata, altrimenti ogni tasto è una query su 200.000
righe.

**Test.** Digitando non parte una richiesta per carattere; selezionare un
suggerimento esegue la ricerca; una ricerca salvata si richiama e dà gli stessi
risultati.

### 12g — Ripulire il registro delle eccezioni

Pagati i debiti, `scripts/wired-exceptions.txt` va **svuotato** della sezione
«Debiti»: se resta una voce, o non è stata pagata o la guardia non la vede più
per un motivo che va capito, non silenziato.

Restano solo i **rinvii** veri — `fase-6` per video e capacità hardware, `ops`
per `/health`, `ci` per `/api/openapi.json`.

**Attenzione a una trappola già vista:** costruire le URL da un parametro
(`/media/${kind}/…`) rende le rotte invisibili alla guardia, che le segnala
come mai usate. Se succede, la correzione è **rendere il consumo visibile**,
non aggiungere un'eccezione per una rotta che è davvero usata.

- [ ] **Step 1-3: Scrivere, verificare, committare** (una unità logica per
      ognuna delle sei aree, non un commit unico)


## Task 13-15: spostati in Fase 2R3

I debiti scoperti dal field test della 2R2 — thumbhash sui duplicati, potatura
del cestino, ritentativo dei derive falliti, WebSocket mai cablato, zoom rotto
sui RAW, derivati senza perdita — e la prova di scala **non stanno più qui**.

Sono in [`2026-08-17-keeppix-fase-2r3.md`](2026-08-17-keeppix-fase-2r3.md), che
si esegue **prima** di questa fase.

**Perché spostati.** Sono debiti e peso, non funzioni multiutente: mescolarli
avrebbe reso questa fase impossibile da rivedere. E la prova di scala serve
*prima*, perché il Task 1 mette l'ereditarietà dei permessi nella query più
calda del prodotto: se il piano di query non regge a 200.000 asset va scoperto
prima di costruirci sopra. Il Task 1 di questa fase **usa** l'impalcatura di
scala che la 2R3 lascia in eredità, invece di doverla scrivere.


## Criteri di completamento

Ognuno è **eseguibile**.

- [ ] `cargo test --workspace -- --test-threads=1` verde; clippy e fmt puliti.
- [ ] I viaggi **V5-V12** passano, oltre a V1-V4 della Fase 2R.
- [ ] **Budget**: `GET /timeline` sotto 300 ms con 50 permessi e 10.000 asset,
      misurato e registrato nel ledger insieme alla strada scelta per
      l'ereditarietà (CTE o `NOT EXISTS`) con i numeri di `EXPLAIN ANALYZE`.
- [ ] **Tutti i debiti del Task 12 sono pagati**, e la sezione «Debiti» di
      `scripts/wired-exceptions.txt` è **vuota**. Restano solo i rinvii veri
      (`fase-6`, `ops`, `ci`). Una voce che resta lì è un debito non pagato,
      non una formalità.
- [ ] **Provato a mano dal browser, senza toccare SQL né `curl`:** creare e
      disabilitare un utente; sfogliare l'albero delle cartelle; aprire il
      cestino e ripristinare una foto; selezionare più foto e spostarne le
      date, poi annullare; salvare una ricerca e richiamarla.
- [ ] **Budget retto a 200.000 asset**, usando l'impalcatura di scala
      lasciata dalla Fase 2R3 (suo Task 8): la query di visibilità del Task 1
      va misurata **lì**, non solo sui 10.000 della riga sopra. È il bersaglio
      dichiarato in `AGENTS.md`.
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

I debiti del field test (thumbhash sui duplicati, cestino, ritentativo dei
derive, WebSocket, zoom sui RAW, derivati senza perdita, prova di scala) sono
saldati nella **Fase 2R3**, che precede questa.

## Cosa NON è in Fase 3

Mappa e geocoding (Fase 4), WebDAV (Fase 5), video, backup, TOTP e sync delta
(Fase 6).

**Dichiarato esplicitamente, perché il silenzio non è una decisione:** la
**selezione collaborativa** sugli album condivisi — i pick di più utenti uniti
con l'avatar di chi li ha messi — è descritta nella spec della Fase 2 §4.1 e
richiede la condivisione, quindi *potrebbe* stare qui. **Non è in questa fase**:
va affrontata in Fase 6 insieme al resto delle funzioni collaborative, o
anticipata solo con una decisione scritta nel ledger.
