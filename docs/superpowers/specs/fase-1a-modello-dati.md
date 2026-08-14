# Fase 1a — Modello dati dell'ingestione

**Stato:** ✅ **chiusa sul branch `fase-1`** — vedi
[`.superpowers/sdd/2026-08-14-keeppix-fase-1a/progress.md`](../../../.superpowers/sdd/2026-08-14-keeppix-fase-1a/progress.md)
**Dipende da:** Fase 0
**Chiusa quando:** si creano librerie, cartelle e asset via repository e li si
interroga con i permessi applicati

Questa spec contiene le **decisioni** del modello dati. Il piano le trasforma in
task eseguibili; se i due divergono, **vince questa**.

1a non ha interfaccia e non tocca un file su disco. È deliberato: **il modello
dati va sbagliato adesso**, quando correggerlo costa una migrazione e non una
riscansione di un terabyte.

---

## 1. Le quattro decisioni portanti

### 1.1 L'identità dell'asset è il percorso, non il contenuto

```sql
assets (folder_id, filename)   -- UNIQUE: è l'identità
        content_hash bytea     -- indicizzato, NON unico
```

La stessa foto in due cartelle sono **due asset distinti**, con cancellazioni
indipendenti.

**Perché non l'hash.** Un'identità basata sul contenuto renderebbe ambigua la
cancellazione: «cancella questa foto» dovrebbe cancellare tutte le copie? Nessuna
risposta è ovvia, e l'utente pensa in termini di cartelle.

È anche il modello di Immich, che usa identità per percorso nelle librerie
esterne e deduplica solo negli upload.

**Cosa si conserva comunque grazie all'hash:**

| | Come |
|---|---|
| Derivati generati una volta sola | indicizzati per `content_hash`: cinque copie, un thumbnail |
| Pagina Duplicati | gruppi con `count(*) > 1` per hash, spazio recuperabile |
| Rilevamento spostamenti | cancellazione + creazione con stesso `(hash, size)` → *move*, con trasferimento di metadati, rating e album |

Lo spostamento è una **regola esplicita, testabile e loggata**, non un effetto
collaterale dell'identità.

### 1.2 L'albero delle cartelle è `ltree` con etichette numeriche

```sql
folders (id, library_id, parent_id, name, path ltree, depth)
CREATE INDEX folders_path_gist ON folders USING gist (path);
```

`path <@ '1.7'` è **una singola condizione indicizzata** per «tutto ciò che sta
sotto questa cartella». È la query su cui poggiano visibilità, timeline e
condivisione.

**Le etichette sono numeri progressivi per libreria, non nomi.** Due ragioni:

1. `ltree` ammette solo `[A-Za-z0-9_-]`: «Matrimonio Rossi 2024» non è
   un'etichetta valida.
2. Tenere i nomi fuori dal percorso evita di dover interpolare testo
   dell'utente in una query.

I numeri vengono da una sequenza **per libreria** (`libraries.next_folder_seq`),
non globale: due librerie possono avere entrambe `1.2.3`.

### 1.3 Nessun percorso assoluto denormalizzato sugli asset

Il percorso su disco si ricostruisce risalendo l'albero e concatenando i `name`
sotto `libraries.root_path`.

**Perché**: spostare una cartella con 40.000 foto tocca le righe di `folders`,
non quelle di `assets`. Con un path denormalizzato sarebbe un `UPDATE` di 40.000
righe a ogni `mv`.

Lo spostamento di un sottoalbero è **una sola query**:

```sql
UPDATE folders
   SET path  = $new_prefix::ltree || subpath(path, nlevel($old_prefix::ltree)),
       depth = nlevel($new_prefix::ltree) + nlevel(path) - nlevel($old_prefix::ltree)
 WHERE library_id = $library AND path <@ $old_prefix::ltree;
```

**Il ciclo va rifiutato prima dell'UPDATE**: se il nuovo genitore discende dalla
cartella che si sta spostando, il sottoalbero si scollega e diventa
irraggiungibile da qualsiasi radice. `DbError::Conflict`.

### 1.4 I metadati originali sono immutabili

