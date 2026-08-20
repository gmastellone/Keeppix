# Analisi GAP — interfaccia contro backend reale

**Fonti.** UI: `documento-funzionale-ui.md` (70 schermate, 64 paragrafi *"Dati necessari"*) e il
prototipo `keeppix-mockup.html`, navigato dal vivo. Backend: tip di `origin/fase-6`, superficie
letta dal router (`crates/keeppix-api/src/lib.rs::api_routes`), non dall'OpenAPI — che è
incompleto (vedi §5).

**Legenda.** ✅ c'è e basta · ⚠️ c'è ma va esteso · ❌ manca del tutto · 🔵 previsto in una fase
già specificata · 🔷 da verificare (dichiarato, non ispezionato).

---

## 1. Le otto richieste che il documento marca come "toccano il backend"

| # | Richiesta | Verdetto | Evidenza |
|---|---|---|---|
| 1 | Proporzioni di **tutti** gli scatti di una vista senza miniature | ❌ | `/timeline/buckets` dà solo `{month,count}`; `/timeline` è paginato a 200 (`timeline.rs`, `limit.clamp(1,200)`) con `AssetView` grasso. Per 214k scatti: ~1.070 richieste. **Il documento dichiara che se non è realizzabile cambia il disegno.** |
| 2 | Operazioni di massa con **esito per-foto** e ragione | ❌ | `POST /flags/batch` → `204 No Content`. `POST /metadata/batch` → solo `{batch_id}`. |
| 3 | Conteggio reale per mese, aggregato | ✅ | `GET /timeline/buckets` con `library` e `bbox`. |
| 4 | Una foto è una **pila** (RAW+JPEG = uno scatto) | ⚠️ | `stacks` + `/assets/{id}/stack` esistono. Ma `AssetView` **non espone lo stack** e la timeline non collassa: due tessere per uno scatto. Manca anche il campo per il badge `RAW`/`RAW+JPEG` (SP-15). |
| 5 | Provenienza di ogni etichetta (IA vs umano) | 🔵 Fase 7 | `asset_tags.source` + `state` (emendato). |
| 6 | Eliminare ha **tre destinazioni**, nessun default | ✅ singola / ❌ massa | `DiskAction::{Kept,MovedToTrash,Purged}` (`domain/trash.rs:11`) è **esattamente** SP-18, obbligatorio, `Purged` solo owner/admin. Ma esiste solo su `DELETE /assets/{id}`. |
| 7 | ≥4 nature di fallimento distinguibili | ❌ | `Problem` (RFC 7807) esiste, ma nessun insieme chiuso di `reason` su cui il frontend possa decidere se mostrare "Riprova". |
| 8 | Volti **mai** su link pubblico, non configurabile | 🔵 Fase 8 | `share.rs::public_assets` costruisce la vista pubblica da `domain_assets` e già filtra la posizione con `hide_metadata`. Nessun volto esiste oggi, quindi nulla trapela — ma **manca il test che lo garantisca per costruzione** quando la Fase 8 aggiungerà i volti. |

---

## 2. Concetti dell'interfaccia che nel backend **non esistono affatto**

Questi non sono dettagli: sono nozioni di prima classe che attraversano molte schermate.

### 2.1 «Preferito» — ❌ non esiste
Zero occorrenze di `favorite`/`preferit` in `keeppix-domain`, `keeppix-db`, `keeppix-api`.
`AssetFlagsBody` (`routes/flags.rs:19`) ha **solo** `rating`, `pick`, `color_label`.

Dove la UI lo usa: il cuoricino su **ogni** tessera (SP-1, §10) · la sezione **"Preferiti"** in
sidebar, che è una vista intera (§9, *"71 foto, da tutte le cartelle"*) · l'azione di massa nella
barra di selezione (SP-2, §12) · il chip "Preferiti" in Cerca (§23) · una condizione degli album
dinamici (§43) · la modifica in blocco (§13).

**Non è `Pick`.** `Pick::{None,Pick,Reject}` è lo stato di culling dentro un lotto; il documento
è esplicito nel glossario: *"sono stati del culling, non della libreria"*. Una foto può essere
`Pick` e non preferita, e viceversa.

→ **Nuovo campo `favorite boolean` in `asset_flags`**, con indice parziale, esposto in
`AssetView`, scrivibile singolo e in blocco, filtrabile (`SearchNode::Favorite`).

