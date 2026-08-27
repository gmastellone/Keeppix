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
| `Favorite` | chip Cerca, Preferiti, filtro dell'album | **10** |
| `Rating{cmp,value}` | filtro dell'album, filtri rapidi | **10** |
| `DateRange`, `Day`, `Month` | il placeholder dice *"Cerca per data…"*; filtro dell'album | **10** |
| `Country` | pillola Paese (nel prototipo si crea ma non filtra) | **10** |
| `Aperture`, `Shutter` | filtro dell'album | **10** |
| `Tag`, `Category` | chip SP-3, pillole, filtro dell'album | 7 |
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
`health`. I client generati non li vedono. → Fase 10, Task 22, con un controllo in CI che
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

## 12. Tre finezze trovate nei sottotitoli di dettaglio

Emerse dal setaccio dei sottotitoli 2-8, non dalle sezioni *"Dati necessari"*.

### 12.1 Il chip «Luogo» del filtro rapido **non filtra per luogo**
Il documento lo dichiara: *«l'etichetta dice "Luogo" ma i valori sono le cartelle e il confronto
è su `p.folderId`. Nel mockup le tre cartelle coincidono con tre luoghi, quindi la finzione
regge; nel prodotto reale sono due concetti diversi.»*

→ Serve una variante **`Place { id }`** di `SearchNode`, distinta da `Folder`. I luoghi
esistono già (`places`, geocodifica inversa della Fase 4): manca l'asse di filtro.

### 12.2 «Nessuna posizione» è un valore, non un'assenza — ✅ già gestito
Il documento avverte: *«valore speciale "nessuna" → nessun luogo, **anche se la cartella ne
avrebbe uno**»*. È un tri-stato: non impostata (eredita dalla cartella) / impostata / **negata
esplicitamente**.

Il backend lo regge già: `MetadataPatchRequest` usa `double_option`
(`Option<Option<GeoPointView>>`), dove assente = non toccare, `Some(None)` = azzera
esplicitamente, `Some(Some(x))` = imposta. È esattamente il pattern giusto.
**Da verificare** che `EffectiveMetadata` faccia vincere l'azzeramento esplicito sull'eredità
della cartella, e non lo confonda con "non impostata".

### 12.3 L'eliminazione dal disco **può fallire**, e il prototipo non lo prevede
Il documento lo marca come *«il buco più rilevante per il backend»*: l'opzione
`"Elimina dal disco adesso"` può fallire per permessi, file in uso o libreria offline, e il
prototipo assume che riesca sempre.

→ È coperto dalla tassonomia (Fase 10 §7) e dall'involucro di riuscita parziale (§3), ma va
detto esplicitamente: **il dialog più distruttivo dell'app è anche quello che ha più modi di
non riuscire**, e l'interfaccia deve saperlo dire per ogni file.

---

## 13. Costo per schermata, e il principio «se pesa troppo, si cambia strada»

Le sezioni precedenti guardano *una query alla volta*. Questa guarda quello che conta davvero per
un'interfaccia: **quante richieste costa una schermata**, e cosa fare quando la risposta è
"troppe" o "troppo grosse". Il bersaglio resta un **Raspberry Pi 5 / 8 GB**, spesso raggiunto da
fuori casa.

### 13.1 La cascata all'avvio a freddo

Aprire la Timeline — la schermata d'ingresso — costa oggi, con tutto ciò che le fasi prevedono:

| # | Richiesta | A cosa serve | Blocca |
|---|---|---|---|
| 1 | `GET /auth/me` | chi sono | **tutto** |
| 2 | `GET /users/me/preferences` | tema, densità griglia | **il layout** |
| 3 | `GET /folders/tree` + conteggi | sidebar | — |
| 4 | `GET /libraries/{id}/storage` | sidebar | — |
| 5 | `GET /timeline/buckets` | mesi e conteggi | lo scrubber |
| 6 | `GET /timeline/geometry` | proporzioni | **il layout** |
| 7 | `GET /timeline?bucket=…` | prima pagina | — |
| 8 | badge culling (Fase 9) | sidebar | — |
| 9 | badge revisione (Fasi 7/8) | sidebar | — |

**Nove richieste prima del primo disegno utile**, con tre catene di dipendenza vere
(`auth → preferenze → densità → layout`, `geometry → layout`, `buckets → scrubber`). In LAN si
notano poco; da fuori casa, con 100 ms di andata e ritorno, diventano quasi un secondo di attesa
prima che appaia qualcosa.

