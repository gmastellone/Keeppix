# Keeppix Fase 3 — Multiutente, condivisione e link pubblici

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Condividere una cartella con un utente o un gruppo e vedergli comparire esattamente quel sottoalbero; mandare un album a chi non ha un account con un link che scade, protetto da password, senza esporre le coordinate GPS di casa.

**Architecture:** Nessun motore nuovo. La funzione di visibilità della Fase 1a — la cui firma era stata congelata apposta — viene estesa con la tabella `permissions`, e **i chiamanti non cambiano**. Un link pubblico è un `AuthContext::ShareLink`, cioè lo stesso motore con un contesto diverso: non una strada parallela con regole proprie.

**Tech Stack:** Rust 1.88 · Postgres 17 (ricorsione su `ltree` per l'ereditarietà) · argon2id per le password dei link · Vue 3 + Tailwind

**Spec:** [`../specs/fase-3-multiutente.md`](../specs/fase-3-multiutente.md) — **leggerla prima**; se piano e spec divergono, vince la spec
**Dipende da:** [`2026-08-17-keeppix-fase-2r.md`](2026-08-17-keeppix-fase-2r.md) — **da completare prima**: senza gestione utenti e librerie da interfaccia, metà di questa fase non sarebbe collaudabile da una persona

---

## Global Constraints

Gli invarianti di [`/AGENTS.md`](../../../AGENTS.md), più quelli che questa fase rende critici.

- **`Forbidden`, mai `NotFound`**, sugli oggetti altrui — anche se l'id non esiste. In questa fase la regola smette di essere teorica: con la condivisione, gli id altrui circolano davvero.
- **Una sola funzione costruisce il filtro di visibilità.** Ogni repository che legge asset la attraversa. Non deve esistere un metodo che legga asset senza `AuthContext`.
- **REST, WebSocket, WebDAV e link pubblici condividono lo stesso controllo.** Un buco nei permessi non può esistere in un solo canale.
- **Nessuna tabella di visibilità materializzata per utente.** Cambiare un permesso è un `INSERT` con effetto immediato.
- **Solo-allow, nessun deny.** Vince il permesso più alto fra quelli applicabili.
- **I gruppi non si trasportano nell'`AuthContext`**: si derivano da `user_id` con un join. Un elenco trasportato è un elenco che può essere stantio, e rimuovere qualcuno da un gruppo deve avere effetto subito.
- Nuove migrazioni con **prefisso a quattro cifre**.

---

## I viaggi utente di questa fase

Ogni task dichiara a quale contribuisce; a fine fase ognuno ha un test end-to-end in `journeys.rs`.

| # | Viaggio |
|---|---|
| **V5** | Condivido una cartella con un altro utente: lui la vede, e vede *solo* quella |
| **V6** | Creo un gruppo «Famiglia», ci metto tre persone, condivido con il gruppo; aggiungo un quarto membro e vede tutto senza che io ricondivida |
| **V7** | Creo un album, ci metto foto da cartelle diverse, lo condivido con un link protetto da password e con scadenza; un estraneo lo apre dal telefono |
| **V8** | Revoco il link: chi ce l'ha non entra più |
| **V9** | Un cliente carica le sue foto attraverso un link di upload; le trovo in una coda di revisione |

---

## Task 1: `permissions` e la visibilità ereditata

**Contribuisce a:** V5, V6

**Files:**
- Create: `crates/keeppix-db/migrations/0015_permissions.sql`
- Create: `crates/keeppix-db/src/permissions.rs`, `tests/permissions.rs`
- Modify: `crates/keeppix-db/src/visibility.rs`

**La query da estendere** (spec §3.2): alla clausola attuale «le librerie che
possiedi» si aggiunge l'unione con i sottoalberi condivisi, direttamente o
tramite gruppo.

**Il contratto da rispettare:** `VisibilityScope` continua a esporre **una
clausola SQL e i suoi parametri**, non un elenco di id. È il motivo per cui i
chiamanti della Fase 1 non devono cambiare — se cambiano, il contratto è stato
rotto e va corretto qui, non nei chiamanti.

- [ ] **Step 1: Test che falliscono**

Devono pinnare, in quest'ordine di importanza:

1. **Un utente senza permessi vede zero asset**, non tutti — il fallimento
   peggiore possibile di una funzione di visibilità è aprirsi invece di
   chiudersi. Va testato per primo e su ogni repository.
2. Condividere una cartella la rende visibile **con tutte le sottocartelle**.
3. `inherit = false` su un nodo **interrompe** l'ereditarietà da lì in giù.
4. Un permesso di gruppo vale per tutti i membri, e **aggiungere un membro ha
   effetto immediato** senza toccare `permissions`.
5. **Rimuovere un membro dal gruppo revoca l'accesso immediatamente** — è il
   test che smaschera un elenco di gruppi trasportato nel token.
6. Vince il permesso più alto: `viewer` da un gruppo + `editor` diretto = `editor`.
7. Chi riceve una cartella condivisa **non vede il percorso reale su disco**.
8. La visibilità è identica su REST, WebSocket e ricerca — stessa funzione,
   non tre copie.

- [ ] **Step 2: Eseguire, osservare i fallimenti, implementare**

Attenzione alla ricorsione: l'ereditarietà con interruzione non è un semplice
`path <@ prefisso`, perché un `inherit = false` su un nodo intermedio taglia il
sottoalbero. Valutare se esprimerlo con una CTE ricorsiva o con
`NOT EXISTS (permesso interrotto fra il nodo e la radice)`; misurare entrambe su
uno schema con 3.000 cartelle e registrare la scelta nel ledger.

- [ ] **Step 3: Budget di prestazione**

Il filtro di visibilità sta sul percorso più caldo del prodotto. Test con 50
permessi e 10.000 asset: **`GET /timeline` sotto 300 ms**. Se degrada, la
mitigazione è cachare i prefissi risolti per utente con invalidazione su
`permissions.changed` — **non** materializzare la visibilità.

- [ ] **Step 4: Verificare e committare**

---

## Task 2: Gruppi

**Contribuisce a:** V6

**Files:**
- Create: `crates/keeppix-db/src/groups.rs`, `crates/keeppix-api/src/routes/groups.rs`

Le tabelle `groups` e `group_members` **esistono dalla Fase 0** e sono vuote.

| Metodo | Percorso | Chi |
|---|---|---|
| `GET`/`POST` | `/api/v1/groups` | admin |
| `PATCH`/`DELETE` | `/api/v1/groups/{id}` | admin |
| `POST`/`DELETE` | `/api/v1/groups/{id}/members/{user_id}` | admin |

- [ ] **Step 1-4: TDD, implementare, verificare, committare**

Test che conta: **cancellare un gruppo non cancella i permessi in modo
silenzioso** — o cascata esplicita con conferma, o rifiuto se ha permessi
attivi. La cascata silenziosa toglie accesso a persone senza che nessuno lo
sappia.

---

## Task 3: Album

**Contribuisce a:** V7

**Files:**
- Create: `crates/keeppix-db/migrations/0016_albums.sql`, `src/albums.rs`, `crates/keeppix-api/src/routes/albums.rs`

Album **virtuali**: nessuno storage, una foto in dieci album pesa una volta.
Ordinamento manuale (`position`).

- [ ] **Step 1: Test che falliscono**

Includere: una foto in più album; rimuoverla da un album non la cancella;
l'ordinamento sopravvive al riordino; **un album può contenere foto che il
destinatario della condivisione non vedrebbe altrimenti** — ed è il punto: la
condivisione di un album concede accesso a quelle foto, non alle loro cartelle.

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 4: Il pannello permessi — condividere davvero

**Contribuisce a:** V5, V6

**Files:**
- Create: `crates/keeppix-api/src/routes/permissions.rs`
- Create: `frontend/src/components/SharePanel.vue`

| Metodo | Percorso | Note |
|---|---|---|
| `GET` | `/api/v1/permissions?object_type=&object_id=` | diretti **ed** ereditati, distinti |
| `POST` | `/api/v1/permissions` | concede a utente o gruppo |
| `DELETE` | `/api/v1/permissions/{id}` | revoca |
| `GET` | `/api/v1/permissions/explain?...` | **la catena del perché** |

`explain` è la funzione che rende il modello solo-allow difendibile: deve
rispondere «hai accesso perché il gruppo *Famiglia* ha ruolo *viewer* su
`/Foto/Vacanze`, ereditato in `/2024/Grecia`». Senza, «perché vedo questa
foto?» resta senza risposta e il modello diventa opaco quanto uno con i deny.

- [ ] **Step 1-4: TDD, implementare, verificare, committare**

Il pannello frontend è lo **stesso componente** per foto, cartella, album e
selezione multipla. Se diventano quattro componenti, la coerenza si perde entro
una fase.

---

## Task 5: Link pubblici

**Contribuisce a:** V7, V8

**Files:**
- Create: `crates/keeppix-db/migrations/0017_share_links.sql`, `src/share_links.rs`
- Create: `crates/keeppix-api/src/routes/share.rs`
- Modify: `crates/keeppix-domain/src/auth.rs` — la variante `Actor::ShareLink`, **prevista dalla Fase 0** e mai implementata

**Le proprietà di sicurezza, non negoziabili** (spec §6.1):

- Token da 32 byte casuali; **in database solo l'hash**. Un dump non apre i link.
- `X-Robots-Tag: noindex, nofollow` e `Referrer-Policy: no-referrer` su tutte
  le pagine pubbliche — il secondo impedisce che il token trapeli nel `Referer`.
- Rate limiting per token e per IP sui tentativi di password.
- Lookup a tempo costante, nessuna enumerazione.
- **`hide_metadata` attivo di default** sui link senza password.

- [ ] **Step 1: Test che falliscono**

Il più importante: **un link non deve concedere più di ciò per cui è stato
creato**. Un link su un album non deve permettere di raggiungere la cartella che
contiene quelle foto, né i loro vicini, né altri album. Testare provando a
uscire dal perimetro, non solo che il perimetro funzioni.

Poi: scadenza rispettata; `max_views` rispettato; revoca immediata; password
sbagliata indistinguibile da link inesistente; `hide_metadata` toglie le
coordinate **anche dall'API**, non solo dall'interfaccia.

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 6: `AuthContext::ShareLink` attraverso tutti i canali

**Contribuisce a:** V7, V8

Questo task non aggiunge funzioni: **verifica che il motore sia davvero uno
solo**. È il task che impedisce la classe di difetto più grave della fase.

- [ ] **Step 1: Test che falliscono**

Per **ogni** canale — REST, WebSocket, ricerca, media, e in Fase 5 WebDAV —
verificare che un `AuthContext::ShareLink` sia soggetto allo stesso filtro di
un utente:

```rust
#[tokio::test]
async fn a_share_link_context_cannot_read_outside_its_scope_on_any_channel() {
    // Stesso token, quattro canali: timeline, ricerca, media, websocket.
    // Tutti devono negare allo stesso modo.
}
```

- [ ] **Step 2: Verificare che non esista una scorciatoia**

`grep` per ogni percorso che legga asset e non passi da `visibility_scope`. Il
risultato atteso è vuoto; se non lo è, è un difetto Critical.

- [ ] **Step 3-4: Correggere se serve, committare**

---

## Task 7: Upload da ospite e coda di revisione

**Contribuisce a:** V9

**Files:**
- Modify: `crates/keeppix-api/src/routes/share.rs`
- Create: coda di revisione in `crates/keeppix-db/src/uploads.rs`

I file arrivano con flag `uploaded_by_guest` e finiscono in una coda che il
proprietario approva o scarta. **Nessuno riempie il disco a tua insaputa**:
`upload_quota_bytes` per link.

- [ ] **Step 1: Test che falliscono**

Includere: la quota è rispettata **durante** il caricamento, non dopo (un file
da 10 GB su una quota da 1 GB va interrotto a 1 GB, non accettato e poi
rifiutato); i file caricati non compaiono in timeline finché non sono
approvati; un ospite non può leggere ciò che altri hanno caricato.

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 8: Audit log

**Contribuisce a:** tutti

**Files:**
- Create: `crates/keeppix-db/migrations/0018_audit_log.sql`, `src/audit.rs`

Registra: creazione e revoca di condivisioni e link, **accessi ai link
pubblici**, cancellazioni dal disco, cambi di ruolo, **accessi dell'admin a
contenuti altrui**, login falliti.

**Debito della Fase 0 da saldare qui:** `sessions.ip` non è mai popolata.
Serve all'audit, e richiede la configurazione «proxy fidati» per leggere
`X-Forwarded-For` — popolarla con l'IP del proxy sarebbe peggio che lasciarla
vuota. Va fatta insieme, o dichiarata ancora differita con la ragione.

- [ ] **Step 1-4: TDD, implementare, verificare, committare**

---

## Task 9: Rate limiting

**Contribuisce a:** V7, V8

**Files:**
- Create: `crates/keeppix-api/src/ratelimit.rs`

**Debito della Fase 0**, differito proprio a questa fase con questa ragione:
è **lo stesso middleware** che serve ai link pubblici, e farlo prima
significava scriverlo due volte.

Copre: `/auth/login`, `/api/v1/setup`, i tentativi di password sui link
pubblici, e gli endpoint dei link in generale.

- [ ] **Step 1: Test che falliscono**

Includere: il limite scatta; si riapre dopo la finestra; **è per IP e per
token separatamente** (un attaccante con molti IP non deve poter forzare un
token, e un IP non deve poter esaurire il limite di tutti); un utente
autenticato legittimo non viene mai limitato sul percorso normale.

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 10: Frontend — condivisione e gestione

**Contribuisce a:** V5, V6, V7

**Files:**
- Create: `frontend/src/views/SharesView.vue`, `GroupsView.vue`, `AlbumsView.vue`
- Create: `frontend/src/views/public/SharedView.vue` — la pagina che vede l'estraneo

La pagina pubblica è **un'applicazione a sé**: nessuna sessione, nessuna barra
di navigazione, nessun accesso al resto. Deve poter essere aperta da un telefono
di qualcuno che non ha mai sentito parlare di Keeppix.

Pagina **«Condivisioni»**: tutto ciò che esce di casa, chi vede cosa, tutti i
link attivi con ultimo accesso, revoca di massa.

- [ ] **Step 1-4: TDD (vitest), implementare, verificare, committare**

Vincolo: la pagina pubblica va in un **chunk lazy separato**, e il bundle
d'ingresso resta sotto 150 KB gzip.

---

## Task 11: I viaggi V5-V9

**Files:**
- Modify: `crates/keeppix-api/tests/journeys.rs`

Cinque test end-to-end nella forma introdotta dalla Fase 2R. Il più importante
è **V8**: la revoca deve avere effetto **immediato**, e il test deve provarlo
usando il link *dopo* la revoca, non solo verificando che la riga sia cambiata.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Criteri di completamento

- [ ] `cargo test --workspace -- --test-threads=1` verde; clippy e fmt puliti.
- [ ] I viaggi V5-V9 passano, oltre a V1-V4 della Fase 2R.
- [ ] Budget: `GET /timeline` sotto 300 ms con 50 permessi e 10.000 asset.
- [ ] **Nessun percorso legge asset senza passare da `visibility_scope`** —
      verificato per `grep`, non per campione.
- [ ] Un utente senza permessi vede **zero** asset su ogni canale.
- [ ] Un link pubblico revocato smette di funzionare immediatamente.
- [ ] `hide_metadata` toglie le coordinate anche dalle risposte API.
- [ ] Provato a mano da browser: condivisione di una cartella a un secondo
      utente, e apertura di un link pubblico da una finestra anonima.
- [ ] `scripts/field-test.sh` ancora verde entro i budget.
- [ ] CI verde sulla PR.

## Debiti saldati in questa fase

| Voce | Task |
|---|---|
| Rate limiting su login e setup | 9 |
| `sessions.ip` mai popolata | 8 |
| `refresh`/`rotate` non ricontrollano `disabled_at` | 1 (con i permessi) |
| `logout` risponde 204 anche se `revoke` fallisce | con `/auth/devices`, Task 4 |

## Cosa NON è in Fase 3

Mappa e geocoding (Fase 4), WebDAV (Fase 5), video, backup, TOTP e sync delta
(Fase 6). La **selezione collaborativa** sugli album condivisi — i pick di più
utenti uniti con l'avatar di chi li ha messi — è descritta nella spec della
Fase 2 ma richiede la condivisione: va fatta qui **oppure** dichiarata
differita alla Fase 6 con la ragione. Non lasciarla in silenzio.
