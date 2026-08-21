# Piano — Fase 9: Organizzazione — culling a cartelle, spostamento sicuro, rinomina

**Specifica:** `docs/superpowers/specs/fase-9-organizzazione.md`
**Base:** dopo la Fase 10 (involucro di riuscita parziale, avanzamento e annullamento delle
operazioni lunghe). **Non** dipende dalle Fasi 7 e 8.
**Branch:** `fase-9`.

> È la fase che **tocca file veri sul disco dell'utente**. Ogni task va scritto con quella
> consapevolezza: qui un difetto non produce una schermata sbagliata, produce un archivio
> rovinato.

---

## Il difetto reale che questa fase chiude

Rinominare o spostare un file **fuori** da Keeppix ne cambia l'identità: l'identità dell'asset è
`(folder_id, filename)` — contratto congelato — quindi il file riappare come asset nuovo e
**perde `asset_flags` e `asset_overrides`**. Sopravvive solo ciò che sta nell'EXIF, via
`moves.rs::after_hash`.

Non è una funzionalità mancante: è una **perdita di dati silenziosa** che accade oggi.

---

## Gruppo A — La primitiva

### Task 1 — `AssetRepo::move_asset`
Lo spostamento sicuro che conserva l'identità: `asset_flags`, `asset_overrides`, appartenenza
agli album, tutto.

**Ruling: il file si sposta prima, la riga si aggiorna dopo. Mai il contrario.** — Se la riga si
aggiorna per prima e lo spostamento fallisce, il database punta a un percorso che non esiste e
l'asset è irraggiungibile. Nell'ordine giusto, un fallimento lascia il file spostato e la riga
vecchia: la scansione successiva lo ritrova. **Il caso peggiore è recuperabile.** — *Costo se
sbagliato:* asset fantasma che nessuna scansione ripara.

Ogni spostamento verifica che la destinazione **non sia occupata**: non si sovrascrive mai —
best-effort contro ciò che Keeppix conosce (vincolo `UNIQUE`), non una garanzia atomica di
filesystem: `rename()` su POSIX sovrascrive in silenzio un file non ancora tracciato (dettagli
in spec §1.2). Il permesso su partenza e destinazione si verifica con **`FolderRepo::assert_editor`
chiamato due volte**, non `assert_can_edit_assets` (che risolve solo la cartella corrente di un
asset esistente, non una destinazione arbitraria — dettagli in spec §1.2).

**`FailureReason` (Fase 10, `crates/keeppix-api/src/bulk.rs`) non ha una variante per la
collisione di nome**: le cinque esistenti (`Unreachable`/`PermissionDenied`/`FileMissing`/
`Timeout`/`Unknown`) mappano oggi una collisione su `Unknown`, che in un ambito da 500 file non
dice all'utente cosa è successo. Aggiungere una variante `Collision` (o simile) prima di questo
task, non dopo.

---

## Gruppo B — Il culling a cartelle

### Task 2 — Cartella radice e ruoli
`libraries.culling_root_folder_id` e `folders.culling_role` (`'taken'` / `'skipped'`).

**Ruling: il ruolo è una colonna, non il nome della cartella.** — Riconoscere `_taken` dal nome
significa che una cartella chiamata così per caso diventa magica, e che rinominarla rompe tutto.
— *Costo se sbagliato:* una migrazione per marcare le cartelle esistenti.

### Task 3 — I lotti
Un lotto = una sottocartella della radice di culling. Elenco con nome, data, conteggi
**presi / scartati / da vedere**.

**Questo è l'unico conteggio esatto rimasto in tutta l'applicazione** (decisione del 20 agosto:
gli altri cinque sono stati tolti), perché *«quante me ne restano da vedere»* è letteralmente la
domanda che l'utente si sta facendo. Ed è anche il più economico: è per lotto, non per libreria.

### Task 4 — Scegliere e scartare **sposta il file**
Dentro un lotto, `"Scelta"` e `"Scarta"` spostano fisicamente in `_taken` / `_skipped` usando la
primitiva del Task 1. Cambiare idea sposta di conseguenza.

**Fuori da un lotto, nella libreria normale, la valutazione resta solo un flag**, come oggi.

**Ruling: lo spostamento è un effetto invisibile, non un gesto dell'utente.** — L'utente preme un
pulsante o un tasto; non trascina file. Visivamente è un'etichetta che cambia. — *Costo se
sbagliato:* si costruisce un'interfaccia a trascinamento che nessuno ha chiesto.

`"Svuota scartati"` elimina definitivamente il contenuto di `_skipped`, con conferma.

**Nota:** dentro un lotto, `"Scarta"` **non** apre il dialog a tre opzioni (SP-18): è uno
spostamento reversibile, non un'eliminazione.

