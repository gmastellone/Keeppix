//! Volti rilevati (Fase 8 Task 3/4). [`Self::insert_detected`] non prende
//! `AuthContext`: è la pipeline di rilevamento, come [`crate::EmbeddingRepo`]
//! per la Fase 7 — non un'azione utente.
//!
//! Le decisioni umane ([`Self::assign`], [`Self::reject`],
//! [`Self::confirm_proposal`]) **prendono** `AuthContext`: un utente non deve
//! poter agire (né apprendere l'esistenza) su un volto di un asset che non
//! vede. Una volta assegnato a mano (`assigned_by` impostato), un volto non
//! viene mai più toccato dal raggruppamento automatico (spec §4.3).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use keeppix_domain::{AssetId, AuthContext, Face, FaceBBox, FaceId, PersonId, UserId};

use crate::visibility::VisibilityScope;
use crate::{AssetRepo, Db, DbError};

/// Un volto confermato su un asset, come [`FaceRepo::confirmed_among`] lo
/// restituisce — `person_id`/nome soltanto, non la riga `faces` intera
/// (bbox/embedding/punteggi non servono al chiamante, SP-3 §11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmedFace {
    pub person_id: PersonId,
    /// `None` per una persona senza nome ("Persona 4" — l'etichetta di
    /// fallback è responsabilità del chiamante, non di questo livello).
    pub person_name: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ConfirmedFaceRow {
    asset_id: uuid::Uuid,
    person_id: uuid::Uuid,
    person_name: Option<String>,
}

/// Allineato a `keeppix_media::face::MODEL_VERSION`. Duplicato qui perché
/// `keeppix-db` non può dipendere da `keeppix-media` (`deny.toml`) — stessa
/// ragione di `EmbeddingRepo::MODEL_VERSION` per Fase 7.
pub const MODEL_VERSION: &str = "scrfd-500mf+arcface";

/// Candidato al rilevamento: ha `content_hash` (quindi può avere miniatura).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFaceScan {
    pub asset_id: AssetId,
    pub content_hash: [u8; 32],
}

#[derive(Debug, sqlx::FromRow)]
struct FaceRow {
    id: uuid::Uuid,
    asset_id: uuid::Uuid,
    bbox_x: f32,
    bbox_y: f32,
    bbox_w: f32,
    bbox_h: f32,
    landmarks: Option<serde_json::Value>,
    detect_score: f32,
    quality: Option<f32>,
    person_id: Option<uuid::Uuid>,
    assigned_by: Option<uuid::Uuid>,
    assigned_at: Option<DateTime<Utc>>,
    rejected_at: Option<DateTime<Utc>>,
    proposed_person_id: Option<uuid::Uuid>,
    proposed_score: Option<f32>,
    model_version: String,
    created_at: DateTime<Utc>,
}

impl FaceRow {
    fn into_domain(self) -> Face {
        Face {
            id: FaceId::from_uuid(self.id),
            asset_id: AssetId::from_uuid(self.asset_id),
            bbox: FaceBBox {
                x: self.bbox_x,
                y: self.bbox_y,
                w: self.bbox_w,
                h: self.bbox_h,
            },
            landmarks: self.landmarks,
            detect_score: self.detect_score,
            quality: self.quality,
            person_id: self.person_id.map(PersonId::from_uuid),
            assigned_by: self.assigned_by.map(UserId::from_uuid),
            assigned_at: self.assigned_at,
            rejected_at: self.rejected_at,
            proposed_person_id: self.proposed_person_id.map(PersonId::from_uuid),
            proposed_score: self.proposed_score,
            model_version: self.model_version,
            created_at: self.created_at,
        }
    }
}

const COLUMNS: &str = "id, asset_id, bbox_x, bbox_y, bbox_w, bbox_h, landmarks, detect_score, \
                       quality, person_id, assigned_by, assigned_at, rejected_at, \
                       proposed_person_id, proposed_score, model_version, created_at";