**Alternativa: un solo `GET /api/v1/bootstrap`** che restituisce in un colpo utente, preferenze,
albero cartelle con conteggi, spazio su disco e i badge. Sono tutti dati piccoli, tutti richiesti
sempre, tutti già in cache lato server. Nove richieste diventano tre: `bootstrap`, `geometry`,
`timeline`.

**Ruling: `bootstrap` è additivo e non sostituisce gli endpoint singoli.** — Le viste che
cambiano un solo pezzo (le preferenze da Impostazioni, i conteggi dopo un import) devono poterlo
rileggere senza riscaricare tutto, e i client non-web usano già i singoli. — *Costo se sbagliato:*
due strade per lo stesso dato, che vanno tenute coerenti — mitigato dal fatto che `bootstrap`
compone gli stessi repository, senza SQL proprio.

### 13.2 La geometria: 4,7 MB è troppo per una connessione mobile

L'endpoint di geometria (§Fase 10 §2) risolve la richiesta n.1 del documento, ma su una libreria
da 214.000 scatti pesa **4,7 MB** (≈1,5 MB con gzip). Su LAN è nulla; in tethering è una pausa
visibile, e su un telefono si paga a ogni avvio a freddo.

**Alternativa, da adottare se la misura conferma il problema: geometria per mese.**

- Si scaricano le proporzioni **solo dei mesi vicini a quello che si sta guardando** (il corrente
  più due per lato), non dell'intera libreria.
