# Fase 9 — Organizzazione: culling a cartelle, spostamento sicuro, rinomina con formule

**Stato:** specifica di progetto, non ancora pianificata in task
**Dipende da:** Fase 1 (identità dell'asset — questa fase ne corregge un buco),
Fase 2 (`asset_flags`, `asset_overrides.title`, sidecar XMP), Fase 3 (permessi:
spostare e rinominare richiede ruolo editor). Non blocca né è bloccata dalla
Fase 5, ma **ne beneficia direttamente** (§2.4).
**Chiusa quando:** un viaggio fotografico reale — import su più giorni, culling,
rinomina, prelievo da WebDAV, sviluppo esterno, cancellazione dei RAW — si
completa senza mai dover intervenire a mano sul filesystem

---

## 0. Il problema che questa fase risolve davvero

Non è "aggiungere uno strumento di rinomina". Il bisogno reale, descritto per
esteso durante il disegno di questa fase, è un flusso completo:

> Fuori casa, importo i RAW, li seleziono nei giorni del viaggio. Tornato a
> casa, dal mio PC prendo **solo quelli scelti**, li sviluppo con il mio
> software, e ricarico il risultato come foto normale — JPEG, HEIF, quello che
> è. Il RAW, finito il suo lavoro, lo cancello da Keeppix.

Verificato sul codice attuale: **oggi questo flusso non è possibile.**
Scegliere/scartare è solo un flag nel database; il sidecar XMP che lo
registra non è nemmeno visibile da WebDAV (PROPFIND legge righe `assets`, i
sidecar non lo sono); e se un file viene rinominato o spostato fisicamente —
da Finder, da rclone, o in futuro dal comando MOVE di WebDAV — **il sistema
perde rating, scelta/scarto e titolo**: crea un asset nuovo con un id nuovo,
e li lascia orfani sulla riga vecchia (dettagli in §1).

Questa fase costruisce, in quest'ordine di dipendenza reale:

1. Un **meccanismo di spostamento sicuro** che non esiste oggi (§1) — senza,
   nessuna delle due funzionalità sotto può esistere senza corrompere lo storico.
2. **Culling a cartelle fisiche** (§2), che dà un luogo reale al flusso di
   triage e rende WebDAV utilizzabile per prelevare gli scelti **senza
   aggiungere niente a WebDAV stesso**.
3. **Rinomina con formule** (§3), disponibile ovunque nell'app, non solo qui.

---

## 1. Lo spostamento sicuro — il debito che tutto il resto eredita

### 1.1 Il difetto verificato

L'identità di un asset è `(folder_id, filename)` — contratto congelato dalla
Fase 1a. `AssetRepo::upsert_discovered` la usa come chiave esatta: se un file
cambia nome o cartella, **non riconosce "stesso file, spostato"**. Crea una
riga nuova con un id nuovo.

Esiste già una riconciliazione parziale (`moves.rs::after_hash`, agganciata
dopo l'hashing): quando trova un file con lo stesso hash e la stessa
dimensione la cui vecchia posizione non esiste più, copia **solo l'EXIF**
sulla riga nuova, e marca la vecchia `offline` (sparisce dalla timeline).
Rating, pick/scarto, titolo, descrizione, posizione assegnata a mano: **non
sopravvivono**. Restano scritti su una riga morta che nessuno vede più.

Questo è già un rischio oggi (chiunque tocchi i file da Finder mentre WebDAV
è montato), e lo sarebbe ancora di più quando il comando MOVE di WebDAV sarà
implementato — ma non è compito di questa fase toccare quel comando: costruisce
solo il meccanismo corretto, perché lo usino sia questa fase sia, quando
arriverà il momento, quello.

### 1.2 Il meccanismo

Una funzione di repository — `AssetRepo::move_asset(ctx, asset_id,
new_folder_id, new_filename)` — che, in una sola transazione:

1. Verifica il permesso di scrittura su cartella di partenza **e** di
   destinazione (`assert_can_edit_assets`, riuso diretto dalla Fase 3).
2. Verifica che `(new_folder_id, new_filename)` non collida con un asset
   esistente (stesso vincolo `UNIQUE` già in schema).
3. Sposta il file fisico (`rename()` — stesso filesystem, quindi atomico e
   istantaneo anche su file da 100 MB, identico principio dei temporanei
   della Fase 5).
4. Sposta il sidecar `.xmp`, se esiste, allo stesso nuovo nome.
5. Aggiorna `folder_id`/`filename` sulla riga **esistente** — stesso
   `asset_id`, quindi `asset_flags`, `asset_overrides`, `asset_tags`, `faces`
   restano collegati senza toccarli: sono chiavi esterne su `asset_id`, che
   non cambia mai.

**L'ordine tra passo 3 e passo 5 conta**: il file fisico si sposta prima, poi
si registra — mai il contrario. Se il processo si interrompe a metà, un file
fisico "in più" senza riga corrispondente lo si ritrova al prossimo giro del
watcher (finisce indicizzato come nuovo, nessun dato perso); una riga che
punta a un file che non esiste è invece invisibile e silenziosa.

### 1.3 La riconciliazione esterna, corretta di conseguenza

`moves.rs::after_hash` (il percorso che intercetta rinomine fatte **fuori**
da Keeppix — Finder, rclone) viene esteso per copiare, oltre all'EXIF, anche
`asset_flags` e `asset_overrides` sulla riga nuova prima di marcare quella
vecchia offline. Non elimina il difetto per costruzione come `move_asset`
(che aggiorna la riga esistente, non ne crea una nuova) — resta un
riconoscimento *a posteriori* — ma chiude il buco pratico: oggi quella
copia non esiste per niente.

---

## 2. Culling a cartelle fisiche

### 2.1 Perché non il modello attuale

Verificato: oggi Culling non ha ambito. Mostra "qualunque cosa la Timeline
aveva caricato in quel momento" — nessun legame con una cartella, nessun
concetto di sessione o importazione. Scegliere/scartare è solo un flag, zero
effetto sul disco.

Questo modello resta corretto per **rivalutare foto già sistemate** nella
loro collocazione definitiva (vedi §2.5) — ma non serve al flusso reale
descritto in §0, dove il bisogno è: portare fuori, da un client esterno, solo
i RAW scelti di *questo* viaggio.

### 2.2 Struttura

Una cartella radice, **una sola per libreria**, designata dall'utente (non un
nome inglese incorporato nel sistema — vedi §2.6):

```
Culling/                        ← radice, designata nelle impostazioni della libreria
  Vacanze 2026-07/              ← una per importazione: la crea l'utente, come qualsiasi cartella
    DSC004.ARW                  ← non ancora valutata: resta nella radice del lotto
    _taken/                     ← creata da Keeppix alla prima decisione utile
    _skipped/                   ← idem
  Vacanze 2026-09/               ← il viaggio successivo, isolato dal precedente
    _taken/
    _skipped/
```

`_taken`/`_skipped` **non sono un nome libero riconosciuto per stringa**: sono
marcate da una colonna (§2.7), cosicché un utente che avesse già una cartella
chiamata così altrove nella libreria non venga confuso con una cartella di
culling — la marcatura, non il nome, decide il comportamento.

### 2.3 L'interazione resta un'etichetta, lo spostamento è un effetto

In Culling, "Scelto"/"Scartato" restano gli stessi due pulsanti di oggi — un
click, mai un trascinamento manuale di file. La differenza è cosa succede
dietro:

```
click "Scelto"    → asset_flags.pick = 'pick'   → move_asset verso _taken
click "Scartato"  → asset_flags.pick = 'reject' → move_asset verso _skipped
click di nuovo su "Scelto" dopo aver scartato
                   → move_asset da _skipped verso _taken (cambio idea)
annulla la decisione
                   → move_asset verso la radice del lotto (di nuovo in attesa)
```

Ogni transizione passa dal meccanismo di §1 — non solo la prima.

**Condizione, non comportamento globale**: questo spostamento avviene **solo
se l'asset si trova già dentro un lotto di culling** (discendente diretto
della radice designata). Una foto valutata altrove nella libreria — già
sistemata nella sua cartella definitiva — continua a comportarsi come oggi:
solo flag, zero effetto sul disco. Nessun interruttore da girare: lo decide
la posizione dell'asset nell'albero, che è già l'informazione che serve.

