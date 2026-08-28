//! Search from a JSON AST. The user string never enters the SQL directly: only binds.

use chrono::{DateTime, NaiveDate, Utc};
use keeppix_domain::{AuthContext, Pick};
use serde::{Deserialize, Serialize};

use crate::assets::A_COLUMNS;
use crate::embeddings::vector_literal;
use crate::stacks::{
    AssetStackRow, AssetWithStack, STACK_BADGE_COLUMNS_SQL, STACK_BADGE_JOIN_SQL,
    STACK_PRIMARY_ONLY_SQL,
};
use crate::visibility::VisibilityScope;
use crate::{Db, DbError};

pub struct SearchRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsoCmp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

// No `Eq`: `Aperture`/`Shutter` carry an `f32`, which does not implement it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SearchNode {
    And {
        args: Vec<SearchNode>,
    },
    Or {
        args: Vec<SearchNode>,
    },
    Not {
        arg: Box<SearchNode>,
    },
    Text {
        value: String,
    },
    Type {
        value: String,
    },
    Camera {
        value: String,
    },
    Lens {
        value: String,
    },
    Iso {
        cmp: IsoCmp,
        value: i32,
    },
    Year {
        value: i32,
    },
    Folder {
        id: uuid::Uuid,
    },
    HasGps,
    /// Vote of the user running the search (per-user: your 5 stars is not
    /// someone else's 5 stars). `IsoCmp` reused: it is the same numeric
    /// comparison as `Iso`, not a second enum for the same purpose.
    Rating {
        cmp: IsoCmp,
        value: i32,
    },
    /// The "Favorites" chip: same per-user scheme as `Rating`.
    Favorite,
    /// Culling status of the user running the search, same per-user
    /// scheme as `Rating`/`Favorite`: `asset_flags.pick` is read here for
    /// the first time by a search — filtering by folder **and** status is
    /// what lets someone clean up a lot after working through it.
    Pick {
        value: Pick,
    },
    /// Explicit range, both ends inclusive.
    DateRange {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
    /// Day of the month (1..=31), independent of month and year — the
    /// recurring counterpart of `Year`/`Month`.
    Day {
        value: i32,
    },
    /// Month of the year (1..=12), independent of year.
    Month {
        value: i32,
    },
    /// Country via `assets.place_id -> places.country_code`. **Not**
    /// `Folder`: in the real product, folder and place are two different
    /// concepts even though they coincided in the prototype.
    Country {
        value: String,
    },
    Aperture {
        cmp: IsoCmp,
        value: f32,
    },
    /// Shutter time in seconds (`1/125` -> `0.008`): `asset_exif.exposure`
    /// is raw EXIF text, converted to a number in the query itself.
    Shutter {
        cmp: IsoCmp,
        value: f32,
    },
    /// A place from the `places` catalog, not a folder.
    Place {
        id: i64,
    },
    /// Tag confirmed on `asset_tags`. Only `state='confirmed'`.
    Tag {
        id: uuid::Uuid,
    },
    /// Category: any child tag (`tags.parent_id`) confirmed on the asset.
    Category {
        id: uuid::Uuid,
    },
    /// CLIP neighbors: membership in the top-K most similar (visible)
    /// results, still ordered by date. `embedding` is filled in by the
    /// API layer (the db does not know about ort); absent -> `Conflict`
    /// at compile time.
    Semantic {
        query: String,
        limit: u32,
        #[serde(skip)]
        embedding: Option<Vec<f32>>,
    },
    /// The "Person" chip: photos in which a face assigned to this person
    /// appears (`faces.person_id`, never a pending proposal or a
    /// rejection — see `Faces::reject`, which clears `person_id` together
    /// with `rejected_at`, so the plain condition `person_id = id` is
    /// enough).
    Person {
        id: uuid::Uuid,
    },
    /// Photos in which **at least one** person of the group appears.
    PersonGroup {
        id: uuid::Uuid,
    },
    /// "Photos with at least N people" — counts the distinct people
    /// assigned on the asset, not the faces (two faces of the same
    /// person in the same photo do not count as two).
    PersonCount {
        cmp: IsoCmp,
        value: i32,
    },
}

pub(crate) enum SearchBind {
    Text(String),
    I32(i32),
    F32(f32),
    Uuid(uuid::Uuid),
    Ts(DateTime<Utc>),
    I64(i64),
}

