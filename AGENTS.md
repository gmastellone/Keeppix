# Istruzioni per agenti che lavorano su Keeppix

Questo file è letto automaticamente da Cursor, Codex e Claude Code a ogni
sessione. Vale per **tutto** il repository. Le regole qui dentro hanno
precedenza sul tuo giudizio: sono state pagate con difetti reali trovati in
review, e ognuna esiste perché qualcosa è andato storto senza di essa.

## Prima di scrivere qualsiasi riga di codice

Leggi in quest'ordine:

1. `docs/superpowers/plans/2026-08-13-keeppix-fase-0-STATO.md` — stato del
   progetto, decisioni prese (R1-R13), difetti noti e differiti.
2. `docs/superpowers/plans/2026-08-13-keeppix-roadmap.md` — le 7 fasi e i
   contratti congelati.
3. `docs/superpowers/specs/2026-08-13-keeppix-design.md` — architettura
   generale.
4. La **spec della fase** su cui stai lavorando, in
   `docs/superpowers/specs/fase-<N>-*.md`. Contiene le decisioni di dettaglio:
   nomi di tabelle, indici, formati, protocolli, ottimizzazioni. **Non
   improvvisare ciò che è già deciso lì.**
5. Il **piano della fase**, in `docs/superpowers/plans/`, se esiste. Contiene
   i task passo per passo con il codice esatto.

Se la spec e il piano divergono, **vince la spec** e lo segnali nel ledger.

## Il vincolo che governa tutto

Keeppix deve essere **estremamente usabile** e insieme **estremamente leggero**.
Non sono in tensione: sono lo stesso requisito visto da due lati.

Il bersaglio dichiarato è un **Raspberry Pi 5, 8 GB di RAM, disco NVMe**, che
serve **200.000 foto**. Ogni scelta va pesata contro quella macchina, non contro
quella su cui stai sviluppando.

- **Nessuna dipendenza nuova** senza una ragione scritta nel ledger: ogni crate
  è tempo di build, superficie di CVE, e RAM a runtime.
- **Niente carica tutto in memoria.** Elenchi paginati con keyset, mai un
  `SELECT` senza `LIMIT` su una tabella che cresce.
- **Il frontend cresce solo in chunk lazy.** Il bundle d'ingresso resta sotto
  **150 KB gzip**: chi guarda le foto non paga per le pagine di amministrazione.
- **Mai `thread::sleep` in contesto async.** Blocca un thread del worker pool,
  non solo quel job: su 4 core è un quarto della capacità.
- **Ogni operazione su percorso caldo ha un budget** verificato da un test.

E, altrettanto importante: **una funzione che l'utente non può raggiungere non
esiste.** Un repository senza rotta, o una rotta senza interfaccia, è lavoro
incompleto — non lavoro fatto in attesa del resto.

## Gli invarianti — violarli è un difetto grave, non una scelta di stile

Sono verificati in review a ogni task. Nessuno di questi è negoziabile senza
una decisione scritta.

### Architettura

- **Nessun SQL fuori da `crates/keeppix-db`.** Gli handler HTTP non scrivono
  query. È imposto anche meccanicamente: `sqlx` è fra le `[dependencies]` del
  solo `keeppix-db`, quindi una query in un handler **non compila**.
- **`keeppix-media` non conosce il database; `keeppix-db` non conosce le
  immagini.** Imposto da una regola `[[bans.deny]]` in `deny.toml`: aggiungere
  quell'arco fa fallire `cargo deny check bans`.
- **Ogni metodo di repository che legge dati di un utente prende un
  `AuthContext` come primo parametro.** Le uniche eccezioni sono quelle già
  documentate nel codice, ognuna con il motivo scritto nel doc comment
  (`count`, `create_bootstrap_admin`, `find_by_username`, `mark_scanned`).
  Non aggiungerne di nuove senza la stessa giustificazione esplicita.
- **`Auth` è l'unico modo in cui un `AuthContext` entra nel livello HTTP.**
  Non creare helper che ne fabbrichino uno.

### Sicurezza

