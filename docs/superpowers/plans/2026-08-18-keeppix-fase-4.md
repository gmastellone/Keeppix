# Keeppix Fase 4 — Mappe e geocoding

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Vedere dove sono state scattate le foto e assegnare posizioni, anche in blocco — senza che una sola richiesta di rete lasci il server. Chiuso quando si assegna una località a 400 foto e la mappa le mostra raggruppate, tile locali, zero chiamate esterne.

**Architecture:** Due dataset separati e indipendenti, per una ragione precisa (§3 della spec): il database dei luoghi (GeoNames, ~11 MB, incluso nell'immagine Docker) risponde sempre, anche a zero mappe scaricate — assegnare "Kyoto" a 400 foto funziona il giorno zero. Le tile (PMTiles) sono lo sfondo visivo, scaricabili a parte, per regione. Nessuna delle due passa da un servizio esterno: PMTiles è un file servito con range request da Keeppix stesso, GeoNames è dentro Postgres.

**Tech Stack:** PostGIS (già attivo) · MapLibre GL JS (chunk pigro, ~230 KB gzip) · file `.pmtiles` (formato Protomaps) · GeoNames `cities500` + `admin1`/`admin2` + `countryInfo` · confini dei fusi orari semplificati (~8 MB)

**Spec:** [`../specs/fase-4-mappe.md`](../specs/fase-4-mappe.md) — **leggerla prima**; se piano e spec divergono, vince la spec
**Roadmap:** [`2026-08-13-keeppix-roadmap.md`](2026-08-13-keeppix-roadmap.md) — Fase 4 dipende da `assets.location` (Fase 1) e `asset_overrides` (Fase 2), entrambe già in schema

---

## Cosa esiste già — non è tutto da zero

Verificato sul codice attuale, non sulla spec (che descrive il traguardo, non lo stato):

- **PostGIS è attivo** dalla migrazione 0001 (`crates/keeppix-db/migrations/0001_users.sql:10`), insieme a `pg_trgm` — abilitati in anticipo perché richiedono superuser.
- **`assets.location geography(Point,4326)` esiste già**, con `place_id bigint` e un CHECK `location_source IN ('exif','user','map_pin','copied','gpx')` (`migrations/0005_assets.sql:32-34`), più un indice GiST parziale `assets_location_gist` (riga 54). `'gpx'` è già un valore valido — l'import GPX di Fase 1a l'aveva già previsto.
- **`asset_overrides` ha già `location`/`place_id`** (`migrations/0012_overrides_flags.sql`, Fase 2), col tipo Rust `GeoPoint { lat: f64, lon: f64 }` in `crates/keeppix-domain/src/overrides.rs`.
- **`location_source` non ha una controparte Rust.** Vive solo nel CHECK SQL — nessun enum in `keeppix-domain`. Task 1 lo introduce.
- **Nessuna estrazione GPS da EXIF.** `crates/keeppix-media/src/exif.rs::parse_header` legge data/camera/lens/ISO/f-number/esposizione/focale/dimensioni, **mai** `GPSLatitude`/`GPSLongitude`. Il GPS oggi entra in Keeppix solo dal lato XMP (`crates/keeppix-media/src/xmp.rs`, che legge/scrive `exif:GPSLatitude` in un `GeoPoint`) — cioè solo se un override è già stato scritto su sidecar, non all'ingest. **Questo è il buco più importante da chiudere**: senza, `assets.location` resta vuoto per ogni foto anche quando il file ha coordinate GPS reali. Task 1.
- **Nessun codice di mappe/geocoding esiste.** Verificato con una ricerca su tutto il repo (`pmtiles`, `maplibre`, `geonames`, `places`, `geocod`): zero hit fuori dai documenti di piano. Si parte da zero su `places`, endpoint, PMTiles, frontend.
- **Un debito ereditato dalla Fase 3** (commit `ebb2e3b`): la spec §6.2 vuole un raggio configurabile attorno a "casa" che omette le coordinate dai contenuti condivisi. È stato deliberatamente rimandato qui perché `AssetView` oggi non espone `lat`/`lon` — non è una fuga attiva. **Ma il giorno in cui Fase 4 aggiunge coordinate a un payload pubblico, il geofence deve esserci nello stesso commit**, altrimenti diventa una fuga reale. Task 9.

---

## Global Constraints

Valgono per **ogni** task. Sono gli invarianti di [`/AGENTS.md`](../../../AGENTS.md), più quelli specifici di questa fase.

- **Rust edition 2024, toolchain 1.88.0.**
- **`keeppix-db` è l'unico crate con SQL.** `keeppix-media` non conosce il database.
- **Ogni metodo di repository che legge dati di un utente prende un `AuthContext` come primo parametro.**
- **`Forbidden`, mai `NotFound`**, quando si sonda un id altrui.
- **Query sempre parametrizzate.**
- **Nessun `unwrap()`/`expect()` in produzione.**
- Clippy `all` + `pedantic` a warn, `-D warnings` pulito. `cargo fmt --check` pulito.
- **Commit convenzionali in inglese**, uno per unità logica.

### Specifiche della Fase 4

- **Zero richieste di rete verso l'esterno, mai.** Non per i tile, non per il geocoding, non per i confini dei fusi. È il vincolo che decide l'architettura intera (§1.1 della spec) — un provider remoto vede quali zone del mondo guardi, cioè dove sono state le tue foto. Qualsiasi PR che introduce un `fetch()`/client HTTP verso un host non presente nell'allowlist dei download regioni è respinta a prescindere dalla comodità che offre.
- **Gli URL di download delle regioni PMTiles usano un'allowlist fissa di host**, hardcoded, non configurabile da input utente: zero superficie SSRF.
- **L'assegnazione di una posizione non è mai bloccata dalla mancanza delle tile.** Cercare e assegnare un luogo deve funzionare anche a zero regioni scaricate — è il motivo per cui `places` (GeoNames) e le tile PMTiles sono dataset indipendenti.
- **Il ricalcolo dei fusi orari è distruttivo su date già catalogate**: richiede sempre un'anteprima (`N foto cambierebbero data, esempio: ...`) e un annullamento in blocco prima di applicare.
- **I permessi sulla mappa passano dalla stessa funzione di visibilità delle altre query** — l'aggregazione a griglia non è una scorciatoia che bypassa `PermissionRepo`.
- **`location_source` è la fonte di verità su chi ha scritto una coordinata** (exif/user/map_pin/copied/gpx): un job di ingest non sovrascrive mai una posizione `user`/`map_pin` già presente in `asset_overrides` — l'`effective()` di Fase 2 (`COALESCE(override, exif)`) già lo garantisce per costruzione, ma ogni nuovo scrittore deve rispettarlo esplicitamente.

---

## Struttura dei file

```
crates/keeppix-domain/src/
├── geo.rs              NEW  LocationSource enum (mirror del CHECK SQL), Place, TzBoundary
└── lib.rs               MOD riesportazioni

crates/keeppix-media/src/
├── exif.rs               MOD estrarre GPSLatitude/GPSLongitude/GPSAltitude → GeoPoint
└── gpx.rs               NEW  parsing traccia, interpolazione lineare per timestamp

crates/keeppix-db/
├── migrations/
│   ├── 0015_places.sql        NEW  tabella places (GeoNames), indici GiST + trgm
│   └── 0016_tz_boundaries.sql NEW  confini dei fusi semplificati
├── src/
│   ├── places.rs        NEW  PlaceRepo — reverse/forward, ranking per popolazione+vicinanza
│   ├── geo.rs            NEW  GeoRepo — cluster a griglia, timezone lookup
│   └── regions.rs        NEW  RegionRepo — stato download regioni PMTiles

crates/keeppix-jobs/src/
├── geotag.rs             NEW  batch geotag da GPX, ricalcolo fusi con anteprima/undo
└── regions.rs            NEW  job download regione, ripristino/ripresa

crates/keeppix-api/src/routes/
├── map.rs                NEW  GET /map/clusters, GET /map/tiles/{region}/{z}/{x}/{y}
├── places.rs             NEW  GET /places/suggest, GET /places/reverse
└── regions.rs             NEW  gestore regioni: lista, download, cancella

data/
├── geonames/              NEW  cities500 + admin1 + admin2 + countryInfo, cotto nell'immagine
├── tz-boundaries/         NEW  confini fusi semplificati, cotto nell'immagine
└── maps/                  NEW  .pmtiles per regione, volume persistente — NON nell'immagine

frontend/src/
├── views/MapView.vue              NEW  chunk pigro (import() in router.ts, come da commento già presente)
├── views/settings/MapsOfflineView.vue NEW  gestore regioni
├── components/MapClusterLayer.vue NEW
├── components/PlacePicker.vue     NEW  autocomplete + pin + copia-da-altra-foto
└── stores/maps.ts                 NEW
```

**Ordine dei task:** 1 → 2 → 3 → (4, 6 in parallelo) → 5 → 7 → 8 → 9. Il Task 1 blocca tutto: senza GPS reale in `assets.location`, non c'è niente da mostrare sulla mappa. Il Task 9 (privacy) va per ultimo perché dipende dal payload pubblico che il Task 4/6 introducono.

---

## Task 1: Estrazione GPS dall'EXIF e `LocationSource`

**Perché prima di tutto:** oggi `assets.location` è sempre `NULL` all'ingest — nessun codice lo popola dal file. Il resto della fase (cluster, geocoding inverso, mappa) non ha nulla da mostrare finché questo non è chiuso.

**Files:**
- Modify: `crates/keeppix-media/src/exif.rs`
- Create: `crates/keeppix-domain/src/geo.rs`
- Modify: `crates/keeppix-jobs/src/ingest.rs` (o dove oggi si scrive `assets` dal risultato dell'EXIF)
- Create: `crates/keeppix-media/tests/exif_gps.rs`

**Interfaces:**
- `GeoPoint { lat: f64, lon: f64 }` — già esiste in `keeppix-domain::overrides`, riusato qui, non duplicato.
- `LocationSource { Exif, User, MapPin, Copied, Gpx }` con `as_str()` che produce esattamente le stringhe del CHECK SQL (`'exif'`, `'user'`, ...) — un test pinna la corrispondenza 1:1 con la constraint, altrimenti un `INSERT` con un valore fuori enum va in errore silenzioso solo a runtime.
- `ExifData` (in `keeppix-domain`) guadagna `pub gps: Option<GeoPoint>`.
- `parse_header` estrae `GPSLatitude`/`GPSLatitudeRef`/`GPSLongitude`/`GPSLongitudeRef` (formato EXIF: gradi/minuti/secondi come razionali, col Ref che decide il segno) in un `GeoPoint` decimale.

**Casi limite da pinnare nei test, prima del codice:**
- File senza tag GPS → `gps: None`, nessun errore.
- `GPSLatitudeRef == "S"` e `GPSLongitudeRef == "W"` → segno negativo su entrambe le coordinate (il bug più comune: dimenticare il Ref e prendere sempre valori positivi, che sposta ogni foto dell'emisfero sud/ovest nell'emisfero opposto).
- Coordinate a **zero** (`0.0, 0.0`) scritte da alcune fotocamere quando il GPS non ha fix: **non è un dato valido**, va scartato come `None`, non salvato come "punto nel Golfo di Guinea".
- Un ARW con GPS embedded nel MakerNote proprietario (non nel blocco EXIF standard) → fuori scope qui, resta `None`; non è una regressione, è semplicemente un dato che l'XMP/GPX matching prenderà più tardi.
- All'ingest, se `assets.location` è già valorizzato con `location_source IN ('user','map_pin')`, **non sovrascrivere**: l'EXIF gira solo alla prima indicizzazione, un rescan non deve calpestare una correzione manuale.

- [ ] **Step 1: Scrivere i test che falliscono** (`exif_gps.rs`, i cinque casi sopra)
- [ ] **Step 2-4: Fallimento, implementazione, verifica** — `cargo test -p keeppix-media`
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(media): extract GPS coordinates from EXIF at ingest"
```

---

## Task 2: `places` — il database GeoNames

**Files:**
- Create: `crates/keeppix-db/migrations/0015_places.sql`
- Create: `crates/keeppix-db/src/places.rs`, `crates/keeppix-db/tests/places.rs`
- Modify: `Dockerfile` (nuovo stage o `COPY` nello stage `runtime` esistente)
- Create: script di import una tantum (build-time, non a runtime): `scripts/build-geonames.sh` o stage Docker dedicato

**Lo schema** (dalla spec, §5.1 — invariato, verificato compatibile con lo schema attuale):

```sql
CREATE TABLE places (
    id           bigint PRIMARY KEY,       -- geonameid originale, per idempotenza sui reimport
    name         text NOT NULL,
    ascii_name   text NOT NULL,
    country_code char(2),
    admin1       text,
    admin2       text,
    location     geography(Point, 4326) NOT NULL,
    population   int NOT NULL DEFAULT 0
);

CREATE INDEX places_location_gist ON places USING gist (location);
CREATE INDEX places_ascii_trgm ON places USING gin (ascii_name gin_trgm_ops);
-- Il reverse pesca "il più vicino sopra una soglia di popolazione" (Task 3):
-- un indice composto evita uno scan quando la soglia scarta i risultati più vicini.
CREATE INDEX places_population_idx ON places (population DESC);
```

**Decisione infrastrutturale — dove vive l'import:** GeoNames (`cities500.zip` + `admin1CodesASCII.txt` + `admin2Codes.txt` + `countryInfo.txt`, ~11 MB compressi) si scarica **una volta, in fase di build dell'immagine Docker**, mai a runtime. Un nuovo stage nel `Dockerfile` (multi-stage, come lo stage `libraw` già esistente) prepara un CSV normalizzato pronto per `COPY` in Postgres; lo stage `runtime` finale (`gcr.io/distroless/cc-debian12:nonroot`) copia solo il CSV compilato, non gli strumenti di build. Il `docker-entrypoint` del container `db` (o una migrazione con `COPY FROM`) lo carica in `places` al primo avvio se la tabella è vuota.

Attenzione: lo stage `runtime` è **distroless** — nessuna shell, nessun `curl`/`wget` disponibile lì dentro. Il download e la normalizzazione vanno in uno stage intermedio con un'immagine base normale (come già fa `libraw`), non nel runtime.

**Casi limite:**
- Idempotenza: rilanciare l'import (nuova versione dell'immagine, GeoNames aggiornato) è un `UPSERT` su `id`, non un `INSERT` cieco che duplica.
- Nomi non-ASCII (es. "München", "北京"): `ascii_name` è la colonna trasogata per il trigram matching; `name` resta il nome originale per la visualizzazione.

- [ ] **Step 1: Test che falliscono** — `PlaceRepo::nearest`, `PlaceRepo::search` su un fixture piccolo (10-20 righe), non sull'intero dataset da 200k
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(db): add GeoNames-backed places table"
```

---

## Task 3: Geocoding — reverse e forward

**Files:**
- Create: `crates/keeppix-api/src/routes/places.rs`
- Modify: `crates/keeppix-api/src/routes/mod.rs` (`pub mod places;`)
- Modify: `crates/keeppix-api/src/lib.rs` (wiring nella catena `Router::new()...route(...)`, stesso pattern di `routes::share`)
- Modify: `crates/keeppix-db/src/places.rs` (metodi `nearest`, `search`)

**Reverse — coordinate → nome** (spec §5.2):

```sql
SELECT * FROM places
 WHERE ST_DWithin(location, $point, $radius_by_population)
 ORDER BY location <-> $point
 LIMIT 1;
```

Il correttivo non negoziabile: **la soglia di distanza è ponderata sulla popolazione**, non fissa. Senza, una foto scattata in mezzo alle Alpi prende il nome del primo paesino a valle — tecnicamente il più vicino, praticamente sbagliato. Un paese di 600 abitanti vale entro ~3 km, una città di 500.000 entro ~25 km; fuori soglia si ripiega su `admin1`/`countryInfo`. Un test dedicato pinna questo esatto scenario (punto equidistante tra un borgo minuscolo e vicino e una città grande e lontana → vince la città).

**Forward con autocomplete** (spec §5.3):

```sql
SELECT *, similarity(ascii_name, $query) AS sim
  FROM places
 WHERE ascii_name % $query   -- pg_trgm
 ORDER BY sim DESC,
          -- vicinanza ai luoghi già frequentati da questo utente,
          -- PRIMA della popolazione grezza — vedi sotto
          ...
 LIMIT 10;
```

**"Vicinanza ai luoghi già frequentati"** richiede una fonte: la posizione media (o un centroid) delle foto già geolocalizzate dell'utente/libreria. Non è nello schema `places` — va calcolato al volo o cachato. Per la v1: centroid delle ultime N assegnazioni manuali dell'utente (query su `asset_overrides.location WHERE updated_by = $me ORDER BY updated_at DESC LIMIT 50`), usato per boostare risultati entro raggio nel ranking. Non bloccante se vuoto: ripiega su popolazione pura.

**Endpoint:**
```
GET /api/v1/places/reverse?lat=&lon=          → PlaceView | 404 se fuori da ogni soglia
GET /api/v1/places/suggest?q=&near_user=true  → Vec<PlaceView>, max 10
```

**Casi limite:**
- `q` sotto i 2 caratteri: 400, non una query trigram su tutta la tabella.
- Due località omonime in paesi diversi ("Sorrento, Campania" vs "Sorrento Valley, California") — l'ordinamento per popolazione **e** vicinanza ai luoghi frequentati è il criterio che le distingue, non serve disambiguazione esplicita da parte dell'utente se il ranking è giusto; un test verifica che con 4000 foto in Campania la query "sorren" restituisca quella italiana per prima.
- Reverse su un punto in mezzo all'oceano: nessun risultato entro soglia → 404 pulito, il chiamante (Task 4) ripiega su "nessun nome, solo coordinate".

- [ ] **Step 1: Test che falliscono** (i tre scenari sopra + il caso Alpi/paesino)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): reverse and forward geocoding against places"
```

---

## Task 4: Assegnare una posizione

**Files:**
- Create: `crates/keeppix-api/src/routes/geotag.rs` (o estensione di `metadata.rs` esistente da Fase 2)
- Modify: `crates/keeppix-db/src/overrides.rs` (`OverrideRepo::apply_batch` già esiste da Fase 2 — riusarlo, non duplicare: assegnare una posizione **è** un `OverridePatch { location: Some(Some(point)), place_id: Some(Some(id)), .. }`)
- Create: `crates/keeppix-media/src/gpx.rs`
- Create: `crates/keeppix-jobs/src/geotag.rs` (matching GPX per timestamp)

**I quattro modi di geolocalizzare (spec §5.4):**

1. **Cerca e assegna** — usa `/places/suggest` (Task 3) + `OverrideRepo::apply_batch` (Fase 2, già esiste) su una selezione, anche 5.000 foto. Nessun endpoint nuovo: è già coperto dal batch-override esistente, il Task 4 aggiunge solo `place_id` al patch.
2. **Trascina il pin sulla mappa** — stesso `apply_batch`, `location_source = 'map_pin'`, nessun `place_id` (coordinata libera, non legata a una riga `places`).
3. **Copia da un'altra foto** — legge `effective().location` della foto sorgente, lo applica come override alle foto selezionate, `location_source = 'copied'`.
4. **Import GPX** — il job produce `Vec<(asset_id, GeoPoint)>` per interpolazione lineare sui timestamp, poi chiama la stessa funzione di geotag batch. `location_source = 'gpx'` (valore già nel CHECK SQL dalla Fase 1a).

**Parsing GPX e interpolazione:**
- Un track GPX è una sequenza `(timestamp, lat, lon)`. Per ogni asset con `taken_at_utc` compreso tra due punti consecutivi del track, si interpola linearmente in base al tempo trascorso.
- **Tolleranza configurabile**: se un asset cade fuori dal range coperto dal GPX (prima del primo punto o dopo l'ultimo) oltre una soglia (default: 5 minuti), non si geolocalizza — niente estrapolazione silenziosa che inventa una posizione.
- Un test pinna: due asset a 2 minuti dai due estremi del track vengono estrapolati entro tolleranza; un terzo a 20 minuti oltre l'ultimo punto resta senza posizione.

**Casi limite:**
- Assegnare una posizione a foto che hanno già `location_source = 'exif'`: sovrascrive, è una correzione esplicita dell'utente — diverso dal caso Task 1 (l'ingest non sovrascrive mai `'user'`/`'map_pin'`, ma un'azione utente esplicita sovrascrive qualunque fonte).
- **L'assegnazione non è mai bloccata dalla mancanza delle tile** (vincolo globale) — il test end-to-end di questo task non deve montare nessuna regione PMTiles per passare.

- [ ] **Step 1: Test che falliscono** (i 4 modi + i due casi limite + interpolazione GPX)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): assign locations by search, pin, copy, and GPX import"
```

---

## Task 5: Correzione del fuso orario

**Files:**
- Create: `crates/keeppix-db/migrations/0016_tz_boundaries.sql`
- Create: `crates/keeppix-db/src/geo.rs` (`GeoRepo::timezone_for`)
- Create: `crates/keeppix-jobs/src/geotag.rs` (estensione: `RecalculateTimezones` con anteprima)
- Modify: `Dockerfile` (confini fusi cotti nell'immagine, ~8 MB, stesso stage di `places`)

**Il problema** (spec §6): una reflex scrive "14:00" senza fuso. Su un server italiano, una foto scattata a Tokyo finisce in timeline alle 06:00, mescolata male con le foto del telefono (che il fuso ce l'hanno).

```sql
CREATE TABLE tz_boundaries (
    tz_name  text PRIMARY KEY,        -- es. 'Asia/Tokyo'
    boundary geography(MultiPolygon, 4326) NOT NULL
);
CREATE INDEX tz_boundaries_gist ON tz_boundaries USING gist (boundary);
```

```sql
SELECT tz_name FROM tz_boundaries
 WHERE ST_Contains(boundary::geometry, $point::geometry)
 LIMIT 1;
```

**Attivo di default**, con salvaguardia obbligatoria: l'azione "ricalcola fusi orari" su una libreria già catalogata

1. calcola il delta per ogni asset con GPS ma senza `location_source` recente,
2. mostra un'anteprima (`"1.847 foto cambierebbero data, esempio: DSC_4412.ARW 06:12 → 14:12"`),
3. applica solo su conferma esplicita, come un `metadata_batches` (Fase 2) — **riusa lo stesso meccanismo di undo batch**, non uno nuovo.

**Per le foto senza GPS**: azione manuale "sposta di N ore" sulla selezione — non serve tabella nuova, è un `OverridePatch.taken_at` con un delta, già coperto da `apply_batch`.

**Casi limite:**
- Un punto esattamente su un confine di fuso (rarissimo ma capita su isole/enclave): `ST_Contains` restituisce **zero o un solo** risultato per costruzione dei poligoni semplificati — un test verifica che non si vada mai in errore per doppio match, e che zero match (punto in mare aperto senza poligono) lasci `taken_at_utc` invariato, non lo azzeri.
- Il ricalcolo su un batch già ricalcolato è idempotente (stesso principio dell'`undo_batch` di Fase 2).

- [ ] **Step 1: Test che falliscono**
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(jobs): correct capture timestamps from GPS timezone boundaries"
```

---

## Task 6: Cluster sulla mappa

**Files:**
- Create: `crates/keeppix-api/src/routes/map.rs`
- Create: `crates/keeppix-db/src/geo.rs` (estensione: `GeoRepo::clusters`)

**Aggregazione a griglia lato server** (spec §4, invariata):

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

`<scope di visibilità>` **non è un filtro nuovo**: è la stessa funzione/join che già applica `PermissionRepo` alle altre query di libreria (timeline, ricerca). Il Task 6 non introduce una scorciatoia — un test verifica esplicitamente che un utente senza permesso su una cartella non veda i suoi cluster, con lo stesso harness dei test di permessi di Fase 3.

**Endpoint:**
```
GET /api/v1/map/clusters?bbox=&zoom=&scope=library|album|folder|search&scope_id=
```
`scope`/`scope_id` sono lo stesso parametro applicabile a qualsiasi contesto (spec §4.1) — un solo endpoint, non uno per timeline e uno per album.

**Sopra zoom 14** si restituiscono punti singoli, non celle — con un tetto configurabile (es. max 500 punti, oltre si torna a cluster anche a zoom alto, per non spedire un payload enorme su una città densa di scatti).

**Interazioni frontend** (per Task 8, ma l'endpoint deve supportarle):
- Disegna un'area → filtro timeline: il bbox del poligono disegnato diventa lo stesso parametro `bbox` usato per i cluster, la timeline lo consuma come filtro aggiuntivo.
- Modalità heatmap: stessa aggregazione, il frontend cambia solo il rendering.

**Casi limite:**
- `bbox` che attraversa l'antimeridiano (longitudine 180°/-180°): `ST_MakeEnvelope` con `xmin > xmax` va gestito esplicitamente (split in due query o normalizzazione), altrimenti l'area coperta è quella sbagliata (il complemento del mondo invece della fetta stretta vicino al meridiano). Test dedicato con un bbox reale su questo caso (es. Figi).
- Risposta target 5-15 KB in 3-9 ms (spec) — un test di performance con qualche migliaio di asset sintetici pinna un budget, non il numero esatto.

- [ ] **Step 1: Test che falliscono** (permessi, antimeridiano, tetto sopra zoom 14)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): grid-clustered map endpoint scoped by permission"
```

---

## Task 7: Tile PMTiles e gestore regioni

**Files:**
- Create: `crates/keeppix-api/src/routes/map.rs` (estensione: serving range-request delle `.pmtiles`)
- Create: `crates/keeppix-db/migrations/0017_map_regions.sql`
- Create: `crates/keeppix-db/src/regions.rs`
- Create: `crates/keeppix-jobs/src/regions.rs` (download riprendibile, verifica checksum)
- Create: `crates/keeppix-api/src/routes/regions.rs`

```sql
CREATE TABLE map_regions (
    id            text PRIMARY KEY,       -- es. 'IT', 'GR', o un'area disegnata → uuid
    label         text NOT NULL,
    file_path     text NOT NULL,
    size_bytes    bigint NOT NULL,
    version       text NOT NULL,
    downloaded_at timestamptz,
    status        text NOT NULL CHECK (status IN ('available','downloading','error')),
    source_url    text NOT NULL,          -- deve matchare l'allowlist, mai libero
    checksum_sha256 text
);
```

**Serving:** `GET /api/v1/map/tiles/{region}/{z}/{x}/{y}` legge da `data/maps/{region}.pmtiles` con `Range` header — non decomprime l'intero file, il formato PMTiles è progettato per range request dirette. Nessuna libreria di tile server, nessun rendering lato server: MapLibre nel browser interpreta i vector tile.

**Download regioni:**
- `source_url` è validato contro un'**allowlist fissa di host** hardcoded nel binario (non in un file di config leggibile/scrivibile da chi non ha accesso al codice) — zero SSRF, il vincolo globale della fase.
- Riprendibile: il job salva l'offset scaricato, un riavvio riparte da lì (`Range: bytes={offset}-`), non da zero.
- Verificato con checksum SHA-256 pubblicato insieme alla regione, prima di marcare `status = 'available'`.
- Interrompibile: l'utente può annullare un download in corso, il file parziale viene ripulito.

**Casi limite:**
- Cancellare una regione mentre MapLibre ha ancora tile in cache nel browser: il file sparisce dal disco, richieste successive per quella regione tornano 404 pulito — il frontend deve gestirlo mostrando "regione non più disponibile", non un errore generico.
- Due download della stessa regione in parallelo (doppio click): il secondo trova `status = 'downloading'` e si accoda/rifiuta, non parte una seconda scrittura sullo stesso file.
- Disco pieno a metà download: l'errore va marcato `status = 'error'` con un messaggio leggibile, il file parziale ripulito — non deve restare un `.pmtiles` corrotto che MapLibre prova a leggere.

- [ ] **Step 1: Test che falliscono** (allowlist violata → rifiutato, ripresa da offset, checksum invalido → non marcato available)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(jobs): resumable PMTiles region download with checksum verification"
```

---

## Task 8: Frontend — vista Mappa

**Files:**
- Create: `frontend/src/views/MapView.vue`
- Create: `frontend/src/views/settings/MapsOfflineView.vue`
- Create: `frontend/src/components/MapClusterLayer.vue`, `PlacePicker.vue`
- Create: `frontend/src/stores/maps.ts`
- Modify: `frontend/src/router.ts` — il commento alla riga 12 anticipa esattamente questo: `{ path: '/map', component: () => import('@/views/MapView.vue'), meta: { auth: true } }`

**Chunk pigro, non negoziabile:** MapLibre (~230 KB gzip) non deve mai finire nel bundle iniziale — il budget dei 150 KB verificato in CI (`.github/workflows/ci.yml`, step "Budget del bundle iniziale") copre solo ciò che `index.html` carica subito. Il pattern `component: () => import(...)` è già lo standard di ogni vista in questo router; `MapView` lo segue, non introduce un pattern nuovo.

**Stato vuoto:** zero regioni scaricate → la vista mostra il catalogo regioni con le dimensioni (riusando `MapsOfflineView`), non una mappa grigia. I cluster (Task 6) funzionano comunque, sovrapposti a uno sfondo assente — è la conseguenza diretta di `places`/PMTiles come dataset indipendenti.

**Interazioni:**
- Click su cluster → zoom; click su punto singolo → apre il visualizzatore foto esistente (riuso, non una lightbox nuova).
- Disegna un'area sulla mappa → naviga a timeline con un filtro bbox attivo (il `scope`/`bbox` del Task 6).
- Mini-mappa nel pannello dettagli di una foto ("vedi le altre foto scattate qui") — riusa `MapClusterLayer` in piccolo, stesso endpoint con bbox stretto attorno al punto.
- Gestore regioni (`MapsOfflineView`): lista scaricate/disponibili, barra di progresso per download in corso, cancellazione, aggregati per continente come nel mockup della spec (§2).

**Casi limite:**
- Ricerca di una località fuori da ogni regione scaricata (spec §3): il banner "Mappa non disponibile per questa zona" con `[Applica] [Scarica Regione]` — l'assegnazione della posizione procede comunque su `[Applica]`, il download è opzionale e in background.
- Cambio di tema chiaro/scuro: MapLibre supporta stili distinti — verificare che il passaggio di tema (toggle già esistente nell'app) ricarichi lo stile della mappa, non lasci una mappa chiara su sfondo scuro.

- [ ] **Step 1: Test component (Vitest) per MapClusterLayer/PlacePicker/MapsOfflineView** — mockare l'API, non montare MapLibre reale nei test unitari (troppo pesante/instabile in jsdom); un test e2e separato (se esiste una suite Playwright nel repo, altrimenti fuori scope di questo task) copre il rendering reale.
- [ ] **Step 2-4: Fallimento, implementazione, verifica** — `npx vitest run`, `npx vue-tsc --noEmit`, `npm run build` + verifica manuale del budget bundle
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(frontend): add lazy-loaded map view with offline region manager"
```

---

## Task 9: Privacy — il geofence di "casa" (chiude il debito di Fase 3)

**Files:**
- Modify: `crates/keeppix-api/src/routes/share.rs` (dove oggi `hide_metadata` zera `taken_at_utc`)
- Modify: `crates/keeppix-domain` — `AssetView` guadagna `location: Option<GeoPoint>` **in questo stesso task**, non prima
- Create: colonna impostazione utente "casa" (punto + raggio in metri) — probabilmente su `users` o una nuova tabella `user_settings`, da decidere in implementazione guardando lo schema utenti attuale

**Il debito esatto** (commit `ebb2e3b`, Fase 3): la spec §6.2 vuole un raggio configurabile attorno a un punto "casa"; le foto scattate lì **appaiono senza coordinate** nei contenuti condivisi (omesse, non zerate — diverso da `hide_metadata` che zera la data). Non era una fuga attiva perché `AssetView` non esponeva ancora `lat`/`lon`. **Il giorno in cui questo task aggiunge coordinate a un payload pubblico, il geofence deve essere nello stesso commit** — altrimenti è esattamente la fuga che il rinvio aveva previsto e documentato.

```sql
CREATE TABLE user_home_locations (
    user_id  uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    location geography(Point, 4326) NOT NULL,
    radius_m integer NOT NULL DEFAULT 200
);
```

**Logica:** prima di serializzare `location` in un payload pubblico (link condiviso, non nell'app autenticata), se `ST_DWithin(asset.location, home.location, home.radius_m)` per il proprietario della libreria, il campo `location`/`place_id` è omesso dalla risposta — non `null` esplicito che comunque rivela "c'era qualcosa qui", proprio assente dal JSON, coerente con come `hide_metadata` già omette (non zera con un valore fittizio) `taken_at_utc`.

**Sui link pubblici con `hide_metadata`**, la mappa non compare affatto e le coordinate non escono nemmeno dall'API (vincolo esistente, riconfermato — non un comportamento nuovo).

**Casi limite:**
- Nessuna casa configurata: nessun geofence applicato, comportamento identico a oggi (nessuna regressione per chi non configura nulla).
- Un asset esattamente sul bordo del raggio (`ST_DWithin` è inclusivo): un test pinna il comportamento al bordo, non solo dentro/fuori con ampio margine.
- Il proprietario della libreria non è chi ha impostato la posizione (biblioteca condivisa tra più utenti): il geofence è per **proprietario della libreria**, non per il viewer del link — un test copre questo, perché è facile confondere "casa di chi guarda" con "casa di chi possiede le foto".

- [ ] **Step 1: Test che falliscono** (i tre casi sopra + verifica che un link `hide_metadata` non esponga comunque nulla)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): omit coordinates near home on public share links"
```

---

## Criteri di completamento della Fase 4

- [ ] Un file con GPS reale, ingerito da zero, ha `assets.location` valorizzato senza alcuna azione manuale (Task 1).
- [ ] Assegnare "Kyoto" a 400 foto funziona con **zero regioni PMTiles scaricate** (Task 2-4).
- [ ] La mappa mostra cluster con miniatura di copertina, rispettando gli stessi permessi della timeline — verificato con un utente senza accesso a una cartella (Task 6).
- [ ] Un fuso orario viene proposto in anteprima prima di essere applicato, e l'anteprima è annullabile in blocco dopo l'applicazione (Task 5).
- [ ] Scaricare e cancellare una regione funziona dal browser, riprendibile a metà download (Task 7-8).
- [ ] **Nessuna richiesta di rete verso l'esterno** durante l'intero flusso sopra — verificato con un test che intercetta/nega ogni connessione in uscita durante la suite e2e, non solo a occhio.
- [ ] Una foto entro il raggio di "casa" non espone coordinate su un link pubblico — verificato da un test, non da ispezione manuale (Task 9).
- [ ] `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `npx vue-tsc --noEmit`, `npx vitest run`, `npm run build` con budget bundle rispettato — tutti verdi.

## Cosa NON è in Fase 4

- **Nominatim** per indirizzi civici e POI: pesante e sconsigliato su ARM; GeoNames copre il 95% del bisogno reale di un archivio fotografico personale.
- **Ricerca semantica dei luoghi** ("mostrami le foto vicino al mare" senza tag espliciti) — fuori dalla v1.
- **Clustering per similarità visiva** sulla mappa (raggruppare per soggetto oltre che per posizione) — fuori dalla v1.
- **Editing dei confini amministrativi o dei fusi** — i dataset (GeoNames, tz boundaries) sono trattati come sola lettura, mai modificabili dall'utente.

## Debiti che questa fase salda

- **Home-radius geofence** (Fase 3, commit `ebb2e3b`, spec §6.2) → Task 9.
- **GPS all'ingest** — non era formalmente un debito dichiarato altrove, ma la spec di Fase 4 lo dava per scontato come "consumato dalla Fase 1"; la verifica sul codice mostra che non è mai stato implementato → Task 1.