### 2.4 Perché WebDAV non ha bisogno di sapere niente di tutto questo

Con file veri in cartelle vere, `PROPFIND`/`GET` (già scritti in Fase 5) le
espongono senza alcuna modifica. Montare `Culling/Vacanze 2026-07/_taken/`
come disco e trascinarla nel software di sviluppo funziona il giorno stesso
in cui questa fase chiude, senza toccare una riga di codice WebDAV.

(Durante il disegno di questa fase era stata considerata una vista virtuale
`/dav/culling/` che rispecchiasse l'albero filtrando sugli scelti. Il modello
a cartelle fisiche la rende superflua: nessuna vista virtuale va costruita.)

### 2.5 Le due modalità, entrambe valide

| | Culling a cartelle (§2.2-2.4) | Valutazione sul posto |
|---|---|---|
| Dove | dentro la radice designata | ovunque nella libreria |
| Effetto sul disco | sposta fisicamente | nessuno, solo flag |
| A cosa serve | RAW in triage, in attesa di essere sviluppati e poi cancellati | foto già sistemate, da valutare senza riorganizzarle |

Non è una sostituisce l'altra: sono due risposte a due bisogni diversi, e
quale delle due si attiva lo decide la posizione dell'asset, non una scelta
esplicita dell'utente.