/// Input di un volto appena rilevato, prima di ogni raggruppamento. Non
/// include `person_id`: l'assegnazione arriva dopo, dal raggruppamento
/// incrementale o da un umano.
#[derive(Debug, Clone)]
pub struct NewDetectedFace {
    pub asset_id: AssetId,
    pub bbox: FaceBBox,
    pub landmarks: Option<serde_json::Value>,
    pub embedding: Option<Vec<f32>>,
    pub detect_score: f32,
    pub quality: Option<f32>,
    pub model_version: String,
}

pub struct FaceRepo<'a> {
    db: &'a Db,
}

impl<'a> FaceRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Inserisce un volto appena rilevato, senza persona. Pipeline interna:
    /// nessun `AuthContext`.
    ///
    /// # Errors
    /// `Connection` se la query fallisce (o se lo schema volti non esiste).
    pub async fn insert_detected(&self, new: NewDetectedFace) -> Result<Face, DbError> {
        let embedding_literal = new
            .embedding
            .as_deref()
            .map(crate::embeddings::vector_literal);
        let row: FaceRow = sqlx::query_as(&format!(
            "INSERT INTO faces (id, asset_id, bbox_x, bbox_y, bbox_w, bbox_h, landmarks, \
                                 embedding, detect_score, quality, model_version) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8::vector, $9, $10, $11) \
             RETURNING {COLUMNS}"
        ))
        .bind(FaceId::new().as_uuid())
        .bind(new.asset_id.as_uuid())
        .bind(new.bbox.x)
        .bind(new.bbox.y)
        .bind(new.bbox.w)
        .bind(new.bbox.h)
        .bind(&new.landmarks)
        .bind(embedding_literal)
        .bind(new.detect_score)
        .bind(new.quality)
        .bind(&new.model_version)
        .fetch_one(self.db.pool())
        .await?;
        Ok(row.into_domain())
    }

    /// Volti di un asset, riquadri compresi — pannello dettagli foto. Esclude
    /// i falsi positivi rifiutati.
    ///
    /// # Errors
    /// `Forbidden` se l'asset non è visibile al chiamante.
    pub async fn list_for_asset(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
    ) -> Result<Vec<Face>, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;
        let rows: Vec<FaceRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM faces \
              WHERE asset_id = $1 AND rejected_at IS NULL \
              ORDER BY bbox_x"
        ))
        .bind(asset_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(FaceRow::into_domain).collect())
    }

    /// Assegna automaticamente un volto a una persona (raggruppamento
    /// incrementale): NON tocca `assigned_by`/`assigned_at`, che restano
    /// riservati alla decisione umana. Non prende `AuthContext`: pipeline.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn auto_assign(&self, face_id: FaceId, person_id: PersonId) -> Result<(), DbError> {
        let old_person = self.person_of(face_id).await?;
        sqlx::query(
            "UPDATE faces SET person_id = $2, proposed_person_id = NULL, proposed_score = NULL \
              WHERE id = $1 AND assigned_by IS NULL",
        )
        .bind(face_id.as_uuid())
        .bind(person_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        self.recompute_affected_centroids(old_person, Some(person_id))
            .await
    }

    async fn person_of(&self, face_id: FaceId) -> Result<Option<PersonId>, DbError> {
        let row: Option<(Option<uuid::Uuid>,)> =
            sqlx::query_as("SELECT person_id FROM faces WHERE id = $1")
                .bind(face_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        Ok(row.and_then(|(id,)| id).map(PersonId::from_uuid))
    }

    /// Ricalcola i centroidi delle persone toccate da un cambio di
    /// composizione — un volto che entra in una persona, o ne esce, cambia
    /// la media dei suoi embedding confermati. Non fallisce se
    /// `PersonRepo::recompute_centroid` viene chiamato due volte sulla
    /// stessa persona (idempotente: rilegge sempre `faces` da capo).
    async fn recompute_affected_centroids(
        &self,
        old_person: Option<PersonId>,
        new_person: Option<PersonId>,
    ) -> Result<(), DbError> {
        let person_repo = crate::PersonRepo::new(self.db);
        if let Some(id) = old_person {
            person_repo.recompute_centroid(id).await?;
        }
        if let Some(id) = new_person
            && Some(id) != old_person
        {
            person_repo.recompute_centroid(id).await?;
        }
        Ok(())
    }

    /// Propone (senza assegnare) un volto a una persona: distanza dubbia dal
    /// centroide più vicino (spec §4.1). Va in coda di revisione (Task 8).
    /// Non prende `AuthContext`: pipeline.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn propose(
        &self,
        face_id: FaceId,
        person_id: PersonId,
        score: f32,
    ) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE faces SET proposed_person_id = $2, proposed_score = $3 \
              WHERE id = $1 AND assigned_by IS NULL AND person_id IS NULL",
        )
        .bind(face_id.as_uuid())
        .bind(person_id.as_uuid())
        .bind(score)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Assegnazione manuale, dal pannello dettagli di una foto o dalla coda
    /// di revisione: imposta `assigned_by`/`assigned_at`, e da qui in poi il
    /// volto non viene più toccato dal raggruppamento automatico.
    ///
    /// # Errors
    /// `Forbidden` se l'asset del volto non è visibile al chiamante (o senza
    /// utente autenticato — un link pubblico non decide mai sui volti).
    /// `NotFound` se il volto non esiste.
    pub async fn assign(
        &self,
        ctx: &AuthContext,
        face_id: FaceId,
        person_id: PersonId,
    ) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        self.assert_face_visible(ctx, face_id).await?;
        let old_person = self.person_of(face_id).await?;

        let result = sqlx::query(
            "UPDATE faces SET person_id = $2, assigned_by = $3, assigned_at = now(), \
                               rejected_at = NULL, proposed_person_id = NULL, proposed_score = NULL \
              WHERE id = $1",
        )
        .bind(face_id.as_uuid())
        .bind(person_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        self.recompute_affected_centroids(old_person, Some(person_id))
            .await
    }

    /// «Non è un volto»: falso positivo permanente. Sparisce dalla revisione
    /// e non viene mai riproposto da una rianalisi successiva.
    ///
    /// # Errors
    /// Come [`Self::assign`].
    pub async fn reject(&self, ctx: &AuthContext, face_id: FaceId) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        self.assert_face_visible(ctx, face_id).await?;
        let old_person = self.person_of(face_id).await?;

        let result = sqlx::query(
            "UPDATE faces SET rejected_at = now(), assigned_by = $2, assigned_at = now(), \
                               person_id = NULL, proposed_person_id = NULL, proposed_score = NULL \
              WHERE id = $1",
        )
        .bind(face_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        self.recompute_affected_centroids(old_person, None).await
    }

    async fn assert_face_visible(&self, ctx: &AuthContext, face_id: FaceId) -> Result<(), DbError> {
        let asset_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT asset_id FROM faces WHERE id = $1")
                .bind(face_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        let Some(asset_id) = asset_id else {
            return Err(DbError::NotFound);
        };
        AssetRepo::new(self.db)
            .assert_visible(ctx, &[AssetId::from_uuid(asset_id)])
            .await
    }

    /// Volti candidati per il raggruppamento incrementale (Task 5): con
    /// impronta calcolata, non ancora legati a una persona, non rifiutati.
    /// Non prende `AuthContext`: pipeline di sistema.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn list_unassigned_with_embedding(
        &self,
        model_version: &str,
        limit: i64,
    ) -> Result<Vec<Face>, DbError> {
        let rows: Vec<FaceRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM faces \
              WHERE person_id IS NULL AND rejected_at IS NULL AND assigned_by IS NULL \
                AND embedding IS NOT NULL AND model_version = $1 \
              ORDER BY created_at \
              LIMIT $2"
        ))
        .bind(model_version)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(FaceRow::into_domain).collect())
    }

    /// L'embedding di un volto, per il confronto col centroide di una
    /// persona candidata (Task 5). Non prende `AuthContext`: pipeline.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn embedding_of(&self, face_id: FaceId) -> Result<Option<Vec<f32>>, DbError> {
        let raw: Option<(Option<String>,)> =
            sqlx::query_as("SELECT embedding::text FROM faces WHERE id = $1")
                .bind(face_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;
        raw.and_then(|(text,)| text)
            .map(|text| crate::embeddings::parse_vector_text(&text))
            .transpose()
    }

    /// Volti proposti (assegnazione dubbia, in attesa di revisione umana),
    /// filtrati sulla visibilità del chiamante — coda di revisione (Task 8),
    /// stessa forma della coda tag (SP-10).
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato.
    pub async fn list_proposed(&self, ctx: &AuthContext) -> Result<Vec<Face>, DbError> {
        if ctx.user_id().is_none() {
            return Err(DbError::Forbidden);
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let columns: Vec<String> = COLUMNS.split(", ").map(|c| format!("fa.{c}")).collect();
        let rows: Vec<FaceRow> = sqlx::query_as(&format!(
            "SELECT {} FROM faces fa \
             JOIN assets a ON a.id = fa.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE fa.proposed_person_id IS NOT NULL AND fa.person_id IS NULL \
               AND fa.rejected_at IS NULL AND {} \
             ORDER BY fa.proposed_score DESC NULLS LAST, fa.id",
            columns.join(", "),
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().map(FaceRow::into_domain).collect())
    }

    /// Conferma una proposta: assegna il volto alla persona proposta, come
    /// se fosse una decisione umana diretta (Task 8, esito «conferma»).
    ///
    /// # Errors
    /// `Forbidden`/`NotFound` come [`Self::assign`]. `Conflict` se il volto
    /// non ha (più) una proposta in attesa.
    pub async fn confirm_proposal(
        &self,
        ctx: &AuthContext,
        face_id: FaceId,
    ) -> Result<(), DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        self.assert_face_visible(ctx, face_id).await?;

        let target: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT proposed_person_id FROM faces \
              WHERE id = $1 AND proposed_person_id IS NOT NULL AND person_id IS NULL",
        )
        .bind(face_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
        let Some((person_id,)) = target else {
            return Err(DbError::Conflict("face has no pending proposal".to_owned()));
        };

        sqlx::query(
            "UPDATE faces SET person_id = $2, assigned_by = $3, assigned_at = now(), \
                               proposed_person_id = NULL, proposed_score = NULL \
              WHERE id = $1",
        )
        .bind(face_id.as_uuid())
        .bind(person_id)
        .bind(user_id.as_uuid())
        .execute(self.db.pool())
        .await?;
        self.recompute_affected_centroids(None, Some(PersonId::from_uuid(person_id)))
            .await
    }

    /// Numero di volti proposti visibili al chiamante — la metà "volti" del
    /// badge combinato `bootstrap.badges.revision` (Fase 7 ha già la metà
    /// "tag": stesso campo, non uno nuovo).
    ///
    /// # Errors
    /// `Connection` in caso di errore diverso da schema assente.
    pub async fn count_proposed_visible(&self, ctx: &AuthContext) -> Result<i64, DbError> {
        if ctx.user_id().is_none() {
            return Ok(0);
        }
        let status = crate::pgvector::probe_pgvector(self.db).await?;
        if !status.available {
            return Ok(0);
        }
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 1);
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM faces fa \
             JOIN assets a ON a.id = fa.asset_id \
             JOIN folders f ON f.id = a.folder_id \
             WHERE fa.proposed_person_id IS NOT NULL AND fa.person_id IS NULL \
               AND fa.rejected_at IS NULL AND {}",
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_one(self.db.pool())
        .await?;
        Ok(count)
    }

    /// Asset immagine con hash, non ancora passati al rilevatore per
    /// `model_version`, fuori dal sottoalbero Culling, in una libreria con
    /// `faces_enabled`. Stesso pattern di `EmbeddingRepo::list_pending`
    /// (Fase 7), con `asset_face_scans` al posto di `asset_embeddings` come
    /// marcatore — un asset senza volti produce zero righe in `faces`, che
    /// da solo non basta a dire "già analizzato". Non prende `AuthContext`:
    /// pipeline di sistema.
    ///
    /// # Errors
    /// `Connection` se la query fallisce (o se lo schema volti non esiste).
    pub async fn list_pending_scan(
        &self,
        model_version: &str,
        limit: i64,
    ) -> Result<Vec<PendingFaceScan>, DbError> {
        let rows: Vec<(uuid::Uuid, Vec<u8>)> = sqlx::query_as(
            "SELECT a.id, a.content_hash \
             FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             JOIN libraries l ON l.id = f.library_id \
             LEFT JOIN folders cull ON cull.id = l.culling_root_folder_id \
             WHERE a.content_hash IS NOT NULL \
               AND a.kind = 'image' \
               AND l.faces_enabled \
               AND (cull.path IS NULL OR NOT (f.path <@ cull.path)) \
               AND NOT EXISTS ( \
                   SELECT 1 FROM asset_face_scans s \
                   WHERE s.asset_id = a.id AND s.model_version = $1 \
               ) \
             ORDER BY a.id \
             LIMIT $2",
        )
        .bind(model_version)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter()
            .map(|(id, hash)| {
                let content_hash: [u8; 32] = hash
                    .as_slice()
                    .try_into()
                    .map_err(|_| DbError::Corrupted(format!("content_hash len {}", hash.len())))?;
                Ok(PendingFaceScan {
                    asset_id: AssetId::from_uuid(id),
                    content_hash,
                })
            })
            .collect()
    }

    /// Quanti asset immagine (fuori culling, libreria con `faces_enabled`)
    /// restano da passare al rilevatore per `model_version`.
    ///
    /// # Errors
    /// `Connection` / schema volti assente.
    pub async fn count_pending_scan(&self, model_version: &str) -> Result<i64, DbError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint \
             FROM assets a \
             JOIN folders f ON f.id = a.folder_id \
             JOIN libraries l ON l.id = f.library_id \
             LEFT JOIN folders cull ON cull.id = l.culling_root_folder_id \
             WHERE a.content_hash IS NOT NULL \
               AND a.kind = 'image' \
               AND l.faces_enabled \
               AND (cull.path IS NULL OR NOT (f.path <@ cull.path)) \
               AND NOT EXISTS ( \
                   SELECT 1 FROM asset_face_scans s \
                   WHERE s.asset_id = a.id AND s.model_version = $1 \
               )",
        )
        .bind(model_version)
        .fetch_one(self.db.pool())
        .await?;
        Ok(n)
    }

    /// Registra che `asset_id` è stato passato al rilevatore, con o senza
    /// volti trovati — il marcatore che rende `list_pending_scan` corretto
    /// anche per una foto senza nessun volto. Non prende `AuthContext`:
    /// pipeline.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn mark_scanned(
        &self,
        asset_id: AssetId,
        model_version: &str,
    ) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO asset_face_scans (asset_id, model_version) VALUES ($1, $2) \
             ON CONFLICT (asset_id) DO UPDATE SET \
               model_version = EXCLUDED.model_version, scanned_at = now()",
        )
        .bind(asset_id.as_uuid())
        .bind(model_version)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Conferma tutte le proposte in attesa per una persona candidata,
    /// limitate ai volti visibili al chiamante — «conferma tutti» della
    /// coda di revisione (Task 8), stesso pattern di
    /// `AssetTagRepo::confirm_all_for_tag`. Restituisce i volti confermati
    /// da questa chiamata.
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato.
    pub async fn confirm_all_proposed_for_person(
        &self,
        ctx: &AuthContext,
        person_id: PersonId,
    ) -> Result<Vec<FaceId>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 3);

        let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(&format!(
            "UPDATE faces fa SET person_id = $1, assigned_by = $2, assigned_at = now(), \
                                  proposed_person_id = NULL, proposed_score = NULL \
              WHERE fa.proposed_person_id = $1 AND fa.person_id IS NULL \
                AND EXISTS ( \
                  SELECT 1 FROM assets a JOIN folders f ON f.id = a.folder_id \
                   WHERE a.id = fa.asset_id AND {} \
                ) \
              RETURNING fa.id",
            filter.sql()
        ))
        .bind(person_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        let ids: Vec<FaceId> = rows
            .into_iter()
            .map(|(id,)| FaceId::from_uuid(id))
            .collect();
        self.recompute_affected_centroids(None, Some(person_id))
            .await?;
        Ok(ids)
    }

    /// Come [`Self::confirm_all_proposed_for_person`], ma rifiuta — «rifiuta
    /// tutti», permanente come [`Self::reject`].
    ///
    /// # Errors
    /// `Forbidden` senza utente autenticato.
    pub async fn reject_all_proposed_for_person(
        &self,
        ctx: &AuthContext,
        person_id: PersonId,
    ) -> Result<Vec<FaceId>, DbError> {
        let Some(user_id) = ctx.user_id() else {
            return Err(DbError::Forbidden);
        };
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("f.path", "f.library_id", "a.id", 3);

        let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(&format!(
            "UPDATE faces fa SET rejected_at = now(), assigned_by = $2, assigned_at = now(), \
                                  person_id = NULL, proposed_person_id = NULL, proposed_score = NULL \
              WHERE fa.proposed_person_id = $1 AND fa.person_id IS NULL \
                AND EXISTS ( \
                  SELECT 1 FROM assets a JOIN folders f ON f.id = a.folder_id \
                   WHERE a.id = fa.asset_id AND {} \
                ) \
              RETURNING fa.id",
            filter.sql()
        ))
        .bind(person_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id,)| FaceId::from_uuid(id))
            .collect())
    }

    /// «Elimina tutti i dati dei volti» (spec §7, Task 10): distinto
    /// dall'interruttore `libraries.faces_enabled`, che smette di calcolare
    /// ma conserva quanto già raccolto. Questo comando fa piazza pulita di
    /// `faces` (embedding compresi), `persons`, `person_groups` — **globale**,
    /// non per libreria: una persona può avere volti in più librerie (i
    /// cluster non sono mai stati scoperti library-scoped, vedi
    /// `PersonRepo::nearest_centroid`), quindi non esiste un confine di
    /// libreria per questa azione più di quanto ne esista uno per la persona
    /// stessa. Azzera anche `asset_face_scans`: dopo la cancellazione ogni
    /// asset è di nuovo "mai analizzato", non "analizzato ma zero volti" —
    /// altrimenti una libreria che riaccende `faces_enabled` non
    /// ririleverebbe mai nulla.
    ///
    /// # Errors
    /// `Forbidden` per chi non è amministratore — stessa soglia di
    /// `LibraryRepo::delete`, altra azione distruttiva e irreversibile.
    pub async fn delete_all_data(&self, ctx: &AuthContext) -> Result<(), DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let mut tx = self.db.pool().begin().await?;
        sqlx::query("DELETE FROM person_groups")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM persons").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM faces").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM asset_face_scans")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Volti confermati per un insieme di asset (Fase 11 Task 7, SP-3 §11 e
    /// `AssetView`) — "confermato" qui è `person_id IS NOT NULL AND
    /// rejected_at IS NULL`, sia esso assegnato a mano (`assigned_by`
    /// impostato) sia dal raggruppamento automatico: entrambi sono
    /// un'identità stabilita, a differenza di `proposed_person_id` (un
    /// suggerimento non ancora deciso, mai qui). Stesso idioma di
    /// [`crate::FlagRepo::favorites_among`]: una query sola per l'intera
    /// pagina. Mappa vuota — non un errore — se pgvector non è installato:
    /// `faces`/`persons` non esistono affatto in quel caso (migrazione
    /// 0046, stesso no-op già in [`Self::count_proposed_visible`]).
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn confirmed_among(
        &self,
        asset_ids: &[AssetId],
    ) -> Result<HashMap<AssetId, Vec<ConfirmedFace>>, DbError> {
        if asset_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let status = crate::pgvector::probe_pgvector(self.db).await?;
        if !status.available {
            return Ok(HashMap::new());
        }
        let ids: Vec<uuid::Uuid> = asset_ids.iter().map(AssetId::as_uuid).collect();
        let rows: Vec<ConfirmedFaceRow> = sqlx::query_as(
            "SELECT fa.asset_id, p.id AS person_id, p.name AS person_name \
               FROM faces fa JOIN persons p ON p.id = fa.person_id \
              WHERE fa.asset_id = ANY($1) AND fa.person_id IS NOT NULL \
                AND fa.rejected_at IS NULL",
        )
        .bind(&ids)
        .fetch_all(self.db.pool())
        .await?;
        let mut out: HashMap<AssetId, Vec<ConfirmedFace>> = HashMap::new();
        for row in rows {
            out.entry(AssetId::from_uuid(row.asset_id))
                .or_default()
                .push(ConfirmedFace {
                    person_id: PersonId::from_uuid(row.person_id),
                    person_name: row.person_name,
                });
        }
        Ok(out)
    }
}
