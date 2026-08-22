# Prompt di continuazione — Keeppix

Incolla **tutto questo file** come primo messaggio di una sessione nuova
(Cursor, Codex, Claude Code, altro). Non riassumere: il modello deve avere
lo stesso contesto, non una versione diluita.

---

Sei un agente che riprende Keeppix, una galleria fotografica self-hosted
(Rust + Vue). Il documento che comanda il tuo comportamento è `AGENTS.md`
nella root: **invarianti prima del giudizio**. Se spec e piano divergono,
vince la spec, e annoti il ruling nel ledger.

Non fare push, PR o merge se l'utente non lo chiede.

## Snapshot (post merge Fase 7)

- **`main`** contiene le Fasi **0-6, 10 e 7**, tutte mergiate. Le PR verso
  questo repo sono rotte lato GitHub in questo momento (API e web mostrano 0
  PR totali nonostante le #10/#11 siano esistite) — merge diretti via git
  dopo CI reale verde su `fase-*` (workflow allargato al push su quei
  branch).
- **Fase 7** (CLIP/pgvector, tag + proposte, SearchNode Tag/Category/Semantic,
  IVFFlat, `AiAnalysis`, review queue API). Ledger in
  `.superpowers/sdd/2026-08-20-keeppix-fase-7/progress.md`. UI coda di
  revisione / pagina Tag → Fase 11.
- **Fase 10**: BulkOutcome, geometria, pile, SearchNode base, sessioni,
  bootstrap, WS, decode PNG/TIFF/WebP/HEIF, OpenAPI, …
- **Prossimo lavoro:** Fase **8** (volti), poi 9 → 11. Prima di partire, i
  piani di 8/9 vanno ripassati col codice vero di 10+7 davanti. Ordine e
  decisioni in [`../superpowers/PROSEGUI.md`](../superpowers/PROSEGUI.md) —
  **rileggerlo all'inizio della Fase 8** (e di nuovo all'inizio della 11).

**Ordine di esecuzione da qui:** `8 → 9 → 11 (in quattro tranche)`.

La **10 ha preceduto la 7, la 8 e la 9** di proposito: involucro di riuscita
parziale, tassonomia errori, `SearchNode`, eventi WebSocket. La **7** ha
aggiunto embedding, tag IA e i nodi Tag/Category/Semantic sullo stesso AST.

## Le decisioni del 20 agosto, che sovrascrivono i documenti più vecchi

Il documento funzionale e il prototipo sono **precedenti** a queste decisioni e
**non sono stati riscritti**. Seguirli alla lettera significherebbe ricostruire
ciò che è stato tolto.

- **Album dinamici: non esistono.** Un album ricorda il filtro con cui è nato e
  ha un pulsante «Aggiorna album».
- **Conteggi accanto alle righe: tolti**, tranne nel culling dove restano esatti.
- **Video: si tiene ma minimo** — una sola resa, solo in background o di notte,
  e non si tocca un video già riproducibile dal browser.
- **Audit: spento di default**, si accende col secondo utente.
- **L'IA non entra nel culling** e **legge la miniatura da 240 px**, mai
  l'originale.

**Precedenza fra le fonti:** decisioni (`docs/ui/costo-beneficio-funzioni.md`) →
prototipo (comportamento) → documento funzionale (cosa mostra, etichette) →
analisi gap (cosa il backend può dare). Con un'eccezione: sull'accessibilità da
tastiera il prototipo **non** è fonte di verità, perché è rotta e lo dice il
documento stesso.

## Cosa leggere, in quest'ordine

1. `AGENTS.md`
2. `docs/ui/costo-beneficio-funzioni.md`, sezione «Decisioni prese» — la fonte
   che vince su tutte le altre.
3. `docs/superpowers/plans/2026-08-20-keeppix-fase-7.md` — l'ultima fase
   chiusa sul branch (dopo la 10 su `main`). Utile soprattutto per «Cosa
   esiste già» e per i numeri nel ledger: è il modello di piano fondato sul
   codice reale.
4. `docs/superpowers/plans/2026-08-13-keeppix-roadmap.md` — le fasi e i
   contratti congelati.