impl<'a> SearchRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Returns only the primary of each stack, like the timeline: a
    /// stacked RAW+JPEG is one result, not two.
    ///
    /// If the AST requires `Semantic` at the top level (reachable only
    /// through `And`, never `Or`/`Not` — [`find_hoistable_semantic`]),
    /// delegates to [`Self::run_semantic_hoisted`]: the top-K CTE drives
    /// the join instead of being a filter applied afterward. This closes
    /// a known performance gap (`SearchRepo Semantic ~1.3-1.4s @ 200k —
    /// start from the top-K CTE instead of filtering the heap`).
    ///
    /// # Errors
    /// `Conflict` if the AST is too deeply nested; `Connection` if the
    /// query fails.
    pub async fn run(
        &self,
        ctx: &AuthContext,
        ast: &SearchNode,
        cursor: Option<(DateTime<Utc>, keeppix_domain::AssetId)>,
        limit: i64,
    ) -> Result<Vec<AssetWithStack>, DbError> {
        let limit = limit.clamp(1, 200);
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        match find_hoistable_semantic(ast).as_slice() {
            [node] => {
                self.run_semantic_hoisted(ctx, ast, node, cursor, limit, &scope)
                    .await
            }
            _ => self.run_plain(ctx, ast, cursor, limit, &scope).await,
        }
    }

    /// The original, unoptimized path: `Semantic` (if present) stays a
    /// filter `a.id = ANY(ARRAY(top-K))` applied to the `WHERE` — used
    /// only when [`find_hoistable_semantic`] does not find exactly one
    /// AND-required `Semantic` node (none, under `Or`/`Not`, or more than
    /// one): cases where forcing the `JOIN` from the CTE would not be
    /// correct.
    async fn run_plain(
        &self,
        ctx: &AuthContext,
        ast: &SearchNode,
        cursor: Option<(DateTime<Utc>, keeppix_domain::AssetId)>,
        limit: i64,
        scope: &VisibilityScope,
    ) -> Result<Vec<AssetWithStack>, DbError> {
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        // Same $1,$2,$3 reused in the Semantic subquery: we want the top K
        // among visible assets, not K globally and then filtered.
        let semantic_vis = scope.filter("vf.path", "vf.library_id", "va.id", 1);
        let mut param = 4_usize;
        let (clause, binds) = compile_for_sql(
            ast,
            &mut param,
            0,
            "a.location",
            ctx.user_id().map(|u| u.as_uuid()),
            Some(semantic_vis.sql()),
        )?;
        let time_p = next(&mut param);
        let id_p = next(&mut param);
        let limit_p = next(&mut param);
        let (cursor_time, cursor_id) = match cursor {
            Some((t, id)) => (Some(t), Some(id.as_uuid())),
            None => (None, None),
        };
        let sql = format!(
            "SELECT {A_COLUMNS}, {STACK_BADGE_COLUMNS_SQL} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_exif e ON e.asset_id = a.id \
             {STACK_BADGE_JOIN_SQL} \
             WHERE {} AND a.status = 'indexed' AND ({clause}) \
               AND (${time_p}::timestamptz IS NULL \
                    OR a.taken_at_utc < ${time_p} \
                    OR (a.taken_at_utc = ${time_p} AND a.id < ${id_p})) \
               AND {STACK_PRIMARY_ONLY_SQL} \
             ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
             LIMIT ${limit_p}",
            filter.sql()
        );
        let mut q = sqlx::query_as::<_, AssetStackRow>(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets());
        for b in &binds {
            q = bind_one(q, b);
        }
        let rows: Vec<AssetStackRow> = q
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .fetch_all(self.db.pool())
            .await?;
        rows.into_iter().map(AssetStackRow::into_domain).collect()
    }

    /// `semantic_node` is AND-required in every row of the result, so its
    /// membership condition can become a `JOIN` instead of a post-hoc
    /// filter. The `topk` CTE materializes at most 500 ids (same
    /// `IVFFlat` query as before, unchanged: `ORDER BY <=> LIMIT k`), then
    /// `assets`/`folders`/`asset_exif` join **onto just those ids** — a
    /// Nested Loop with at most 500 index lookups, not a scan that tests
    /// membership row by row in `taken_at_utc` order until it finds
    /// `limit` matches (the plan the planner used to pick with
    /// `a.id = ANY(ARRAY(...))` applied afterward, when the top-K ids are
    /// sparse in that order). The final `ORDER BY`/`LIMIT` only sorts
    /// those <=500 candidates, not the entire visible history.
    ///
    /// `ast` with `semantic_node` substituted by an empty `And` (->
    /// `TRUE`, [`substitute_with_true`]) is still compiled in full: the
    /// other AND-ed axes (`Tag`, `Camera`, ...) remain `WHERE` filters as
    /// always, only `Semantic` changes mechanism.
    async fn run_semantic_hoisted(
        &self,
        ctx: &AuthContext,
        ast: &SearchNode,
        semantic_node: &SearchNode,
        cursor: Option<(DateTime<Utc>, keeppix_domain::AssetId)>,
        limit: i64,
        scope: &VisibilityScope,
    ) -> Result<Vec<AssetWithStack>, DbError> {
        let SearchNode::Semantic {
            limit: k_limit,
            embedding,
            ..
        } = semantic_node
        else {
            unreachable!("find_hoistable_semantic only returns Semantic nodes")
        };
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let semantic_vis = scope.filter("vf.path", "vf.library_id", "va.id", 1);
        let mut param = 4_usize;
        let (mv_p, vec_p, k_p, cte_binds) =
            semantic_query_params(*k_limit, embedding.as_ref(), &mut param)?;
        let modified = substitute_with_true(ast, std::ptr::from_ref(semantic_node));
        let (clause, binds) = compile_for_sql(
            &modified,
            &mut param,
            0,
            "a.location",
            ctx.user_id().map(|u| u.as_uuid()),
            Some(semantic_vis.sql()),
        )?;
        let time_p = next(&mut param);
        let id_p = next(&mut param);
        let limit_p = next(&mut param);
        let (cursor_time, cursor_id) = match cursor {
            Some((t, id)) => (Some(t), Some(id.as_uuid())),
            None => (None, None),
        };
        let semantic_vis_sql = semantic_vis.sql();
        let filter_sql = filter.sql();
        // `MATERIALIZED`, not a plain `WITH`: from Postgres 12 onward the
        // planner can inline a CTE that is not referenced multiple times
        // inside the outer query when it looks cheaper — which here
        // recreates exactly the plan this function exists to avoid (a
        // `taken_at_utc` scan with row-by-row membership testing).
        // Measured: without `MATERIALIZED`, the exact same SQL alternated
        // between good plans (~170ms) and fallback ones (~2100ms) across
        // successive runs on the same 200k fixture — not an edge case, a
        // defect a single lucky measurement would have hidden.
        let sql = format!(
            "WITH topk AS MATERIALIZED ( \
               SELECT ae.asset_id \
               FROM asset_embeddings ae \
               JOIN assets va ON va.id = ae.asset_id \
               JOIN folders vf ON vf.id = va.folder_id \
               WHERE ae.model_version = ${mv_p} \
                 AND va.status = 'indexed' \
                 AND ({semantic_vis_sql}) \
               ORDER BY ae.embedding <=> ${vec_p}::vector \
               LIMIT ${k_p} \
             ) \
             SELECT {A_COLUMNS}, {STACK_BADGE_COLUMNS_SQL} FROM topk \
             JOIN assets a ON a.id = topk.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_exif e ON e.asset_id = a.id \
             {STACK_BADGE_JOIN_SQL} \
             WHERE {filter_sql} AND a.status = 'indexed' AND ({clause}) \
               AND (${time_p}::timestamptz IS NULL \
                    OR a.taken_at_utc < ${time_p} \
                    OR (a.taken_at_utc = ${time_p} AND a.id < ${id_p})) \
               AND {STACK_PRIMARY_ONLY_SQL} \
             ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
             LIMIT ${limit_p}"
        );
        let mut q = sqlx::query_as::<_, AssetStackRow>(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets());
        for b in &cte_binds {
            q = bind_one(q, b);
        }
        for b in &binds {
            q = bind_one(q, b);
        }
        let rows: Vec<AssetStackRow> = q
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .fetch_all(self.db.pool())
            .await?;
        rows.into_iter().map(AssetStackRow::into_domain).collect()
    }

    /// Typed suggestions for the search bar: the frontend needs to know
    /// *what type* each result is to build the right pill, and for a tag
    /// also the dot's color.
    ///
    /// `tag` has no source yet: the tags table did not exist when this
    /// enum was designed. The enum is nonetheless complete — it is the
    /// shape that needed fixing now, not the sources, otherwise it would
    /// change twice. The other six sources (`camera`, `filename`,
    /// `folder`, `iso`, `year`, `country`) read data already present.
    ///
    /// `country` reads `assets.place_id` directly, without `COALESCE`
    /// against `asset_overrides.place_id` — same choice as
    /// `SearchNode::Country` (that column is not written by any path yet).
    ///
    /// # Errors
    /// `Connection` if the query fails; `Corrupted` if the database
    /// returns a `kind` outside the expected closed set (should not
    /// happen: the `SELECT` list is literal, not user data).
    pub async fn suggest(&self, ctx: &AuthContext, q: &str) -> Result<Vec<Suggestion>, DbError> {
        let q = q.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let asset_filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let folder_filter = scope.filter_for_folder_aggregate("f.path", "f.library_id", "f.id", 1);
        let pattern = like_prefix(q);
        let sql = format!(
            "(SELECT 'camera' AS kind, e.camera_model AS value, e.camera_model AS label, \
                      NULL::text AS color \
                FROM asset_exif e \
                JOIN assets a ON a.id = e.asset_id \
                JOIN folders f ON f.id = a.folder_id \
               WHERE {asset_filter} AND e.camera_model ILIKE $4 ESCAPE E'\\\\' \
               LIMIT 6) \
             UNION \
             (SELECT 'filename', a.filename, a.filename, NULL::text \
                FROM assets a \
                JOIN folders f ON f.id = a.folder_id \
               WHERE {asset_filter} AND a.filename ILIKE $4 ESCAPE E'\\\\' \
               LIMIT 6) \
             UNION \
             (SELECT 'folder', f.id::text, f.name, NULL::text \
                FROM folders f \
               WHERE {folder_filter} AND f.name ILIKE $4 ESCAPE E'\\\\' \
               LIMIT 6) \
             UNION \
             (SELECT 'year', EXTRACT(YEAR FROM a.taken_at_utc)::int::text, \
                      EXTRACT(YEAR FROM a.taken_at_utc)::int::text, NULL::text \
                FROM assets a \
                JOIN folders f ON f.id = a.folder_id \
               WHERE {asset_filter} AND a.taken_at_utc IS NOT NULL \
                 AND EXTRACT(YEAR FROM a.taken_at_utc)::int::text ILIKE $4 ESCAPE E'\\\\' \
               LIMIT 6) \
             UNION \
             (SELECT 'iso', e.iso::text, e.iso::text, NULL::text \
                FROM asset_exif e \
                JOIN assets a ON a.id = e.asset_id \
                JOIN folders f ON f.id = a.folder_id \
               WHERE {asset_filter} AND e.iso IS NOT NULL \
                 AND e.iso::text ILIKE $4 ESCAPE E'\\\\' \
               LIMIT 6) \
             UNION \
             (SELECT 'country', p.country_code::text, p.country_code::text, NULL::text \
                FROM assets a \
                JOIN folders f ON f.id = a.folder_id \
                JOIN places p ON p.id = a.place_id \
               WHERE {asset_filter} AND p.country_code IS NOT NULL \
                 AND p.country_code::text ILIKE $4 ESCAPE E'\\\\' \
               LIMIT 6) \
             LIMIT 12",
            asset_filter = asset_filter.sql(),
            folder_filter = folder_filter.sql(),
        );
        let rows: Vec<SuggestionRow> = sqlx::query_as(&sql)
            .bind(asset_filter.bind())
            .bind(asset_filter.holes())
            .bind(asset_filter.assets())
            .bind(pattern)
            .fetch_all(self.db.pool())
            .await?;
        rows.into_iter().map(SuggestionRow::into_domain).collect()
    }

    /// # Errors
    /// `Forbidden` without a user; `Connection` if the query fails.
    pub async fn list_saved(&self, ctx: &AuthContext) -> Result<Vec<SavedSearch>, DbError> {
        let owner = ctx.user_id().ok_or(DbError::Forbidden)?;
        let rows: Vec<SavedSearchRow> = sqlx::query_as(
            "SELECT id, name, query_text, created_at \
               FROM saved_searches WHERE owner_id = $1 ORDER BY created_at DESC",
        )
        .bind(owner.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(SavedSearchRow::into_domain).collect())
    }

    /// # Errors
    /// `Forbidden` without a user; `Connection` if the insert fails.
    pub async fn create_saved(
        &self,
        ctx: &AuthContext,
        name: &str,
        query_text: &str,
    ) -> Result<SavedSearch, DbError> {
        let owner = ctx.user_id().ok_or(DbError::Forbidden)?;
        let row: SavedSearchRow = sqlx::query_as(
            "INSERT INTO saved_searches (id, owner_id, name, query_text) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, name, query_text, created_at",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(owner.as_uuid())
        .bind(name)
        .bind(query_text)
        .fetch_one(self.db.pool())
        .await?;
        Ok(row.into_domain())
    }

    /// Loads and parses a saved search belonging to the caller.
    ///
    /// # Errors
    /// `Forbidden` for unknown ids or ones belonging to another user;
    /// `Conflict` if the saved text can no longer be parsed.
    pub async fn saved_query(
        &self,
        ctx: &AuthContext,
        id: uuid::Uuid,
    ) -> Result<SearchNode, DbError> {
        let owner = ctx.user_id().ok_or(DbError::Forbidden)?;
        let query_text: Option<String> = sqlx::query_scalar(
            "SELECT query_text FROM saved_searches WHERE id = $1 AND owner_id = $2",
        )
        .bind(id)
        .bind(owner.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        let query_text = query_text.ok_or(DbError::Forbidden)?;
        parse_query_text(&query_text)
    }
}

/// Closed set: the frontend decides the pill based on this, so an
/// unexpected extra value would break the contract instead of degrading
/// gracefully.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    Tag,
    Camera,
    Folder,
    Iso,
    Year,
    Country,
    Filename,
}