### 2.2 Conteggio foto per cartella — ❌ non esiste
`FolderView` (`routes/folders.rs:14`) è `{id, library_id, parent_id, name, depth}`.
La sidebar mostra `Urbino 556`, `Lago di Braies 110`, `Chioggia e Venezia 246` (§2), e la
sotto-pagina mobile "Cartelle" mostra `"556 foto"` per scheda (§6).

→ `asset_count` come aggregato su `/folders/tree`, in cache `moka` con invalidazione esplicita
su import e cestinamento.

### 2.3 Titolo e conteggio di un link pubblico — ⚠️
`CreateLinkRequest` (`routes/share.rs:29`) è **più ricco** di quanto la UI chieda: `password`,
`expires_at`, `max_views`, `allow_download`, `allow_original`, `allow_upload`,
`allow_cdn_cache`, `hide_metadata`. Ottimo. Manca solo, in lettura, il **numero di elementi**
del link (§29 mostra *"246 elementi"*, *"84 elementi"*).

---

## 3. Matrice schermata → endpoint

### Parte I — Struttura (§1-7)
| Schermata | Serve | Verdetto |
|---|---|---|
| §1 Shell, §3 Menu account, §7 Router | solo stato di sessione | ✅ nessun dato server |
| §2 Sidebar | cartelle con **nome + conteggio** | ❌ conteggio (§2.2) |
| | badge culling (foto da valutare su tutti i lotti) | 🔵 Fase 9 |
| | badge revisione (tag + volti in attesa) | 🔵 Fasi 7/8 |
| | colore avatar utente | ❌ preferenze utente |
| | spazio libero/totale del server | ❌ nessun endpoint |
| §4 Breadcrumb | nomi di cartella/album/persona/lotto correnti | ✅ / 🔵 |
| §5-6 Shell mobile, "Altro" | come sopra + numero cartelle | ❌ conteggio |

### Parte II — Libreria (§8-13)
| Schermata | Serve | Verdetto |
|---|---|---|
| §8 Timeline | proporzioni di tutta la vista | ❌ **richiesta #1** |
| | conteggio per mese | ✅ `/timeline/buckets` |
| | per foto: id, cartella, mese/giorno, nome file, proporzione, miniatura | ✅ `/timeline` + `/media/thumb` |
| | è RAW e di che tipo (badge) | ❌ manca `raw_kind` |
| | è preferita | ❌ (§2.1) |
| | densità griglia scelta in Impostazioni | ❌ preferenze utente |
| §9 Preferiti | tutte le foto con preferito vero + totale | ❌ (§2.1) |
| §10 Tile (SP-1) | come §8 | come §8 |
| §11 Filtro rapido (SP-3) | 6 assi: tipo, persone, tag, categorie, fotocamera, cartelle | ✅ tipo/fotocamera/cartella · 🔵 tag/categorie (7), persone (8) |
| §12 Selezione (SP-2) | preferito di massa | ❌ |
| | album di appartenenza delle selezionate | ⚠️ nessun endpoint "in quali album sta questo asset" |
| | eliminazione di massa a 3 vie | ❌ |
| §13 Modifica in blocco | rating, pick, preferito, cartella, titolo su N foto | ⚠️ rating/pick via `/flags/batch`; titolo via `/metadata/batch`; **cartella** = spostamento → 🔵 Fase 9; preferito ❌ |

### Parte III — Culling (§14-17)
🔵 **interamente Fase 9.** Nota: il badge di sidebar e il selettore rapido di lotto richiedono
un aggregato "foto da valutare per lotto" che la spec Fase 9 deve prevedere esplicitamente.

### Parte IV — Dettaglio (§18-22)
| Serve | Verdetto |
|---|---|
| EXIF completo (fotocamera, obiettivo, diaframma, tempo, ISO, pixel) | ✅ `asset_exif` |
| titolo, posizione (impostata / ereditata da cartella / assente) | ✅ `/assets/{id}/metadata` (`EffectiveMetadataView` ha `title`, `location`, `place_id`) |
| dimensione MB di RAW e JPEG affiancati | ⚠️ `size_bytes` c'è per asset; serve per membro della pila |
| album di appartenenza (manuali **e** dinamici calcolati) | ❌ |
| volti confermati con riquadro | 🔵 Fase 8 |
| tag confermati/suggeriti con provenienza | 🔵 Fase 7 |
| vicinato per frecce e filmino | ✅ derivabile da `/timeline` |
| download originale, rotazione | ✅ `/media/original/{id}` · ❌ rotazione |

