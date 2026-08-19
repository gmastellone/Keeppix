# Keeppix — Roadmap delle fasi

**Data:** 2026-08-13
**Spec:** [`../specs/2026-08-13-keeppix-design.md`](../specs/2026-08-13-keeppix-design.md)

Questo documento sta fra lo spec e i piani. Lo spec dice *cosa* costruire; i piani dicono *come*, passo per passo, una fase alla volta. Qui c'è ciò che serve per non perdere di vista l'insieme: cosa produce ogni fase, cosa consuma dalle precedenti, quali rischi porta, e quali contratti sono già congelati e non vanno rinegoziati.

**Parla di interfacce, non di codice.** È il motivo per cui non invecchia come invecchierebbe un piano dettagliato scritto in anticipo.

---

## Contratti congelati

Decisi nello spec, validi da subito, **non rinegoziabili per fase**. Ogni fase li implementa anche quando sembrano prematuri: è ciò che impedisce di dover smontare il lavoro precedente.

| Contratto | Dove nasce | Perché va rispettato dalla Fase 0 |
|---|---|---|
| Ogni repository che legge dati utente prende un `AuthContext` come primo parametro | §4.2 | Aggiungerlo dopo significa toccare ogni firma e ogni chiamata |
| L'SQL vive solo in `keeppix-db`; gli handler non scrivono query | §3.2 | Il controllo dei permessi ha un solo punto di applicazione |
| Identità dell'asset = `(folder_id, filename)`, `content_hash` indicizzato non-unique | §4.1 | Cambiarlo dopo l'indicizzazione di 200k file è una migrazione dolorosa |
| I metadati originali sono immutabili; le modifiche stanno in `asset_overrides` | §4.4 | Il modello "sovrascrivi" non si converte in "override" a posteriori |
| Errori RFC 9457 con `type` stabile; il backend non traduce | §9.2 | Il client mobile ramifica sui codici; cambiarli rompe le app installate |
| `/api/v1` congelato: solo aggiunte | §9.1 | Un'app sul telefono continua a funzionare mentre il server avanza |
| Il WebSocket è canale di notifica, non fonte di verità | §9.3 | La correttezza non deve dipendere dalla consegna dei messaggi |
| ID generati come UUID v7, lato client quando serve | §9.2 | Nessuna riconciliazione di identità per gli upload offline |
| Nessun percorso filesystem arriva dal client | §9.5 | Path traversal escluso per costruzione, non per controlli sparsi |
| Decoder C (ffmpeg, libraw) in processo separato con seccomp | §9.5 | Sandbox aggiunta dopo = rifattorizzare tutta la pipeline media |

---

## Grafo delle dipendenze

```
      ┌─────────┐
      │ Fase 0  │  scheletro: workspace, DB, auth, Docker, CI
      └────┬────┘
           │
      ┌────▼────┐
      │ Fase 1  │  ingestione: librerie, asset, derivati, timeline
      └──┬───┬──┘
         │   └──────────────┬──────────────┐
    ┌────▼────┐        ┌────▼────┐    ┌────▼────┐
    │ Fase 2  │        │ Fase 5  │    │ Fase 4  │
    │  RAW    │        │ WebDAV  │    │  Mappe  │
    └────┬────┘        └────┬────┘    └────┬────┘
         │                  │              │
      ┌──▼──────────────────▼──────────────▼──┐
      │              Fase 3                    │  multiutente e condivisione
      └────┬────────────────┬─────────────┬────┘
           │                │             │
      ┌────▼────┐      ┌────▼────┐   ┌────▼────┐
      │ Fase 6  │      │ Fase 7  │   │ Fase 9  │  organizzazione:
      │consolid.│      │AI: tag, │   │culling, │
      └─────────┘      │scene    │   │rinomina │
                        └────┬────┘   └─────────┘
                             │
                        ┌────▼────┐
                        │ Fase 8  │  volti: cluster, correzioni, gruppi
                        └─────────┘
```

**Vincoli reali di ordine**, non preferenze:

