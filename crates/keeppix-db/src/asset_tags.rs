//! Assegnazioni `asset_tags` proposte dall'IA (Fase 7 Task 8) e la coda di
//! revisione umana su quelle proposte (Fase 7 Task 9).
//!
//! [`Self::propose_for_tag`] / [`Self::propose_for_assets`] non prendono
//! `AuthContext`: è la pipeline di analisi di sistema, come
//! [`crate::EmbeddingRepo`]. L'abbinamento non è un'azione utente — scatta
//! dopo create/patch di un tag con embedding, o dopo un lotto di embed foto.
//!
//! Le decisioni umane ([`Self::confirm`], [`Self::reject`] e le loro varianti
//! in blocco) **prendono** `AuthContext`: sono azioni utente, e un utente non
//! deve poter decidere (né apprendere l'esistenza) di una proposta su un
//! asset che non vede. Una volta decise, `confirmed`/`rejected` non vengono
//! mai sovrascritte dal rematch (`ON CONFLICT ... WHERE state = 'proposed'`
//! in [`Self::propose_for_tag`]).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use keeppix_domain::{AssetId, AuthContext, TagId};

use crate::pgvector::probe_pgvector;
use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError};

/// Un tag confermato su un asset, come [`AssetTagRepo::confirmed_among`] lo
/// restituisce — non l'assegnazione grezza della tabella (`state`/`source`
/// non servono al chiamante, che ha già filtrato su `state='confirmed'`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmedTag {
    pub tag_id: TagId,
    pub name: String,
    pub color: Option<String>,
    /// `parent_id` del tag: le "categorie" del documento funzionale (SP-3
    /// §11) sono tag con `kind='category'`, non una tabella a parte — un
    /// tag senza genitore (o la cui gerarchia non è impostata) ha `None`.
    pub category_id: Option<TagId>,
}

#[derive(Debug, sqlx::FromRow)]
struct ConfirmedTagRow {
    asset_id: uuid::Uuid,
    tag_id: uuid::Uuid,
    name: String,
    color: Option<String>,
    parent_id: Option<uuid::Uuid>,
}

/// Le due decisioni umane possibili su una proposta. Costante interna: non è
/// mai serializzata, la traduzione da/verso stringa SQL resta qui.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Confirmed,
    Rejected,
}

impl Decision {
    const fn as_state(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }
}

/// Una proposta in attesa, come la coda di revisione la mostra: già arricchita
/// con nome del tag e nome file, per non costringere il chiamante a un
/// secondo giro di query per riga.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalView {
    pub asset_id: AssetId,
    pub tag_id: TagId,
    pub tag_name: String,
    pub score: Option<f32>,
    pub filename: String,
    pub taken_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct ProposalRow {
    asset_id: uuid::Uuid,
    tag_id: uuid::Uuid,
    tag_name: String,
    score: Option<f32>,
    filename: String,
    taken_at_utc: Option<DateTime<Utc>>,
}

impl ProposalRow {
    fn into_view(self) -> ProposalView {
        ProposalView {
            asset_id: AssetId::from_uuid(self.asset_id),
            tag_id: TagId::from_uuid(self.tag_id),
            tag_name: self.tag_name,
            score: self.score,
            filename: self.filename,
            taken_at_utc: self.taken_at_utc,
        }
    }
}

/// Banda sotto la soglia del tag: score ≥ `threshold − BAND` produce ancora
/// una proposta (score più basso → fondo coda). Costante di sistema, non
/// esposta in API — un punto percentuale (0.01).
pub const TAG_MATCH_BAND: f32 = 0.01;

pub struct AssetTagRepo<'a> {
    db: &'a Db,
}