### Parte V — Ricerca, mappa, condivisione (§23-30)
| Serve | Verdetto |
|---|---|
| assi di ricerca | vedi §4 |
| ricerche salvate | ✅ `/saved-searches` |
| suggerimenti **tipizzati** (tag col pallino colorato, fotocamera, cartella, ISO, anno, paese) | ⚠️ `/search/suggest` restituisce `Vec<String>` — **stringhe piatte**, e solo da `camera_model` e `filename` (`db/search.rs:107`). La UI deve sapere *di che tipo* è un suggerimento per creare la pillola giusta |
| cluster mappa con copertina e conteggio | ⚠️ `MapClusterView` è `{lat, lon, count, cover_asset_id, clustered}`. Il popover (§27) chiede in più l'**etichetta leggibile del luogo** e l'**id di destinazione** per aprire la cartella |
| luoghi noti, geocodifica | ✅ `/places/suggest`, `/places/reverse` |
| regioni scaricabili con peso e stato | ✅ `/map/regions` |
| persone con accesso, ruolo, **ereditarietà** (gruppo + cartella) | ✅ `/permissions/explain` — nato esattamente per questo |
| link pubblici con scadenza/password/download/elementi | ✅ tranne conteggio elementi (§2.3) |
| **condivisi con me** | ❌ non esiste. `permissions` è interrogabile solo **per oggetto** (`ListQuery{object_type, object_id}`); la scheda "Condivisi con me" (§29) chiede l'inverso: tutti gli oggetti condivisi **con l'utente corrente**, con proprietario e ruolo. Serve `GET /shared-with-me` |

### Parte VI — Persone e volti (§31-40)
🔵 **interamente Fase 8.**

### Parte VII — Album e manutenzione (§41-50)
| Serve | Verdetto |
|---|---|
| album: conteggio membri, intervallo date, condiviso, tinta, dinamico | ❌ `AlbumView` è `{id,name,description,owner_id,cover_asset_id,created_at,updated_at}` |
| aggiungi a album in blocco | ⚠️ esiste solo `POST /albums/{id}/assets` per singolo asset |
| cestino con **giorni residui** | ✅ `days_remaining` in `trash_item_view` |
| duplicati: gruppi per hash, motivo, MB, quale tenere, modalità eliminazione | ✅ **completo** — `POST /duplicates/{hash}/resolve` prende `keep` **e** `disk_action`. Migliore del prototipo, che raccoglie la modalità e non la usa |
| problemi: elenco piatto con gravità, titolo, descrizione, azioni proposte | ⚠️ `ProblemsView` è `{offline_libraries, failed_jobs, error_assets}` — materia prima, non problemi composti. Mancano gravità, testo naturale, azioni, e l'azione "Riprova connessione" |
| dialog eliminazione a 3 opzioni | ✅ modello / ❌ forma di massa |

### Parte VIII — Organizzazione automatica (§51-59)
🔵 **interamente Fase 7**, con i due emendamenti già applicati alla spec (soglia **per tag**;
stato esplicito `proposed/confirmed/rejected`).

### Parte IX — Preferenze (§60-64)
| Serve | Verdetto |
|---|---|
| tema, densità griglia (2 valori), 3 notifiche, lingua | ❌ nessuna preferenza utente persistita |
| regioni mappa | ✅ |
| cartella radice culling | 🔵 Fase 9 |
| livello IA, modello, ms/foto misurati | 🔵 Fase 7 |
| riconoscimento volti on/off + "elimina tutti i dati dei volti" | 🔵 Fase 8 |
| profilo: nome, email, ruolo, avatar | ⚠️ `UserView` (`auth.rs:20`) è `{id, username, display_name, email, role, locale, disabled_at}` — copre nome/email/ruolo. Mancano: **colore avatar** (→ preferenze), **nome del server**, **data dell'ultima modifica password** (§61 mostra *"Ultima modifica: 3 mesi fa"*) |
| 2FA | ✅ TOTP completo (Fase 6) |
| **sessioni attive** con dispositivo, ultimo accesso, revoca singola e "esci dagli altri" | ❌ tabella `sessions` c'è, endpoint no |
| cambio password | ✅ `/users/me/password` |
| rinomina con formula | 🔵 Fase 9 |