- **1 dipende da 0**: serve il database, le migrazioni e il server.
- **2, 4, 5 dipendono da 1**: tutte lavorano su asset già indicizzati.
- **2, 4, 5 sono indipendenti fra loro**: l'ordine fra queste tre è una tua scelta, guidata da cosa ti serve prima.
- **3 può iniziare dopo la 1**, ma conviene dopo che almeno una delle 2/4/5 è chiusa: la condivisione ha più senso quando c'è più materiale da condividere.
- **7 dipende da 3**: un tag non deve rivelare foto che l'utente non può vedere. Il filtro di visibilità dev'essere già lì.
- **8 dipende da 7**, e non per l'argomento: la 7 porta il motore di inferenza, pgvector, il probe hardware esteso e il backfill a priorità energetica. La 8 li **riusa** — da sola dovrebbe costruirseli.
- **9 dipende da 1, 2, 3**: corregge un buco nell'identità dell'asset (1), riusa flag e sidecar del culling (2), riusa i permessi (3). **Non** dipende dalla 5: il modello a cartelle fisiche rende gli scelti visibili da WebDAV senza che WebDAV debba saperne nulla — ma ne beneficia, se la 5 è già chiusa, dal giorno stesso in cui la 9 chiude.
- **6 è indipendente da 7/8/9**: può stare prima, in mezzo o dopo. È l'unica che chiude comunque.

**Variante consigliata se vuoi caricare il TB presto:** `0 → 1 → 5 → 2 → 3 → 4 → 6`. Sposta il WebDAV subito dopo l'ingestione, così inizi a versare i file mentre si sviluppa il resto.

---

## Fase 0 — Scheletro

**Piano:** [`2026-08-13-keeppix-fase-0.md`](2026-08-13-keeppix-fase-0.md) · 15 task · **scritto**

**Obiettivo.** Un binario che si avvia, migra il database, serve il frontend e permette login. Nessuna funzione fotografica.

**Produce**
- Workspace a 7 crate con i confini dello spec §3.2.
- `Db`, migrazioni, harness testcontainers.
- `UserRepo`, `SessionRepo`, `SettingsRepo`.
- `AuthContext`, `Auth`/`AdminAuth` extractor, `Problem` (RFC 9457).
- `AppState`, `router_parts()`, header di sicurezza, OpenAPI congelata.
- Frontend Vue + Tailwind + Reka con i18n, setup e login.
- Immagine distroless multi-arch, compose a profili, CI completa.

**Rischi**
- Toolchain locale a 1.82: edition 2024 richiede 1.85+. *Mitigazione: primo step del piano.*
- Cache offline `sqlx` da rigenerare a ogni cambio di query. *Mitigazione: comando documentato, controllo in CI.*

**Dimensione:** ~2-3 giorni.

**Chiusa quando:** setup del primo admin, logout, login, sessione persistente al riavvio, CI verde, immagine senza shell.

---

## Fase 1 — Ingestione

**Obiettivo.** Puntare Keeppix al tuo TB e ottenere una timeline navigabile con miniature, ricerca e cartelle.

**Consuma dalla 0:** `Db`, `AuthContext`, `AppState`, coda job (da creare qui), WebSocket (da creare qui).

**Produce**
- Migrazioni: `libraries`, `folders` (ltree), `assets`, `asset_exif`, `folder_month_counts`, `jobs`, `change_log`.
- `LibraryRepo`, `FolderRepo`, `AssetRepo`, `TimelineRepo` — tutti con `AuthContext`.
- `keeppix-jobs`: coda `SKIP LOCKED`, worker pool, 4 livelli di priorità, profili energetici.
- `keeppix-media`: `probe_metadata()`, `extract_exif()`, `make_derivatives()` con la singola decodifica per due derivati.
- Watcher con debounce, rilevamento inotify e ripiego su polling.
- Endpoint `/timeline/buckets`, `/timeline`, `/folders/tree`, `/search`, `/media/*`.
- WebSocket con ticket monouso, backpressure, coalescing.
- Frontend: griglia giustificata, scrubber, thumbhash, vista cartelle, ricerca.
- Rilevamento capacità hardware al primo avvio.

**Rischi**
- **Il più grosso della roadmap.** È la fase con più superficie. *Mitigazione: dividerla internamente in due piani — 1a ingestione e job, 1b timeline e frontend — se supera i 20 task.*
- Il virtual scroll di urocissa va portato da Vuetify a Tailwind. *Mitigazione: la logica è TypeScript quasi puro; isolare il componente e testarlo a parte.*
- Le stime di throughput (~2h10 per i derivati) sono mie, non misurate. *Mitigazione: la Fase 1 produce i numeri veri, che ricalibrano le fasi successive.*

**Dimensione:** ~2-3 settimane. È la fase più lunga.

