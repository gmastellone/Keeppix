# Fase 11 — Interfaccia

**Stato:** specifica. Nessuna riga è stata implementata.
**Fonte di verità del comportamento:** `docs/ui/keeppix-mockup.html` — prototipo interattivo,
navigabile, senza server. **Dove il prototipo e qualunque testo divergono, ha ragione il
prototipo.**
**Fonte di verità della descrizione:** `docs/ui/documento-funzionale-ui.md` — 70 schermate, ogni
controllo con etichetta esatta, ogni scorciatoia, ogni stato disabilitato con la ragione, ogni
durata di transizione.
**Marchio:** `docs/ui/brand-sheet.png`.

---

## 1. Che tipo di lavoro è

**Non è un progetto nuovo.** Il frontend esiste: Vue 3.5, Pinia, `vue-router` 5, `vue-i18n` 11,
Tailwind 4, Vite 8, Vitest 4, con 16 viste già scritte (Timeline, Culling, Map, Search, Albums,
Shares, Trash, Users, Groups, Problems, Setup, Login, Player, BatchEdit, Folders, ShareTarget) e
una manciata di componenti (`AssetViewer`, `Filmstrip`, `RatingStars`, `PlacePicker`,
`MapClusterLayer`, `UploadPanel`, `SharePanel`).

Questa fase porta quelle viste alla forma descritta dal documento funzionale, aggiunge le
schermate che mancano, e introduce i pattern condivisi (SP-1…SP-30) come componenti veri invece
che come codice ripetuto.

**Ruling: le viste esistenti si riscrivono, non si affiancano.** — Avere `TimelineView.vue` e
`TimelineView2.vue` durante la transizione sembra prudente e invece raddoppia la superficie da
mantenere proprio mentre cambia. Le viste esistenti hanno già i test (`*.spec.ts`): quei test
sono la rete di sicurezza della riscrittura. — *Costo se sbagliato:* una vista può restare rotta
per la durata di un task invece che avere un fallback.

---

## 2. Librerie: cosa c'è, cosa si aggiunge, cosa si scrive

Il vincolo è esplicito: **meno librerie possibile, e quelle che ci sono devono essere note,
ufficiali e aggiornate.**

### Già presenti e sufficienti

| Libreria | Ruolo | Perché resta |
|---|---|---|
| `vue` 3.5 | — | — |
| `pinia` 4 | stato | store ufficiale Vue |
| `vue-router` 5 | rotte | vedi §4: la UI diventa indirizzabile |
| `vue-i18n` 11 | lingua | Impostazioni offre Italiano/English |
| `reka-ui` 2 | primitive accessibili | dialog, menu, popover, tooltip, switch con focus trap e ARIA corretti. È la ragione per cui **non** serve nessuna libreria di componenti |
| `tailwindcss` 4 | stile | token di tema in CSS custom properties, già usati |
| `maplibre-gl` + `pmtiles` | mappa | Fase 4, tile servite dal server |
| `hls.js` | video | Fase 6 |
| `hash-wasm` | checksum upload | Fase 5 |

### Da aggiungere: **nessuna**

### Da scrivere in casa, e perché

**Il layout giustificato e la virtualizzazione della timeline.**

È il punto in cui il documento dice che *"il frontend reale dovrà fare il lavoro più delicato —
CSS da solo non basta"*. La tentazione è `@tanstack/vue-virtual`.

**Ruling: virtualizzatore scritto in casa, ~120 righe, nessuna dipendenza.** — I virtualizzatori
generici esistono per risolvere un problema che qui **non c'è**: altezze di riga sconosciute
finché non le misuri. Grazie all'endpoint di geometria della Fase 10 (§2) conosciamo larghezza e
altezza di *ogni* scatto **prima di disegnare**, quindi le altezze di riga sono calcolabili in
anticipo con esattezza. Il problema si riduce a: somme prefisse delle altezze di riga, e una
ricerca binaria su `scrollTop` per sapere quali righe sono visibili. Adottare un virtualizzatore
a misurazione significherebbe pagare un ciclo di misura/ridisegno per ogni riga che entra —
esattamente il costo che la geometria anticipata serve a eliminare. — *Costo se sbagliato:*
~120 righe da mantenere e testare, contro una dipendenza in meno e un ciclo di layout in meno
per riga.

L'algoritmo giustificato è quello classico: si accumulano scatti finché la somma dei rapporti
d'aspetto moltiplicata per l'altezza obiettivo supera la larghezza disponibile, poi si scala la
riga per riempirla esattamente. Deterministico, `O(n)`, nessuna misura del DOM.

---

## 3. Prestazioni: i numeri da rispettare

Il bersaglio hardware resta il **Raspberry Pi 5 / 8 GB**, e il budget di bundle iniziale è già
imposto dalla CI: **150 KB gzip** per gli asset che `index.html` carica subito
(`.github/workflows/ci.yml`, job `frontend`).