impl<'a> AssetTagRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Abbina tutte le foto con embedding allo stesso `model_version` del tag.
    /// Inserisce/aggiorna solo righe `state='proposed'`, `source='ai'`.
    ///
    /// # Errors
    /// `Connection` se la query fallisce (o se lo schema AI non esiste).
    pub async fn propose_for_tag(&self, tag_id: TagId) -> Result<u64, DbError> {
        let result = sqlx::query(
            "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) \
             SELECT ae.asset_id, t.id, 'proposed', 'ai', \
                    (1.0 - (ae.embedding <=> t.embedding))::real \
             FROM tags t \
             JOIN asset_embeddings ae \
               ON ae.model_version = t.model_version \
             WHERE t.id = $1 \
               AND t.kind = 'tag' \
               AND t.embedding IS NOT NULL \
               AND t.model_version IS NOT NULL \
               AND (1.0 - (ae.embedding <=> t.embedding)) \
                   >= (t.threshold - $2::real) \
             ON CONFLICT (asset_id, tag_id) DO UPDATE \
               SET score = EXCLUDED.score \
               WHERE asset_tags.state = 'proposed'",
        )
        .bind(tag_id.as_uuid())
        .bind(TAG_MATCH_BAND)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Abbina gli asset dati a tutti i tag con embedding (stesso
    /// `model_version`). Stesse regole di [`Self::propose_for_tag`].
    ///
    /// # Errors
    /// `Connection` se la query fallisce (o se lo schema AI non esiste).
    pub async fn propose_for_assets(&self, asset_ids: &[AssetId]) -> Result<u64, DbError> {
        if asset_ids.is_empty() {
            return Ok(0);
        }
        let uuids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let result = sqlx::query(
            "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) \
             SELECT ae.asset_id, t.id, 'proposed', 'ai', \
                    (1.0 - (ae.embedding <=> t.embedding))::real \
             FROM tags t \
             JOIN asset_embeddings ae \
               ON ae.model_version = t.model_version \
             WHERE ae.asset_id = ANY($1) \
               AND t.kind = 'tag' \
               AND t.embedding IS NOT NULL \
               AND t.model_version IS NOT NULL \
               AND (1.0 - (ae.embedding <=> t.embedding)) \
                   >= (t.threshold - $2::real) \
             ON CONFLICT (asset_id, tag_id) DO UPDATE \
               SET score = EXCLUDED.score \
               WHERE asset_tags.state = 'proposed'",
        )
        .bind(&uuids)
        .bind(TAG_MATCH_BAND)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Elenco delle proposte in attesa (`state = 'proposed'`), filtrato sulla
    /// visibilità del chiamante e, opzionalmente, su un solo tag. Ordinato per
    /// `score` decrescente (i punteggi nulli, teoricamente impossibili per
    /// una riga proposta, in fondo).
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato (un link pubblico non ha una coda
    /// di revisione); `Connection` su errore DB o schema AI assente.
    pub async fn list_proposed(
        &self,
        ctx: &AuthContext,
        tag_id: Option<TagId>,
    ) -> Result<Vec<ProposalView>, DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 2);
        let rows: Vec<ProposalRow> = sqlx::query_as(&format!(
            "SELECT at.asset_id, at.tag_id, t.name AS tag_name, at.score, \
                    a.filename, a.taken_at_utc \
             FROM asset_tags at \
             JOIN tags t ON t.id = at.tag_id \
             JOIN assets a ON a.id = at.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE at.state = 'proposed' \
               AND ($1::uuid IS NULL OR at.tag_id = $1) \
               AND {} \
             ORDER BY at.score DESC NULLS LAST, at.asset_id",
            filter.sql()
        ))
        .bind(tag_id.map(|id| id.as_uuid()))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(ProposalRow::into_view).collect())
    }

    /// Numero di `asset_tags` in `state = 'proposed'` il cui asset è visibile
    /// al chiamante — la metà "tag" del badge combinato `bootstrap.badges.revision`
    /// (Fase 8 aggiungerà la metà "volti" sullo stesso campo).
    ///
    /// A differenza degli altri metodi di questo repository, **non propaga**
    /// l'errore quando lo schema AI non esiste (pgvector assente sul
    /// Postgres collegato): `bootstrap` è il bundle di avvio di tutta
    /// l'applicazione, non un endpoint opzionale come `/tags`, quindi un
    /// Postgres esterno senza pgvector non deve rompere l'avvio dell'intera
    /// interfaccia — coerente con la Ruling di Task 3 ("l'avvio non
    /// fallisce").
    ///
    /// # Errors
    /// `Connection` se la query (probe pgvector incluso) fallisce per un
    /// motivo diverso dall'estensione assente.
    pub async fn count_proposed_visible(&self, ctx: &AuthContext) -> Result<i64, DbError> {
        if ctx.user_id().is_none() {
            return Ok(0);
        }
        let status = probe_pgvector(self.db).await?;
        if !status.available {
            return Ok(0);
        }

        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM asset_tags at \
             JOIN assets a ON a.id = at.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE at.state = 'proposed' AND {}",
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_one(self.db.pool())
        .await?;
        Ok(count)
    }

    /// Conferma una proposta: `state = 'confirmed'`, `decided_by`/`decided_at`
    /// impostati. Idempotente se era già confermata.
    ///
    /// # Errors
    /// `Forbidden` se l'asset non è visibile al chiamante (o il contesto non
    /// ha un utente) — mai `NotFound`, per non offrire un oracolo di
    /// esistenza sull'asset. `NotFound` se l'asset è visibile ma questo tag
    /// non gli è mai stato proposto. `Conflict` se la coppia è già stata
    /// decisa nella direzione opposta (`rejected`): una decisione permanente
    /// non si inverte.
    pub async fn confirm(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        self.decide(ctx, tag_id, asset_id, Decision::Confirmed)
            .await
    }

    /// Rifiuta una proposta, in modo permanente: `state = 'rejected'`,
    /// `decided_by`/`decided_at` impostati. Il rematch non la resuscita mai
    /// ([`Self::propose_for_tag`] aggiorna solo righe `state = 'proposed'`).
    /// Idempotente se era già rifiutata.
    ///
    /// # Errors
    /// Come [`Self::confirm`], con i ruoli di `confirmed`/`rejected` invertiti.
    pub async fn reject(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        self.decide(ctx, tag_id, asset_id, Decision::Rejected).await
    }

    async fn decide(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
        decision: Decision,
    ) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;

        let target = decision.as_state();
        let transitioned: Option<(String,)> = sqlx::query_as(
            "UPDATE asset_tags SET state = $3, decided_by = $4, decided_at = now() \
             WHERE tag_id = $1 AND asset_id = $2 AND state = 'proposed' \
             RETURNING state",
        )
        .bind(tag_id.as_uuid())
        .bind(asset_id.as_uuid())
        .bind(target)
        .bind(user_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        if transitioned.is_some() {
            return Ok(());
        }

        // Non era 'proposed': un aggiornamento onesto invece di un 0-row
        // silenzioso — distingue "mai proposto", "già deciso uguale"
        // (idempotente) e "già deciso al contrario" (conflitto reale).
        let current: Option<(String,)> =
            sqlx::query_as("SELECT state FROM asset_tags WHERE tag_id = $1 AND asset_id = $2")
                .bind(tag_id.as_uuid())
                .bind(asset_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        match current {
            None => Err(DbError::NotFound),
            Some((state,)) if state == target => Ok(()),
            Some((state,)) => Err(DbError::Conflict(format!(
                "asset_tags already decided as '{state}'; cannot become '{target}'"
            ))),
        }
    }

    /// Conferma tutte le proposte in attesa per un tag, limitate agli asset
    /// visibili al chiamante — «Conferma tutte» della coda di revisione.
    /// Restituisce gli id confermati da questa chiamata.
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato; `Connection` sul database.
    pub async fn confirm_all_for_tag(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
    ) -> Result<Vec<AssetId>, DbError> {
        self.decide_all_for_tag(ctx, tag_id, Decision::Confirmed)
            .await
    }

    /// Come [`Self::confirm_all_for_tag`], ma rifiuta — «Rifiuta tutte»,
    /// permanente come [`Self::reject`].
    ///
    /// # Errors
    /// Come [`Self::confirm_all_for_tag`].
    pub async fn reject_all_for_tag(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
    ) -> Result<Vec<AssetId>, DbError> {
        self.decide_all_for_tag(ctx, tag_id, Decision::Rejected)
            .await
    }

    async fn decide_all_for_tag(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        decision: Decision,
    ) -> Result<Vec<AssetId>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 4);
        let target = decision.as_state();

        let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(&format!(
            "UPDATE asset_tags at SET state = $2, decided_by = $3, decided_at = now() \
             WHERE at.tag_id = $1 AND at.state = 'proposed' \
               AND EXISTS ( \
                 SELECT 1 FROM assets a JOIN folders f ON f.id = a.folder_id \
                  WHERE a.id = at.asset_id AND {} \
               ) \
             RETURNING at.asset_id",
            filter.sql()
        ))
        .bind(tag_id.as_uuid())
        .bind(target)
        .bind(user_id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id,)| AssetId::from_uuid(id))
            .collect())
    }

    /// Assegna un tag a un asset per decisione diretta di una persona (Fase
    /// 11 Task 7, §13.3 campo 5, SP-12: "un'aggiunta manuale è già una
    /// conferma, non passa dalla coda di revisione"). A differenza di
    /// [`Self::confirm`] — che transita solo da `'proposed'` ed è in
    /// conflitto su un `'rejected'` già deciso — qui la persona sta
    /// decidendo *ora*, non risolvendo una proposta IA passata: scrive
    /// sempre `state='confirmed', source='user'`, anche sopra un rifiuto
    /// precedente. Idempotente.
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato o se l'asset non è visibile al
    /// chiamante. `Connection` se la scrittura fallisce.
    pub async fn assign(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        sqlx::query(
            "INSERT INTO asset_tags (asset_id, tag_id, state, source, decided_by, decided_at) \
             VALUES ($1, $2, 'confirmed', 'user', $3, now()) \
             ON CONFLICT (asset_id, tag_id) DO UPDATE SET \
               state = 'confirmed', source = 'user', decided_by = $3, decided_at = now()",
        )
        .bind(asset_id.as_uuid())
        .bind(tag_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Toglie un tag assegnato a mano da un asset — la freccia opposta di
    /// [`Self::assign`] (§13.3 campo 5: "attiva/disattiva un tag per
    /// aggiungerlo o toglierlo da tutti", verificato sul prototipo reale,
    /// `openTagPickerDialog`/`removeTagFromPhoto` in
    /// `docs/ui/keeppix-mockup.html`). Una `DELETE` vera della riga, non una
    /// transizione a `state='rejected'`: quello stato è la decisione
    /// **permanente** della coda di revisione IA ([`Self::reject`], che
    /// rifiuta esplicitamente di invertirsi su un conflitto) — semantica
    /// sbagliata per "ho ripensato a un tag che avevo messo io a mano".
    /// Con una `DELETE`, riassegnare più tardi lo stesso tag passa di nuovo
    /// da [`Self::assign`] senza scontrarsi con `Conflict`. Idempotente:
    /// nessuna riga da cancellare non è un errore.
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato o se l'asset non è visibile al
    /// chiamante. `Connection` se la cancellazione fallisce.
    pub async fn unassign(
        &self,
        ctx: &AuthContext,
        tag_id: TagId,
        asset_id: AssetId,
    ) -> Result<(), DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        sqlx::query("DELETE FROM asset_tags WHERE asset_id = $1 AND tag_id = $2")
            .bind(asset_id.as_uuid())
            .bind(tag_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Tag confermati per un insieme di asset (Fase 11 Task 7, SP-3 §11 e
    /// `AssetView`) — solo `state='confirmed'`, mai proposte in attesa né
    /// rifiutate (§11.3: "si guardano solo i tag confermati della foto").
    /// Stesso idioma di [`crate::FlagRepo::favorites_among`]: una query sola
    /// per l'intera pagina. Mappa vuota — non un errore — se pgvector non è
    /// installato: `tags`/`asset_tags` non esistono affatto in quel caso
    /// (migrazione 0043, stesso no-op già in [`Self::count_proposed_visible`]).
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn confirmed_among(
        &self,
        asset_ids: &[AssetId],
    ) -> Result<HashMap<AssetId, Vec<ConfirmedTag>>, DbError> {
        if asset_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let status = probe_pgvector(self.db).await?;
        if !status.available {
            return Ok(HashMap::new());
        }
        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<ConfirmedTagRow> = sqlx::query_as(
            "SELECT at.asset_id, t.id AS tag_id, t.name, t.color, t.parent_id \
               FROM asset_tags at JOIN tags t ON t.id = at.tag_id \
              WHERE at.asset_id = ANY($1) AND at.state = 'confirmed'",
        )
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;
        let mut out: HashMap<AssetId, Vec<ConfirmedTag>> = HashMap::new();
        for row in rows {
            out.entry(AssetId::from_uuid(row.asset_id))
                .or_default()
                .push(ConfirmedTag {
                    tag_id: TagId::from_uuid(row.tag_id),
                    name: row.name,
                    color: row.color,
                    category_id: row.parent_id.map(TagId::from_uuid),
                });
        }
        Ok(out)
    }
}
