# SDD ledger — plan: docs/superpowers/plans/2026-08-14-keeppix-fase-1a.md

Spec: docs/superpowers/specs/2026-08-13-keeppix-design.md
Roadmap: docs/superpowers/plans/2026-08-13-keeppix-roadmap.md
Stato Fase 0: docs/superpowers/plans/2026-08-13-keeppix-fase-0-STATO.md (versione su `main`)
Branch: `fase-1` (da `main` @ 0b1839f)
Workspace: `.superpowers/sdd/2026-08-14-keeppix-fase-1a/`

Ruling: lavoro in-place sul branch, non in un worktree separato — stesso
precedente della Fase 0 (R1) e richiesta esplicita dell'utente di un branch
`fase-1`. Il vincolo SDD è "non su main", che il branch soddisfa.

PR: bozza dopo la chiusura della 1a (la CI gira); merge su main solo a fine
Fase 1 (1a+1b+1c), come chiesto dall'utente. Questa sessione esegue **solo 1a**.

## Scansione pre-volo

### Coppie di task che condividono file o interfacce

| Task A | Task B | Cosa produce A / consuma B | Esito |
|---|---|---|---|
| 1 | 4,5,7,8 | convenzione `FromRow` + `row::corrupted` | ok, tutti i repo nuovi la usano |
| 2 | 4,5 | tabelle `libraries`/`folders` + `seed_admin` | ok |
| 2 | 5 | `0004` senza `next_folder_seq` ↔ Task 5 la aggiunge con ALTER | **F1** |
| 2 | 6 | `folder_month_counts` nata in 0004, `assets` in 0005 | ok |
| 3 | 4,5,7 | tipi di dominio | ok, 3 è puro |
| 4 | 5 | `LibraryRepo` per il seeding dei test cartelle | ok |
| 5 | 7 | `FolderRepo::ensure_*` per il seeding degli asset | ok |
| 6 | 7 | tabella `assets` | ok |
| 7 | 8 | trigger su `assets` alimenta `change_log` | ok, 8 dopo 7 |

### Coerenza interna

| Task | Problema | Esito |
|---|---|---|
| 2 | gli INSERT di `schema_0004.rs` omettono `depth`, che è `NOT NULL` senza default | **F2** |
| 3 | spec §4.1 mette `camera_*` su `assets` e `status=active`; il piano li mette su `asset_exif` e usa `discovered`/`indexed` | **F3** (si segue il piano) |
| 4 | `format!` interpola la costante `COLUMNS` | ok, il piano lo dichiara; i dati restano su `bind` |
| 7 | `VisibilityScope` è specificato a parole, non con codice completo | ok, l'implementatore ha giudizio sul "metodo che produce la clausola SQL" |
| 8 | test transazioni sovrapposte è il cuore del task | ok, non comprimibile |

### Rulings pre-volo

**F1 — `next_folder_seq` nasce in `0004`, non in un ALTER del Task 5.**
Il Task 5 chiede `ALTER TABLE libraries ADD COLUMN next_folder_seq …` sulla
migrazione già scritta dal Task 2. Non c'è un rilascio in mezzo, quindi
modificare `0004` è legale (stesso ragionamento di PostGIS in `0001`), ma
farlo *dopo* che i test del Task 2 l'hanno applicata è rumore inutile: la
colonna serve all'albero, quindi sta nella migrazione dell'albero.
Ruling: il Task 2 include `next_folder_seq bigint NOT NULL DEFAULT 1` in
`0004`. Il Task 5 non tocca la migrazione, usa la colonna.
*Costo se sbagliato:* nessuno — è la stessa colonna, scritta prima.

**F2 — gli INSERT di `schema_0004.rs` devono includere `depth`.**
La migrazione del Task 2 dichiara `depth int NOT NULL` senza default; i test
del piano inseriscono solo `(id, library_id, parent_id, name, path)`. I test
fallirebbero *dopo* la migrazione, non prima.
Ruling: ogni INSERT nei test del Task 2 porta `depth` (1 per la radice, 2
per i figli). Nessun `DEFAULT` sulla colonna: il valore lo calcola
`FolderRepo`, i test di schema parlano SQL grezzo.
*Costo se sbagliato:* i test di schema diventerebbero verdi per la ragione
sbagliata.

**F3 — si segue lo schema del piano, non lo sketch dello spec §4.1.**
`AssetStatus::{Discovered, Indexed, Offline, Error, Trashed}` e i campi
fotocamera su `asset_exif`, non su `assets`. Lo spec §4.1 è uno sketch; il
piano 1a è la decisione operativa, e allinea lo status alle fasi della
pipeline (§5) invece di un unico `active`.
*Costo se sbagliato:* la Fase 1b dovrebbe migrare 200k righe per
introdurre `discovered`.

**F4 — workspace SDD versionato, non auto-ignorato.**
Lo script `sdd-workspace` della skill scrive `.superpowers/sdd/.gitignore`
con `*`. Vince R11 della Fase 0 e `.gitignore` del repo: il ledger si
versiona. I brief/report vivono in questa cartella e vengono committati,
come in Fase 0.

