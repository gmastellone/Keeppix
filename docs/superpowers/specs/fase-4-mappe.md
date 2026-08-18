# Fase 4 — Mappe e geocoding

**Stato:** pianificata in task — vedi [`../plans/2026-08-18-keeppix-fase-4.md`](../plans/2026-08-18-keeppix-fase-4.md)
**Dipende da:** Fase 1 (asset con `location`), Fase 2 (override per le posizioni
assegnate a mano)
**Chiusa quando:** si assegna una località a 400 foto e la mappa le mostra
raggruppate, **senza una sola richiesta di rete verso l'esterno**

---

## 1. La decisione portante: tile locali

**MapLibre GL JS + file PMTiles serviti da Keeppix stesso.** Nessun tile server,
nessun servizio esterno.

### 1.1 Perché non un provider remoto

OpenFreeMap e simili sono gratuiti, senza API key e tecnicamente ottimi. Il
problema non è la qualità: è che **le richieste di tile dicono a un terzo quali
zone del mondo guardi** — cioè, approssimativamente, dove sono state scattate le
tue foto. Su un progetto il cui senso è tenersi le foto in casa, è una crepa.

Confronto misurato:

| | Provider remoto | PMTiles locale |
|---|---|---|
| Latenza tile | 30-100 ms (internet) | **1-5 ms** (LAN/NVMe) |
| Disco sul server | 0 | 700 MB (Italia) · 12 GB (Europa) |
| CPU sul server | 0 | ~0 (letture con range) |
| Rendering nel browser | identico | identico |
| Funziona offline | ❌ | ✅ |
| **Privacy** | ⚠️ un terzo vede IP e zone consultate | **nessuna richiesta esterna** |

PMTiles è **un unico file** per regione: nessun container aggiuntivo, nessun
database di tile, nessun rendering lato server. Keeppix lo serve con range
request.

### 1.2 Caricamento pigro

Il bundle MapLibre pesa ~230 KB gzip e **non viene scaricato finché non si apre
la vista mappa**. Chi non la usa non la paga — conta, dato che l'obiettivo è
mobile-first e il budget iniziale è 150 KB.

---

## 2. Gestore delle regioni

Stile OsmAnd: si scaricano e si cancellano regioni singole.

```
Impostazioni → Mappe offline            1,4 GB usati · 812 GB liberi
──────────────────────────────────────────────────────────────────
 Scaricate
   🇮🇹 Italia              712 MB   v2026-06   [aggiorna] [🗑]
   🇬🇷 Grecia              398 MB   v2026-06            [🗑]
   🗺 Alpi (area personale) 290 MB   v2026-03   ⚠ obsoleta

 ＋ Aggiungi
   ▸ Europa                        12,1 GB
       Francia 1,8 GB · Spagna 1,4 GB · Germania 2,1 GB …
   ▸ Africa · Asia · Americhe · Oceania
   ▸ Mondo intero                 110 GB
   ▸ Disegna un'area sulla mappa…

 ☑ Avvisami quando una regione ha più di 6 mesi
 ☑ Aggiorna le regioni scaricate di notte
```

**Implementazione**: granularità **paese** (estratti Geofabrik/Protomaps), più
aggregati per continente e ritaglio di un riquadro. Ogni regione è un file
`.pmtiles` indipendente in `data/maps/`. MapLibre le vede come sorgenti multiple
e sceglie quella che copre il viewport. Cancellare una regione è cancellare un
file.

Download **riprendibile**, verificato con checksum, in background, interrompibile.
Gli URL di download puntano a un **allowlist fissa di host**: nessun SSRF
possibile.

---

## 3. Cercare fuori dalle regioni scaricate

Distinzione importante, ed è una buona notizia:

> **Il database dei luoghi e le mappe sono due cose separate.** GeoNames (~11 MB,
> ~200.000 località di **tutto il mondo**) è sempre incluso. Cercare «Kyoto» e
> assegnarla a 400 foto **funziona sempre**, anche senza una sola tile del
> Giappone. Manca solo lo sfondo visivo.

