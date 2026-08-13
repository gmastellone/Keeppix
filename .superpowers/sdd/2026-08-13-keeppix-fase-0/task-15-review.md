# Task 15 — Integrazione continua — Review

## Spec Compliance

❌ **Issues found.** I tre file richiesti dal brief (`ci.yml`, `release.yml`,
`deny.toml`) sono stati creati, con le quattro correzioni imposte dal
preflight (R4, R2, P2, P4) applicate correttamente e verificate — vedi
Strengths. Ma il brief promette, in "Interfaces → Produces": *"CI che
blocca il merge su fmt, clippy, test... e una pipeline di release che
pubblica l'immagine multi-arch firmata"*. Ho verificato che entrambe le
metà di questa promessa sono rotte:

- il job `backend` **non compila** in CI (vedi Critical #1) — non "blocca
  il merge sui test", semplicemente non arriva mai a eseguirli;
- il passaggio `cosign sign` di `release.yml` **fallisce sempre** su una
  release reale (vedi Critical #2) — l'immagine viene pubblicata ma **non
  firmata**, il contrario esatto di quanto promesso.

Non sono scostamenti di perimetro (nessun file mancante, nessuna feature
extra), sono difetti funzionali nei file consegnati. Per questo la
compliance è ❌ e non ⚠️: sono verificabili dal diff stesso, non richiedono
di allargare la ricerca fuori da esso.

- ⚠️ Cannot verify from diff: il job `image` di `ci.yml` (build Docker senza
  push) — bloccato dalla policy di egress anche per me, come per
  l'implementer. Ho letto il `Dockerfile` (fuori diff, per un rischio
  nominato: "il job `backend` ha lo stesso problema di ordine frontend/
  backend del job `image`?") e la risposta è no: il `Dockerfile` builda il
  frontend in un proprio stage multi-fase indipendente da questo workflow,
  quindi quel job non eredita il difetto Critical #1. Non ho potuto
  eseguirlo per confermare che builda con successo.

## Strengths

- **`deny.toml` — verificato riga per riga, non solo letto.** Ho eseguito
  io stesso `cargo deny check advisories bans licenses` con la lista del
  diff: `advisories ok, bans ok, licenses ok`. Ho anche testato in
  isolamento le due modifiche dichiarate nel report, rimuovendole una alla
  volta da una copia locale del file:
  - senza `AGPL-3.0-or-later` (lasciando `AGPL-3.0` come da brief),
    `cargo deny` rigetta davvero tutti i crate `keeppix-*` del workspace
    (`license = "AGPL-3.0-or-later"` in `Cargo.toml:9`, radice del
    workspace);
  - senza `CDLA-Permissive-2.0`, `cargo deny` rigetta davvero
    `webpki-roots` 0.26.11 e 1.0.9 (`license = "CDLA-Permissive-2.0"` nel
    loro `Cargo.toml`).
  Entrambe le motivazioni nel report (`task-15-report.md:65-76`) sono
  esatte e la lista non è più ampia del necessario — nessuna licenza
  aggiunta "per sicurezza".
- **Ruling del preflight tutti applicati e verificabili nel diff**:
  `SQLX_OFFLINE` rimosso (R4, `ci.yml` non contiene più la chiave),
  toolchain `@1.88.0` (R2, coerente con `rust-toolchain.toml:2` e
  `Cargo.toml:8` `rust-version = "1.88"`), nessun `git push` (P2, branch
  `fase-0` ancora 1 commit avanti a `origin/fase-0`), commento su
  `KEEPPIX_TEST_DATABASE_URL` presente nello step "Test" (P4).
- **`release.yml`: permessi minimi e coerenti con quanto fa** — `contents:
  read` (checkout), `packages: write` (push su ghcr.io), `id-token: write`
  (OIDC per `cosign sign --yes` keyless). Nessun permesso superfluo.
- **`sbom: true` / `provenance: mode=max`** in `release.yml` implementano
  correttamente il requisito di spec "SBOM a ogni release" (design
  §9, riga 639).
- Ho verificato (fuori diff, rischio nominato: *"`ghcr.io/${{
  github.repository }}` con un repo che si chiama `Keeppix` — maiuscola —
  produce un riferimento immagine non valido?"*) che `docker/
  metadata-action@v5` normalizza automaticamente in minuscolo il valore di
  `images:` (confermato dal README ufficiale dell'azione). Quindi
  `steps.meta.outputs.tags`, usato per il push, è corretto. Il problema è
  altrove — vedi Critical #2.

## Issues

### Critical (Must Fix)

#### 1. Il job `backend` di `ci.yml` non compila in CI: nessuno step builda il frontend prima di `cargo clippy`/`cargo test`

`.github/workflows/ci.yml`, job `backend` (righe ~40-61 del diff): gli step
sono `checkout` → toolchain → `rust-cache` → `fmt` → `clippy` → `test` →
`git diff openapi.json`. Non c'è nessun `npm ci && npm run build`, nessun
`needs: [frontend]`, nessun `actions/upload-artifact`/`download-artifact`
che porti `frontend/dist` da un job all'altro. Il job `frontend` è
completamente separato e gira su un runner diverso: il suo `dist/` non è
visibile al job `backend`.

`crates/keeppix-server/src/embed.rs:13` usa `#[derive(Embed)]` con
`#[folder = "$CARGO_MANIFEST_DIR/../../frontend/dist"]` (rust-embed 8.12,
non in modalità debug). **Ho riprodotto in locale** cosa succede quando
quella cartella non esiste (come accadrà su un checkout pulito in CI, dato
che `frontend/dist` è in `.gitignore:10` e non è mai tracciato):

```
$ mv frontend/dist /tmp/dist-backup && cargo check -p keeppix-server
error[E0599]: no function or associated item named `get` found for struct `Assets`
  --> crates/keeppix-server/src/embed.rs:33:19
   |
14 | struct Assets;
   | ------------- function or associated item `get` not found for this struct
...
error: could not compile `keeppix-server` (lib) due to 3 previous errors
```

Non è un fallback silenzioso: `#[derive(Embed)]` su una cartella assente
**non implementa il trait**, e ogni chiamata a `Assets::get` diventa un
errore di compilazione. Poiché `cargo clippy --workspace --all-targets`
compila l'intero workspace incluso `keeppix-server`, questo è lo step
"Lint" che fallirà, subito dopo "Formattazione" — non un test rosso isolato
ma un **hard failure del job**, prima ancora di arrivare a `cargo test`.

**Quando si manifesta:** alla primissima esecuzione della CI su GitHub,
sul primo push. Il job `backend` non passerà mai finché non si aggiunge un
passo di build frontend (o un artifact condiviso) prima di "Lint".

Nota collaterale: anche se si "risolvesse" solo evitando l'errore di
compilazione (es. una cartella `frontend/dist/` vuota placeholder), il
problema non sparirebbe del tutto — diventerebbe il secondo scenario
descritto dal commento in `crates/keeppix-server/tests/embed.rs:5-6`
("Il test gira solo quando il frontend è stato compilato: in CI la build
del frontend precede quella del backend" — affermazione che, con questo
`ci.yml`, è falsa): i 4 test di `embed.rs` si auto-salterebbero
silenziosamente, ed è esattamente il falso verde permanente descritto nel
task. Il fix corretto è far compilare davvero il frontend nel job
`backend` prima di `cargo clippy`/`cargo test` (o unificare i due job, o
condividere `dist/` come artifact), non solo evitare l'errore hard.

Il report dell'implementer (`task-15-report.md:33-39`) dichiara "non
c'erano altri problemi individuati in fase di lettura o di verifica
locale": questo è impreciso — la verifica locale è stata eseguita
costruendo sempre il frontend *prima* a mano (`task-15-report.md:114-115`),
riproducendo il flusso corretto ma non l'effettivo grafo dei job GitHub
Actions, dove `backend` e `frontend` sono job indipendenti e paralleli.

#### 2. `release.yml`: il passaggio `cosign sign` firma un riferimento immagine sbagliato — fallisce sempre su una release reale

`.github/workflows/release.yml`, step "Firma l'immagine" (ultime righe del
diff):

```yaml
- name: Firma l'immagine
  run: |
    cosign sign --yes \
      ghcr.io/${{ github.repository }}@${{ steps.build.outputs.digest }}
```

Il repository si chiama `gmastellone/Keeppix` (K maiuscola — verificato
con `git remote -v`). `github.repository` **preserva il case esatto** del
nome del repo (documentazione GitHub Actions ufficiale: l'esempio dato è
letteralmente `octocat/Hello-World`). Il passaggio "meta" (`docker/
metadata-action@v5`, righe ~139-146) **normalizza in minuscolo** il campo
`images:` prima di generare `steps.meta.outputs.tags` — confermato dal
README ufficiale dell'azione ("this action will automatically: Lowercase
the image name"). Quindi l'immagine viene **davvero pubblicata** su
`ghcr.io/gmastellone/keeppix:...` (minuscolo), come dovrebbe.

Ma lo step di firma **non usa `steps.meta.outputs.tags`**: ricostruisce il
riferimento a mano interpolando di nuovo `github.repository`, che non passa
mai dalla normalizzazione di `metadata-action`. Il comando eseguito è quindi:

```
cosign sign --yes ghcr.io/gmastellone/Keeppix@sha256:...
```

— un riferimento con la maiuscola, che punta a un repository path diverso
da quello realmente pubblicato (le specifiche OCI/Docker non ammettono
maiuscole nei nomi immagine: quel path non esiste su ghcr.io). `cosign
sign` fallirà con un errore di manifest/repository non trovato, **ad ogni
release e ad ogni rebuild settimanale**, senza eccezioni: non dipende dal
contenuto del codice, dipende solo dal nome del repository.

**Effetto pratico**: l'immagine multi-arch viene pubblicata (lo step
`build-push-action` con `push: true` va a buon fine, perché usa i tag già
normalizzati), ma **non viene mai firmata**, e il job `publish` termina in
rosso all'ultimo step — l'esatto contrario di "pipeline di release che
pubblica l'immagine multi-arch firmata" promesso dal brief.

