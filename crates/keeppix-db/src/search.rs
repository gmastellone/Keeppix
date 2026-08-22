//! Ricerca da AST JSON. La stringa utente non entra mai nell'SQL: solo bind.

use chrono::{DateTime, NaiveDate, Utc};
use keeppix_domain::AuthContext;
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

// Niente `Eq`: `Aperture`/`Shutter` portano un `f32`, che non lo implementa.
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
    /// Voto dell'utente che esegue la ricerca (spec §4.1, per-utente: il tuo
    /// 5 stelle non è il 5 stelle di un altro). `IsoCmp` riusato: è lo stesso
    /// confronto numerico di `Iso`, non un secondo enum per lo stesso scopo.
    Rating {
        cmp: IsoCmp,
        value: i32,
    },
    /// Chip "Preferiti" (Task 6/Task 10): stesso schema per-utente di
    /// `Rating`. La colonna `asset_flags.favorite` esiste da questo task
    /// (migrazione 0037) — il resto del concetto (scrittura, `AssetView`,
    /// `AssetFlags` di dominio) resta del Task 10, che la userà già pronta.
    Favorite,
    /// Intervallo esplicito, entrambi gli estremi inclusi.
    DateRange {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
    /// Giorno del mese (1..=31), indipendente dal mese e dall'anno — la
    /// controparte ricorrente di `Year`/`Month`.
    Day {
        value: i32,
    },
    /// Mese dell'anno (1..=12), indipendente dall'anno.
    Month {
        value: i32,
    },
    /// Paese via `assets.place_id → places.country_code`. **Non** è
    /// `Folder`: nel prodotto reale cartella e luogo sono due concetti
    /// diversi anche se nel prototipo coincidevano (spec fase-10 §6).
    Country {
        value: String,
    },
    Aperture {
        cmp: IsoCmp,
        value: f32,
    },
    /// Tempo di scatto in secondi (`1/125` → `0.008`): `asset_exif.exposure`
    /// è testo EXIF grezzo, convertito a un numero nella query stessa.
    Shutter {
        cmp: IsoCmp,
        value: f32,
    },
    /// Un luogo del catalogo `places` (Fase 4), non una cartella.
    Place {
        id: i64,
    },
    /// Tag confermato su `asset_tags` (Fase 7). Solo `state='confirmed'`.
    Tag {
        id: uuid::Uuid,
    },
    /// Categoria: qualsiasi tag figlio (`tags.parent_id`) confermato sull'asset.
    Category {
        id: uuid::Uuid,
    },
    /// Vicini CLIP: membership nei K più simili (visibili), risultati ancora
    /// ordinati per data. `embedding` è riempito dal layer API (db non conosce
    /// ort); assente → `Conflict` in compile.
    Semantic {
        query: String,
        limit: u32,
        #[serde(skip)]
        embedding: Option<Vec<f32>>,
    },
    /// Il chip «Persona» (Fase 8 Task 9): foto in cui compare un volto
    /// assegnato a questa persona (`faces.person_id`, mai proposto/rifiutato
    /// — vedi `Faces::reject`, che pulisce `person_id` insieme a
    /// `rejected_at`, quindi la sola condizione `person_id = id` basta).
    Person {
        id: uuid::Uuid,
    },
    /// Foto in cui compare **almeno una** persona del gruppo (spec §6).
    PersonGroup {
        id: uuid::Uuid,
    },
    /// «Foto con almeno N persone» — conta le persone distinte assegnate
    /// sull'asset, non i volti (due volti della stessa persona nella stessa
    /// foto non ne fanno due).
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

    /// Restituisce solo il primario di ogni pila (Task 3, come la
    /// timeline): un RAW+JPEG impilato è un risultato, non due.
    ///
    /// # Errors
    /// `Conflict` se l'AST è troppo profondo; `Connection` se la query fallisce.
    pub async fn run(
        &self,
        ctx: &AuthContext,
        ast: &SearchNode,
        cursor: Option<(DateTime<Utc>, keeppix_domain::AssetId)>,
        limit: i64,
    ) -> Result<Vec<AssetWithStack>, DbError> {
        let limit = limit.clamp(1, 200);
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        // Stessi $1,$2,$3 riusati nella subquery Semantic (spec §4.2: K fra i
        // visibili, non K globali poi filtrati).
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

    /// Suggerimenti tipizzati per la barra di ricerca (spec fase-10 §23): il
    /// frontend deve sapere *di che tipo* è ogni risultato per costruire la
    /// pillola giusta, e per un tag anche il colore del pallino.
    ///
    /// `tag` resta senza fonte in questo task: la tabella dei tag non esiste
    /// ancora (Fase 7). L'enum è comunque completo — è la forma che va
    /// fissata ora, non le fonti, altrimenti cambierebbe due volte. Le altre
    /// sei fonti (`camera`, `filename`, `folder`, `iso`, `year`, `country`)
    /// leggono dati già presenti: le prime due esistevano già, le ultime
    /// quattro sfruttano gli assi di ricerca del Task 6.
    ///
    /// `country` legge `assets.place_id` direttamente, senza `COALESCE` con
    /// `asset_overrides.place_id` — stessa scelta di `SearchNode::Country`
    /// (quella colonna non viene ancora scritta da nessun percorso).
    ///
    /// # Errors
    /// `Connection` se la query fallisce; `Corrupted` se il database
    /// restituisce un `kind` fuori dall'insieme chiuso previsto (non
    /// dovrebbe accadere: la lista di `SELECT` è letterale, non dati
    /// dell'utente).
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

/// Insieme chiuso: il frontend decide la pillola in base a questo, quindi un
/// ottavo valore non previsto romperebbe il contratto invece di degradare.
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

/// Suggerimento tipizzato per la barra di ricerca (spec fase-10 §23).
/// `value` è ciò che alimenta il `SearchNode` corrispondente se l'utente
/// sceglie la pillola (l'id di cartella per `Folder`, il testo per gli
/// altri); `label` è ciò che si mostra.
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

/// `user_id` alimenta gli assi per-utente (`Rating`, `Favorite`): `None` per
/// un `AuthContext` senza utente (link pubblico) fa fallire quei due nodi con
/// `Forbidden` invece di produrre un confronto silenziosamente vuoto — non
/// hanno senso senza un utente che vota.
///
/// `semantic_vis` è la clausola di visibilità che riusa `$1,$2,$3` con alias
/// `vf`/`va` (generata da `VisibilityScope::filter` in `run`). `None` →
/// `TRUE` dentro la subquery (album/mappa dove lo scope esterno basta).
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

/// Ogni variante che non è un combinatore (`And`/`Or`/`Not`): non ricorre,
/// quindi non ha bisogno di `depth` — separata da [`compile_for_sql`] solo
/// per restare sotto il limite di righe per funzione di clippy. A sua volta
/// delega gli assi nuovi del Task 6 a [`compile_search_axis`], per lo stesso
/// motivo.
fn compile_leaf(
    node: &SearchNode,
    param: &mut usize,
    gps_sql: &str,
    user_id: Option<uuid::Uuid>,
    semantic_vis: Option<&str>,
) -> Result<(String, Vec<SearchBind>), DbError> {
    match node {
        SearchNode::And { .. } | SearchNode::Or { .. } | SearchNode::Not { .. } => {
            unreachable!("i combinatori sono gestiti da compile_for_sql")
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

/// Le nove varianti nuove del Task 6 (spec fase-10 §6): nessuna ricorre, e
/// nessuna dipende da `gps_sql` — separate da [`compile_leaf`] solo per
/// restare sotto il limite di righe per funzione di clippy.
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
        | SearchNode::HasGps => unreachable!("gestite da compile_for_sql/compile_leaf"),
        SearchNode::Rating { cmp, value } => {
            let user = user_id.ok_or(DbError::Forbidden)?;
            let op = cmp_op(*cmp);
            let uid_p = next(param);
            let val_p = next(param);
            Ok((
                format!(
                    "EXISTS (SELECT 1 FROM asset_flags af \
                     WHERE af.asset_id = a.id AND af.user_id = ${uid_p} \
                       AND af.rating {op} ${val_p})"
                ),
                vec![SearchBind::Uuid(user), SearchBind::I32(*value)],
            ))
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

/// Tag / Category / Semantic (Fase 7 Task 10) — separate per il tetto clippy.
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
            let Some(emb) = embedding.as_ref() else {
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
            let k = i32::try_from((*limit).clamp(1, 500)).unwrap_or(500);
            let vis = semantic_vis.unwrap_or("TRUE");
            let mv_p = next(param);
            let vec_p = next(param);
            let k_p = next(param);
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
                vec![
                    SearchBind::Text(crate::embeddings::MODEL_VERSION.to_owned()),
                    SearchBind::Text(vector_literal(emb)),
                    SearchBind::I32(k),
                ],
            ))
        }
        _ => unreachable!("compile_fase7_axis only for Tag/Category/Semantic"),
    }
}

/// Person / `PersonGroup` / `PersonCount` (Fase 8 Task 9) — separate per lo
/// stesso motivo delle funzioni sopra. Nessuna richiede una visibilità
/// propria: `a` è già filtrata da `VisibilityScope` in `run`, stessa
/// assunzione di `compile_fase7_axis` per `Tag`/`Category`.
///
/// Nessuno di questi tre nodi fallisce mai (a differenza di `Day`/`Month`
/// nella funzione sorella), ma la firma resta `Result` per uniformità col
/// resto del dispatch — lo stesso punto di chiamata gestisce tutti gli assi
/// con `?`, un `Result` in meno qui romperebbe quella simmetria.
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

/// Converte `asset_exif.exposure` (testo EXIF grezzo, `"1/125"` o `"2"`) in
/// secondi. `NULL` per qualunque testo che non rispetti una delle due forme
/// attese, invece di far fallire la query con un cast non valido — un EXIF
/// scritto male resta cosmetico (nessun campo perso dal filtro), non un
/// errore 500.
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