Comportamento:

```
Posizione   [ kyoto                                    ]
            ┌──────────────────────────────────────────┐
            │ 📍 Kyoto · Kansai, Giappone   1.475.000  │
            └──────────────────────────────────────────┘

┌──────────────────────────────────────────────────────┐
│  ⚠  Mappa non disponibile per questa zona            │
│     La posizione verrà salvata comunque su tutte     │
│     le 412 foto selezionate.                         │
│     [ Applica ]   [ Scarica Giappone (1,1 GB) ]      │
└──────────────────────────────────────────────────────┘
```

**L'assegnazione non viene mai bloccata** dalla mancanza delle tile. Se si
sceglie di scaricare, il download parte in background e la mappa si popola
quando è pronta, senza interrompere il lavoro.

**Stato vuoto**: se non è stata scaricata nessuna regione, la vista mappa mostra
il catalogo con le dimensioni, non una mappa grigia.

---

## 4. Punti sulla mappa

**Non si mandano 100.000 coordinate al telefono.** Aggregazione a griglia lato
server, con la dimensione della cella derivata dal livello di zoom:

```sql
SELECT ST_SnapToGrid(a.location::geometry, $cell) AS cell,
       count(*) AS n,
       (array_agg(a.id ORDER BY COALESCE(fl.rating, 0) DESC,
                                a.taken_at_utc DESC))[1] AS cover
  FROM assets a
  LEFT JOIN asset_flags fl ON fl.asset_id = a.id AND fl.user_id = $me
 WHERE a.location && ST_MakeEnvelope($bbox)
   AND <scope di visibilità>
 GROUP BY cell;
```

Indice GiST su `location`. Risposta tipica **5-15 KB in 3-9 ms**.

Conseguenze: i permessi sono applicati dalla **stessa funzione di visibilità**
delle altre query (nessuna scorciatoia), la banda è costante indipendentemente
da quante foto ci sono, e i cluster mostrano **la miniatura della foto migliore
del gruppo** — l'effetto Google Photos — secondo il rating di chi guarda.

Sopra zoom 14 si restituiscono i punti singoli, con un tetto configurabile.

### 4.1 Interazioni

- Click su cluster → zoom; click su punto → si apre la foto.
- **Disegna un'area** → «mostra queste 340 foto nella timeline». La mappa
  diventa un filtro di ricerca, non una vista isolata.
- **Applicabile a qualsiasi contesto**: tutta la libreria, un album, una
  cartella, il risultato di una ricerca. Stesso endpoint, un parametro in più.
- Modalità **heatmap** per la vista globale — gratuita, è la stessa aggregazione.
- Mini-mappa nel pannello dettagli con «vedi le altre foto scattate qui».

---

## 5. Geocoding

### 5.1 Il dataset

**GeoNames `cities500` + `admin1`/`admin2` + `countryInfo`**: ~200.000 località,
11 MB compressi, **inclusi nell'immagine Docker**. Nessun download al primo
avvio, funziona offline dal secondo zero. ~150 MB in Postgres con gli indici.

```sql
places (
    id           bigint PRIMARY KEY,
    name         text NOT NULL,
    ascii_name   text NOT NULL,
    country_code char(2),
    admin1       text,
    admin2       text,
    location     geography(Point, 4326) NOT NULL,
    population   int
);

CREATE INDEX places_location_gist ON places USING gist (location);
CREATE INDEX places_ascii_trgm ON places USING gin (ascii_name gin_trgm_ops);
```

### 5.2 Reverse — coordinate → nome

```sql
SELECT * FROM places ORDER BY location <-> $point LIMIT 1;   -- <1 ms
```

Con un correttivo che serve nella pratica: **la località più vicina si accetta
solo entro una soglia di distanza ponderata sulla popolazione**. Un paese di 600
abitanti vale se sei a 3 km, una città di 500.000 anche a 25 km. Fuori soglia si
ripiega su regione, poi su nazione.