- **Un utente che sonda un id che non gli appartiene riceve `Forbidden`, mai
  `NotFound`.** Altrimenti l'endpoint diventa un oracolo di esistenza: si
  scopre quali id esistono sondandoli. Vale per utenti, librerie, cartelle,
  asset, album, tutto.
- **Query sempre parametrizzate.** Mai concatenazione di stringhe in SQL.
  L'unica interpolazione ammessa è di costanti del codice (elenchi di colonne),
  mai di valori che arrivino dall'esterno.
- **sqlx solo nelle forme funzione** (`sqlx::query`, `sqlx::query_as`) e
  `#[derive(sqlx::FromRow)]` per il mapping. **Mai** le macro `query!`, mai una
  directory `.sqlx/`, mai `SQLX_OFFLINE`. Vedi R4 in STATO.md.
- **Nessun `unwrap()` / `expect()` in codice di produzione.** Nei test sì, con
  `#[allow(clippy::unwrap_used)]` locale sulla funzione.
- **Nessun percorso filesystem arriva dal client.** Si accede ai media per `id`
  o `content_hash`; il percorso lo risolve il server dall'albero.
- **I decoder scritti in C** (ffmpeg, libraw) girano in un processo separato
  usa-e-getta con `rlimit` e seccomp. Non chiamarli in-process.
- **Il cookie `__Host-kpx_session` porta `Secure` incondizionatamente.** Non
  reintrodurre logica condizionale sull'host: vedi R7 in STATO.md, è già stato
  sbagliato una volta.

### HTTP

- **Ogni errore è RFC 9457** `application/problem+json` con un campo `type`
  stabile prefissato `keeppix/`. Il backend **non traduce**: `title` è in
  inglese e serve al debug, la traduzione avviene nel frontend a partire dal
  codice `type`.
- **`/api/v1` è congelato**: solo aggiunte, mai rimozioni o cambi di
  significato. Una rottura genera `/api/v2`.
- **`.fallback(...)` va registrato PRIMA di `with_common_layers(...)`.** In
  axum 0.8 `Router::fallback` sovrascrive il catch-all invece di fondersi con
  quello già avvolto: mettendolo dopo, ogni 404 esce senza header di sicurezza.
  Vale per tutti i punti di montaggio, `embed::mount()` compreso. Vedi R5.
- **Le rejection di axum passano da `keeppix_api::Json<T>`**, non da
  `axum::Json`, così 415/400/422 restano in `problem+json`.

### Dati

- **Identità dell'asset = `(folder_id, filename)`.** `content_hash` è
  indicizzato ma **non** unico: la stessa foto in due cartelle sono due asset,
  con cancellazioni indipendenti. La deduplica è una scelta di presentazione.
- **Nessun percorso assoluto denormalizzato sugli asset.** Si ricostruisce
  dall'albero `ltree`, altrimenti spostare una cartella con 40.000 foto diventa
  un UPDATE di 40.000 righe.
- **I metadati originali sono immutabili.** `asset_exif` non si riscrive mai;
  le modifiche dell'utente vivono in `asset_overrides`, e il valore mostrato è
  `COALESCE(override, exif)`.
- **Nessuna tabella di visibilità materializzata per utente.** Cambiare un
  permesso deve avere effetto immediato.
- **Un file RAW non si riscrive mai.** I metadati vanno in un sidecar `.xmp`.

## Metodo di lavoro

### TDD, davvero

1. Scrivi il test che fallisce.
2. **Eseguilo e osserva il fallimento.** Non saltare questo passo: un test che
   non hai visto fallire non sai se prova qualcosa.
3. Implementa il minimo che lo fa passare.
4. Riesegui.

Un test deve fallire se il comportamento che il suo nome dichiara regredisce.
Nella Fase 0 tre test scritti seguendo il piano passavano **senza provare ciò
che il loro nome affermava** — per esempio un test di logout che passava anche
cancellando la revoca lato server, perché il client non mandava più il cookie.
Chiediti sempre: *se rompo di proposito la cosa che questo test protegge, fallisce?*

### Verifica prima di dichiarare fatto

Prima di considerare chiuso un task, esegui **tutto** questo e guarda l'output:

```bash
cd frontend && npm ci && npm run build   # obbligatorio: senza dist/ il backend non compila
cd .. && cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
./scripts/test.sh
```