**Fix suggerito** (da applicare nel branch, non qui): usare il digest
qualificato che `docker/metadata-action`/`build-push-action` già
espongono in forma corretta, ad es. costruendo il riferimento dal primo
tag generato (`(steps.meta.outputs.tags | fromJSON... )` in bash, o più
semplicemente derivando l'immagine base da una variabile calcolata in
minuscolo con `${GITHUB_REPOSITORY,,}` in uno step precedente) invece di
reinterpolare `github.repository` grezzo.

**Quando si manifesta:** al primo tag `v*` pushato o al primo cron
settimanale — cioè esattamente il momento descritto nel prompt come "nessuno
lo scoprirà prima di un tag di release". Non è verificabile eseguendo il
job in locale (richiede push reale su ghcr.io), ma è un difetto di lettura
puro: la stringa è deterministicamente sbagliata indipendentemente
dall'ambiente.

### Important (Should Fix)

#### 3. Il budget bundle misura "tutto il JS in `dist/assets`", non "il bundle iniziale" — corretto oggi, strutturalmente sbagliato appena arrivano chunk lazy grandi (plan-mandated)

`.github/workflows/ci.yml`, step "Budget del bundle iniziale" nel job
`frontend`:

```bash
SIZE=$(find dist/assets -name '*.js' -exec gzip -c {} \; | wc -c)
```