`asset_exif` porta ciò che è stato letto dal file e **non viene mai riscritto**.
Le modifiche dell'utente vivranno in `asset_overrides` (Fase 2), e il valore
mostrato sarà `COALESCE(override, exif)`.

Conseguenze: «ripristina originale» esiste sempre, e l'editing batch su 5.000
file è una `INSERT … ON CONFLICT UPDATE` invece di 5.000 riscritture su disco.

---

## 2. Schema

Tre migrazioni: `0004_libraries_folders.sql`, `0005_assets.sql`,
`0006_change_log.sql`.

### 2.1 `libraries`

`id` · `name` · `owner_id` → users ON DELETE CASCADE · `root_path` ·
`scan_enabled` · `exclude_patterns text[]` · `status` (CHECK
`active`/`offline`) · `last_scan_at` · `next_folder_seq` · `created_at` ·
`updated_at`

**Indice unico su `root_path`**: due librerie che indicizzano lo stesso albero
produrrebbero asset duplicati con cancellazioni ambigue.

Lo stato `offline` significa «percorso non raggiungibile»: la scansione si ferma
e **non viene cancellato nulla**. Un disco non montato non è una libreria
svuotata.

### 2.2 `folders`

`id` · `library_id` → CASCADE · `parent_id` (self-FK) → CASCADE · `name` ·
`path ltree` · `depth` · `created_at`

Indici: GiST su `path`; unico su `(library_id, path)`; unico su
`(parent_id, name)` `WHERE parent_id IS NOT NULL` (due sorelle non possono
chiamarsi uguale); unico su `(library_id)` `WHERE parent_id IS NULL` (una sola
radice per libreria — serve un indice separato perché in Postgres `NULL` non è
uguale a `NULL`).

La radice ha `name` vuoto e `parent_id` nullo.

### 2.3 `assets`

`id uuid` · `folder_id` → CASCADE · `filename` · `content_hash bytea` ·
`size_bytes` · `mtime` · `inode` · `kind` (CHECK) · `status` (CHECK) ·
`error_detail` · `taken_at_utc` · `tz_offset_minutes` · `width` · `height` ·
`duration_ms` · `location geography(Point,4326)` · `place_id` ·
`location_source` (CHECK) · `stack_id` · `created_at` · `updated_at`

Indici:

| Indice | A cosa serve |
|---|---|
| unico `(folder_id, filename)` | l'identità |
| `content_hash WHERE NOT NULL` | duplicati, spostamenti |
| `(taken_at_utc DESC, id DESC) WHERE status='indexed'` | **la timeline**, con keyset pagination |
| `folder_id` | navigazione cartelle |
| `status WHERE status IN ('discovered','error')` | coda e pagina Problemi |
| GiST su `location WHERE NOT NULL` | mappa (Fase 4) |

**Colonne predisposte e vuote**: `location`, `place_id`, `location_source`,
`stack_id`, `duration_ms`. Aggiungere colonne a una tabella con 200.000 righe
costa; prevederle no.

`status`: `discovered` (trovato dal walker) → `indexed` (metadati e derivati
pronti) · `offline` (file sparito, **non** cancellato: se torna, tornano rating
e album) · `error` · `trashed`.

### 2.4 `asset_exif`

`asset_id PK` → CASCADE · `raw jsonb` · `camera_make` · `camera_model` ·
`lens` · `iso` · `f_number` · `exposure` · `focal_length` · `parsed_at`

I campi estratti sono denormalizzati accanto al `jsonb` perché sono quelli su
cui si filtra.

### 2.5 `folder_month_counts`

`(folder_id, month)` PK · `asset_count`

Creata **qui** anche se i trigger arrivano in 1c: crearla dopo significherebbe
ricalcolarla su tutta la libreria.

### 2.6 `change_log`

`seq bigserial PK` · `entity` (CHECK) · `entity_id` · `op` (CHECK) · `at`

Alimentato da trigger su `assets`. **Va attivato da subito**: accenderlo dopo
significherebbe che tutto ciò che è successo prima è invisibile alla
sincronizzazione del client mobile (Fase 6).