### Parte X — Scala, caricamento, errore (§65-70)
| Serve | Verdetto |
|---|---|
| geometria (§66) | ❌ **richiesta #1** |
| granularità delle richieste per fallire una alla volta (§67) | ✅ già granulari |
| 4 nature di errore (§68) | ❌ **richiesta #7** |
| riuscita parziale (§69) | ❌ **richiesta #2** |

---

## 4. Assi di ricerca: presenti vs richiesti

`SearchNode` (`crates/keeppix-db/src/search.rs:27`) ha oggi:
`And, Or, Not, Text, Type, Camera, Lens, Iso, Year, Folder, HasGps`.

| Asse mancante | Serve a | Fase |
|---|---|---|
| `Favorite` | chip Cerca, Preferiti, album dinamici | **10** |
| `Rating{cmp,value}` | album dinamici, filtri | **10** |
| `DateRange`, `Day`, `Month` | il placeholder dice *"Cerca per data…"*; album dinamici | **10** |
| `Country` | pillola Paese (nel prototipo si crea ma non filtra) | **10** |
| `Aperture`, `Shutter` | condizioni album dinamici | **10** |
| `Tag`, `Category` | chip SP-3, pillole, album dinamici | 7 |
| `Person` | chip SP-3 (oggi disabilitato apposta) | 8 |
| `Pick` | filtro cartella+stato | 9 |
| `Semantic` | ricerca per descrizione libera | 7 |

---

## 5. Audit di query e indici

### 5.1 Indici esistenti: 43 su 32 migrazioni
Coprono bene ciò per cui sono nati. Notevoli: `assets_timeline_idx (taken_at_utc DESC, id DESC)`,
`folders_path_gist (path)` per la visibilità `ltree`, `assets_location_gist` parziale,
`assets_filename_trgm` GIN, e i trigram di Fase 6 su `asset_exif.camera_model` e `.lens`.

### 5.2 Il difetto concreto: la timeline non ha un indice che copra il suo predicato

`TimelineRepo::page` (`crates/keeppix-db/src/timeline.rs:134`) esegue:

```sql
... WHERE <visibility> AND a.status = 'indexed' AND a.kind <> 'unknown'
    AND a.taken_at_utc >= $1 AND a.taken_at_utc < $2 AND <keyset>
ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC LIMIT $5
```

L'indice disponibile è `assets_timeline_idx (taken_at_utc DESC, id DESC)`: copre `ORDER BY`,
intervallo e keyset, ma **non** `status` né `kind`, che restano filtri applicati **dopo** il
recupero dalla heap. E `assets_status_idx` è parziale su `('discovered','error')` — cioè
**l'insieme opposto** a quello che la timeline cerca: non aiuta mai.

Questo risponde anche alla domanda rimasta aperta sull'indice `status <> 'trashed'`: il predicato
reale non è `<> 'trashed'` ma `= 'indexed'`, quindi l'indice giusto è parziale sul valore cercato.

```sql
CREATE INDEX assets_timeline_indexed_idx ON assets (taken_at_utc DESC, id DESC)
    WHERE status = 'indexed' AND kind <> 'unknown';
```

### 5.3 Indici richiesti dai concetti nuovi
```sql
-- preferito: minoranza della libreria (~8% nel prototipo) → indice parziale
CREATE INDEX asset_flags_favorite_idx ON asset_flags (user_id, asset_id) WHERE favorite;
CREATE INDEX asset_flags_rating_idx   ON asset_flags (user_id, rating)   WHERE rating > 0;
-- geometria: index-only scan, niente accesso alla heap
CREATE INDEX assets_geometry_idx ON assets (folder_id, taken_at_utc DESC, id DESC)
    INCLUDE (width, height) WHERE status = 'indexed';
```

### 5.4 Aggregati che diventeranno N+1 se non li si progetta ora
Quattro conteggi che l'interfaccia mostra **per riga di un elenco**, cioè il posto dove un
`COUNT` per elemento diventa N+1:

| Aggregato | Dove | Rimedio |
|---|---|---|
| foto per cartella | sidebar, ogni render | `GROUP BY folder_id` singolo + cache `moka` |
| membri per album | griglia album | `GROUP BY album_id` singolo |
| elementi per link pubblico | Condivisioni | idem |
| foto per tag / per persona / da valutare per lotto | Fasi 7/8/9 | stessa regola, da fissare nelle rispettive fasi |

