# Documentazione di progetto — indice

Tutto ciò che serve per lavorare su Keeppix senza dover ricostruire il contesto
dalla cronologia git.

## Da dove partire

**Se stai per scrivere codice**, leggi in quest'ordine:

1. [`/AGENTS.md`](../../AGENTS.md) — invarianti, metodo di lavoro, cosa non
   fare. È letto automaticamente da Cursor, Codex e Claude Code.
2. [`plans/2026-08-13-keeppix-fase-0-STATO.md`](plans/2026-08-13-keeppix-fase-0-STATO.md)
   — stato del progetto, decisioni prese (R1-R13), difetti noti e differiti con
   la fase in cui vanno affrontati.
3. [`plans/2026-08-13-keeppix-roadmap.md`](plans/2026-08-13-keeppix-roadmap.md)
   — le fasi e i **contratti congelati** che nessuna fase può violare.
4. La **spec della fase** su cui lavori (sotto).
5. Il **piano della fase**, se esiste.

## I tre tipi di documento, e perché sono separati

| Tipo | Cosa contiene | Invecchia? |
|---|---|---|
| **Spec** | Decisioni: schemi, indici, protocolli, formati, ottimizzazioni | **No** — parla di contratti |
| **Piano** | Task passo per passo, codice esatto, test da scrivere | **Sì** — si scrive una fase alla volta |
| **Ledger** | Cronologia dell'esecuzione, decisioni prese durante il lavoro | Storico |

I piani si scrivono **una fase alla volta**, quando la precedente è chiusa. Un
piano di dettaglio scritto prima del codice su cui poggia è finzione plausibile:
inventa firme contro API che non esistono ancora.

Le spec invece si scrivono tutte in anticipo, perché descrivono decisioni prese
e non codice da scrivere.

## Architettura

- [`specs/2026-08-13-keeppix-design.md`](specs/2026-08-13-keeppix-design.md) —
  **il documento madre**. Architettura completa, modello dati, decisioni con
  l'alternativa scartata e il perché.

## Spec di fase

| Fase | Documento | Contenuto |
|---|---|---|
| 1a | *(nel piano)* | modello dati: librerie, cartelle `ltree`, asset, visibilità |
| 1b | [`specs/fase-1b-ingestione.md`](specs/fase-1b-ingestione.md) | coda job, worker, profili energetici, `keeppix-media`, walker, hash, derivati, watcher, fallimenti |
| 1c | [`specs/fase-1c-timeline.md`](specs/fase-1c-timeline.md) | timeline a bucket, ricerca, endpoint, **protocollo WebSocket**, frontend |
| 2 | [`specs/fase-2-raw-culling.md`](specs/fase-2-raw-culling.md) | pipeline RAW, sidecar XMP, culling, stack RAW+JPEG, cancellazione, duplicati |
| 3 | [`specs/fase-3-multiutente.md`](specs/fase-3-multiutente.md) | permessi, gruppi, album, link pubblici, audit log |
| 4 | [`specs/fase-4-mappe.md`](specs/fase-4-mappe.md) | PMTiles, gestore regioni, clustering, GeoNames, fusi orari |
| 5 | [`specs/fase-5-webdav-upload.md`](specs/fase-5-webdav-upload.md) | upload tus, WebDAV, wizard di configurazione |
| 6 | [`specs/fase-6-consolidamento.md`](specs/fase-6-consolidamento.md) | video, backup/ripristino, TOTP, sync delta, PWA, API pubblica |

## Piani

| Piano | Stato |
|---|---|
| [`plans/2026-08-13-keeppix-fase-0.md`](plans/2026-08-13-keeppix-fase-0.md) | ✅ completato e mergiato |
| [`plans/2026-08-14-keeppix-fase-1a.md`](plans/2026-08-14-keeppix-fase-1a.md) | ⬜ **il prossimo da eseguire** |
| Fase 1b, 1c, 2, 3, 4, 5, 6 | da scrivere, una alla volta |

## Consegne

- [`plans/2026-08-13-keeppix-fase-0-STATO.md`](plans/2026-08-13-keeppix-fase-0-STATO.md)
  — consegna della Fase 0: cosa è stato fatto, cosa è stato deciso, cosa resta.

## Ledger di esecuzione

`.superpowers/sdd/<piano>/progress.md` — cronologia delle decisioni prese
durante l'esecuzione, con `Ruling: <cosa> — <perché> — <costo se sbagliato>`.

Versionati di proposito (vedi **R11** in STATO.md): sono ciò che permette a
chiunque di riprendere il lavoro senza rileggere la cronologia git.

## Operatività

- [`/docs/DEPLOY.md`](../DEPLOY.md) — installazione, variabili d'ambiente,
  volumi, aggiornamento, arresto, reverse proxy, diagnosi.

## Le regole che non si negoziano

Sono in [`/AGENTS.md`](../../AGENTS.md), ma vale la pena ricordarne tre qui
perché sono quelle che si violano per distrazione:

1. **Nessun SQL fuori da `keeppix-db`.** È imposto anche meccanicamente: `sqlx`
   è dipendenza del solo `keeppix-db`, quindi una query in un handler non
   compila.
2. **`Forbidden`, mai `NotFound`**, quando un utente sonda un id che non gli
   appartiene. Altrimenti l'endpoint diventa un oracolo di esistenza.
3. **Un test deve fallire se ciò che il suo nome dichiara regredisce.** Nella
   Fase 0 tre test passavano senza provare nulla di ciò che affermavano.