**Chiusa quando:** il TB reale è indicizzato, la timeline scorre fluida su RPi, la riscansione richiede ~2 minuti.

---

## Fase 2 — RAW e culling

**Obiettivo.** Rivedere 11.000 RAW a risoluzione piena, votarli, modificarne i metadati in blocco, cancellare gli scarti.

**Consuma dalla 1:** `AssetRepo`, pipeline derivati, coda job, griglia frontend.

**Produce**
- Migrazioni: `asset_overrides`, `asset_flags`, `stacks`, `trash_entries`.
- `keeppix-media`: estrazione preview incorporata (ARW, NEF, CR2, CR3, DNG, ORF, RAF), fallback libraw in processo sandbox.
- Lettura e scrittura sidecar XMP.
- `OverrideRepo`, `FlagRepo`; editing batch su selezione.
- Rilevamento stack RAW+JPEG; pagina Duplicati.
- Dialogo di cancellazione a tre opzioni; cestino con recupero a 30 giorni.
- Frontend: modalità culling con filmstrip, avanzamento automatico, zoom 1:1 con prefetch, confronto affiancato.

**Rischi**
- Corpi macchina senza preview full-size. *Mitigazione: fallback libraw + pagina Problemi; misurare la copertura reale sul tuo archivio alla Fase 1.*
- Scrittura XMP su filesystem in sola lettura. *Mitigazione: rilevamento del mount, azioni disabilitate con spiegazione.*
- Sovrapposizione fra visualizzatore e culling. *Mitigazione: la regola dura dello spec §10.1 — nel visualizzatore solo rating e preferito.*

**Dimensione:** ~1-1,5 settimane.

**Chiusa quando:** una sessione di culling su 800 scatti si completa da tastiera senza attese percepibili.

---

## Fasi 2R, 2R2, 2R3 — rimedio (non previste da questa roadmap)

Fra la Fase 2 e la Fase 3 si sono inserite tre fasi di rimedio, nate da field
test su un archivio reale invece che dalla pianificazione. **Non erano
previste qui, e vale la pena che restino visibili proprio per questo:** ciò che
le ha rese necessarie è che i test unitari non vedevano un'intera classe di
difetti — funzioni scritte, testate, e mai collegate al percorso reale.

| | Cosa ha risolto |
|---|---|
| **2R** | Creazione di librerie e gestione utenti raggiungibili dal browser; niente più riavvio del container per scansionare |
| **2R2** | La pipeline RAW non veniva eseguita **mai** in produzione (`detect_kind` senza chiamanti); la riscansione riaccodava tutto e non si fermava più |
| **2R3** | Derivati senza perdita (3,3% → **0,4%**, ~308 GB → ~36 GB su 200.000 foto); zoom sui RAW; guardia in CI contro la classe di difetti sopra; prova di scala a 200.000 asset |

La lezione è nel documento di continuazione (`docs/CONTINUE.md`) e nella
guardia `scripts/check-wired.py`, che ora fallisce la CI se una funzione
pubblica non ha chiamanti o una rotta montata non ha consumatori.

**Conseguenza sulla Fase 3:** assorbe anche le **17 interfacce mancanti** che
quella guardia ha scoperto (suo Task 12) — gestione utenti, navigazione
cartelle, cestino, modifica in blocco, ricerche salvate, rinnovo sessione.

---

## Fase 3 — Multiutente e condivisione

**Obiettivo.** Più utenti, gruppi, condivisione di foto, cartelle e album, link pubblici.

**Consuma dalla 1 e 2:** `visibility_scope` (già previsto dalla 1), `AuthContext` (dalla 0), asset e album.

**Produce**
- Migrazioni: `permissions`, `albums`, `album_assets`, `share_links`, `audit_log`; `groups` popolati.
- `PermissionRepo` con risoluzione ereditata e catena del "perché hai accesso".
- `AlbumRepo`, `ShareLinkRepo`.
- `AuthContext::ShareLink` — la variante prevista fin dalla Fase 0.
- Pagine pubbliche con `noindex`, rate limit, `hide_metadata` attivo di default.
- Upload da ospite con coda di revisione.
- Frontend: pannello permessi (diretti vs ereditati), pagina Condivisioni, gestione utenti e gruppi.

**Rischi**
- Le query di visibilità rallentano con molte condivisioni. *Mitigazione: i prefissi autorizzati restano <10 nei casi reali; misurare con dati veri, non ipotizzare.*
- Regressioni di sicurezza silenziose. *Mitigazione: suite di test dedicata "chi vede cosa", eseguita su ogni canale (REST, WebDAV, WebSocket, link).*