La cache `moka` introdotta in Fase 6 (`keeppix-db/src/lib.rs:96`) è senza TTL e con invalidazione
esplicita: gli aggregati vanno lì **con lo stesso patto**, perché un conteggio scaduto qui è un
numero sbagliato mostrato all'utente, non un rallentamento.

### 5.5 Cosa è già ottimizzato e non va toccato
- Paginazione **keyset** ovunque (`taken_at|id`), mai `OFFSET`.
- Visibilità risolta una volta (`VisibilityScope::resolve`) e compilata in `WHERE`, non per riga.
- `POST /viewport` esiste già per promuovere la generazione delle miniature che si stanno
  guardando: il frontend deve usarlo, non reinventarlo.
- Derivati con perdita ridotti dal 3,3% allo 0,4% (Fase 2R3).
- `compile_for_sql` è parametrizzato con guardia di profondità: niente SQL costruito per
  concatenazione di input.

---

## 6. OpenAPI incompleto

Lo spec generato copre **68 path / 81 operazioni**, ma il router ne registra di più: mancano
del tutto `albums`, `share`, `groups`, `permissions`, `audit`, `backup`, `restore`, `upload`,
`health`. I client generati non li vedono. → Fase 10, Task 10, con un controllo in CI che
fallisce se una rotta registrata non compare nello spec.

---

## 7. Copertura di questa analisi

Verificati riga per riga: le 64 sezioni *"Dati necessari"*, la Parte X (scala, caricamento,
errore, riuscita parziale), la Parte XI (i 30 pattern condivisi) e la Parte XII (assunzioni e
domande aperte). Lato backend: il router completo, l'OpenAPI generato, e il codice di timeline,
flags, trash, stacks, search, albums, folders, problems, duplicates, share, permissions, map,
auth, più l'inventario di tutti i 43 indici sulle 32 migrazioni.

I quattro punti lasciati aperti nella prima stesura — `/search/suggest`, `/map/clusters`,
`/auth/me`, "condivisi con me" — sono ora **verificati**, e tre dei quattro erano lacune reali
(suggerimenti non tipizzati, cluster senza etichetta né destinazione, "condivisi con me"
inesistente).

Resta fuori, deliberatamente, il dettaglio d'interazione dei sottotitoli 3-8 di ogni schermata
(controlli, mouse, tastiera, animazioni, stati, navigazione): non tocca il contratto col
backend, ed è il materiale della **Fase 11**, dove va letto per intero.

---

## 8. Il verso opposto: funzioni del backend che l'interfaccia **non ha disegnato**

La Parte XII del documento elenca ciò che il prototipo *"non copre affatto"*, avvertendo che
l'assenza **non va letta come "non serve"**. Incrociandola con ciò che il backend ha già
costruito emergono funzioni spedite e senza interfaccia. Vanno disegnate seguendo il documento
funzionale e il brand, non lasciate scoperte.

| Funzione del backend | Fase | Stato UI | Nota |
|---|---|---|---|
| **Video**: probe, transcodifica HLS, player, poster | 6 | ❌ **nessun disegno** | Il documento dichiara: *"l'intero disegno assume fotografie"*. Ma la Fase 6 ha spedito la pipeline video completa. È il buco più grande in questo verso |
| Importazione iniziale: scelta percorsi, prima scansione, avanzamento | 1 | ❌ | `GET/POST /libraries/{id}/scan`, `/libraries/preview` esistono |
| Amministrazione: utenti, gruppi, permessi | 3 | ⚠️ parziale | Esistono `UsersView`, `GroupsView` nel frontend reale, ma il documento non le descrive |
| Backup e ripristino | 6 | ⚠️ parziale | `BackupView` esiste nel frontend; nessuna schermata nel documento |
| **Vista pubblica di un link condiviso** | 3 | ❌ | Il documento descrive come si *crea* una condivisione, non cosa vede chi riceve il link |
| Configurazione WebDAV / app-password | 5 | ❌ | `/users/me/app-passwords` esiste |
| Registro di controllo (audit) | 3 | ❌ | `GET /audit` esiste |
| Comportamento offline e sincronizzazione | 5-6 | ❌ | `sync/delta` + service worker esistono |
| Notifiche vere | — | ❌ | Le *preferenze* sono disegnate (§60), il meccanismo non esiste da nessuna delle due parti |

## 9. Avanzamento e annullamento delle operazioni lunghe

