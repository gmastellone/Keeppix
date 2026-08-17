# SDD ledger — plan: docs/superpowers/plans/2026-08-17-keeppix-fase-3.md

Spec: docs/superpowers/specs/fase-3-multiutente.md
Branch: `fase-3` da `main` @ `f29f1ca`
Workspace: in-place (checkout cloud già isolato; l'utente ha chiesto il
branch `fase-3`, non un worktree).

Ruling: lavoro in-place sul branch `fase-3`. Costo se sbagliato: spostare
il branch, un comando.

Nessun push/PR/merge. Field test e prova a mano dal browser: li esegue
l'operatore.

Ruling: la selezione collaborativa sugli album condivisi (spec Fase 2 §4.1
e spec Fase 3 §5) **non è in questa fase**, come dichiarato dal piano e
dall'utente. Costo se sbagliato: va anticipata con una decisione scritta.

Ruling: spec §5 menziona i pick collaborativi; il piano li esclude. Vince
il piano su questo punto (e l'istruzione dell'utente). La query di
visibilità della spec §3.2 non tratta `inherit = false`; vince la spec
sulla semantica solo-allow e il piano sul come misurare CTE vs
`NOT EXISTS`.

---

## Task 1 — visibilità ereditata

Ruling: i chiamanti di `VisibilityScope::filter` **sono cambiati**. Il piano
diceva che la firma congelata (una clausola + un bind `uuid[]` di librerie)
restava. Con la condivisione per cartella un `path <@ ANY(ltree[])` senza
`library_id` è un buco: due librerie hanno entrambe la radice `"1"`. Il
filtro ora occupa **due** parametri consecutivi (id cartelle concesse,
id buchi) e le espressioni `path`/`library_id` devono essere **qualificate**.
Costo se sbagliato: ogni query nuova che dimentica il secondo bind o usa
`path` nudo riapre il perimetro. Test che lo tengono: `a_share_does_not_open_another_library_with_the_same_ltree_label`, `inherit_false_stops_the_subtree_at_that_node`, `sharing_a_folder_grants_its_whole_subtree`.

Ruling: `inherit = false` va nei **buchi**, non fra i concessi. `path <@ hole`
include uguaglianza, quindi il nodo e i discendenti spariscono. È
un'interruzione di ereditarietà, non un deny ACE. Costo se sbagliato:
un permesso solo sul nodo con `inherit=false` lo nasconde invece di
mostrarlo senza figli — il piano testa il caso padre+figlio, non il
nodo isolato.

Ruling: scelta della query = **EXISTS grant + NOT EXISTS hole**, non CTE
ricorsiva. Misura su 200.000 asset, 50 permessi (impalcatura 2R3), runner
locale con Postgres 17 condiviso:

| | |
|---|---|
| seed | 5.94 s |
| `buckets` | 4.29 ms |
| `page` (200) | 3.96 ms (budget 300 ms) |
| EXPLAIN EXISTS/NOT EXISTS | exec **0.509 ms** (planning 1.176 ms) |
| EXPLAIN CTE ricorsiva | exec **0.334 ms** (planning 0.205 ms) |

La CTE è un filo più veloce su questo campione (i 50 grant sono foglie:
la ricorsione non scende, e **non applica i buchi**). Entrambe ≪ 300 ms.
Si tiene EXISTS/NOT EXISTS perché è l'unica delle due che implementa
`inherit=false` senza una seconda struttura. Il budget **non** si alza.

EXPLAIN: seq scan su ~51 cartelle nei SubPlan. A migliaia di cartelle
può servire la cache dei prefissi della spec §3. Differito, non silenziato.

Ruling: `NewGrant` invece di otto argomenti su `PermissionRepo::grant` —
clippy `-D too_many_arguments`. Costo: un struct in più, niente semantica.

Ruling: `FolderRepo::visible` usa `LibraryRepo::load_for_scan` dopo
`scope.allows`, non `find_by_id`. Quest'ultimo è ownership-only e
restituirebbe 403 al destinatario di una condivisione. Costo se sbagliato:
path su disco esposto a chi vede la cartella — `absolute_path` resta
dietro `visible`, e il test `a_shared_folder_never_exposes_the_real_disk_path`
copre la risposta HTTP/API, non questo helper.

