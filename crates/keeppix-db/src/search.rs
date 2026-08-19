//! Ricerca da AST JSON. La stringa utente non entra mai nell'SQL: solo bind.

use chrono::{DateTime, NaiveDate, Utc};
use keeppix_domain::{Asset, AuthContext};
use serde::{Deserialize, Serialize};

use crate::assets::{A_COLUMNS, AssetRow};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SearchNode {
    And { args: Vec<SearchNode> },
    Or { args: Vec<SearchNode> },
    Not { arg: Box<SearchNode> },
    Text { value: String },
    Type { value: String },
    Camera { value: String },
    Lens { value: String },
    Iso { cmp: IsoCmp, value: i32 },
    Year { value: i32 },
    Folder { id: uuid::Uuid },
    HasGps,
}

pub(crate) enum SearchBind {
    Text(String),
    I32(i32),
    Uuid(uuid::Uuid),
    Ts(DateTime<Utc>),
}

impl<'a> SearchRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// `Conflict` se l'AST è troppo profondo; `Connection` se la query fallisce.
    pub async fn run(
        &self,
        ctx: &AuthContext,
        ast: &SearchNode,
        cursor: Option<(DateTime<Utc>, keeppix_domain::AssetId)>,
        limit: i64,
    ) -> Result<Vec<Asset>, DbError> {
        let limit = limit.clamp(1, 200);
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let mut param = 4_usize;
        let (clause, binds) = compile_for_sql(ast, &mut param, 0, "a.location")?;
        let time_p = next(&mut param);
        let id_p = next(&mut param);
        let limit_p = next(&mut param);
        let (cursor_time, cursor_id) = match cursor {
            Some((t, id)) => (Some(t), Some(id.as_uuid())),
            None => (None, None),
        };
        let sql = format!(
            "SELECT {A_COLUMNS} FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             LEFT JOIN asset_exif e ON e.asset_id = a.id \
             WHERE {} AND a.status = 'indexed' AND ({clause}) \
               AND (${time_p}::timestamptz IS NULL \
                    OR a.taken_at_utc < ${time_p} \
                    OR (a.taken_at_utc = ${time_p} AND a.id < ${id_p})) \
             ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC \
             LIMIT ${limit_p}",
            filter.sql()
        );
        let mut q = sqlx::query_as::<_, AssetRow>(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets());
        for b in &binds {
            q = bind_one(q, b);
        }
        let rows: Vec<AssetRow> = q
            .bind(cursor_time)
            .bind(cursor_id)
            .bind(limit)
            .fetch_all(self.db.pool())
            .await?;
        rows.into_iter().map(AssetRow::into_domain).collect()
    }

    /// Prefissi su `camera_model` e filename visibili.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn suggest(&self, ctx: &AuthContext, q: &str) -> Result<Vec<String>, DbError> {
        let q = q.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let pattern = like_prefix(q);
        let sql = format!(
            "(SELECT e.camera_model AS s FROM asset_exif e \
              JOIN assets a ON a.id = e.asset_id \
              JOIN folders f ON f.id = a.folder_id \
              WHERE {} AND e.camera_model ILIKE $4 ESCAPE E'\\\\' \
              LIMIT 8) \
             UNION \
             (SELECT a.filename AS s FROM assets a \
              JOIN folders f ON f.id = a.folder_id \
              WHERE {} AND a.filename ILIKE $4 ESCAPE E'\\\\' \
              LIMIT 8) \
             LIMIT 10",
            filter.sql(),
            filter.sql()
        );
        let rows: Vec<(String,)> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .bind(pattern)
            .fetch_all(self.db.pool())
            .await?;
        Ok(rows.into_iter().map(|(s,)| s).collect())
    }

    /// # Errors
    /// `Forbidden` senza utente; `Connection` se la query fallisce.
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
    /// `Forbidden` senza utente; `Connection` se l'inserimento fallisce.
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

    /// Carica e interpreta una ricerca salvata del chiamante.
    ///
    /// # Errors
    /// `Forbidden` per id sconosciuti o appartenenti a un altro utente;
    /// `Conflict` se il testo salvato non è più interpretabile.
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
    q: sqlx::query::QueryAs<'q, sqlx::Postgres, AssetRow, sqlx::postgres::PgArguments>,
    b: &'q SearchBind,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, AssetRow, sqlx::postgres::PgArguments> {
    match b {
        SearchBind::Text(s) => q.bind(s),
        SearchBind::I32(n) => q.bind(n),
        SearchBind::Uuid(u) => q.bind(u),
        SearchBind::Ts(t) => q.bind(t),
    }
}

pub(crate) fn compile_for_sql(
    node: &SearchNode,
    param: &mut usize,
    depth: usize,
    gps_sql: &str,
) -> Result<(String, Vec<SearchBind>), DbError> {
    if depth > 16 {
        return Err(DbError::Conflict("search too nested".to_owned()));
    }
    match node {
        SearchNode::And { args } if args.is_empty() => Ok(("TRUE".to_owned(), Vec::new())),
        SearchNode::And { args } => join(args, " AND ", param, depth, gps_sql),
        SearchNode::Or { args } if args.is_empty() => Ok(("FALSE".to_owned(), Vec::new())),
        SearchNode::Or { args } => join(args, " OR ", param, depth, gps_sql),
        SearchNode::Not { arg } => {
            let (inner, binds) = compile_for_sql(arg, param, depth + 1, gps_sql)?;
            Ok((format!("NOT COALESCE(({inner}), FALSE)"), binds))
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
            let op = match cmp {
                IsoCmp::Gt => ">",
                IsoCmp::Gte => ">=",
                IsoCmp::Lt => "<",
                IsoCmp::Lte => "<=",
                IsoCmp::Eq => "=",
            };
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
    }
}

fn join(
    args: &[SearchNode],
    sep: &str,
    param: &mut usize,
    depth: usize,
    gps_sql: &str,
) -> Result<(String, Vec<SearchBind>), DbError> {
    let mut parts = Vec::new();
    let mut binds = Vec::new();
    for arg in args {
        let (sql, b) = compile_for_sql(arg, param, depth + 1, gps_sql)?;
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
