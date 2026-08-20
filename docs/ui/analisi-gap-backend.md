# Audit GAP — UI (documento funzionale) vs backend reale (origin/fase-6, 569fe3d)

Fonte UI: `docs/ui/documento-funzionale-ui.md` (10.823 righe, 70 schermate + SP-1..SP-30)
Fonte prototipo: `docs/ui/keeppix-mockup.html` — navigato dal vivo, interattivo, JS reale.
Fonte backend: worktree su `origin/fase-6`; superficie API letta dal router (`crates/keeppix-api/src/lib.rs::api_routes`).

---

## A. Le otto richieste dichiarate "che toccano il backend"

| # | Richiesta | Stato | Evidenza |
|---|---|---|---|
| 1 | Proporzioni di **tutti** gli scatti di una vista senza caricarne le miniature (id, w, h, mese) | ❌ **NON soddisfatta** | `/timeline/buckets` restituisce solo `{month, count}`. Le dimensioni stanno in `AssetView` di `/timeline`, che è paginata (limit max 200) e grassa (content_hash, size_bytes, kind, status, taken_at, thumbhash, location…). Per 214k foto servirebbero ~1.070 richieste pesanti solo per calcolare la geometria. Il documento dichiara questa la richiesta n.1 e dice esplicitamente che "se non fosse realizzabile, cambia il disegno". |
| 2 | Operazioni di massa con **esito per-foto** e ragione di ogni fallimento | ❌ **NON soddisfatta** | `POST /flags/batch` → `204 No Content`. `POST /metadata/batch` → solo `{batch_id}`. Nessun endpoint di massa restituisce liste riuscite/fallite. È esattamente l'anti-pattern "fatto / non fatto" che il documento chiama "una bugia". |
| 3 | Conteggio reale di foto **per mese**, aggregato indipendente dall'elenco | ✅ **soddisfatta** | `GET /timeline/buckets` → `[{month:"YYYY-MM", count}]`, con supporto `bbox` e `library`. |
| 4 | Una foto è una **pila** (RAW+JPEG = un solo scatto) | ⚠️ **parziale** | Tabella `stacks` + `GET /assets/{id}/stack` + `POST /assets/{id}/stack/primary` esistono (Fase 2). **Ma `AssetView` della timeline non espone lo stack**: la timeline restituisce RAW e JPEG come due asset distinti → due tile. Manca anche il campo per il badge `RAW` / `RAW+JPEG` (SP-15). Il documento dice che la pila "attraversa tutto il modello: conteggi, selezione, eliminazione, rinomina". |
| 5 | **Provenienza** di ogni etichetta conservata, non dedotta (IA vs umano) | 🔵 **specificata, non costruita** | Non esiste nulla di tag nel backend. La spec Fase 7 prevede `asset_tags.source IN ('ai','user')` + `decided_by`/`decided_at`. Da verificare che copra tutti i casi SP-12. |
| 6 | Eliminare ha **tre destinazioni** e nessun default implicito | ✅ **soddisfatta (singola)** | `DiskAction::{Kept, MovedToTrash, Purged}` = solo indice / `.keeppix-trash` 30gg / disco. Parametro obbligatorio, nessun default. `Purged` ristretto a owner/admin. ⚠️ **Ma esiste solo su `DELETE /assets/{id}` (singolo)**: non c'è eliminazione di massa, che la UI usa in SP-2 e nei duplicati. |
| 7 | Distinguere **≥4 nature di fallimento** (irraggiungibile / permessi / file assente / timeout) | ⚠️ **da verificare** | Esiste il tipo `Problem` (RFC 7807) con codici. Da mappare 1:1 sulle quattro nature richieste dalla UI. |
| 8 | I volti **mai** su link pubblico, non configurabile | 🔵 **specificata, non costruita** | Nessun volto nel backend (Fase 8 non implementata). La regola va garantita *dove i link pubblici vengono serviti* (`routes/share.rs`), non solo in UI. |

---

## B. Superficie API esistente (Fasi 0–6) — 68 path, 81 operazioni

Presenti e utilizzabili dalla UI:
auth (login/refresh/logout/me, TOTP completo, app-passwords) · setup · **timeline + buckets** ·
folders (tree/children/relocate) · **viewport** (promozione miniature) · search (+suggest, saved-searches) ·
places (reverse/suggest) · map (clusters, tiles, regions) · ws (ticket/connect) · sync/delta ·
problems · duplicates (list/members/resolve) · libraries (CRUD/scan/preview) · groups (CRUD/membri) ·
users (CRUD/password/home/disable/enable) · assets (get/delete/restore/stack/flags/metadata) ·
trash (list/empty) · metadata (batch/shift/timezone/undo, geotag copy-location/import-gpx) ·
flags (get/set/batch) · albums (CRUD + assets add/remove/reorder) · permissions (list/grant/explain/patch/revoke) ·
share (link CRUD + public info/assets/auth/upload) · audit · backup/restore · upload (tus) · health ·
media (thumb/preview/full/original, video playback/poster/hls)

⚠️ **L'OpenAPI generato copre solo 68 path ma NON include** albums, share, groups, permissions, audit,
backup, restore, upload, health: i client generati non li vedono. Debito da pagare.

---

## C. Assi di ricerca: presenti vs richiesti dalla UI