impl SuggestionKind {
    fn parse(raw: &str) -> Result<Self, DbError> {
        match raw {
            "tag" => Ok(Self::Tag),
            "camera" => Ok(Self::Camera),
            "folder" => Ok(Self::Folder),
            "iso" => Ok(Self::Iso),
            "year" => Ok(Self::Year),
            "country" => Ok(Self::Country),
            "filename" => Ok(Self::Filename),
            other => Err(DbError::Corrupted(format!(
                "unknown search suggestion kind: {other}"
            ))),
        }
    }
}

/// A typed suggestion for the search bar. `value` is what feeds the
/// corresponding `SearchNode` if the user picks the pill (the folder id
/// for `Folder`, the text for the others); `label` is what is displayed.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub kind: SuggestionKind,
    pub value: String,
    pub label: String,
    pub color: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SuggestionRow {
    kind: String,
    value: String,
    label: String,
    color: Option<String>,
}

impl SuggestionRow {
    fn into_domain(self) -> Result<Suggestion, DbError> {
        Ok(Suggestion {
            kind: SuggestionKind::parse(&self.kind)?,
            value: self.value,
            label: self.label,
            color: self.color,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SavedSearch {
    pub id: uuid::Uuid,
    pub name: String,
    pub query_text: String,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct SavedSearchRow {
    id: uuid::Uuid,
    name: String,
    query_text: String,
    created_at: DateTime<Utc>,
}

impl SavedSearchRow {
    fn into_domain(self) -> SavedSearch {
        SavedSearch {
            id: self.id,
            name: self.name,
            query_text: self.query_text,
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    And,
    Or,
    Not,
    LeftParen,
    RightParen,
    Value(String),
    QuotedValue(String),
}

struct Parser {
    tokens: Vec<Token>,
    next: usize,
}

impl Parser {
    fn parse(mut self) -> Result<SearchNode, DbError> {
        if self.tokens.is_empty() {
            return Ok(SearchNode::And { args: Vec::new() });
        }
        let node = self.parse_or()?;
        if self.next == self.tokens.len() {
            Ok(node)
        } else {
            Err(invalid_saved_search())
        }
    }

    fn parse_or(&mut self) -> Result<SearchNode, DbError> {
        let mut args = vec![self.parse_and()?];
        while matches!(self.tokens.get(self.next), Some(Token::Or)) {
            self.next += 1;
            args.push(self.parse_and()?);
        }
        Ok(single_or_node(args))
    }

    fn parse_and(&mut self) -> Result<SearchNode, DbError> {
        let mut args = vec![self.parse_not()?];
        loop {
            match self.tokens.get(self.next) {
                Some(Token::And) => {
                    self.next += 1;
                    args.push(self.parse_not()?);
                }
                Some(Token::Or | Token::RightParen) | None => break,
                Some(_) => args.push(self.parse_not()?),
            }
        }
        Ok(single_and_node(args))
    }

    fn parse_not(&mut self) -> Result<SearchNode, DbError> {
        if matches!(self.tokens.get(self.next), Some(Token::Not)) {
            self.next += 1;
            return Ok(SearchNode::Not {
                arg: Box::new(self.parse_primary()?),
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<SearchNode, DbError> {
        let token = self
            .tokens
            .get(self.next)
            .cloned()
            .ok_or_else(invalid_saved_search)?;
        self.next += 1;
        match token {
            Token::LeftParen => {
                let node = self.parse_or()?;
                if !matches!(self.tokens.get(self.next), Some(Token::RightParen)) {
                    return Err(invalid_saved_search());
                }
                self.next += 1;
                Ok(node)
            }
            Token::Value(value) => Ok(value_node(&value)),
            Token::QuotedValue(value) => Ok(SearchNode::Text { value }),
            Token::And | Token::Or | Token::Not | Token::RightParen => Err(invalid_saved_search()),
        }
    }
}

fn parse_query_text(input: &str) -> Result<SearchNode, DbError> {
    Parser {
        tokens: tokenize(input)?,
        next: 0,
    }
    .parse()
}

fn tokenize(input: &str) -> Result<Vec<Token>, DbError> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at].is_whitespace() {
            at += 1;
            continue;
        }
        if chars[at] == '(' {
            tokens.push(Token::LeftParen);
            at += 1;
            continue;
        }
        if chars[at] == ')' {
            tokens.push(Token::RightParen);
            at += 1;
            continue;
        }
        if chars[at] == '"' {
            at += 1;
            let mut value = String::new();
            while at < chars.len() && chars[at] != '"' {
                value.push(chars[at]);
                at += 1;
            }
            if at == chars.len() {
                return Err(invalid_saved_search());
            }
            at += 1;
            tokens.push(Token::QuotedValue(value));
            continue;
        }

        let mut value = String::new();
        let mut quoted = false;
        while at < chars.len() {
            let ch = chars[at];
            if ch == '"' {
                quoted = !quoted;
                at += 1;
                continue;
            }
            if !quoted && (ch.is_whitespace() || ch == '(' || ch == ')') {
                break;
            }
            value.push(ch);
            at += 1;
        }
        if quoted || value.is_empty() {
            return Err(invalid_saved_search());
        }
        tokens.push(match value.to_ascii_lowercase().as_str() {
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            _ => Token::Value(value),
        });
    }
    Ok(tokens)
}

fn value_node(value: &str) -> SearchNode {
    if let Ok(year) = value.parse::<i32>()
        && value.len() == 4
    {
        return SearchNode::Year { value: year };
    }
    let Some((field, raw)) = value.split_once(':') else {
        return SearchNode::Text {
            value: value.to_owned(),
        };
    };
    match field.to_ascii_lowercase().as_str() {
        "type" => SearchNode::Type {
            value: raw.to_ascii_lowercase(),
        },
        "camera" => SearchNode::Camera {
            value: raw.to_owned(),
        },
        "lens" => SearchNode::Lens {
            value: raw.to_owned(),
        },
        "iso" => iso_node(value, raw),
        "folder" => uuid::Uuid::parse_str(raw).map_or_else(
            |_| SearchNode::Text {
                value: value.to_owned(),
            },
            |id| SearchNode::Folder { id },
        ),
        "has" if raw.eq_ignore_ascii_case("gps") => SearchNode::HasGps,
        _ => SearchNode::Text {
            value: value.to_owned(),
        },
    }
}

fn iso_node(original: &str, raw: &str) -> SearchNode {
    let (cmp, number) = if let Some(value) = raw.strip_prefix(">=") {
        (IsoCmp::Gte, value)
    } else if let Some(value) = raw.strip_prefix("<=") {
        (IsoCmp::Lte, value)
    } else if let Some(value) = raw.strip_prefix('>') {
        (IsoCmp::Gt, value)
    } else if let Some(value) = raw.strip_prefix('<') {
        (IsoCmp::Lt, value)
    } else {
        (IsoCmp::Eq, raw.strip_prefix('=').unwrap_or(raw))
    };
    number.parse::<i32>().map_or_else(
        |_| SearchNode::Text {
            value: original.to_owned(),
        },
        |value| SearchNode::Iso { cmp, value },
    )
}

fn single_or_node(mut args: Vec<SearchNode>) -> SearchNode {
    if args.len() == 1 {
        return args.remove(0);
    }
    SearchNode::Or { args }
}

fn single_and_node(mut args: Vec<SearchNode>) -> SearchNode {
    if args.len() == 1 {
        return args.remove(0);
    }
    SearchNode::And { args }
}

fn invalid_saved_search() -> DbError {
    DbError::Conflict("invalid saved search".to_owned())
}

fn bind_one<'q>(
    q: sqlx::query::QueryAs<'q, sqlx::Postgres, AssetStackRow, sqlx::postgres::PgArguments>,
    b: &'q SearchBind,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, AssetStackRow, sqlx::postgres::PgArguments> {
    match b {
        SearchBind::Text(s) => q.bind(s),
        SearchBind::I32(n) => q.bind(n),
        SearchBind::F32(n) => q.bind(n),
        SearchBind::Uuid(u) => q.bind(u),
        SearchBind::Ts(t) => q.bind(t),
        SearchBind::I64(n) => q.bind(n),
    }
}

/// `user_id` feeds the per-user axes (`Rating`, `Favorite`): `None` for an
/// `AuthContext` with no user (public link) makes those two nodes fail
/// with `Forbidden` instead of producing a silently empty comparison —
/// they make no sense without a user who votes.
///
/// `semantic_vis` is the visibility clause that reuses `$1,$2,$3` with
/// `vf`/`va` aliases (generated by `VisibilityScope::filter` in `run`).
/// `None` -> `TRUE` inside the subquery (album/map where the outer scope
/// is enough).
pub(crate) fn compile_for_sql(
    node: &SearchNode,
    param: &mut usize,
    depth: usize,
    gps_sql: &str,
    user_id: Option<uuid::Uuid>,
    semantic_vis: Option<&str>,
) -> Result<(String, Vec<SearchBind>), DbError> {
    if depth > 16 {
        return Err(DbError::Conflict("search too nested".to_owned()));
    }
    match node {
        SearchNode::And { args } if args.is_empty() => Ok(("TRUE".to_owned(), Vec::new())),
        SearchNode::And { args } => {
            join(args, " AND ", param, depth, gps_sql, user_id, semantic_vis)
        }
        SearchNode::Or { args } if args.is_empty() => Ok(("FALSE".to_owned(), Vec::new())),
        SearchNode::Or { args } => join(args, " OR ", param, depth, gps_sql, user_id, semantic_vis),
        SearchNode::Not { arg } => {
            let (inner, binds) =
                compile_for_sql(arg, param, depth + 1, gps_sql, user_id, semantic_vis)?;
            Ok((format!("NOT COALESCE(({inner}), FALSE)"), binds))
        }
        leaf => compile_leaf(leaf, param, gps_sql, user_id, semantic_vis),
    }
}

/// Every variant that is not a combinator (`And`/`Or`/`Not`): it does not
/// recurse, so it does not need `depth` — kept separate from
/// [`compile_for_sql`] only to stay under clippy's per-function line
/// limit. In turn delegates the newer axes to [`compile_search_axis`],
/// for the same reason.
fn compile_leaf(
    node: &SearchNode,
    param: &mut usize,
    gps_sql: &str,
    user_id: Option<uuid::Uuid>,
    semantic_vis: Option<&str>,
) -> Result<(String, Vec<SearchBind>), DbError> {
    match node {
        SearchNode::And { .. } | SearchNode::Or { .. } | SearchNode::Not { .. } => {
            unreachable!("combinators are handled by compile_for_sql")
        }
        SearchNode::Text { value } => {
            let p = next(param);
            Ok((
                format!("a.filename ILIKE ${p} ESCAPE E'\\\\'"),
                vec![SearchBind::Text(like_contains(value))],
            ))
        }
        SearchNode::Type { value } => {
            let kind = match value.as_str() {
                "image" | "raw_image" | "video" | "unknown" => value.clone(),
                "raw" => "raw_image".to_owned(),
                _ => return Ok(("FALSE".to_owned(), Vec::new())),
            };
            let p = next(param);
            Ok((format!("a.kind = ${p}"), vec![SearchBind::Text(kind)]))
        }
        SearchNode::Camera { value } => {
            let p = next(param);
            Ok((
                format!("e.camera_model ILIKE ${p} ESCAPE E'\\\\'"),
                vec![SearchBind::Text(like_contains(value))],
            ))
        }
        SearchNode::Lens { value } => {
            let p = next(param);
            Ok((
                format!("e.lens ILIKE ${p} ESCAPE E'\\\\'"),
                vec![SearchBind::Text(like_contains(value))],
            ))
        }
        SearchNode::Iso { cmp, value } => {
            let op = cmp_op(*cmp);
            let p = next(param);
            Ok((format!("e.iso {op} ${p}"), vec![SearchBind::I32(*value)]))
        }
        SearchNode::Year { value } => {
            let end_year = value
                .checked_add(1)
                .ok_or_else(|| DbError::Conflict("invalid year".to_owned()))?;
            let start = NaiveDate::from_ymd_opt(*value, 1, 1)
                .ok_or_else(|| DbError::Conflict("invalid year".to_owned()))?;
            let end = NaiveDate::from_ymd_opt(end_year, 1, 1)
                .ok_or_else(|| DbError::Conflict("invalid year".to_owned()))?;
            let p1 = next(param);
            let p2 = next(param);
            Ok((
                format!("a.taken_at_utc >= ${p1} AND a.taken_at_utc < ${p2}"),
                vec![
                    SearchBind::Ts(midnight(start)),
                    SearchBind::Ts(midnight(end)),
                ],
            ))
        }
        SearchNode::Folder { id } => {
            let p = next(param);
            Ok((
                format!("f.path <@ (SELECT path FROM folders WHERE id = ${p})"),
                vec![SearchBind::Uuid(*id)],
            ))
        }
        SearchNode::HasGps => Ok((format!("{gps_sql} IS NOT NULL"), Vec::new())),
        axis => compile_search_axis(axis, param, user_id, semantic_vis),
    }
}

/// The nine newer axis variants: none of them recurse, and none depend on
/// `gps_sql` — kept separate from [`compile_leaf`] only to stay under
/// clippy's per-function line limit.
fn compile_search_axis(
    node: &SearchNode,
    param: &mut usize,
    user_id: Option<uuid::Uuid>,
    semantic_vis: Option<&str>,
) -> Result<(String, Vec<SearchBind>), DbError> {
    match node {
        SearchNode::And { .. }
        | SearchNode::Or { .. }
        | SearchNode::Not { .. }
        | SearchNode::Text { .. }
        | SearchNode::Type { .. }
        | SearchNode::Camera { .. }
        | SearchNode::Lens { .. }
        | SearchNode::Iso { .. }
        | SearchNode::Year { .. }
        | SearchNode::Folder { .. }
        | SearchNode::HasGps => unreachable!("handled by compile_for_sql/compile_leaf"),
        SearchNode::Rating { cmp, value } => {
            let user = user_id.ok_or(DbError::Forbidden)?;
            let op = cmp_op(*cmp);
            let uid_p = next(param);
            let val_p = next(param);
            let sql = format!(
                "EXISTS (SELECT 1 FROM asset_flags af \
                 WHERE af.asset_id = a.id AND af.user_id = ${uid_p} AND af.rating {op} ${val_p})"
            );
            Ok((sql, vec![SearchBind::Uuid(user), SearchBind::I32(*value)]))
        }
        SearchNode::Favorite => {
            let user = user_id.ok_or(DbError::Forbidden)?;
            let uid_p = next(param);
            Ok((
                format!(
                    "EXISTS (SELECT 1 FROM asset_flags af \
                     WHERE af.asset_id = a.id AND af.user_id = ${uid_p} AND af.favorite)"
                ),
                vec![SearchBind::Uuid(user)],
            ))
        }
        SearchNode::Pick { value } => compile_pick_axis(*value, param, user_id),
        SearchNode::DateRange { from, to } => {
            let p1 = next(param);
            let p2 = next(param);
            Ok((
                format!("a.taken_at_utc >= ${p1} AND a.taken_at_utc <= ${p2}"),
                vec![SearchBind::Ts(*from), SearchBind::Ts(*to)],
            ))
        }
        SearchNode::Day { value } => {
            if !(1..=31).contains(value) {
                return Err(DbError::Conflict("invalid day".to_owned()));
            }
            let p = next(param);
            Ok((
                format!("EXTRACT(DAY FROM a.taken_at_utc)::int = ${p}"),
                vec![SearchBind::I32(*value)],
            ))
        }
        SearchNode::Month { value } => {
            if !(1..=12).contains(value) {
                return Err(DbError::Conflict("invalid month".to_owned()));
            }
            let p = next(param);
            Ok((
                format!("EXTRACT(MONTH FROM a.taken_at_utc)::int = ${p}"),
                vec![SearchBind::I32(*value)],
            ))
        }
        SearchNode::Country { value } => {
            let p = next(param);
            Ok((
                format!(
                    "EXISTS (SELECT 1 FROM places p \
                     WHERE p.id = a.place_id AND p.country_code = ${p})"
                ),
                vec![SearchBind::Text(value.to_uppercase())],
            ))
        }
        SearchNode::Aperture { cmp, value } => {
            let op = cmp_op(*cmp);
            let p = next(param);
            Ok((
                format!("e.f_number {op} ${p}"),
                vec![SearchBind::F32(*value)],
            ))
        }
        SearchNode::Shutter { cmp, value } => {
            let op = cmp_op(*cmp);
            let p = next(param);
            Ok((
                format!("({}) {op} ${p}", shutter_seconds_sql("e")),
                vec![SearchBind::F32(*value)],
            ))
        }
        SearchNode::Place { id } => {
            let p = next(param);
            Ok((format!("a.place_id = ${p}"), vec![SearchBind::I64(*id)]))
        }
        fase8 @ (SearchNode::Person { .. }
        | SearchNode::PersonGroup { .. }
        | SearchNode::PersonCount { .. }) => compile_fase8_axis(fase8, param),
        fase7 => compile_fase7_axis(fase7, param, semantic_vis),
    }
}

/// `SearchNode::Pick` — kept separate from `compile_search_axis` for the
/// same clippy line cap, not for a conceptual reason: `Pick::None` is not
/// "the literal value `'none'` in an existing row". Most assets have
/// never had a row written to `asset_flags` for this user (no column
/// default, "never evaluated" = no row). An `EXISTS ... pick = 'none'`
/// identical to the `Pick`/`Reject` branch would only find assets
/// explicitly reset to `None` by `set_pick`, silently excluding
/// everything never touched — the exact opposite of "to be reviewed".
/// `NOT EXISTS` on `pick IN ('pick', 'reject')` catches both cases at once.
fn compile_pick_axis(
    value: Pick,
    param: &mut usize,
    user_id: Option<uuid::Uuid>,
) -> Result<(String, Vec<SearchBind>), DbError> {
    let user = user_id.ok_or(DbError::Forbidden)?;
    let uid_p = next(param);
    match value {
        Pick::None => Ok((
            format!(
                "NOT EXISTS (SELECT 1 FROM asset_flags af \
                 WHERE af.asset_id = a.id AND af.user_id = ${uid_p} \
                   AND af.pick IN ('pick', 'reject'))"
            ),
            vec![SearchBind::Uuid(user)],
        )),
        Pick::Pick | Pick::Reject => {
            let val_p = next(param);
            Ok((
                format!(
                    "EXISTS (SELECT 1 FROM asset_flags af \
                     WHERE af.asset_id = a.id AND af.user_id = ${uid_p} AND af.pick = ${val_p})"
                ),
                vec![
                    SearchBind::Uuid(user),
                    SearchBind::Text(value.as_str().to_owned()),
                ],
            ))
        }
    }
}

/// Tag / Category / Semantic — kept separate for the clippy line cap.
fn compile_fase7_axis(
    node: &SearchNode,
    param: &mut usize,
    semantic_vis: Option<&str>,
) -> Result<(String, Vec<SearchBind>), DbError> {
    match node {
        SearchNode::Tag { id } => {
            let p = next(param);
            Ok((
                format!(
                    "EXISTS (SELECT 1 FROM asset_tags atag \
                     WHERE atag.asset_id = a.id AND atag.tag_id = ${p} \
                       AND atag.state = 'confirmed')"
                ),
                vec![SearchBind::Uuid(*id)],
            ))
        }
        SearchNode::Category { id } => {
            let p = next(param);
            Ok((
                format!(
                    "EXISTS (SELECT 1 FROM asset_tags atag \
                     JOIN tags tcat ON tcat.id = atag.tag_id \
                     WHERE atag.asset_id = a.id AND tcat.parent_id = ${p} \
                       AND atag.state = 'confirmed')"
                ),
                vec![SearchBind::Uuid(*id)],
            ))
        }
        SearchNode::Semantic {
            limit, embedding, ..
        } => {
            let (mv_p, vec_p, k_p, binds) =
                semantic_query_params(*limit, embedding.as_ref(), param)?;
            let vis = semantic_vis.unwrap_or("TRUE");
            Ok((
                format!(
                    "a.id = ANY (ARRAY( \
                       SELECT ae.asset_id \
                       FROM asset_embeddings ae \
                       JOIN assets va ON va.id = ae.asset_id \
                       JOIN folders vf ON vf.id = va.folder_id \
                       WHERE ae.model_version = ${mv_p} \
                         AND va.status = 'indexed' \
                         AND ({vis}) \
                       ORDER BY ae.embedding <=> ${vec_p}::vector \
                       LIMIT ${k_p} \
                     ))"
                ),
                binds,
            ))
        }
        _ => unreachable!("compile_fase7_axis handles only Tag/Category/Semantic"),
    }
}

/// Validates `embedding`/`limit` and reserves the three parameters
/// (`$mv`, `$vec`, `$k`) of the `IVFFlat` subquery — shared between the
/// old `a.id = ANY(ARRAY(...))` path (above) and the `topk` CTE of
/// [`SearchRepo::run_semantic_hoisted`]: same validation, same K clamp,
/// one single place — the two paths cannot silently diverge on an error
/// or on how many candidates the top-K considers.
fn semantic_query_params(
    limit: u32,
    embedding: Option<&Vec<f32>>,
    param: &mut usize,
) -> Result<(usize, usize, usize, Vec<SearchBind>), DbError> {
    let Some(emb) = embedding else {
        return Err(DbError::Conflict(
            "semantic embedding missing (API must embed query text)".to_owned(),
        ));
    };
    if emb.len() != 512 {
        return Err(DbError::Conflict(format!(
            "semantic embedding must be 512-d, got {}",
            emb.len()
        )));
    }
    let k = i32::try_from(limit.clamp(1, 500)).unwrap_or(500);
    let mv_p = next(param);
    let vec_p = next(param);
    let k_p = next(param);
    Ok((
        mv_p,
        vec_p,
        k_p,
        vec![
            SearchBind::Text(crate::embeddings::MODEL_VERSION.to_owned()),
            SearchBind::Text(vector_literal(emb)),
            SearchBind::I32(k),
        ],
    ))
}

/// `Semantic` nodes guaranteed present in every row of the result:
/// reachable only through a chain of `And` (never `Or`/`Not`, which
/// invert or loosen the constraint — forcing a `JOIN` there would be
/// incorrect, not just suboptimal). [`SearchRepo::run`] uses the CTE
/// ([`SearchRepo::run_semantic_hoisted`]) only when this function finds
/// **exactly one** such node; otherwise (zero, or more than one) it falls
/// back to the old `a.id = ANY(ARRAY(...))` via `run_plain`.
fn find_hoistable_semantic(node: &SearchNode) -> Vec<&SearchNode> {
    match node {
        SearchNode::And { args } => args.iter().flat_map(find_hoistable_semantic).collect(),
        SearchNode::Semantic { .. } => vec![node],
        _ => Vec::new(),
    }
}

/// Clones the AST, substituting (by pointer identity, not value: two
/// `Semantic` nodes with identical fields must not be confused) `target`
/// with an empty `And` — which [`compile_for_sql`] already compiles to
/// `TRUE`. No new `SearchNode` variant just for this marker: the real
/// top-K membership condition comes from the `JOIN` with the `topk` CTE,
/// not from this clause.
fn substitute_with_true(node: &SearchNode, target: *const SearchNode) -> SearchNode {
    if std::ptr::eq(node, target) {
        return SearchNode::And { args: Vec::new() };
    }
    match node {
        SearchNode::And { args } => SearchNode::And {
            args: args
                .iter()
                .map(|a| substitute_with_true(a, target))
                .collect(),
        },
        SearchNode::Or { args } => SearchNode::Or {
            args: args
                .iter()
                .map(|a| substitute_with_true(a, target))
                .collect(),
        },
        SearchNode::Not { arg } => SearchNode::Not {
            arg: Box::new(substitute_with_true(arg, target)),
        },
        other => other.clone(),
    }
}

/// Person / `PersonGroup` / `PersonCount` — kept separate for the same
/// reason as the functions above. None of them require their own
/// visibility: `a` is already filtered by `VisibilityScope` in `run`,
/// same assumption as `compile_fase7_axis` for `Tag`/`Category`.
///
/// None of these three nodes ever fails (unlike `Day`/`Month` in the
/// sibling function), but the signature stays `Result` for uniformity
/// with the rest of the dispatch — the same call site handles every axis
/// with `?`, and one fewer `Result` here would break that symmetry.
#[allow(clippy::unnecessary_wraps)]
fn compile_fase8_axis(
    node: &SearchNode,
    param: &mut usize,
) -> Result<(String, Vec<SearchBind>), DbError> {
    match node {
        SearchNode::Person { id } => {
            let p = next(param);
            Ok((
                format!(
                    "EXISTS (SELECT 1 FROM faces fc WHERE fc.asset_id = a.id AND fc.person_id = ${p})"
                ),
                vec![SearchBind::Uuid(*id)],
            ))
        }
        SearchNode::PersonGroup { id } => {
            let p = next(param);
            Ok((
                format!(
                    "EXISTS (SELECT 1 FROM faces fc \
                     JOIN person_group_members pgm ON pgm.person_id = fc.person_id \
                     WHERE fc.asset_id = a.id AND pgm.group_id = ${p})"
                ),
                vec![SearchBind::Uuid(*id)],
            ))
        }
        SearchNode::PersonCount { cmp, value } => {
            let op = cmp_op(*cmp);
            let p = next(param);
            Ok((
                format!(
                    "(SELECT COUNT(DISTINCT fc.person_id) FROM faces fc \
                     WHERE fc.asset_id = a.id AND fc.person_id IS NOT NULL) {op} ${p}"
                ),
                vec![SearchBind::I32(*value)],
            ))
        }
        _ => unreachable!("compile_fase8_axis only for Person/PersonGroup/PersonCount"),
    }
}

fn cmp_op(cmp: IsoCmp) -> &'static str {
    match cmp {
        IsoCmp::Gt => ">",
        IsoCmp::Gte => ">=",
        IsoCmp::Lt => "<",
        IsoCmp::Lte => "<=",
        IsoCmp::Eq => "=",
    }
}

/// Converts `asset_exif.exposure` (raw EXIF text, `"1/125"` or `"2"`) to
/// seconds. `NULL` for any text that does not match one of the two
/// expected shapes, instead of failing the query with an invalid cast —
/// a badly written EXIF stays cosmetic (no field lost from the filter),
/// not a 500 error.
fn shutter_seconds_sql(exif_alias: &str) -> String {
    format!(
        "CASE \
           WHEN {exif_alias}.exposure ~ '^[0-9]+/[0-9]+$' \
                AND split_part({exif_alias}.exposure, '/', 2) <> '0' \
             THEN split_part({exif_alias}.exposure, '/', 1)::real \
                  / split_part({exif_alias}.exposure, '/', 2)::real \
           WHEN {exif_alias}.exposure ~ '^[0-9]+(\\.[0-9]+)?$' \
             THEN {exif_alias}.exposure::real \
           ELSE NULL \
         END"
    )
}

fn join(
    args: &[SearchNode],
    sep: &str,
    param: &mut usize,
    depth: usize,
    gps_sql: &str,
    user_id: Option<uuid::Uuid>,
    semantic_vis: Option<&str>,
) -> Result<(String, Vec<SearchBind>), DbError> {
    let mut parts = Vec::new();
    let mut binds = Vec::new();
    for arg in args {
        let (sql, b) = compile_for_sql(arg, param, depth + 1, gps_sql, user_id, semantic_vis)?;
        parts.push(format!("({sql})"));
        binds.extend(b);
    }
    Ok((parts.join(sep), binds))
}

fn next(param: &mut usize) -> usize {
    let n = *param;
    *param += 1;
    n
}

fn like_prefix(raw: &str) -> String {
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("{escaped}%")
}

fn like_contains(raw: &str) -> String {
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn midnight(d: NaiveDate) -> DateTime<Utc> {
    d.and_hms_opt(0, 0, 0)
        .map_or(DateTime::<Utc>::UNIX_EPOCH, |ndt| ndt.and_utc())
}