**Dimensione:** ~1,5-2 settimane.

**Chiusa quando:** una cartella condivisa a un utente esterno mostra esattamente quel sottoalbero, e un link pubblico con password e scadenza funziona da fuori casa.

---

## Fase 4 — Mappe e geocoding

**Obiettivo.** Vedere dove sono state scattate le foto e assegnare posizioni, anche in blocco.

**Consuma dalla 1:** `assets.location`, EXIF GPS. **Dalla 2:** `asset_overrides` per le assegnazioni manuali.

**Produce**
- Estensione PostGIS attiva (già abilitata dalla migrazione `0001`), tabella `places` con GeoNames.
- Normalizzazione dei fusi orari da confini geografici.
- Endpoint `/map/clusters`, `/places/suggest`.
- Servizio PMTiles con range request; gestore regioni con download riprendibile.
- Frontend: MapLibre in chunk pigro, cluster con miniatura di copertina, disegno area come filtro, mini-mappa nel dettaglio.
- Assegnazione posizione: ricerca, pin, copia da altra foto.

**Rischi**
- Il ricalcolo dei fusi cambia le date su foto già catalogate. *Mitigazione: anteprima delle modifiche e annullamento in blocco.*
- Dimensione delle regioni PMTiles su hardware piccolo. *Mitigazione: granularità per paese, spazio mostrato prima del download.*

**Dimensione:** ~1 settimana.

**Chiusa quando:** assegni una località a 400 foto e la mappa le mostra raggruppate, senza una sola richiesta di rete verso l'esterno.

---

## Fase 5 — WebDAV e upload

**Obiettivo.** Montare Keeppix come disco, sincronizzare cartelle locali, caricare da browser e telefono con ripresa.

**Consuma dalla 1:** `FolderRepo`, watcher, coda job. **Dalla 3 (se già fatta):** permessi; altrimenti solo il proprietario.

**Produce**
- `keeppix-dav`: `PROPFIND` dal database in streaming, `LOCK`/`UNLOCK` Class 2, `MOVE` che conserva i metadati, `DELETE` sempre nel cestino.
- `dav_locks`, app-password con revoca individuale.
- Upload tus: pre-check per hash, chunk adattivi, checksum per chunk ed end-to-end, verifica di decodificabilità, `rename()` atomico.
- Wizard WebDAV con configurazioni pronte e indicatore di prima connessione.
- Frontend: pannello upload persistente e riprendibile; PWA con Share Target.

**Rischi**
- Compatibilità Finder e Windows Explorer. *Mitigazione: matrice di test client documentata; rclone come riferimento.*
- `PROPFIND` su cartelle enormi. *Mitigazione: risposta dal DB in streaming, verificata su una cartella da 40.000 file.*

**Dimensione:** ~1,5 settimane.

**Chiusa quando:** `rclone bisync` completa un ciclo su una cartella reale e i file caricati compaiono in timeline entro pochi secondi.

---

## Fase 6 — Consolidamento

**Obiettivo.** Rendere il sistema mantenibile e pronto per il client mobile.

**Consuma:** tutto.

**Produce**
- Video: probe, direct play, transcodifica on-demand in HLS con cache, poster e anteprima animata.
- Backup e ripristino: formato `.kpxb`, destinazioni S3/WebDAV/SFTP/locale, wizard, prova di ripristino mensile.
- 2FA TOTP con codici di recupero.
- Scheduler di manutenzione completo: scrubbing d'integrità, pulizie, `VACUUM`.
- `/sync/delta` con tombstone e cursore corretto rispetto alle transazioni.
- OpenAPI pubblicata, client TypeScript generato, generazione Kotlin/Swift verificata.
- PWA completa, service worker, stati offline.
- Documentazione utente.

**Rischi**
- Il cursore di `change_log` con transazioni concorrenti. *Mitigazione: arretramento a `pg_snapshot_xmin`, con test dedicato che apre transazioni sovrapposte.*
- La transcodifica software su ARM è lenta. *Mitigazione: direct play copre il 90%; la transcodifica è on-demand e in cache.*

**Dimensione:** ~2 settimane.

**Chiusa quando:** dalla specifica OpenAPI si genera un client funzionante, e un ripristino da backup su macchina vuota riporta l'istanza allo stato esatto.