### 2.6 Designare la radice

Un campo su `libraries` (`culling_root_folder_id`, nullable), impostato dal
proprietario nelle impostazioni della libreria — su una cartella già
esistente o creata per l'occasione, con qualunque nome l'utente scelga. Senza
una radice designata, Culling si comporta esattamente come oggi (nessun
comportamento nuovo forzato su chi non lo usa).

### 2.7 Schema

```sql
ALTER TABLE libraries ADD COLUMN culling_root_folder_id uuid REFERENCES folders(id);

-- NULL per ogni cartella normale, incluse le radici dei lotti. Marca solo le
-- due sottocartelle che Culling crea e gestisce da sé.
ALTER TABLE folders ADD COLUMN culling_role text CHECK (culling_role IN ('taken','skipped'));

CREATE INDEX folders_culling_role_idx ON folders (culling_role) WHERE culling_role IS NOT NULL;
```

### 2.8 Filtrare per cartella e stato, poi cancellare

Nessuna azione automatica "ho finito, archivia" — verificato non necessaria:
il flusso reale è filtrare per cartella (il viaggio) **e** stato
(scelto/scartato), poi usare il cestino già esistente (Fase 2, a tre
opzioni) sul risultato.

Il filtro per cartella esiste già nell'AST di ricerca (`Folder{id}`). Manca
lo stato: si aggiunge `Pick{value}` come nuova variante di `SearchNode`,
stesso pattern delle aggiunte già previste in Fase 7 (`Tag`, `Semantic`) —
un nodo in più, non un secondo filtro.

---

## 3. Rinomina con formule

### 3.1 I token

| Token | Sorgente | Se manca il dato |
|---|---|---|
| `{originale}` | nome file attuale, senza estensione | sempre presente |
| `{data}` | `taken_at`, formato configurabile (default `AAAA-MM-GG`) | sempre presente (fallback: mtime, come il resto del sistema) |
| `{fotocamera}` | EXIF, modello | segmento vuoto |
| `{obiettivo}` | EXIF, lente | segmento vuoto |
| `{luogo}` | posizione effettiva (Fase 4: geocoding inverso o assegnazione manuale) | segmento vuoto |
| `{titolo}` | `asset_overrides.title` | segmento vuoto |
| `{prog}` | contatore nel lotto selezionato, `{prog:03}` → `001` | su una foto singola vale sempre `1` |
| `{ext}` | estensione originale | gestita a parte, mai dentro il testo libero della formula |

