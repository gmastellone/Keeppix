# Costo contro beneficio — cosa vale la pena tenere

**A cosa serve questo documento.** Mettere accanto a ogni funzione dell'interfaccia il prezzo
che fa pagare al sistema, misurato dove possibile, così che tagliare o tenere sia una decisione
e non un'omissione. Il bersaglio è un **Raspberry Pi 5 / 8 GB** con ~200.000 scatti su ~1 TB.

Tre punti di vista, tenuti distinti apposta perché **non concordano**:

- **Architetto** — quanto costa in CPU, RAM, I/O, complessità e superficie da mantenere.
- **Analista** — quanto spesso viene usata davvero, e cosa succede se non c'è.
- **Chi usa la piattaforma** — un fotografo che scarica schede, sceglie, cerca e ritrova.

---

## Riepilogo: la classifica

| # | Funzione | Costo | Uso reale | Verdetto |
|---|---|---|---|---|
| 1 | **Video e transcodifica HLS** | altissimo | **nessuno** — non è nemmeno disegnata | **tagliare o congelare** |
| 2 | **Album dinamici** | altissimo | basso, e **ridondante con i tag** | **tagliare** |
| 3 | **Conteggi esatti ovunque** | alto, continuo | cosmetico | **degradare** |
| 4 | **Registro di controllo (audit)** | medio, continuo | nullo se sei solo | **spegnere di default** |
| 5 | **Riconoscimento volti** | altissimo | **dipende da te** | **decisione tua** |
| 6 | **Ricerca semantica e tag automatici** | altissimo | **dipende da te** | **decisione tua** |
| 7 | **Scrubber dei mesi** | basso | medio | tenere |
| 8 | **Mappe offline** | medio (disco) | medio | tenere |
| 9 | **Duplicati** | alto ma **inevitabile** | medio | tenere: il costo si paga comunque |
| 10 | **Geometria e thumbhash** | bassissimo | **abilitanti** | tenere e basta |

---

## 1. Video e transcodifica HLS — **il candidato più chiaro al taglio**

**Architetto.** È la funzione più cara che il sistema possa eseguire. Transcodificare in HLS su
un Pi 5 significa occupare tutti i core per minuti per ogni video, con un profilo energetico che
compete con l'ingestione e la navigazione. Porta con sé `hls.js` sul frontend (~150 KB gzip, in
chunk separato), tre rotte (`playback`, `poster`, `hls`), una cache di transcodifica da
mantenere e svuotare, e ffmpeg nel perimetro sandbox.

**Analista.** Il documento funzionale dichiara, alla lettera: *«Video: l'intero disegno assume
fotografie.»* Nessuna delle 70 schermate lo prevede. È stato costruito in Fase 6 e **non ha
consumatori nel disegno**.

**Utente.** Se scarichi schede di una reflex, i video sono una minoranza e spesso li gestisci
altrove. Se invece riprendi anche video, ti serve — ma allora serve *anche* la sua interfaccia,
che non esiste.

> **Raccomandazione: congelare.** Non cancellare il codice — è scritto, testato e funzionante —
> ma **disattivarlo di default** dietro un interruttore, escludere `hls.js` dal bundle finché non
> serve, e non costruirne l'interfaccia in Fase 11. Si riaccende il giorno in cui decidi che
> Keeppix gestisce anche i video, e quel giorno si disegna.
> **Risparmio:** l'intera voce di transcodifica dal carico del server, e una tranche di lavoro UI.

---

## 2. Album dinamici — **caro e ridondante**

**Architetto.** È la query più costosa che l'interfaccia sappia innescare: i membri non sono
materializzati (giustamente — sono raccolte *«vive»*), quindi **ogni apertura della griglia Album
lancia una scansione del catalogo per ogni album dinamico**. Otto album = otto scansioni su
200.000 righe. Va mitigato con cache e tetti, cioè con altra complessità.

