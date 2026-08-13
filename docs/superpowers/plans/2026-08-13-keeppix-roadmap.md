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
      └───────────────────┬────────────────────┘
                          │
                     ┌────▼────┐
                     │ Fase 6  │  consolidamento
                     └─────────┘
```

**Vincoli reali di ordine**, non preferenze:

- **1 dipende da 0**: serve il database, le migrazioni e il server.
- **2, 4, 5 dipendono da 1**: tutte lavorano su asset già indicizzati.
- **2, 4, 5 sono indipendenti fra loro**: l'ordine fra queste tre è una tua scelta, guidata da cosa ti serve prima.
- **3 può iniziare dopo la 1**, ma conviene dopo che almeno una delle 2/4/5 è chiusa: la condivisione ha più senso quando c'è più materiale da condividere.
- **6 chiude tutto.**

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

Stime a sviluppo continuativo. Vanno lette come rapporti fra le fasi, non come promesse sul calendario: la Fase 1 vale da sola un quarto del totale, ed è lì che va messa l'attenzione.

## Momenti in cui questa roadmap va rivista

Non è un documento da firmare e archiviare. Va aggiornato quando:

1. **La Fase 1 produce i numeri reali** — conteggio file, copertura preview RAW, throughput effettivo. Ricalibrano 2, 4 e 6.
2. **Decidi di riordinare le fasi 2/4/5** — sono intercambiabili, l'ordine dipende da cosa ti serve prima.
3. **Una fase supera i 20 task in fase di pianificazione** — va divisa in due piani, come già previsto per la Fase 1.
4. **Emerge un requisito che tocca un contratto congelato** — in quel caso si torna allo spec, non si aggira il contratto in un piano.