Il documento lo dichiara aperto (§2.1 della Parte XII): *"restano senza stato di avanzamento le
operazioni lunghe sul disco: rinomina di massa, spostamenti, scansioni. Lì non basta uno
scheletro — serve un avanzamento con una percentuale, e probabilmente la possibilità di annullare
a metà. Non è disegnato."*

È una richiesta al backend, non solo all'interfaccia, e **il canale c'è già**: il WebSocket
(`/ws`, `/ws/ticket`) è nato come canale di notifica, e il contratto congelato dice che non è
fonte di verità — perfetto per un avanzamento. Serve però:
- un identificativo di operazione lunga restituito all'avvio;
- eventi di avanzamento su quel canale;
- un endpoint di annullamento, e la garanzia che annullare a metà lasci uno stato coerente
  (le rinomine e gli spostamenti già fatti restano fatti, e sono elencabili).

Si innesta naturalmente sull'involucro di riuscita parziale (Fase 10 §3): un'operazione annullata
a metà **è** una riuscita parziale.

## 10. Le dieci decisioni di prodotto rimaste aperte, e chi le ha già chiuse

Il documento ne elenca dieci. Cinque hanno già una risposta nel backend o nelle spec:

| # | Domanda aperta nel documento | Risposta |
|---|---|---|
| 3 | *"La scadenza del cestino a 30 giorni è dichiarata ma non implementata: chi la applica?"* | **Il server.** `purge_expired` è agganciato allo scheduler di manutenzione (Fase 6, Task 8) |
| 7 | *"Il criterio con cui due foto sono duplicate è mostrato ma non definito"* | **Stesso `content_hash`.** `assets_content_hash_idx`, non-unique per contratto congelato. L'interfaccia lo dice già: *"stesso hash del contenuto"* |
| 5 | *"Se l'unione di due persone sia reversibile"* | **Separare non ripristina: crea una persona nuova.** La spec Fase 8 ha `person_separations`, che rende la separazione manuale *permanente* contro il riaccorpamento automatico |
| 2 | *"Se le decisioni umane sopravvivano all'eliminazione di un tag"* | **No, e va detto nel dialog.** La spec Fase 7 elimina le decisioni insieme al tag; il dialog "modifica tag" mostra già il numero di foto coinvolte prima di confermare |
| 6 | *"Cosa accade ai volti già calcolati quando il riconoscimento viene spento"* | **Spegnere e cancellare restano due cose diverse** — sono due comandi distinti in §60. Spento = non si calcola più, i dati restano; il comando dedicato li elimina |

Restano genuinamente aperte, e **richiedono una decisione tua**, non del backend:

1. **Politica di lungo periodo degli scartati di un lotto** — restano per sempre? scadono? vanno nel cestino? (tocca la Fase 9)
4. **Se una coppia RAW+JPEG possa essere separata** nei due file, e cosa comporti
8. **I numeri esatti della pausa automatica dell'analisi** — il principio è giusto, la taratura va fatta sull'hardware vero (Fase 7, Task 1)
9. **Se il filtro rapido sia per vista o globale** — il documento lo segnala come scelta, non come difetto
10. **Se serva un annullamento generale** — oggi nessuna azione è annullabile. Per le azioni sul disco è un rischio; `metadata_batches` + `undo` esiste già e potrebbe estendersi

## 11. Difetti del prototipo da **non** replicare

La Parte XII ne elenca una trentina. Quelli con conseguenze sul backend:

- **Le scorciatoie da tastiera si attivano anche digitando in un campo di testo**: nel culling,
  scrivere `1` cambia la valutazione della foto sottostante. Da risolvere alla radice.
- **La rinomina non verifica le collisioni contro il disco**, solo dentro il gruppo selezionato;
  nessuna sanificazione completa dei caratteri, nessun limite di lunghezza, segnaposto vuoti che
  lasciano separatori orfani. **Da irrobustire prima di toccare file veri** (Fase 9).
- **`"Rinomina cartella…"` rinomina solo le foto passate dai filtri attivi** mentre il sottotitolo
  dichiara *"Tutta la cartella"*. L'ambito va reso esplicito nella richiesta.
- **L'eliminazione in blocco contrassegna ma non rimuove dalla timeline**, pur dichiarando
  *"N foto eliminate."*
- **L'anno è fisso a "2026"** nelle intestazioni: va derivato dai dati.
- **`"1 fota rinominata"`, `"3 fote"`** — la flessione automatica sbaglia: "foto" è invariabile.
  Riguarda i messaggi che il backend produrrà (Fase 10, Task 13).