**Analista.** Fa **la stessa cosa dei tag**, che la Fase 7 porta comunque: «Tramonti» come album
dinamico e «Tramonti» come tag sono la stessa raccolta viva, ottenuta in due modi. E le
**ricerche salvate** — che esistono già e costano una frazione — coprono il terzo caso.

**Utente.** Un album lo crei per *curare* una selezione: «i migliori 30 del viaggio». Quella è
manuale per definizione. Il caso «tutte le foto con 5 stelle» è una ricerca, e la vuoi come
ricerca.

> **Raccomandazione: tagliare gli album dinamici, tenere gli album manuali.** Il caso d'uso
> resta coperto da tag e ricerche salvate, che costano molto meno.
> **Risparmio:** la voce di costo più alta dell'interfaccia, più `rule jsonb`, il vincolo di
> coerenza, la cache dei conteggi e il `409` sugli album dinamici.

---

## 3. I conteggi esatti, ovunque — **degradare, non togliere**

**Architetto.** L'interfaccia mostra un numero accanto a quasi ogni riga di ogni elenco: foto per
cartella (sidebar, a ogni render), membri per album, elementi per link, foto per tag, foto per
persona, da valutare per lotto. **Sei aggregati**, ognuno dei quali diventa N+1 se scritto nel
modo ovvio, e ognuno dei quali va invalidato a ogni import, cestinamento e spostamento.

**Analista.** Nessuna decisione dell'utente cambia se «Urbino 556» diventa «Urbino ~550» o
«Urbino». Il numero serve a dare **peso relativo**, non precisione.

**Utente.** Vuoi sapere dove sta la massa delle foto. Non ti serve la cifra esatta, e non te ne
accorgi se è di ieri.

> **Raccomandazione: tenerli ma degradarli.** Cache senza invalidazione fine (un aggiornamento
> periodico basta), tetto a 1.000 con «più di 999» oltre, e **niente conteggio** dove non aiuta
> a decidere. La precisione al singolo scatto serve in un posto solo: **il badge del culling**,
> perché lì «quante me ne restano» è la domanda.

---

## 4. Registro di controllo (audit) — **spegnere di default**

**Architetto.** Una scrittura per ogni azione, una tabella che cresce senza limite, due indici da
mantenere. Costo unitario piccolo, moltiplicato per tutto.

**Analista.** Serve quando più persone toccano la stessa libreria e bisogna sapere chi ha fatto
cosa. Su un server di casa con un utente, **non risponde a nessuna domanda**.

**Utente.** Non lo aprirai mai.

> **Raccomandazione: spento di default, si accende quando crei il secondo utente.** Il codice
> resta, la scrittura no.

---

## 5. Riconoscimento volti — **la tua decisione, ma con i numeri davanti**

**Architetto.** Rilevamento (SCRFD) più embedding (ArcFace) per **ogni volto**, non per foto: un
archivio di famiglia fa facilmente 1,5 volti per scatto, quindi ~300.000 embedding su 200.000
foto. A 512 dimensioni sono **~600 MB di vettori**, più l'indice, più i ritagli, più il
raggruppamento incrementale da rieseguire quando arrivano foto nuove. Su un Pi con 8 GB, dove
Postgres dovrebbe già prendersi ~2 GB, è una voce seria. In più sono **dati biometrici**, con una
regola non negoziabile da garantire dove i link pubblici vengono serviti.

**Analista.** È la funzione con la varianza d'uso più alta di tutte: **fondamentale** per un
archivio di famiglia, **inutile** per un archivio di paesaggi.

**Utente.** Solo tu sai se cerchi «foto di Marta» o «tramonti in Val d'Orcia».

> **Raccomandazione: tenerla ma spenta di default, e chiedere all'utente al primo avvio.** È già
> il modello previsto (interruttore in Impostazioni più comando distinto per cancellare i dati).
> Se il tuo archivio è di paesaggi, **spegnila e risparmi la voce più cara dopo l'import**.