Ho eseguito lo stesso identico comando sulla build reale prodotta
dall'implementer: `dist/assets/` contiene oggi **6 file JS**, non uno solo
come lascia intendere l'output parziale citato nel report
(`task-15-report.md:129-131`, che mostra solo `index-CumzRq_k.js`):

```
Button-BQTkaF0V.js     355 byte gzip
HomeView-icDcHuHT.js    443 byte gzip
LoginView-DyNHOm4z.js   843 byte gzip
SetupView-D5tbduPr.js   977 byte gzip
TextField-BxL5NRZb.js   682 byte gzip
index-CumzRq_k.js    73593 byte gzip
--------------------------------
totale                76893 byte  (= il numero riportato, confermato)
```

`HomeView`, `LoginView`, `SetupView` sono chunk **lazy per-rotta** creati
da `import()` dinamici in `frontend/src/router.ts:8-10` — cioè esattamente
il pattern di code-splitting che la spec prevede (design
riga 707: *"Budget bundle iniziale: 150 KB gzip; mappa, culling,
impostazioni e player video in chunk separati"*). Il numero del report è
matematicamente corretto (l'ho riprodotto byte per byte), ma la
**metrica** somma indiscriminatamente bundle iniziale + tutti i chunk
lazy esistenti. Oggi l'impatto è trascurabile (~2.6 KB su 76.9 KB), ma è
lo stesso meccanismo che, quando in una fase futura verrà aggiunto il
chunk MapLibre (~230 KB gzip dichiarati a riga 507 della spec, lazy per
design), farà fallire il budget anche se il bundle *realmente iniziale*
resta sotto i 150 KB — il contrario dell'intento dichiarato dal nome dello
step.

Questo script è identico, carattere per carattere, a quello del brief
(Step 2): l'implementer non lo ha modificato né discusso, e nel report non
compare alcuna nota su questo punto nonostante il preflight non lo
escluda. Per la regola "plan-mandated" del protocollo di review, lo segnalo
come Important e non come difetto introdotto dall'implementer.

**Quando si manifesta:** a un push futuro (una fase successiva che
aggiunge un chunk lazy sopra ~75 KB gzip, es. la mappa) — non blocca Task
15 né la Fase 0 attuale, ma romperà la CI per un motivo indipendente dalla
dimensione reale del bundle iniziale, ed è bene che il controller lo sappia
prima di scoprirlo da un fallimento CI apparentemente incomprensibile in
una fase futura.

### Minor (Nice to Have)

#### 4. Il budget bundle non verifica l'esistenza di `dist/assets`: un `find` senza corrispondenze vale 0, sotto qualsiasi soglia

Stesso step di cui sopra. Se in futuro un refactor di `vite.config.ts`
spostasse l'output (`build.outDir` o la struttura interna di
`assets/`), `find dist/assets -name '*.js'` non troverebbe nulla, `gzip`
non verrebbe mai invocato, `wc -c` restituirebbe `0`, e `0 -gt 153600` è
falso: lo step passerebbe silenziosamente anche se il bundle reale non è
mai stato misurato. Oggi non succede (config Vite di default, verificato
in `frontend/vite.config.ts:11-15` — nessun `outDir` personalizzato), quindi
non è un difetto attivo, ma non c'è alcuna riga che verifichi `[ -d
dist/assets ]` prima di procedere. Suggerimento: `test -d dist/assets ||
{ echo "::error::dist/assets non trovato"; exit 1; }` prima del `find`.

#### 5. Il budget conta solo `.js`, non `.css`

Lo stesso step ignora `dist/assets/index-*.css` (~9.5 KB non compresso
nella build reale). Se "bundle iniziale" nella spec (design riga 44, 707)
è inteso come intero payload critico servito al primo render, il numero
riportato dalla CI sottostima leggermente il costo reale. Impatto minimo
allo stato attuale (qualche KB gzip), non bloccante, ma vale una nota per
chi in futuro leggerà "150 KB" e assumerà che includa tutto.

## Assessment

**Task quality:** Needs fixes.

**Reasoning:** Il lavoro su `deny.toml` è solido, verificato in modo
indipendente e ben motivato — nessun problema lì. Ma i due file workflow
contengono ciascuno un difetto che rompe esattamente la garanzia che il
brief chiede loro di fornire: il job `backend` non compila mai in CI per
un problema di ordinamento fra i job `frontend`/`backend` (riproducibile
con un `cargo check` locale), e il passaggio `cosign sign` di
`release.yml` firma sistematicamente un riferimento immagine con il case
sbagliato, quindi fallisce ad ogni release reale. Entrambi sono bug di
lettura pura, verificabili senza eseguire i job stessi, e nessuno dei due
si sarebbe manifestato come "test rosso" nel senso comune — il primo
sarebbe apparso come un errore di compilazione alla prima Lint CI (non
catastrofico, ma bloccante), il secondo sarebbe passato inosservato fino
al primo tag di release, con un'immagine pubblicata ma silenziosamente
non firmata se qualcuno non avesse letto l'ultimo step del job fino in
fondo.