- L'altezza dei mesi non ancora scaricati si **stima** da `conteggio × rapporto d'aspetto medio`
  — un numero che il server può restituire dentro `/timeline/buckets` a costo zero.
- Quando un mese entra nella finestra, la sua geometria vera sostituisce la stima e l'altezza si
  corregge.

**Ruling: si parte dalla geometria intera, e si passa a quella per mese solo su misura.** — La
versione intera è più semplice e rende lo scrubber **esatto** invece che approssimato; il
documento chiede esplicitamente di conoscere l'altezza dell'intera libreria in anticipo.
Frammentare subito significherebbe pagare complessità per un problema che su LAN — il caso d'uso
primario di un server di casa — non esiste. Ma la soglia va **misurata**, non intuita: se il
primo disegno su rete mobile supera i 2 secondi, si passa alla versione per mese. — *Costo se
sbagliato:* si riscrive il caricatore della geometria, non il layout né il virtualizzatore, che
restano identici.

### 13.3 ~~Conteggio dei membri degli album dinamici~~ — **risolto togliendo la funzione**

> **Superato dalla decisione del 20 agosto 2026.** Gli album dinamici non esistono più: un album
> ricorda il filtro e lo rilancia su richiesta, quindi i membri stanno sempre in `album_assets` e
> il conteggio è una lettura banale. L'analisi resta qui perché **è la ragione per cui la
> funzione è stata tagliata**.

#### L'analisi originale

Un album dinamico non ha membri materializzati (ed è giusto così: sono raccolte *"vive"*). Ma la
griglia Album mostra **"81 foto"** accanto a ognuno. Con otto album dinamici, aprire quella
griglia significa **otto interrogazioni sull'intero catalogo**.

Su 200.000 asset, su un Pi, è la query più cara che l'interfaccia sappia innescare — e si innesca
a ogni apertura di una schermata di navigazione.

**Tre rimedi, in ordine di preferenza:**
1. **Cache `moka` con invalidazione esplicita** (già la convenzione della Fase 6), agganciata a
   import, cestinamento e alle modifiche di metadati che l'AST può toccare.
2. **Conteggio con tetto**: `LIMIT 1000` e mostrare *"più di 999"* oltre. L'utente non ha bisogno
   della cifra esatta per un album da migliaia di foto, e il costo diventa costante.
3. **Calcolo differito**: la griglia mostra la copertina subito e il conteggio quando arriva.

**Ruling: 1 e 2 insieme, non 3.** — Un numero che compare in ritardo fa "saltare" la scheda ed è
proprio l'effetto che il documento chiede di evitare con gli scheletri. Cache più tetto danno un
numero immediato e stabile. — *Costo se sbagliato:* per gli album enormi si mostra un
approssimato; è una perdita accettabile, l'alternativa è una griglia che si muove sotto il dito.

### 13.4 I suggerimenti di ricerca girano a ogni battuta

`SearchRepo::suggest` (`db/search.rs:107`) fa una `UNION` di due `ILIKE` con il filtro di
visibilità, **a ogni carattere digitato**. Su 200.000 righe, su un Pi, è troppo per stare dietro
alla digitazione.

- Il prefisso è già usato (`like_prefix`), quindi gli indici trigram possono lavorare: bene.
- Manca il **ritardo di digitazione**: 150 ms lato frontend eliminano la maggior parte delle
  richieste senza che l'utente se ne accorga.
- I modelli di **fotocamera e obiettivo distinti** sono poche decine e cambiano solo agli import:
  vanno tenuti in cache in memoria, non ricavati con una query per battuta.

### 13.5 Cosa è già ottimale e non va toccato

- **Le miniature sono già gratis alla seconda visita.** `/media/thumb/{hash}` risponde con
  `private, max-age=31536000, immutable` (`routes/media.rs:20`) e la chiave è il **content hash**:
  l'URL cambia solo se cambia il contenuto. È il modo giusto, e vale anche per preview e full.
- **Paginazione keyset ovunque** (`taken_at|id`), mai `OFFSET`.
- **Visibilità risolta una volta** per richiesta (`VisibilityScope::resolve`) e compilata nel
  `WHERE`, non valutata per riga.
- **`POST /viewport`** esiste già per dire al server quali miniature servono per prime.
- **Derivati con perdita** ridotti dal 3,3% allo 0,4% (Fase 2R3): ~36 GB invece di ~308 GB su
  200.000 foto.

---

## 14. Tre ottimizzazioni trovate al secondo giro, di cui una gate sulle altre

### 14.1 Postgres gira con le impostazioni di fabbrica — e questo **annulla** parte del lavoro sugli indici

`compose.yaml` avvia `postgis/postgis:17-3.5` **senza nessun parametro**. Restano quindi i
default, pensati per una macchina qualunque degli anni Duemila:

| Parametro | Default | Su un Pi 5 / 8 GB dovrebbe essere | Perché conta |
|---|---|---|---|
| `random_page_cost` | **4.0** | **1.1** su SSD/NVMe | Dice al pianificatore che una lettura casuale costa **quattro volte** una sequenziale. Su disco rotante era vero; su SSD no. **È il parametro che decide se Postgres userà gli indici che stiamo aggiungendo o preferirà una scansione sequenziale.** |
| `shared_buffers` | 128 MB | ~2 GB (25% della RAM) | Con 128 MB su una libreria da 200k asset, la cache interna non tiene nulla |
| `effective_cache_size` | 4 GB | ~6 GB | Altra stima che entra nel calcolo del pianificatore |
| `work_mem` | 4 MB | 32–64 MB | Sotto questa soglia ordinamenti e hash **finiscono su disco**. La timeline ordina su `taken_at DESC, id DESC` |
| `max_connections` | 100 | 20 | Ogni connessione costa memoria; l'app è un processo solo con un pool solo |

**Questo va fatto prima di misurare qualunque indice.** Aggiungere `assets_geometry_idx`,
`assets_timeline_indexed_idx` e gli indici parziali su `favorite`/`rating` mentre il
pianificatore crede che l'I/O casuale costi quattro volte quello sequenziale significa
costruirli e vederseli ignorare — e concludere, sbagliando, che "gli indici non servono".

Va anche distinto **SD card contro SSD**: su microSD `random_page_cost` resta alto e il profilo
cambia del tutto. Il valore giusto dipende dal supporto, quindi va **misurato all'installazione**,
non cablato.

### 14.2 `thumbhash` è già nel payload, e nessuno lo sta usando

`AssetView` porta già `thumbhash` (`routes/timeline.rs:56`) — l'impronta minuscola (~25 byte) da
cui si ricostruisce un'anteprima sfocata.

Significa che **il primo disegno della griglia non ha bisogno di nessuna richiesta di
miniatura**: le tessere si dipingono subito dalle impronte già arrivate con la pagina, e le
miniature vere le sostituiscono man mano. Su una griglia da 60 tessere sono **60 richieste tolte
dal percorso critico**, non rimandate.

È l'antidoto giusto al problema opposto a quello della geometria: la geometria dà le
**proporzioni** prima di disegnare, `thumbhash` dà il **colore**. Insieme, il primo fotogramma è
completo e corretto senza aver scaricato una sola immagine.

### 14.3 Gli index-only scan dipendono dall'autovacuum

L'indice di copertura della geometria (`INCLUDE (width, height)`) rende la query un *index-only
scan* **solo se la mappa di visibilità è aggiornata**. Su una tabella che riceve import massicci
e cestinamenti, se l'autovacuum resta indietro Postgres deve comunque andare in heap, e il
guadagno sparisce senza nessun errore visibile.

→ `autovacuum_vacuum_scale_factor` più aggressivo su `assets`, e un `VACUUM ANALYZE` alla fine di
ogni import massiccio (lo scheduler di manutenzione della Fase 6 è il posto giusto).

### 14.4 Il payload della timeline è più grasso del necessario

`AssetView` porta `content_hash`, `size_bytes`, `kind`, `status`, `taken_at_utc`, `thumbhash`,
`location`, `width`, `height`, `folder_id`, `filename`. Per **disegnare una tessera** servono:
id, `thumbhash`, `content_hash` (è la chiave dell'URL della miniatura), `raw_kind`, `favorite`,
`filename`. Non servono `size_bytes`, `status`, `location`, `taken_at`.

Su 200 elementi per pagina è una differenza modesta (~60 KB contro ~35 KB), ma è gratis: basta
un parametro `fields=grid`. Da fare **solo se la misura lo giustifica** — è esattamente il tipo
di ottimizzazione che non vale la complessità se il numero non la chiede.

---

## 15. Il WebSocket emette due eventi, l'interfaccia ne chiede nove

`crates/keeppix-api/src/routes/ws.rs` emette **solo**:

- `assets.upserted` — `{ids, count}`
- `assets.deleted` — `{ids}`

Il canale è versionato (`v`) e ben fatto, ma è uno stub rispetto a ciò che l'interfaccia
mostra come **dato che cambia da solo**, senza che l'utente faccia nulla:

| Cosa cambia da solo | Dove | Evento necessario |
|---|---|---|
| **Avanzamento dell'analisi IA** — *"128.450 di 214.000 (60%)"*, stato in pausa/in corso, velocità stimata | §57 Analisi libreria | `analysis.progress` |
| **Badge Revisione** — tag e volti in attesa | sidebar, pagina "Altro" | `suggestions.changed` |
| **Badge Culling** — foto ancora da valutare | sidebar, header mobile | `culling.changed` |
| **Avanzamento di scansione/import** | import iniziale, Problemi | `scan.progress` |
| **Avanzamento delle operazioni lunghe** (rinomina di massa, spostamenti) | §Fase 10 Task 16 | `operation.progress` |
| **Nuovo problema rilevato** (job fallito, libreria offline) | §47 Problemi | `problems.changed` |
| **Spazio su disco** | sidebar | dentro `bootstrap`, oppure `storage.changed` |
| **Transcodifica video completata** | player (Fase 6) | `asset.derivative.ready` |
| **Esito di un backup** | Impostazioni (Fase 6) | `backup.finished` |

**§57 «Analisi libreria» è una schermata di avanzamento dal vivo**: senza push si ridurrebbe a
un'interrogazione a intervalli, che su un Pi è esattamente il carico che non si vuole aggiungere
mentre l'analisi gira.

Il contratto congelato aiuta: *«il WebSocket è canale di notifica, non fonte di verità»*. Quindi
questi eventi possono essere **magri** — un segnale che dice "ricarica questo contatore" — senza
dover trasportare stato consistente. È la forma giusta e va rispettata: un evento che porta il
numero è una comodità, non una garanzia.

## 16. Ritardi e soglie dichiarati, con il loro significato

La palette dei tempi del prototipo è piccola e coerente. **La prima estrazione che avevo fatto
era sbagliata** (uno script leggeva `.12s` come "12s"): questa è quella corretta.

| Valore | Occorrenze | Cos'è |
|---|---|---|
| `.12s` | 54 | tooltip, comparsa dei comandi sulla tessera |
| `.2s` | 53 | toast, transizioni generiche |
| `.15s` | 14 | rotazione della freccia dei gruppi di navigazione |
| `.25s` | 2 | cambio di tema su `#app` |
| `.1s`, `.18s`, `.3s` | 5 | casi isolati |