---

## 6. Ricerca semantica e tag automatici — **la stessa domanda, con un'attenuante**

**Architetto.** Un embedding CLIP per foto: a 512 dimensioni sono **~400 MB di vettori** su
200.000 scatti, più un indice HNSW dello stesso ordine di grandezza. Il calcolo costa **42 ms per
foto in «Piena»** e **260 ms in «Ridotta»** (misure dichiarate dal disegno): 2,3 ore contro 14,4
ore per l'intera libreria. Richiede **pgvector**, quindi un'immagine Postgres personalizzata.

**L'attenuante è vera e va pesata:** *un solo vettore per foto serve tre funzioni* — ricerca per
descrizione libera, abbinamento dei tag, «foto simili». Non è un costo che si paga tre volte.

**Analista.** È l'unica funzione che risponde a *«dov'era quella foto con la barca rossa?»*
quando non ricordi né data né cartella. Quel bisogno cresce con la libreria: a 5.000 foto non
esiste, a 200.000 è il motivo per cui apri l'applicazione.

**Utente.** Se organizzi per cartelle e date e ti ci ritrovi, non ti serve. Se cerchi per
*contenuto*, non c'è alternativa.

> **Raccomandazione: tenerla, ma i tre livelli sono la vera risposta.** «Piena» / «Ridotta» /
> «Spenta» esistono proprio per questo, e vanno presentati al primo avvio con **i numeri veri
> misurati sulla tua macchina**, non come preferenza astratta: *«analisi completa: ~2 ore, poi
> qualche minuto al giorno»* è una scelta informata; *«livello di IA»* non lo è.

---

## 7-10. Le quattro da tenere, e perché

**Scrubber dei mesi** — costo marginale quasi nullo: la geometria che gli serve viene comunque
caricata per il layout. Le tacche sono già equidistanti e non proporzionali, cioè già
approssimate. Su una timeline di 200.000 scatti è l'unico modo per saltare a un periodo. *Tenere.*

**Mappe offline (PMTiles)** — 640 MB per l'Italia, 4,8 GB per il mondo. Su un disco che contiene
1 TB di foto è lo 0,5%. Il vero motivo non è la mappa: è che **nessuna richiesta lascia la tua
rete**. Per un archivio personale geolocalizzato è la funzione con il miglior rapporto fra costo
e principio. *Tenere, scaricando solo le regioni che servono.*

**Duplicati** — richiede l'hash del contenuto di ogni file: ~3 ore per TB, ed è la fase più lenta
dell'import dopo l'overhead. **Ma il costo si paga comunque**: l'hash è la chiave degli URL dei
derivati (`/media/thumb/{hash}`, immutabili e cacheabili per sempre) ed è ciò che permette
all'upload di saltare i file già presenti. Togliere i duplicati **non farebbe risparmiare nulla**.
*Tenere: è gratis, il conto è già stato pagato altrove.*

**Geometria e thumbhash** — la geometria costa **0,44 MB** per 200.000 scatti e abilita layout
giustificato, virtualizzazione e scrubber. `thumbhash` è già nel payload e toglie **60 richieste
dal percorso critico** a ogni schermata. Sono le due cose col miglior rapporto costo/beneficio di
tutto il sistema. *Tenere, e usarle davvero — oggi thumbhash non lo usa nessuno.*

---

## Se tagli le prime quattro

| Voce | Cosa sparisce |
|---|---|
| Video | la transcodifica dal carico del server; `hls.js` dal frontend; una tranche di UI da disegnare |
| Album dinamici | la query più cara dell'interfaccia; `rule jsonb` e il suo vincolo; una cache da invalidare |
| Conteggi esatti | cinque dei sei aggregati per-riga, con le loro invalidazioni |
| Audit | una scrittura per ogni azione e una tabella che cresce sempre |