## Ledger

Task 1: complete (commits 0b1839f..63dd6a6, review clean)
Task 2: complete (commits b9caaaa..971e5f4, review clean)
Task 3: complete (commits 5ad848f..0564f66, review clean)

Minor differiti (Task 2, da riesaminare in review finale):
- `sibling_folders_cannot_share_a_name` collide anche su `path` (`1.2`), quindi non isola `folders_sibling_name_key` (plan-mandated).
- `folders_single_root_key` e `folder_month_counts` esistono ma non hanno un test dedicato.

Minor differiti (Task 3):
- `FolderPath::root`/`child` accettano `i64` negativi che `parse` rifiuta (plan-mandated).
- `AssetName` rifiuta NUL ma il caso non è testato.

Task 4: complete (commits 0713b8e..3506b5c, review clean)

Minor differiti (Task 4):
- `find_by_id` admin+id inesistente → `NotFound` non ha un test dedicato.
- `set_status` Forbidden non ha un test dedicato (delega a `find_by_id`).

Nota per Task 5: `ON CONFLICT` su `folders_sibling_name_key` deve usare il predicato parziale `WHERE parent_id IS NOT NULL`. Lo stesso per la radice: `folders_single_root_key` è `ON (library_id) WHERE parent_id IS NULL`. `next_folder_seq` è già in `0004` (F1): non fare ALTER.

Ruling: lo snippet SQL di spec §1.3 / piano Task 5 tratta `$new_prefix` come il **nuovo path del nodo spostato**; l'implementazione passa il path del **genitore** e usa `subpath(..., nlevel(old)-1)` più `depth … + 1`. Equivalente. Lo snippet omette anche `parent_id` del nodo spostato, che l'`UPDATE` deve riscrivere. — La spec descrive l'effetto, non il bind. — *Costo se sbagliato:* un `mv` con path/depth sbagliati; i test di Task 5 lo inchiodano.

Ruling: le eccezioni `AuthContext` di `ensure_root`/`ensure_child`/`ensure_path` sono nello spec §4, non nel vincolo globale del piano (che dice «solo le tre della Fase 0»). Vince lo spec. Ogni metodo lo dichiara nel doc comment, come `mark_scanned`. — *Costo se sbagliato:* lo scanner della 1b dovrebbe inventarsi un utente.

Ruling: checkpoint prestazioni Task 5 — **rimando al Task 8**. `cargo test -p keeppix-db -- --test-threads=1` → 79 esecuzioni, real ~292s (~4m53s), ~5s per boot di container. Rapporto tempo/test ancora lineare. Fase 0 CI `backend` caldo era 5m23s per *tutto* il backend; keeppix-db da solo è già lì, e i Task 6-8 aggiungeranno altri test. Non cambio l'harness ora. — *Costo se sbagliato:* si scrivono ~20-30 test in più sullo schema lento prima del Task 8.

Task 5: complete (commit aca5b5e, test verdi: 17 in `--test folders` inclusi 3 extra oltre i 11 del piano + 3 harness; workspace verde dopo retry di un flake `PortNotExposed` su `--test libraries`)

Minor differiti (Task 5):
- `move_subtree` cross-library → `Conflict` non ha un test dedicato.
- `find_by_id` admin+id cartella inesistente → `NotFound` non ha un test dedicato.
- il criterio «lo spostamento non tocca `assets`» non è verificabile prima del Task 6 (la tabella non esiste).
- `ensure_*` incrementa `next_folder_seq` anche su `ON CONFLICT DO NOTHING` (buchi nella sequenza, non duplicati).

Task 6: complete (commit 141c01a, 6 test di schema verdi)

Ruling: il test `an_unknown_kind_is_rejected` deve inserire prima un asset valido e poi asserire il codice `23514`. Un `is_err()` nudo passava a tabella assente (42P01) — stesso genere di falso verde della Fase 0. — *Costo se sbagliato:* il CHECK su `kind` potrebbe sparire e la suite resterebbe verde.

Minor differiti (Task 6):
- CHECK su `status` e `location_source` non hanno un test dedicato.
- `assets`/`asset_exif` non sono in `expected_tables_exist` (quel test elenca solo le tabelle della Fase 0).
- `LibraryRepo::mark_scanned` dice ancora «quarta e ultima eccezione»: è stale dopo `ensure_*`.

## Avanzamento

| # | Task | Stato | Commit |
|---|---|---|---|
| 1 | Mapping delle righe uniforme | ✅ review pulita | `63dd6a6` |
| 2 | Migrazione librerie e cartelle | ✅ review pulita | `971e5f4` |
| 3 | Tipi di dominio | ✅ review pulita | `0564f66` |
| 4 | `LibraryRepo` | ✅ review pulita | `3506b5c` |
| 5 | `FolderRepo` + checkpoint CI | ✅ | `aca5b5e` |
| 6 | Migrazione asset ed EXIF | ✅ | `141c01a` |
| 7 | `AssetRepo` e visibilità | — | |
| 8 | Registro delle modifiche | — | |