Ruling: `filter_library` (change_log, librerie offline) è **grosso**:
id delle librerie che contengono almeno una cartella concessa; i buchi
non nascondono una libreria intera. Il secondo bind è una tautologia
per tenere il contratto a due parametri. Differito: il change_log non è
il percorso caldo e non elenca asset. Costo: un utente con un buco in
una libreria propria (impossibile oggi: i buchi arrivano solo da grant
`inherit=false`) vedrebbe eventi di cartelle nascoste.

Ruling: `SessionRepo::rotate` e `authenticate` fanno join su `users` e
pretendono `disabled_at IS NULL`. Il disable HTTP revoca già le sessioni;
questo è la rete sotto, se una riga resta. `FOR UPDATE OF s` perché il
join su `users` non è nella riga bloccata.

Task 1: complete (commit f2d8e83, test `permissions` 14, `visibility` 7,
`sessions` 16, `scale_200k::timeline_with_fifty_permissions` verdi)

Ruling (post-commit): `grant_ids()` era pubblico solo per i test e la
guardia `check-wired` lo segnalava. I test usano `filter().bind()`.
`PermissionRepo::effective_role` resta in Rinvii `fase-3` fino al Task 4
(explain). Costo: dimenticarsene a fine fase lascia un rinvio della
fase corrente. Task 4 deve togliere quella riga.

Differito: `check-wired` non vede `PermissionRepo::grant` come inutilizzato
perché `\bgrant\b` matcha il nome del parametro nello stesso file. La
guardia ha lo stesso buco per ogni fn il cui nome è anche un ident locale.

---

## Task 12b — rinnovo sessione

Ruling: niente `expires_at` su `/auth/me`. Il cookie è HttpOnly, il TTL
di produzione è 30 giorni. Watchdog: `setInterval` 12 ore **solo** con
`document.visibilityState === 'visible'`; al ritorno in primo piano un
refresh immediato. Costo se troppo raro: si cade dopo 30 giorni di
scheda sempre aperta senza un giro. Costo se troppo frequente: rotate
inutili sul Pi. 12 ore è due ordini sotto il TTL e non gira di notte.

Task 12b: complete (commit 29733c3, vitest session 8, `refresh_rejects_a_disabled_user`
e `refresh_slides_expiry_so_an_active_session_survives` verdi; `check-wired`
verde per `/auth/refresh`)

---

## Task 12c — navigazione cartelle

Ruling: `GET /folders/tree?roots=true` è un'aggiunta (query opzionale).
Senza, l'endpoint 1c restituisce l'albero intero — vietato su migliaia
di cartelle. Per un utente con uno share la «radice» è la cartella
concessa, non `parent_id IS NULL` (quella è la radice della libreria
altrui). Costo se sbagliato: il destinatario vede un albero vuoto.

Ruling: `PATCH /api/v1/folders/{id}` è un'aggiunta a `/api/v1`. Spostare
richiede owner della libreria, admin, o `editor` **diretto** sulla
cartella. Un viewer condiviso prende `Forbidden`. L'`editor` ereditato
da un antenato non basta — `effective_role` non cammina gli antenati
(Task 4 / explain). Costo: un editor sul padre non sposta i figli
finché Task 4 non allarga il ruolo effettivo.

Ruling: dopo lo spostamento si chiama `regroup_folder` sulla cartella
mossa, sul vecchio padre e sul nuovo. Gli asset non cambiano
`folder_id`, quindi è lavoro a vuoto, ma è il chiamante di produzione
che paga il debito della guardia. Costo: tre query extra per move.

`./scripts/test.sh` dopo Task 1+12b: verde (~20 min, poi `cargo clean`).

Task 12c: complete (commit 77bf440, vitest FoldersView 2, permissions 16,
timeline folder tests e openapi verdi; `check-wired` verde per tree,
children, move_subtree, regroup_folder)