Curva: `ease` in 108 casi su 111. **Tre valori soltanto** coprono il 92% delle animazioni.

Soglie e ritardi che **non** sono animazioni, e che hanno conseguenze fuori dal CSS:

| Valore | Significato |
|---|---|
| **10 ms** | ritardo prima di mostrare il toast (per far scattare la transizione) |
| **2400 ms** | durata del toast di successo; **4,2 s** per errore e riuscita parziale; **6,5 s** se ha un'azione, con il timer **fermo mentre il puntatore è sopra** |
| **250 ms** | rimozione del toast dal DOM dopo la dissolvenza |
| **500 ms** | tocco prolungato su mobile per entrare in selezione, con **vibrazione di 15 ms** |
| **700 ms** | ritardo **simulato** del prototipo fra avvio ed esito di un'azione. È scaffolding — ma il documento nota che *«durante i 700 ms si può premere di nuovo, e il codice non lo impedisce»*: nel prodotto vero è esattamente il caso che **SP-30 (pulsante occupato)** deve coprire |
| **1,4 s** | pulsazione dell'indicatore mentre l'analisi gira |
| **4000 ms** | **la soglia della pausa automatica dell'analisi**: riprende 4 secondi dopo l'ultimo cambio di vista. È un comportamento del server, non dell'interfaccia |
| **42 ms / 260 ms** | inferenza per foto, livello **Piena** contro **Ridotta**: la modalità ridotta è **6 volte più lenta**, e il documento lo dichiara all'utente (*"la stessa coda residua viene dichiarata ~6 volte più lunga"*) |