Restano intatti: culling, timeline a scala reale, ricerca strutturata, mappa, condivisione,
album manuali, cestino a tre vie, duplicati, rinomina, WebDAV.

**Nessuno dei tagli tocca il percorso principale** — scaricare una scheda, scegliere, organizzare,
ritrovare. È il criterio con cui sono stati scelti.

---

## Le due che non sono tagli ma decisioni

Volti e ricerca semantica **non vanno decise da me**: dipendono da cosa contiene il tuo archivio.
La proposta è che l'applicazione **te lo chieda al primo avvio, con i numeri misurati sulla tua
macchina** — non come preferenza astratta in una pagina di impostazioni che nessuno apre.

---

# Decisioni prese — 20 agosto 2026

Quattro decise, due rinviate a dopo una spiegazione migliore.

## 1. Video e transcodifica — **si tiene, ma resa semplice**

Non si congela. Si tiene con tre vincoli, che tolgono la parte cara senza togliere la funzione:

- **Solo quando il sistema non è usato, o di notte.** La transcodifica scende a
  `JobPriority::Background`, quindi `EnergyProfile::Interactive` non la reclama mai: se stai
  navigando, non parte. La finestra notturna la fa girare a piena velocità.
- **Un solo formato.** Niente scala di rendition multiple: **una** resa, e basta. La complessità
  di HLS multi-bitrate non si paga su un server di casa con un utente.
- **Solo se serve davvero.** Un video già piccolo o già in un formato riproducibile dal browser
  **non si tocca**: si serve l'originale. Si transcodifica solo sopra una soglia di peso o per
  formati che il browser non sa aprire.

È lo stesso principio già applicato ai derivati fotografici: `SKIP_PREVIEW_PX` e
`SKIP_PREVIEW_BYTES` evitano di generare una preview quando il sorgente è già adatto. Qui la
regola è la stessa, applicata al video.

**Resta aperto:** l'interfaccia del video non è disegnata. Va fatta in Fase 11, minima —
il documento non la prevede perché *«l'intero disegno assume fotografie»*.

## 2. Album dinamici — **tolti, sostituiti da «Aggiorna album»**

Gli album dinamici spariscono. Al loro posto: un album **normale, materializzato**, che però
**ricorda il filtro con cui è stato creato** e ha un pulsante **`"Aggiorna album"`**.

È una soluzione migliore di entrambe le alternative che avevo proposto:

- il costo della scansione si paga **quando lo chiedi tu**, non a ogni apertura della griglia —
  che era l'obiezione principale;
- il conteggio dei membri torna a essere una lettura banale di `album_assets`, quindi **sparisce
  anche il problema dei conteggi cari** per gli album;
- niente `rule` da valutare in lettura, niente cache da invalidare, niente `409` sugli album
  dinamici;
- e si guadagna una cosa che gli album dinamici **non** avevano: **il controllo**. Un album che
  cambia da solo può perdere una foto che avevi voluto tenere; qui aggiorni quando vuoi, e vedi
  cosa cambia.

Lo schema si semplifica: resta `rule jsonb` (per poter rilanciare il filtro) ma **sparisce**
`kind`, sparisce il vincolo che lega `rule` a `kind='dynamic'`, e i membri stanno sempre in
`album_assets`.

## 3. Conteggi esatti — **tolti ovunque tranne il culling**

Spariscono da: sidebar (cartelle), griglia album, link pubblici, tag, persone.
**Restano solo nel culling** — badge di navigazione e selettore di lotto — perché lì *«quante me
ne restano da vedere»* è esattamente la domanda che l'utente si fa, e la precisione conta.

Conseguenza tecnica: **cinque dei sei aggregati per-riga spariscono**, con le loro cache e le
loro invalidazioni. Resta il solo conteggio del culling, che è per lotto e quindi piccolo.

## 4. Registro di controllo — **spento di default**

Si accende quando esiste un secondo utente. Il codice resta, la scrittura no.