Un segmento vuoto non lascia un separatore doppio a vista (`_titolo__data_`
diventa `_data_`, non con un buco). Il resto della formula è testo libero
scelto dall'utente, mescolato ai token — `Vacanze2026_giorno1_{fotocamera}_{prog}`
è già una formula valida così com'è, senza bisogno di un token dedicato al
"giorno del viaggio": quello lo scrive l'utente.

### 3.2 Anteprima obbligatoria

Nessuna rinomina si applica senza aver prima mostrato i nomi calcolati e
segnalato ogni collisione (due foto che risulterebbero con lo stesso nome —
capita più spesso di quanto sembri, tipicamente quando `{luogo}` è vuoto su
più scatti dello stesso minuto). Le collisioni bloccano l'applicazione finché
la formula non le distingue — stesso principio già stabilito per il
ricalcolo dei fusi orari in Fase 4.

### 3.3 Annullabile

Stesso meccanismo `metadata_batches` già esistente dalla Fase 2 — non un
sistema di undo nuovo. Un batch di rinomina è, per l'undo, indistinguibile da
un batch di modifica titolo: cambia solo quale campo viene ripristinato.

### 3.4 Tre punti d'ingresso, un solo meccanismo

- **Foto singola** — un'azione nel pannello dettaglio (§4), condiviso da
  Timeline, Culling, Album e ricerca: si scrive una volta, compare ovunque.
- **Selezione multipla** — dentro il pannello di modifica in blocco già
  esistente (`BatchEditView`), nuova sezione accanto a titolo/descrizione.
- **Cartella intera** — un'azione sulla cartella stessa, con l'opzione
  "includi sottocartelle" (serve esattamente per applicare una formula a
  `Vacanze 2026-07/_taken/` senza dover selezionare a mano ogni foto, o per
  includere anche `_skipped` se lo si desidera).

Tutti e tre finiscono sullo stesso endpoint con la stessa validazione,
anteprima e undo — cambia solo come si arriva alla lista di asset su cui
applicare la formula.

Ogni rinomina passa dal meccanismo di spostamento sicuro (§1): stesso
`asset_id`, `folder_id` invariato (la rinomina non è uno spostamento tra
cartelle), solo `filename` cambia, sidecar incluso.

---

## 4. Il percorso della foto, nel dettaglio

Il pannello dettaglio (condiviso da tutte le viste) mostra il percorso
attuale della cartella come breadcrumb navigabile — cliccarlo porta alla
cartella nella vista Cartelle.

Non è un'aggiunta cosmetica: ora che una foto **può spostarsi da sola** in
conseguenza di un click su "Scelto"/"Scartato" (§2.3), sapere dove si trova
in questo momento è un'informazione che prima non serviva e ora sì.

---

## 5. Cosa NON è in Fase 9

- **Il comando MOVE di WebDAV**: resta 501 come oggi. Questa fase costruisce
  il meccanismo che quel comando dovrà usare quando arriverà il suo turno,
  non lo implementa.
- **Rinomina massiva dell'intera libreria in un colpo solo** senza passare da
  una selezione/cartella esplicita: fuori scope, e probabilmente indesiderata
  — un'anteprima su 200.000 file non è più un'anteprima.
- **Un token per il "giorno del viaggio" calcolato automaticamente**: il testo
  libero della formula già lo copre; si aggiunge solo se in pratica risulta
  scomodo scriverlo a mano ogni volta.
- **Regole di rinomina automatiche/pianificate** (es. "rinomina sempre così
  ogni nuovo import"): questa fase è uno strumento su richiesta, non un
  automatismo in background.