## 17. Inventario completo di dialog, menu, popover e selettori

Sono **24**, e nel documento hanno ciascuno una sezione propria. Vanno costruiti sopra i due
componenti condivisi (SP-5 dialog modale, SP-14 menu a comparsa), non uno per uno:

**Menu a comparsa (SP-14):** menu account (desktop e mobile) · menu "altre azioni" ⋯ del lightbox ·
selettore rapido di lotto · menu sul riquadro del volto · popover della mappa · picklist della
creazione album.

**Dialog modali (SP-5):** cartella radice di culling (×2 contesti) · imposta posizione · ricerca
di regione · condividi selezione · scegli copertina · assegna a gruppo · unisci persone · separa
persona · selettore di persona · selettore di tag · aggiungi ad album · file con problemi ·
**eliminazione a 3 opzioni (SP-18)** · informazione · conferma · modifica tag · modifica
categoria · rinomina con formula · inserimento testo generico.

Regole che valgono per tutti e che il prototipo **non** rispetta uniformemente:
il focus va al primo elemento all'apertura e **torna al trigger** alla chiusura (questo il
prototipo lo fa quasi ovunque, ed è il dettaglio da non perdere); **Esc chiude**; il focus è
**confinato** nella scheda (il prototipo non lo fa in nessun dialog); il click sul velo — da
decidere: probabilmente sì per i selettori, no per i distruttivi.

Due eccezioni deliberate da preservare: nel **dialog di eliminazione** il focus va sulla **prima
opzione, la meno distruttiva**; nel **dialog di conferma** va su **"Annulla"**. Chi preme Invio
d'istinto compie l'azione innocua.

---

## 18. L'importazione iniziale è il vero collo di bottiglia, e non ha interfaccia

### 18.1 I numeri misurati, non stimati

Una prova sul campo reale ha misurato:

| Metrica | Valore |
|---|---|
| File sorgente | 1.558 (37 GB) |
| Asset creati | 779 |
| Durata totale | **7m52s** → **1,65 asset/s** |
| Velocità di hash | 89 MB/s |
| Derivati su disco | 139 MB (**0,4%** degli originali) |
| RAW con preview | 779/779 |

**Estrapolando a 200.000 asset** — la scala dichiarata dal prototipo:

- su **quell'** hardware (Mac, NVMe): **~34 ore**;
- su un **Raspberry Pi 5**, che è il bersaglio dichiarato: fra **4 e 7 giorni**, a seconda del
  fattore reale (3–5× più lento su decodifica RAW ed encode WebP).

### 18.2 La scomposizione dice dove sta il costo

| Componente | Costo a 200.000 asset | Natura |
|---|---|---|
| Overhead per file (~272 ms, misurato in Fase 1b: coda + DB) | **~15 ore** | **il dominante** |
| Hash di 1 TB a 89 MB/s | ~3 ore | I/O |
| Decodifica RAW + resize + encode WebP | il resto | CPU |

**Le quindici ore di overhead per-file sono il bersaglio giusto**, non la decodifica: sono coda e
database, cioè lavoro che si può raggruppare. Un import che inserisce a lotti invece che a file
singolo attacca la voce più grossa senza toccare la pipeline media.

### 18.3 Cosa già mitiga, e va tenuto

- **Le preview incorporate nei RAW vengono usate** (779/779 con preview): non si decodifica il
  RAW pieno per fare una miniatura. È l'ottimizzazione che conta di più ed è già lì.
