export type IsoCmp = 'gt' | 'gte' | 'lt' | 'lte' | 'eq'

/** AST della ricerca (spec fase-10 §23-25): mai costruito da una sintassi
 * digitata (Task 9, `frontend/src/search/parse.ts`, ritirato in questo
 * task — vedi `SearchView.vue`) ma solo da pillole strutturate + un nodo
 * `text` per la descrizione libera, esattamente come nel mockup. Ogni
 * variante rispecchia `SearchNode` di `crates/keeppix-db/src/search.rs`
 * (`#[serde(tag="op", rename_all="snake_case")]`): questo file non è che
 * il sottoinsieme che la barra di ricerca sa produrre, non l'intero enum
 * del backend (che ha anche `Rating`/`Pick`/`DateRange`/`Day`/`Month`/
 * `Aperture`/`Shutter`/`Place`/`Category`/`Semantic`/`Person`/
 * `PersonGroup`/`PersonCount` — fuori campo per Task 9, altre schermate). */
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
