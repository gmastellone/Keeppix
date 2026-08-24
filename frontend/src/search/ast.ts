export type IsoCmp = 'gt' | 'gte' | 'lt' | 'lte' | 'eq'

/** Voto di culling (spec fase-2/9): rispecchia `keeppix_domain::Pick`
 * (`#[serde(rename_all="snake_case")]`, `{None,Pick,Reject}`). */
export type PickValue = 'none' | 'pick' | 'reject'

/** AST della ricerca (spec fase-10 §23-25): mai costruito da una sintassi
 * digitata (Task 9, `frontend/src/search/parse.ts`, ritirato in questo
 * task — vedi `SearchView.vue`) ma solo da pillole strutturate + un nodo
 * `text` per la descrizione libera, esattamente come nel mockup. Ogni
 * variante rispecchia `SearchNode` di `crates/keeppix-db/src/search.rs`
 * (`#[serde(tag="op", rename_all="snake_case")]`): questo file non è che
 * il sottoinsieme che la barra di ricerca, la creazione album (Task 12
 * 2/N, `Rating`/`Pick`/`DateRange`) e la griglia/dettaglio Persone (Task
 * 16 1/N, `Person`) sanno produrre, non l'intero enum del backend (che ha
 * anche `Day`/`Month`/`Aperture`/`Shutter`/`Place`/`Category`/`Semantic`/
 * `PersonGroup`/`PersonCount` — restano fuori campo, altre schermate). */
export type SearchNode =
  | { op: 'and'; args: SearchNode[] }
  | { op: 'or'; args: SearchNode[] }
  | { op: 'not'; arg: SearchNode }
  | { op: 'text'; value: string }
  | { op: 'type'; value: string }
  | { op: 'camera'; value: string }
  | { op: 'lens'; value: string }
  | { op: 'iso'; cmp: IsoCmp; value: number }
  | { op: 'year'; value: number }
  | { op: 'folder'; id: string }
  | { op: 'has_gps' }
  /** `SearchNode::Favorite` nel backend: variante unitaria, serializzata
   * come `{op:'favorite'}` da sola. Costruita a mano dalla vista Preferiti
   * (Task 7) e dal chip "Preferiti" della barra di ricerca (Task 9). */
  | { op: 'favorite' }
  /** Pillola `tag` (§24.2): `SearchNode::Tag{id}` nel backend — solo tag
   * **confermati** (`state='confirmed'`), mai proposte IA in attesa. */
  | { op: 'tag'; id: string }
  /** Pillola `country` (§24.2): `SearchNode::Country{value}`, confronto
   * esatto (case-insensitive lato backend) sul codice paese ISO di
   * `places.country_code` — non un nome leggibile, vedi `SearchView.vue`
   * per il perché non esiste una tabella di traduzione codice→nome. */
  | { op: 'country'; value: string }
  /** Task 12 (2/N), campo "Valutazione minima" della creazione album:
   * `SearchNode::Rating{cmp,value}` nel backend, per-utente, `IsoCmp`
   * riusato (stesso confronto numerico di `Iso`) — sempre `cmp:'gte'` da
   * qui, "valutazione minima" non è un intervallo. */
  | { op: 'rating'; cmp: IsoCmp; value: number }
  /** Campo "Pick/Scarta": `SearchNode::Pick{value}`, stato di culling
   * dell'utente che esegue la ricerca. */
  | { op: 'pick'; value: PickValue }
  /** Campo "Intervallo di date": `SearchNode::DateRange{from,to}`,
   * entrambi gli estremi inclusi, timestamp UTC. */
  | { op: 'date_range'; from: string; to: string }
  /** Fase 11 Task 16 (1/N): `SearchNode::Person{id}` nel backend — foto
   * con un volto **confermato** di questa persona (mai proposte in
   * attesa). Portato qui dentro l'ambito del file (era esplicitamente
   * "fuori campo" nel commento sopra, scritto prima che esistesse un
   * consumatore reale): la griglia Persone e il dettaglio persona lo
   * usano per "le foto di questa persona" — `photosForPerson()` del
   * documento (§32) — non esiste altra rotta che lo calcoli. */
  | { op: 'person'; id: string }