- `SKIP_PREVIEW_PX = 1600` e `SKIP_PREVIEW_BYTES = 400 KB`: se il sorgente è già piccolo, la
  preview non si genera affatto.
- **Profili energetici** con tetto di priorità: l'import non compete con chi sta navigando.
- I derivati pesano lo **0,4%** degli originali (Fase 2R3): ~36 GB su 1 TB invece di ~308 GB.

### 18.4 L'alternativa, se i giorni restano giorni

Il principio «se pesa troppo si cambia strada» qui si applica al **quando**, non al *se*:

**Import in due tempi.** Prima passata: cammina l'albero, legge EXIF e **thumbhash**, scrive gli
asset. Niente hash del contenuto, niente derivati. La libreria diventa **navigabile in
un'ora invece che in giorni** — con le tessere già a colori grazie a thumbhash (§14.2) e le
proporzioni già note. Seconda passata, in background e di notte: hash del contenuto (che serve
solo per i duplicati) e derivati veri.

**Ruling: da decidere con il numero in mano, non prima.** — La prova sul campo è su 1.558 file su
un Mac; il bersaglio è 200.000 su un Pi. Il fattore vero fra i due va **misurato**, perché fra
"34 ore" e "7 giorni" cambia la risposta. — *Costo se sbagliato:* si consegna un prodotto in cui
il primo avvio richiede una settimana prima di mostrare qualcosa, che è il momento in cui un
utente decide se tenerlo.

### 18.5 E non ha nessuna interfaccia

Il documento dichiara l'importazione iniziale **fuori dal disegno di questa fase**. Ma è
l'operazione più lunga che Keeppix esegua, la prima che un utente incontra, e oggi non ha:
schermata, avanzamento, stima del tempo rimanente, né modo di sapere che sta funzionando.
→ Va disegnata (Fase 11), e ha bisogno di `scan.progress` sul WebSocket (Fase 10 Task 19).

## 19. Due discrepanze fra ciò che l'interfaccia dichiara e ciò che il backend fa

1. **Finestra notturna**: `default_night_window()` (`keeppix-jobs/src/profile.rs:29`) è
   **2:00–6:00**; il testo dell'interfaccia (§57) dichiara all'utente
   *«Di notte (2:00–7:00) l'analisi lavora a piena velocità»*. Un'ora di differenza, in un testo
   che l'utente legge come una promessa. Il documento stesso annota che quel testo *«è solo
   copy: nessuno scheduler notturno»* — ma lo scheduler ora **esiste** (Fase 6 Task 8), quindi
   la discrepanza è diventata reale.
2. **Regioni mappa**: `RegionView` porta già `downloaded_bytes`, `status` e `last_error` — cioè
   l'avanzamento del download **esiste come dato** ma non viene mai spinto. Serve
   `region.progress` sul WebSocket, altrimenti l'unica strada è interrogare a intervalli.

## 20. WebDAV: cosa c'è

Metodi implementati: `PROPFIND`, `GET`, `HEAD`, `PUT`, `DELETE`, `MKCOL`, **`MOVE`**, `LOCK`,
`UNLOCK`. `MOVE` è rilevante: è ciò che rende praticabile il modello a cartelle fisiche della
Fase 9 (spostare i «presi» da un altro computer).

Mancano `COPY` (resta `501`, **dichiarato onestamente** nel codice e nel ledger) e `PROPPATCH`.
Nessuno dei due è richiesto dall'interfaccia.

## 21. Profilo di memoria del frontend

| Struttura | A 200.000 scatti | Verdetto |
|---|---|---|
| Geometria in `ArrayBuffer` (6 byte/scatto) | **1,2 MB** | trascurabile |
| Somme prefisse delle altezze di riga (~50.000 righe × 8 byte) | **0,4 MB** | trascurabile |
| Tessere vive nel DOM (~100 × miniatura 240px decodificata) | **~15 MB** | accettabile, ed è il tetto |
| **Cache delle pagine caricate** | **cresce senza limite** | ⚠️ **è il rischio vero** |

La geometria non è il problema di memoria: **lo è la cache delle pagine**. Scorrendo l'intera
libreria si accumulano fino a 200.000 oggetti asset — decine di megabyte più pressione sul
garbage collector — se nulla li sfratta.

→ **Fase 11: una LRU sulle pagine caricate**, con un tetto esplicito (per esempio le ultime 50
pagine, ~10.000 asset). Le pagine sfrattate si ricaricano in una richiesta; la geometria, che è
ciò che tiene in piedi il layout, **non si sfratta mai** perché costa 1,2 MB in tutto.