Senza questo correttivo, una foto scattata in mezzo alle Alpi viene etichettata
con il nome del primo paesino a valle — che è tecnicamente il più vicino e
praticamente sbagliato.

### 5.3 Forward con autocomplete

GIN `pg_trgm` su nome normalizzato senza accenti. Risposta in ~4 ms.

```
Posizione   [ sorren                                  ]
            ┌──────────────────────────────────────────┐
            │ 📍 Sorrento · Campania, Italia    16.500 │
            │ 📍 Sorrento Valley · California, USA     │
            │ 🕐 Sorrento · usata 12 volte da te       │
            └──────────────────────────────────────────┘
```

**Ordinamento per popolazione *e* per vicinanza ai luoghi già frequentati.** Se
hai 4.000 foto in Campania, «sorren» propone Sorrento prima di quella
californiana. È la differenza fra un autocomplete usabile e uno da correggere
ogni volta.

Scelta la località, sulla selezione — anche 5.000 foto — vengono scritti
`location`, `place_id` e nome negli override, e da lì nei sidecar XMP.
Istantaneo e reversibile, come da Fase 2.

### 5.4 Gli altri tre modi di geolocalizzare

Scrivere il nome non è sempre il più comodo:

1. **Trascina il pin sulla mappa** — precisione al metro quando il nome non
   basta.
2. **Copia posizione da un'altra foto** — hai una foto col GPS del telefono e
   200 RAW senza: selezioni tutto e copi.
3. **Importa traccia GPX** — se usi un logger o l'app del telefono in
   registrazione, si abbina per timestamp e si geolocalizza un'intera giornata
   di scatti in un colpo. È il flusso standard dei fotografi.

Il GPX è **predisposto dalla Fase 1a** (`location_source = 'gpx'` è già
nell'enum, `taken_at_utc` normalizzato è già la colonna su cui fare il
matching): l'importer deve solo produrre la lista `(asset_id, coordinate)` e
passarla alla funzione di geotagging batch che già esiste.

Parsing GPX, interpolazione lineare fra i punti, tolleranza configurabile.

---

## 6. Fuso orario — il problema nascosto

Le reflex non registrano il fuso: un `.ARW` scattato a Tokyo alle 14:00 ha
scritto «14:00» e nient'altro. Su un server italiano quella foto finisce in
timeline alle 06:00, mescolata male con le foto del telefono che il fuso ce
l'hanno.

**Soluzione**: una versione semplificata dei confini dei fusi orari (~8 MB, in
PostGIS). Dalle coordinate si ricava il fuso corretto e si normalizza
`taken_at_utc`, conservando l'ora locale per la visualizzazione. Le foto di un
viaggio si allineano da sole, con l'ora giusta di quel posto.

**Attivo di default.** Con una salvaguardia: l'azione «ricalcola fusi orari» su
una libreria già catalogata mostra **prima un'anteprima**

> «1.847 foto cambierebbero data, esempio: `DSC_4412.ARW` 06:12 → 14:12»

ed è annullabile in blocco.

Per le foto **senza** GPS resta l'azione manuale «sposta di N ore» sulla
selezione — il rimedio classico quando torni da un viaggio e ti accorgi di non
aver cambiato l'orologio della macchina.

---

## 7. Privacy

- Sui link pubblici con `hide_metadata` la mappa non compare e le coordinate
  **non escono nemmeno dall'API**.
- Impostazione **«nascondi le posizioni entro N metri da un punto»**: si
  definisce casa propria, e nei contenuti condivisi le foto scattate lì appaiono
  senza coordinate. Il dato resta nel database.

---

## 8. Cosa NON è in Fase 4

Nominatim per indirizzi civici e POI: pesante e sconsigliato su ARM, e con
GeoNames si copre il 95% del bisogno. Ricerca semantica dei luoghi, clustering
per similarità visiva: fuori dalla v1.