`SearchNode` (crates/keeppix-db/src/search.rs:27) ha: `And, Or, Not, Text, Type, Camera, Lens, Iso, Year, Folder, HasGps`.

La UI (§23–25 Cerca, §11 filtro rapido SP-3, §43 album dinamici) richiede in più:

| Asse | Dove serve | Stato |
|---|---|---|
| **Tag** (id, confermato) | chip SP-3, pillola Cerca, album dinamico | ❌ manca (Fase 7) |
| **Categoria** di tag | chip SP-3 | ❌ manca (Fase 7) |
| **Persona / volto confermato** | chip SP-3 (oggi disabilitato apposta), Persone | ❌ manca (Fase 8) |
| **Semantica / scena** (embedding) | Cerca testo libero | ❌ manca (Fase 7) |
| **Data**: giorno, mese, intervallo | placeholder topbar dice "data"; album dinamici | ❌ manca (c'è solo `Year`) |
| **Preferito** | chip "Preferiti" in Cerca, album dinamici | ❌ manca |
| **Valutazione 0–5** | album dinamici, filtri | ❌ manca |
| **Stato pick/scarta** | album dinamici, filtro cartella+stato (Fase 9) | ❌ manca (Fase 9) |
| **Paese** | pillola Cerca | ❌ manca |
| Diaframma / tempo | album dinamici | ❌ manca |

---

## D. Album: modello attuale insufficiente

`0016_albums.sql`: `albums(id, name, description, owner_id, cover_asset_id, created_at, updated_at)` +
`album_assets(album_id, asset_id, position, added_by, added_at)`.

La UI (§41 griglia, §42 dettaglio, §43 creazione) richiede in più:
- **album dinamici** = condizioni di filtro + operatore (tutte / almeno una), membri calcolati, non materializzati ❌
- flag **condiviso** (badge in griglia) ❌
- **tinta della copertina** e flag monocromatico ❌
- **intervallo di date testuale** (es. "Gen 2026 – Lug 2026") — derivabile, ma serve aggregato ❌
- conteggio membri come aggregato ⚠️

---

## E. Cosa manca del tutto e in quale fase è già previsto

| Area UI | Fase che la copre | Note |
|---|---|---|
| Tag, categorie, prompt, soglia, coda revisione, provenienza | **Fase 7** (spec scritta) | Il prototipo mostra `prompt` e `soglia` **per tag** → verificare che la spec `tags` li preveda |
| Ricerca semantica, analisi libreria, livelli Pieno/Ridotto/Spento | **Fase 7** | prototipo: progresso 128.450/214.000, pausa automatica, finestra notturna 2:00–7:00 |
| Persone, volti, gruppi, unisci/separa, coda volti | **Fase 8** | prototipo: gruppi Famiglia/Amici, 23 proposte |
| Culling a lotti, `_taken`/`_skipped`, rinomina con formula, spostamento file | **Fase 9** | prototipo: 3 lotti, P/X, "Svuota scartati", "Rinomina lotto…" |
| **Geometria timeline (richiesta #1)** | **nessuna** | → nuova fase |
| **Riuscita parziale (richiesta #2)** | **nessuna** | → nuova fase |
| **Stack collassato nelle viste di browse (richiesta #4)** | **nessuna** | → nuova fase |
| **Eliminazione di massa a 3 vie** | **nessuna** | → nuova fase |
| **Album dinamici + condiviso + tinta** | **nessuna** | → nuova fase |
| **Assi di ricerca: data, preferito, rating, paese** | **nessuna** | → nuova fase |
| **Tassonomia errori a 4 nature (richiesta #7)** | **nessuna** | → nuova fase |
| Sessioni attive elencabili/revocabili (§61 Profilo) | **nessuna** | → nuova fase |
| Spazio libero/totale del server (sidebar) | **nessuna** | → nuova fase |
| Preferenze utente persistite (tema, densità griglia, notifiche, lingua) | **nessuna** | → nuova fase |

---

## F. Note dal prototipo navigato dal vivo

- Interattivo davvero: `P` su un lotto porta 422→421 nel badge, "1 presi", stato "Presi".
- **Tag hanno `prompt` testuale e `soglia` %** (es. «cielo al tramonto o all'alba, colori caldi», 80%) — è il testo per l'embedding CLIP.
- Analisi: "Per ogni foto calcola **una volta** un vettore che serve sia per abbinare i tag … sia per la ricerca per descrizione libera" → conferma il disegno a embedding unico della spec Fase 7.
- Impostazioni: modello dichiarato `CLIP ViT-B/32 (locale, via ONNX Runtime)`, velocità `42 ms/foto` misurata; la spec Fase 7 dice MobileCLIP2-S2 → il nome modello è **dato dal backend**, nessun conflitto, ma da allineare.
- Mappe offline: "tile servite da questo server, mai da provider esterni" → coerente con PMTiles Fase 4.
- Condivisioni: ruoli Visualizzatore/Editor, accesso **ereditato** da gruppo+cartella, link con scadenza/password/download-originali/conteggio, "metadati ed EXIF nascosti di default senza password".
- Problemi: due nature reali — sidecar XMP non scrivibile (permessi) e libreria offline (percorso di rete) con "Riprova connessione".
- Cestino: 30 giorni, testo coerente con `DiskAction::MovedToTrash`.
- Profilo: sessioni attive per dispositivo/browser con "Esci da tutti gli altri dispositivi".