| Vincolo | Valore | Come si rispetta |
|---|---|---|
| Bundle iniziale | ≤ 150 KB gzip | rotte in `import()` pigro, già la convenzione in `src/router.ts` |
| Tessere vive nel DOM | ≤ ~3 schermate di righe | virtualizzatore §2 |
| Geometria di 214k scatti | 1 richiesta, ~4,7 MB binari | Fase 10 §2, con `ETag` |
| Miniature | solo le righe visibili + 1 schermata di margine | `IntersectionObserver`, `loading="lazy"`, `decoding="async"` |
| Priorità di generazione | le miniature che si stanno guardando per prime | `POST /api/v1/viewport` **esiste già** ed è nato per questo |
| Ricalcolo del layout | mai durante lo scroll | il layout dipende solo da larghezza del contenitore e geometria; si ricalcola su `ResizeObserver`, non su `scroll` |

**Ruling: `maplibre-gl` e `hls.js` non entrano mai nel bundle iniziale.** — Sono le due
dipendenze pesanti (rispettivamente ~200 KB e ~150 KB gzip): da sole sfonderebbero il budget
tre volte. Vivono solo nei chunk di Mappa e del player video, caricati su navigazione. —
*Costo se sbagliato:* la CI fallisce, che è il comportamento voluto.

**Ruling: la geometria si chiede una volta per vista e si tiene in un `ArrayBuffer`, non in
oggetti.** — 214.000 oggetti JavaScript `{id, w, h, month}` costano ~50 MB di heap e mettono
sotto pressione il garbage collector a ogni scroll. Lo stesso dato in un `ArrayBuffer` è 4,7 MB
e non produce spazzatura. — *Costo se sbagliato:* l'accesso passa da `arr[i].w` a una `DataView`,
incapsulata in una classe di ~30 righe.

---

## 4. Le due decisioni che il prototipo lascia aperte

Il documento le marca come *"decisioni da prendere consapevolmente, non da ereditare"*.

### 4.1 Le schermate devono essere indirizzabili

Il prototipo non ha rotte: lo stato della vista è una proprietà in memoria, il tasto Indietro del
browser non funziona, nessuna schermata ha un link.

**Ruling: ogni vista ha un URL, e i parametri di vista stanno nell'URL.** — Keeppix è
auto-ospitato e usato anche da mobile come PWA: un link a un album o a una persona è una
funzione, non un lusso, e il tasto Indietro di Android *deve* funzionare. `vue-router` è già una
dipendenza. — *Costo se sbagliato:* le regole di ripristino dello stato (§7 del documento
funzionale) vanno espresse come navigazioni, il che è più lavoro ma anche più leggibile delle
sedici righe di azzeramenti manuali che il prototipo esegue a ogni click di sidebar.

Corollario: **le regole di reset di §7 vanno rispettate alla lettera**, comprese le
incoerenze dichiarate — cliccare una voce di sidebar azzera i filtri rapidi, cliccare una
cartella no. Non sono sviste da correggere in questa fase: sono comportamento documentato, e
correggerle è una decisione di prodotto separata.

### 4.2 Il commutatore Desktop/Mobile non esiste nel prodotto

È scaffolding del prototipo. Nel prodotto il passaggio avviene **per larghezza dello schermo**.
Lo stesso vale per il pannello "Anteprima stati" in fondo a Impostazioni: si toglie, ma **la
macchina a stati che c'è dietro resta** — caricamento / pronto / errore — mossa dalle risposte
del server invece che da un interruttore.

---

## 5. I pattern condivisi diventano componenti

I trenta pattern SP-1…SP-30 sono la parte più riusata del documento. Diventano componenti veri,
ognuno con i suoi test:

| Pattern | Componente | Note |
|---|---|---|
| SP-1 tile foto | `PhotoTile.vue` | badge RAW/RAW+JPEG (serve `raw_kind` dalla Fase 10 §8.4), cuore, spunta |
| SP-2 selezione + barra azioni | `SelectionBar.vue` + store | sopravvive al cambio sezione (§7) |
| SP-3 filtro rapido a chip | `QuickFilter.vue` | sei dimensioni |
| SP-4 seleziona tutto quello che vedi | dentro `SelectionBar` | |
| SP-5 dialog modale | su `reka-ui` Dialog | Esc chiude, focus trap — il prototipo **non** lo fa ovunque: qui sì |
| SP-6 toast | `ToastHost.vue` | 10 ms in, 2400 ms visibile, 250 ms out |
| SP-7 tooltip | su `reka-ui` Tooltip | |
| SP-8 attivabile da tastiera | direttiva `v-activatable` | |
| SP-9 stelle | `RatingStars.vue` **esiste** | da allineare |
| SP-10 coda di conferma | `SuggestionQueue.vue` | tag (Fase 7) e volti (Fase 8) |
| SP-12 provenienza IA vs utente | `ProvenanceBadge.vue` | mai confuse, in nessun punto |
| SP-15 badge RAW | dentro `PhotoTile` | |
| SP-16 avatar | `Avatar.vue` | colore sincronizzato in tre punti — in Vue è gratis |
| SP-17 shell mobile | `AppShell.vue` | per larghezza, non per interruttore |
| SP-18 dialog eliminazione a 3 opzioni | `DeleteDialog.vue` | **nessun default implicito** |