---

## Fase 7 — Scene, tag e ricerca semantica

**Spec:** [`../specs/fase-7-ai-tag-scene.md`](../specs/fase-7-ai-tag-scene.md)

**Obiettivo.** Trovare una foto descrivendola («tramonto con casa»), e vedere le categorie popolarsi da sole — con i tag decisi dall'utente, mai inventati dalla macchina.

**Consuma dalla 1:** coda job, `EnergyProfile`, watcher. **Dalla 3:** `VisibilityScope` — obbligatoria, non opzionale.

**Produce**
- Un solo embedding CLIP per foto (MobileCLIP2-S2 via ONNX Runtime, crate `ort`), che serve **contemporaneamente** ricerca semantica, abbinamento tag e «foto simili».
- pgvector nello stesso Postgres, con immagine DB custom (PostGIS **e** `vector`) e degradazione pulita se manca.
- `tags`/`asset_tags` con soglie per-tag, suggerimenti separati dalle assegnazioni, e decisioni umane immuni ai ricalcoli.
- Probe hardware **esteso e reale**: misura ms/inferenza sulla macchina vera, salda il debito che oggi restituisce `"unprobed"`.
- Backfill che si ferma da solo quando qualcuno usa la galleria (eredita `JobPriority::Background`).
- Due varianti nuove dell'AST di ricerca: `Tag`, `Semantic`.

**Rischi**
- *I benchmark del modello non sono misurati su Pi.* Mitigazione: il primo task misura sull'hardware vero; se i numeri sono peggiori cambia la stima del backfill, non l'architettura.
- *`tract` (Rust puro, più leggero) potrebbe non supportare tutti gli operatori.* Mitigazione: si prova per prima; `ort` è il ripiego, deciso da una misura non da una preferenza.
- *L'immagine DB custom è infrastruttura nuova.* Mitigazione: chi usa Postgres esterno resta supportato, con le funzioni AI spente e un messaggio che spiega come attivarle.

**Dimensione:** ~1,5 settimane.

**Chiusa quando:** su Pi 5, «tramonto con casa» risponde in meno di un secondo su libreria reale, e creare un tag nuovo lo popola **senza rianalizzare le foto**.

---

## Fase 8 — Volti

**Spec:** [`../specs/fase-8-volti.md`](../specs/fase-8-volti.md)

**Obiettivo.** Raggruppare le persone, lasciando all'utente l'ultima parola: unire, separare, scartare i falsi positivi — e non doverlo rifare mai più.

**Consuma dalla 7:** motore di inferenza, pgvector, probe, backfill. Non ne duplica nessuno.

**Produce**
- SCRFD (rilevamento) + allineamento Umeyama + ArcFace (identità), via il crate `face_id`, stesso stack `ort` della 7.
- Raggruppamento **incrementale**: un volto alla volta, nessuna riaggregazione globale che cancellerebbe le correzioni.
- `person_separations` — la tabella che rende permanente una separazione fatta a mano.
- Persone, gruppi di persone (distinti dai `groups` di utenti della Fase 3), copertine, «nascondi».
- Varianti di ricerca `Person`, `PersonGroup`, `PersonCount`.

**Rischi**
- *Le correzioni manuali cancellate dal ricalcolo successivo.* È il difetto tipico di questa funzione. Mitigazione: `person_separations` e `faces.assigned_by` — la decisione umana batte sempre la misura.
- *Dati biometrici.* Mitigazione: tutto locale, disattivabile per intero, cancellabile davvero, e **mai** esposto sui link pubblici.

**Dimensione:** ~1,5 settimane.

**Chiusa quando:** le foto di una persona stanno sotto un nome, e unioni/separazioni/rifiuti sopravvivono a una rianalisi completa.

---

## Fase 9 — Organizzazione: culling a cartelle, spostamento sicuro, rinomina

**Spec:** [`../specs/fase-9-organizzazione.md`](../specs/fase-9-organizzazione.md)

**Riscrive parte del comportamento della Fase 2** (già chiusa e mergiata):
Culling oggi non ha ambito e scegliere/scartare è solo un flag. Questa fase
non è un difetto della 2 da correggere in silenzio — è un requisito reale
scoperto dopo, con una spec propria, come da metodo del progetto.

**Obiettivo.** Chiudere il flusso reale: importare RAW in viaggio, sceglierli,
tornare a casa e prendere dal proprio PC **solo quelli scelti** via WebDAV,
svilupparli, cancellare il RAW da Keeppix a lavoro finito — senza mai
intervenire a mano sul filesystem.