5. `docs/superpowers/specs/2026-08-13-keeppix-design.md` — architettura.
6. La spec e il piano della fase su cui lavori.

L'indice completo è `docs/superpowers/README.md`.

## La lezione che questo progetto ha pagato cinque volte

Cinque difetti, in cinque fasi diverse, erano **lo stesso difetto**: una
funzione scritta, testata, e mai collegata al percorso reale.

| Cosa | Come è stato trovato |
|---|---|
| `restat_if_stable` dormiva 5 s per file | field test |
| La scansione richiedeva il riavvio del container | field test |
| `detect_kind` mai chiamata → pipeline RAW morta in produzione | field test |
| `TrashRepo::cleanup_expired` mai chiamata | `grep` |
| Il WebSocket montato e mai usato dal frontend | `grep` |

Nessuno è stato trovato dai test unitari, **per costruzione**: un test unitario
invoca la funzione direttamente, che è esattamente ciò che la produzione non
fa.

Ce ne sono stati altri due che nemmeno un `grep` poteva vedere:

- `dcraw_emu` non veniva **spedito** nell'immagine Docker: lo zoom sui RAW
  rispondeva 503 per sempre, e una fotocamera con anteprime incorporate piccole
  avrebbe prodotto un archivio senza miniature.
- `probe()` restituiva `"software"` come costante: il chiamante esisteva e
  girava, salvava il dato, e il dato era inventato.

**Conseguenze pratiche per te:**

- `scripts/check-wired.py` gira in CI e fallisce se una funzione pubblica non
  ha chiamanti, o se una rotta montata non ha consumatori nel frontend. Se
  diventa rossa, la correzione di norma è **rendere il consumo visibile**, non
  aggiungere un'eccezione. Le eccezioni vanno in `scripts/wired-exceptions.txt`,
  separate fra **rinvii** (fase futura) e **debiti** (fase già chiusa).
- Un test deve fallire se ciò che il suo nome dichiara regredisce. Chiediti
  sempre: *se rompo di proposito la cosa che questo test protegge, fallisce?*
- Una fase si chiude con un passaggio **end-to-end su dati reali**, non con la
  suite verde.

## Il vincolo che governa tutto

**Raspberry Pi 5, 8 GB, NVMe, 200.000 foto.** Ogni scelta va pesata contro
quella macchina, non contro quella su cui sviluppi.

Numeri misurati sull'archivio reale (779 ARW Sony, 36 GB), utili come
riferimento:

| | |
|---|---|
| Derivati | **178 KB per foto** (~36 GB su 200.000) |
| Rapporto derivati/originali | **0,4%** |
| Hash | ~85 MB/s |
| Demosaic di un RAW da 24 MP | **894 ms** a freddo, 29 ms dalla cache |
| Query timeline a 200.000 asset sintetici | 3-5 ms contro budget di 300 ms |

Attenzione: le misure di prestazione locali vengono da Docker Desktop su
macOS, dove il bind mount passa da virtiofs e uno `stat` costa ~190 ms invece
di microsecondi. **Da quelle non si estrapola il Pi.** Le misure sulle query,
sì.

## Verifica prima di dichiarare fatto

```bash
cd frontend && npm ci && npm run build   # senza dist/ il backend non compila
cd .. && cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
./scripts/test.sh
```

`npm run build` fa anche il **type-check**: eseguilo **dopo** l'ultima modifica
ai file `.ts`, non prima. `vitest` non fa type-check, quindi da solo non basta —
è già costato una CI rossa.

`./scripts/test.sh` forza `--jobs 1 --test-threads=1` e ci mette ~21 minuti:
avvia un PostGIS reale per modulo di test. È lento di proposito. In produzione
invece il runtime è multi-thread e i worker sono `(core - 1)` fino a 8.

## Il ledger

Ogni decisione che il piano non specifica del tutto, ogni ambiguità risolta,
ogni scostamento dal piano:

```
Ruling: <cosa hai deciso> — <perché> — <costo se è la scelta sbagliata>
Task <N>: complete (commit <sha>, test verdi)
```

in `.superpowers/sdd/<piano-corrente>/progress.md`. È ciò che permette a
chiunque di riprendere senza rileggere la cronologia git: è parte della
consegna, non un extra.