### Task 5 — `SearchNode::Pick`
Filtrare per cartella **e stato** (presa / scartata / da valutare) è ciò che permette di
ripulire dopo, che è il modo in cui l'utente ha detto di voler lavorare.

---

## Gruppo C — La rinomina

> È la parte del prototipo **con più conseguenze sul disco e le convalide più deboli**. Il
> documento funzionale ne elenca cinque difetti espliciti. Vanno chiusi tutti **prima** di
> toccare file veri.

### Task 6 — Il motore delle formule
Segnaposto: `{data}`, `{fotocamera}`, `{luogo}`, `{titolo}`, `{prog:03}`, `{ext}`.
Il luogo si risolve con precedenza: posizione della foto → posizione della cartella → nome del
lotto → niente.

### Task 7 — Le cinque convalide che il prototipo non ha
1. **Collisioni verificate contro il disco**, non solo dentro il gruppo selezionato: anche
   contro i file già presenti, quelli fuori ambito e le sottocartelle escluse.
2. **Segnaposto vuoti che non lasciano separatori orfani**: `{data}_{luogo}_{n}` senza luogo non
   deve produrre `2026-08-14__001`.
3. **Sanificazione completa** dei caratteri vietati dai filesystem, e **limite di lunghezza**.
4. **Estensione sempre presente**: un nome senza estensione è un risultato malformato.
5. **`"Applica"` davvero disabilitato**, non solo sbiadito: nel prototipo da tastiera resta
   premibile e non dà riscontro.

**Ruling: l'anteprima è obbligatoria e blocca sulle collisioni.** — È l'ultima occasione in cui
l'utente vede cosa sta per succedere a file che non può recuperare. — *Costo se sbagliato:*
un'operazione irreversibile su centinaia di file, partita per errore.

### Task 8 — I tre ambiti, e uno da correggere
Foto singola, selezione, cartella o lotto intero.

**Da correggere:** nel prototipo `"Rinomina cartella…"` rinomina **solo le foto passate dai
filtri attivi**, mentre il sottotitolo dichiara *«Tutta la cartella "X" (N foto)»*. L'ambito va
reso esplicito nella richiesta e nel testo.

**I file affiancati si rinominano insieme:** il RAW e il JPEG di una stessa pila prendono lo
stesso nome. Il prototipo ha un solo `filename` per foto e non affronta il caso.

### Task 9 — Annullare
**Non è un drop-in del `undo` esistente.** `metadata_batches`/`OverridesRepo::undo_batch`
(`crates/keeppix-db/src/overrides.rs`) opera **solo** su colonne di `asset_overrides` (titolo,
descrizione, data, posizione, orientamento) — `filename`/`folder_id` vivono sulla tabella
`assets`, e `restore_previous` non tocca il filesystem: scrive/cancella righe, non sposta file.
Si riusa il **concetto** di raggruppamento e controllo di `metadata_batches` (stesso `batch_id`,
stesso audit), ma serve un ramo nuovo che richiami `move_asset` **al contrario** per ogni file
del lotto, non solo un ripristino di colonna diversa. Anche la guardia "già sincronizzato"
(`xmp_written_at >= applied_at`) è specifica alla sincronizzazione XMP e non si applica a un
annullamento di rinomina — verificare se serve una guardia equivalente per lo spostamento fisico.

---

## Gruppo D — Chiusura

### Task 10 — Operazioni lunghe
Rinomina di massa e spostamenti usano avanzamento e annullamento della **Fase 10 Task 16**:
`operation_id`, eventi sul WebSocket, `cancel`. **Annullare a metà produce una riuscita
parziale, non un rollback**: le rinomine già fatte sono fatte, e vanno elencate.

**Due cose da aggiungere esplicitamente, non assumere:**
- `OperationKind` (`crates/keeppix-domain/src/operation.rs`) oggi ha **una sola variante**,
  `LibraryScan` — serve una nuova variante (es. `BulkRename`) prima che questo task possa
  registrare un'operazione; il commento nel codice la anticipa apposta.
- La risposta della rinomina di massa **è** `BulkOutcome`/`BulkFailure` con `FailureReason`
  (`crates/keeppix-api/src/bulk.rs`, stesso involucro della Fase 10) — non una forma nuova.

### Task 11 — Documenti e la prova che conta

**Chiusa quando** un viaggio vero — import su più giorni, culling, rinomina, prelievo da WebDAV,
sviluppo esterno, cancellazione dei RAW — **si completa senza toccare il filesystem a mano**, e
senza che nessuna foto perda valutazione, tag o appartenenza agli album lungo il percorso.