**Consuma dalla 1:** identità dell'asset (**e ne corregge un buco reale**: oggi
rinominare o spostare un file fuori da Keeppix perde rating/scelta/titolo,
copiando solo l'EXIF sulla riga nuova). **Dalla 2:** `asset_flags`, sidecar
XMP, `asset_overrides.title`. **Dalla 3:** permessi editor su spostamento e
rinomina.

**Produce**
- `AssetRepo::move_asset` — spostamento sicuro che aggiorna la riga esistente
  (stesso `asset_id`) invece di crearne una nuova: il meccanismo che
  Culling e la rinomina condividono, e che in futuro userà anche MOVE di
  WebDAV (non implementato qui).
- Culling a cartelle fisiche: una radice designata per libreria, sottocartelle
  `_taken`/`_skipped` create automaticamente, click su scelto/scartato che
  sposta il file **solo** se l'asset è già dentro un lotto di culling — altrove
  nella libreria resta il comportamento a solo flag di oggi.
- Rinomina con formule (`{data}`, `{fotocamera}`, `{luogo}`, `{titolo}`,
  `{prog}` + testo libero), con anteprima e collisioni bloccanti, annullabile
  con lo stesso meccanismo `metadata_batches` della Fase 2. Tre punti
  d'ingresso: foto singola, selezione multipla, cartella intera.
- Nuovo filtro `Pick` nell'AST di ricerca, per "cartella X, stato scartato"
  prima di cestinare in blocco.
- Percorso della cartella visibile e navigabile nel pannello dettaglio —
  serve ora che una foto può spostarsi da sola in conseguenza di una scelta.

**Rischi**
- *Lo spostamento fisico rompe l'ordine scrittura-file / scrittura-riga a
  metà operazione.* Mitigazione: il file si sposta sempre prima della riga;
  un file orfano lo ritrova il watcher al giro successivo, una riga orfana no.
- *Le due modalità (cartelle vs. solo flag) si confondono in testa
  all'utente.* Mitigazione: la decide la posizione dell'asset nell'albero,
  non un interruttore da ricordarsi di girare.

**Dimensione:** ~1 settimana.

**Chiusa quando:** un viaggio reale — import su più giorni, culling,
rinomina, prelievo da WebDAV, sviluppo esterno, cancellazione dei RAW —
si completa senza toccare il filesystem a mano.

---

## Stima complessiva

| Fase | Dimensione | Cumulato |
|---|---|---|
| 0 Scheletro | 2-3 giorni | ~3 g |
| 1 Ingestione | 2-3 settimane | ~4 sett. |
| 2 RAW | 1-1,5 settimane | ~5,5 sett. |
| 3 Multiutente | 1,5-2 settimane | ~7,5 sett. |
| 4 Mappe | 1 settimana | ~8,5 sett. |
| 5 WebDAV | 1,5 settimane | ~10 sett. |
| 6 Consolidamento | 2 settimane | ~12 sett. |
| 7 AI scene e tag | 1,5 settimane | ~13,5 sett. |
| 8 Volti | 1,5 settimane | ~15 sett. |
| 9 Organizzazione | 1 settimana | ~16 sett. |

Stime a sviluppo continuativo. Vanno lette come rapporti fra le fasi, non come promesse sul calendario: la Fase 1 vale da sola un quarto del totale, ed è lì che va messa l'attenzione.

Le stime di 7 e 8 sono **le più fragili delle nove**: dipendono da quanto costa davvero un'inferenza sull'hardware di destinazione, che nessuno ha ancora misurato. Il primo task della Fase 7 esiste apposta per sostituire questo numero con una misura.

## Momenti in cui questa roadmap va rivista

Non è un documento da firmare e archiviare. Va aggiornato quando:

1. **La Fase 1 produce i numeri reali** — conteggio file, copertura preview RAW, throughput effettivo. Ricalibrano 2, 4 e 6.
2. **Decidi di riordinare le fasi 2/4/5** — sono intercambiabili, l'ordine dipende da cosa ti serve prima.
3. **Una fase supera i 20 task in fase di pianificazione** — va divisa in due piani, come già previsto per la Fase 1.
4. **Emerge un requisito che tocca un contratto congelato** — in quel caso si torna allo spec, non si aggira il contratto in un piano.
