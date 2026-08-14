# Task 15 — note di pre-volo del controller

Da leggere insieme a `task-15-brief.md`. **Dove queste note contraddicono il
brief, vincono le note.**

## P1 — Due ruling già presi correggono il workflow

**Ruling R4 (sqlx).** Il blocco `env:` del brief contiene `SQLX_OFFLINE: "true"`.
Non esiste alcuna cache `.sqlx/` — il codice usa le forme funzione di sqlx,
verificate a runtime, non le macro. La variabile non serve a nulla e mente su
come funziona la build. **Rimuovila.**

**Ruling R2 (toolchain).** Il brief usa `dtolnay/rust-toolchain@1.85.0`. Il
workspace richiede **1.88** (let-chain). Usa `@1.88.0`, coerente con
`rust-toolchain.toml` e con il `rust-version` del workspace.

## P2 — Lo step 5 vuole pushare su `main`. Non farlo

Il brief chiude con `git push -u origin main`. Il lavoro di questa fase vive sul
branch **`fase-0`**, e il merge su `main` è una decisione dell'utente, non tua.
Committa e basta: al push penso io. Non eseguire alcun `git push`.

## P3 — `deny.toml`: la lista delle licenze quasi certamente non basta, e va provata

Il brief propone `allow = ["MIT", "Apache-2.0", …, "AGPL-3.0"]`. Due problemi:

1. I crate di questo workspace dichiarano `license = "AGPL-3.0-or-later"`, che
   è un identificatore SPDX **diverso** da `AGPL-3.0`. Con la lista così com'è,
   `cargo deny` rifiuta i crate del progetto stesso.
2. L'albero delle dipendenze contiene quasi certamente licenze non elencate
   (`Unicode-DFS-2016`, `BSL-1.0`, `CDLA-Permissive-2.0`, `OpenSSL`, …), e non
   si può indovinare quali: vanno viste.

**Esegui davvero `cargo deny check advisories bans licenses` in locale**
(`cargo install cargo-deny` funziona: crates.io è raggiungibile) e itera finché
non passa. Per ogni licenza che aggiungi alla lista, **scrivi nel report perché
è accettabile** in un progetto AGPL — non allargare la lista finché il comando
tace. Se `advisories` segnala una vulnerabilità reale, riportala: è un finding,
non un ostacolo da aggirare con un'eccezione.

## P4 — La CI eseguirà i test in un ambiente diverso dal nostro

Qui testcontainers non funziona (policy di egress) e la suite gira contro un
Postgres locale via `KEEPPIX_TEST_DATABASE_URL`. **Sui runner GitHub Docker c'è**,
quindi il percorso predefinito degli harness — container usa-e-getta
`postgis/postgis:17-3.5` — funziona e va lasciato come sta: non impostare
`KEEPPIX_TEST_DATABASE_URL` nel workflow.

Vale però la pena che il workflow lo **menzioni in un commento**, perché chi
guarderà la CI dopo di noi deve sapere che quella variabile esiste e a cosa
serve.

## P5 — Numeri stantii nei criteri di completamento

Il brief chiude con i criteri di completamento della Fase 0, che dicono
«≈40 test». Dopo i fix round dei Task 10 e 11 la suite ne conta molti di più.
Non è compito tuo aggiornare il piano, ma **non usare quel numero come
riferimento**: il criterio è che la suite sia verde, non che abbia una certa
cardinalità.

## P6 — Verifica quello che puoi, e dichiara quello che non puoi

Lo step 4 chiede di eseguire in locale gli stessi comandi della CI: fallo, per
il job `backend` e per il job `frontend`, e riporta l'output reale.

Restano **non verificabili in questo ambiente**:

- il job `image` (`docker/build-push-action`), perché il pull delle immagini di
  base è bloccato dalla policy di egress;
- lo step 6 (guardare la pagina Actions su GitHub), perché la CI gira solo dopo
  il push, che farò io.

Dichiarali esplicitamente nel report come non eseguiti, con il motivo. Non
scrivere che la CI è verde: non lo sai ancora, e quel numero finisce nei criteri
di chiusura della fase.

## P7 — Il job `backend` e l'ordine dei comandi

`git diff --exit-code docs/api/openapi.json` viene dopo `cargo test`. È giusto:
lo snapshot del Task 11 si rigenera solo se il file manca, e il test fallisce se
diverge — il `git diff` è la seconda rete, quella che intercetta il caso in cui
qualcuno rigenera il file senza committarlo. Verifica in locale che questa
sequenza lasci davvero l'albero pulito.

## P8 — Confini

Non toccare `crates/`, `frontend/`, `docs/`. Se scopri che un comando della CI
fallisce per un difetto del codice, **segnalalo nel report** invece di
correggerlo qui.