**Il dettaglio che va preso bene:** una transazione con `seq` più basso può
committare **dopo** una con `seq` più alto. Un client che legge fino all'ultimo
`seq` visto si perderebbe righe. Il cursore restituito va **arretrato al limite
delle transazioni certamente concluse** (`pg_snapshot_xmin`).

Il test che lo dimostra apre **due transazioni sovrapposte** e le committa in
ordine inverso.

---

## 3. La funzione di visibilità

Nasce qui, e la sua **firma è un contratto congelato**: la Fase 3 la estenderà
con la tabella `permissions` senza che i chiamanti cambino.

In 1a la visibilità è: *«le librerie che possiedi, o tutte se sei admin»*.
In Fase 3 diventerà: *«più i sottoalberi condivisi con te o con i tuoi gruppi»*.

**Requisito di progettazione:** `VisibilityScope` deve esporre **un metodo che
produce la clausola SQL e i suoi parametri**, non l'elenco grezzo degli id.
Altrimenti la Fase 3 dovrà riscrivere ogni chiamante.

**Nessuna tabella di visibilità materializzata per utente.** Cambiare un
permesso deve avere effetto immediato, non innescare la ricostruzione di niente.

### 3.1 La regola che vale ovunque

> Un utente che sonda un id che non gli appartiene riceve **`Forbidden`**, mai
> `NotFound` — **anche quando l'id non esiste**.

Altrimenti l'endpoint diventa un oracolo di esistenza: si scopre quali id sono
validi sondandoli e confrontando le risposte.

Vale per librerie, cartelle e asset. `NotFound` si restituisce solo a un admin
che chiede un id inesistente.

---

## 4. Repository

Tutti in `keeppix-db`, tutti con `AuthContext` come primo parametro. Le
eccezioni sono quelle chiamate dallo **scanner**, che non agisce per conto di un
utente, e ognuna deve dichiarare il motivo nel doc comment:

| Repository | Metodi con eccezione | Perché |
|---|---|---|
| `LibraryRepo` | `mark_scanned` | la chiama lo scanner |
| `FolderRepo` | `ensure_root`, `ensure_child`, `ensure_path` | idem |

Le `ensure_*` devono essere **idempotenti sotto concorrenza**:
`INSERT … ON CONFLICT DO NOTHING` seguito da rilettura, **non** un `SELECT`
seguito da `INSERT`. Riscansionare non deve duplicare cartelle.

---

## 5. Mapping riga → struct

**`#[derive(sqlx::FromRow)]`**, con una `into_domain()` separata che converte al
tipo di dominio validando ciò che il database non può garantire.

Il nucleo di R4 non cambia: restano le **forme funzione** di sqlx, nessuna macro
`query!`, nessuna `.sqlx/`, nessun `SQLX_OFFLINE`. `FromRow` è un derive di
mapping colonna→campo, non una verifica di schema a compile time.

**Il Task 1 converte anche le tre struct della Fase 0**, per non lasciare due
stili accanto: una divergenza fra crate è esattamente il tipo di incoerenza che
la review finale della Fase 0 ha censurato altrove.

---

## 6. Checkpoint: prestazioni della suite

Non è un task, è una decisione da prendere **durante** l'esecuzione, con innesco
al **Task 5**.

Numeri di partenza misurati: CI `backend` **10m28s** a cache fredda, **5m23s** a
caldo, per 107 test. La causa non è la compilazione: **ogni test avvia un
container Postgres**, e `--test-threads=1` li esegue in sequenza.

1a ne aggiunge ~45-55. Dopo il Task 5 si confronta il tempo di `keeppix-db` con
quello di fine Fase 0: se il rapporto tempo/test è rimasto lineare si rimanda,
altrimenti si agisce prima di scrivere altri 30 test con lo stesso schema.

Strade valutate nel piano: container per binario con schema per test ·
`#[sqlx::test]` · frammentazione dei job CI. Qualunque si scelga va applicata
**anche** ai test della Fase 0, non solo ai nuovi.

---

## 7. Cosa NON è in Fase 1a

Coda job, worker, profili energetici, `keeppix-media`, walker, hashing,
derivati, watcher, endpoint HTTP, WebSocket, timeline, frontend. Sono
[Fase 1b](fase-1b-ingestione.md) e [Fase 1c](fase-1c-timeline.md).
