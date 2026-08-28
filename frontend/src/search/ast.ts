export type IsoCmp = 'gt' | 'gte' | 'lt' | 'lte' | 'eq'

/** Culling verdict: mirrors `keeppix_domain::Pick`
 * (`#[serde(rename_all="snake_case")]`, `{None,Pick,Reject}`). */
export type PickValue = 'none' | 'pick' | 'reject'

/** Search AST: never built from typed syntax — only from structured pills
 * plus a `text` node for free-text description, exactly as in the mockup.
 * Each variant mirrors `SearchNode` from `crates/keeppix-db/src/search.rs`
 * (`#[serde(tag="op", rename_all="snake_case")]`): this file is only the
 * subset that the search bar, album creation (`Rating`/`Pick`/`DateRange`),
 * and the People grid/detail view (`Person`) know how to produce — not the
 * backend's full enum (which also has
 * `Day`/`Month`/`Aperture`/`Shutter`/`Place`/`Category`/`Semantic`/
 * `PersonGroup`/`PersonCount` — those are out of scope, used by other
 * screens). */
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
  /** `SearchNode::Favorite` in the backend: a unit variant, serialized as
   * `{op:'favorite'}` alone. Built by hand from the Favorites view and the
   * "Favorites" chip in the search bar. */
  | { op: 'favorite' }
  /** `tag` pill: `SearchNode::Tag{id}` in the backend — only **confirmed**
   * tags (`state='confirmed'`), never pending AI suggestions. */
  | { op: 'tag'; id: string }
  /** `country` pill: `SearchNode::Country{value}`, an exact
   * (case-insensitive on the backend) comparison against the ISO country
   * code in `places.country_code` — not a human-readable name; see
   * `SearchView.vue` for why there's no code→name translation table. */
  | { op: 'country'; value: string }
  /** The "Minimum rating" field in album creation:
   * `SearchNode::Rating{cmp,value}` in the backend, per-user, reusing
   * `IsoCmp` (same numeric comparison as `Iso`) — always `cmp:'gte'` from
   * here, since "minimum rating" isn't a range. */
  | { op: 'rating'; cmp: IsoCmp; value: number }
  /** The "Pick/Reject" field: `SearchNode::Pick{value}`, the culling state
   * set by the user running the search. */
  | { op: 'pick'; value: PickValue }
  /** The "Date range" field: `SearchNode::DateRange{from,to}`, both ends
   * inclusive, UTC timestamps. */
  | { op: 'date_range'; from: string; to: string }
  /** `SearchNode::Person{id}` in the backend — photos with a **confirmed**
   * face of this person (never pending suggestions). Used by the People
   * grid and person detail view for "photos of this person" — no other
   * route computes that. */
  | { op: 'person'; id: string }