`frontend/dist` non è un prerequisito dei test ma **della compilazione**:
`rust-embed` incorpora quella cartella a compile time.

`./scripts/test.sh` è `cargo test --workspace --jobs 1 -- --test-threads=1`
e, anche se i test falliscono, rimuove i container testcontainers e fa
`cargo clean`. Non usare `cargo test --workspace` a mano: senza `--jobs 1`
parte un PostGIS per crate in parallelo e `target/` resta a ~9 GB.
`--test-threads=1` serve ai test di `keeppix-server/tests/config.rs`, che
manipolano l'ambiente di processo. Clippy **prima** dei test: lo script
cancella `target/` dopo.

Non dire "fatto" senza aver visto l'output verde. Se qualcosa è rosso, è rosso.

### Il ledger delle decisioni

Ogni volta che decidi qualcosa che il piano non specifica del tutto, risolvi
un'ambiguità, o ti scosti dal piano perché il codice reale è diverso da come il
piano lo immaginava: **scrivilo**, non deciderlo in silenzio.

Appendi a `.superpowers/sdd/<piano-corrente>/progress.md`:

```
Ruling: <cosa hai deciso> — <perché> — <costo se è la scelta sbagliata>
Task <N>: complete (commit <sha>, test verdi)
```

Guarda `.superpowers/sdd/2026-08-13-keeppix-fase-0/progress.md` per lo stile.
Questo file è ciò che permette a chiunque di riprendere il lavoro senza
rileggere la cronologia git. È parte della consegna.

### Commit

Commit convenzionali, **in inglese**, uno per unità logica di lavoro — non un
commit gigante a fine task.

```
feat(db): add library repository
fix(api): a database outage is a 503, not a session expiry
test(domain): add coverage for User::is_active()
docs: explain the migration checksum error
```

Il corpo del messaggio spiega **perché**, non cosa: il diff dice già cosa.

## Integrazione

- Si lavora su un branch di fase (`fase-1a`, `fase-1b`, …), **mai su `main`**.
- **Non fare push, non aprire PR, non fare merge** senza che l'utente lo chieda.
- Il flusso è: si lavora sul branch → si consolida con una review → si allinea
  `main` con una PR (che è anche l'unico modo di far girare la CI prima del
  merge, perché i workflow si attivano su `pull_request`).

## Quando fermarti a chiedere

Non fermarti per ogni dettaglio: se qualcosa è ambiguo, prendi la decisione più
piccola e ragionevole, implementala, scrivila nel ledger come `Ruling`.

Fermati e chiedi solo per:

- azioni distruttive o irreversibili (cancellazione di dati, `git push --force`,
  reset);
- push, merge, apertura di PR;
- quando ogni strada plausibile richiede un'informazione che solo l'utente ha.

## Stack e vincoli tecnici

- **Rust 1.88.0**, edition 2024. Let-chains disponibili. Vedi R2.
- **Axum 0.8**, sqlx 0.8, PostgreSQL 17 + PostGIS 3.5.
- **Vue 3 + TypeScript + Vite + Tailwind v4 + Reka UI**. Niente Vuetify.
- **Budget bundle iniziale del frontend: 150 KB gzip**, verificato in CI. I
  chunk lazy per rotta sono fuori dal budget.
- Immagine Docker **distroless**, senza shell, non-root.
- Traduzioni in `frontend/src/i18n/{it,en}.json`. Nessuna stringa utente
  hard-coded nei componenti. Le due lingue devono avere le stesse chiavi (c'è
  un test in CI).

## Cosa NON fare

- Non aggiungere dipendenze senza necessità reale: ogni dipendenza è tempo di
  build in CI e superficie di CVE. `cargo deny` controlla licenze e advisory.
- Non implementare cose di fasi successive perché "tanto ci vuole poco". Ogni
  fase ha i suoi confini scritti, e superarli significa saltare la review.
- Non "sistemare" codice fuori dal task corrente. Se noti un difetto, scrivilo
  nel ledger come voce differita.
- Non modificare le migrazioni già applicate. Dal primo rilascio in poi si
  aggiungono solo file nuovi: sqlx verifica il checksum e rifiuta di partire.