**Ruling: accessibilità corretta anche dove il prototipo non ce l'ha.** — Il documento è onesto
nel dichiarare che sidebar, tab bar, menu account e righe di libreria sono `<div>` non
raggiungibili da tastiera, e che Esc non chiude i menu. Sono limiti del prototipo, non scelte di
prodotto: il documento stesso li segnala come "codice morto che documenta l'intenzione". Con
`reka-ui` la versione corretta costa meno di quella sbagliata. — *Costo se sbagliato:* nessuno;
è l'unico punto in cui **non** si copia il prototipo alla lettera.

---

## 6. Le schermate, raggruppate per dipendenza

Non tutte le 70 schermate sono costruibili subito: alcune dipendono da fasi non ancora fatte.

**Costruibili con Fasi 0–6 + Fase 10** (il grosso):
shell desktop e mobile, sidebar, topbar, menu account, pagina "Altro", router, Foto/Timeline,
Preferiti, tile, filtro rapido, selezione multipla, modifica in blocco, lightbox e pannello info,
menu "altre azioni", Cerca (barra, pillole, risultati — senza semantica), Mappa e popover,
dialog posizione, Condivisioni e dialog condividi, Album (griglia, dettaglio, creazione,
aggiungi ad album), Cestino, Duplicati, Problemi, dialog file con problemi, dialog eliminazione
a 3 opzioni, dialog generici, Impostazioni, Profilo, dialog inserimento testo, e tutta la
Parte X (scala, caricamento, errore, riuscita parziale).

**Dipendono dalla Fase 7:** Tag e categorie, dialog modifica tag, dialog modifica categoria,
selettore di tag, Revisione–tag, Analisi libreria, livelli IA, provenienza IA/utente, la parte
semantica di Cerca.

**Dipendono dalla Fase 8:** Persone (griglia e dettaglio), scegli copertina, assegna a gruppo,
unisci persone, separa persona, selettore di persona, menu sul riquadro del volto,
Revisione–volti, il chip "Persona" del filtro rapido.

**Dipendono dalla Fase 9:** Culling (griglia lotti, lotto aperto, selettore di lotto, dialog
cartella radice), dialog "Rinomina con formula", il filtro per stato pick.

**Ruling: la Fase 11 si divide in tranche che seguono le fasi da cui dipendono.** — Costruire
Persone contro un backend che non ha volti significa costruire contro dati finti, e i dati finti
nascondono esattamente i problemi che l'integrazione deve trovare. — *Costo se sbagliato:* la
UI arriva in quattro consegne invece che in una; in cambio ogni consegna è integrata davvero.

---

## 7. Stati di caricamento, errore e riuscita parziale

Il documento dedica un'intera parte (X) a questo, ed è la parte che il prototipo simula con
interruttori. In produzione:

- **Ogni insieme di dati** ha tre stati: in caricamento, pronto, errore. Nessuna schermata
  assume che i dati ci siano.
- **Gli scheletri di caricamento hanno la forma del contenuto**, non uno spinner centrato: per
  la timeline la geometria arriva prima delle miniature, quindi lo scheletro è già nella
  posizione e nelle proporzioni giuste. È un vantaggio diretto della richiesta #1.
- **Gli errori distinguono quattro nature** (Fase 10 §7) e mostrano "Riprova" solo nelle due
  dove ha senso.
- **La riuscita parziale ha una schermata sua**: "183 su 400 non sono state modificate", con
  l'elenco e la possibilità di ritentare solo quelle. Consuma l'involucro della Fase 10 §3.

---

## 8. Come si verifica che l'interfaccia sia quella giusta

- **Test unitari** (Vitest) su ogni pattern condiviso e su ogni vista, come già oggi.
- **Le etichette esatte sono un test.** Il documento riporta ogni etichetta alla lettera e fra
  virgolette: `"Svuota scartati"`, `"Nessuna foto corrisponde ai filtri"`, `"Solo demo — …"`.
  Vanno nei file di traduzione e asserite.
- **Il budget di bundle è già un test** in CI e non va allentato.
- **Confronto con il prototipo**: il prototipo si apre con un doppio click, senza server. Ogni
  task si chiude aprendo la schermata corrispondente nei due e confrontandole.

---

## 9. Cosa questa fase non fa

- Non introduce nessuna funzionalità che il documento non descriva.
- Non "migliora" il disegno: le divergenze notate (incoerenze di reset, `folder-item` senza
  `user-select`, regole CSS morte) sono documentate come tali e restano decisioni di prodotto
  separate.
- Non copre importazione iniziale né amministrazione del server: il documento dichiara
  esplicitamente di non averle disegnate.
