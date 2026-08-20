use std::collections::HashMap;

use chrono::{DateTime, Utc};
use keeppix_domain::{Asset, AssetId, AuthContext, FolderId, StackId};
use uuid::Uuid;

use crate::assets::{A_COLUMNS, AssetRow};
use crate::{AssetRepo, Db, DbError};

/// Vale il valore di `assets.kind` scritto dalla migrazione 0005: usato per
/// preferire il RAW come primario senza tirare in ballo `AssetKind` di
/// dominio solo per un confronto di stringa.
const RAW_IMAGE: &str = "raw_image";

/// Badge additivo di stack per la vista di browse (fase-10, Task 3):
/// `stack_size == 1` per un asset non impilato. `raw_kind` distingue le tre
/// composizioni che l'interfaccia mostra come badge (SP-15): `"raw"`,
/// `"jpeg"`, `"raw+jpeg"` — `None` solo per kind che non sono né l'uno né
/// l'altro (video, unknown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackBadge {
    pub stack_size: u16,
    pub raw_kind: Option<String>,
}

/// Un [`Asset`] con il suo [`StackBadge`]: usato dove la vista di browse
/// mostra solo il primario di ogni pila (timeline, ricerca — Task 3).
/// Il `Deref` verso `Asset` lascia inalterato il codice che legge solo i
/// campi dell'asset (id, `taken_at_utc`, cursori, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetWithStack {
    pub asset: Asset,
    pub stack: StackBadge,
}

impl std::ops::Deref for AssetWithStack {
    type Target = Asset;

    fn deref(&self) -> &Asset {
        &self.asset
    }
}

/// `LEFT JOIN` che porta il primario dello stack (per filtrare) di ogni
/// riga `assets a`. Da solo (senza [`STACK_BADGE_JOIN_SQL`]) per le query
/// che devono solo escludere i non-primari senza mostrare il badge
/// (geometria): un join in meno da pianificare.
pub(crate) const STACK_PRIMARY_JOIN_SQL: &str = "LEFT JOIN stacks s ON s.id = a.stack_id";

/// Riga esclusa se è un membro non-primario di uno stack. Richiede
/// [`STACK_PRIMARY_JOIN_SQL`] (o [`STACK_BADGE_JOIN_SQL`], che lo include)
/// nella query.
pub(crate) const STACK_PRIMARY_ONLY_SQL: &str = "(a.stack_id IS NULL OR a.id = s.primary_asset_id)";

/// Come [`STACK_PRIMARY_JOIN_SQL`], più un aggregato laterale sui membri
/// dello stack per calcolare `stack_size`/`raw_kind` — eseguito solo per le
/// righe che superano `WHERE`/`LIMIT` (non per ogni candidato), quindi il
/// costo resta legato al numero di tessere restituite, non alla scansione.
pub(crate) const STACK_BADGE_JOIN_SQL: &str = "LEFT JOIN stacks s ON s.id = a.stack_id \
     LEFT JOIN LATERAL ( \
         SELECT count(*)::int2 AS stack_size, \
                CASE WHEN bool_or(m.kind = 'raw_image') AND bool_or(m.kind = 'image') \
                     THEN 'raw+jpeg' \
                     WHEN bool_or(m.kind = 'raw_image') THEN 'raw' \
                     WHEN bool_or(m.kind = 'image') THEN 'jpeg' \
                     ELSE NULL END AS raw_kind \
           FROM assets m WHERE m.stack_id = a.stack_id \
     ) si ON a.stack_id IS NOT NULL";

/// Colonne aggiuntive da affiancare ad [`A_COLUMNS`] per ottenere
/// `stack_size`/`raw_kind` nella stessa riga. Un asset non impilato deriva
/// il badge dal proprio `kind` (nessun aggregato da leggere); uno impilato
/// legge l'aggregato calcolato da [`STACK_BADGE_JOIN_SQL`].
pub(crate) const STACK_BADGE_COLUMNS_SQL: &str = "CASE WHEN a.stack_id IS NULL THEN 1::int2 ELSE si.stack_size END AS stack_size, \
     CASE WHEN a.stack_id IS NULL THEN \
         CASE a.kind WHEN 'raw_image' THEN 'raw' WHEN 'image' THEN 'jpeg' ELSE NULL END \
     ELSE si.raw_kind END AS raw_kind";

/// Riga grezza di una query `{A_COLUMNS}, {STACK_BADGE_COLUMNS_SQL}`: stesso
/// pattern di `AlbumAssetRow` in `albums.rs` — un secondo tipo con tutti i
/// campi di `AssetRow` più le colonne extra, convertito passando per
/// `AssetRow::from_raw` perché i campi di `AssetRow` sono privati al modulo
/// `assets`.
#[derive(sqlx::FromRow)]
pub(crate) struct AssetStackRow {
    id: Uuid,
    folder_id: Uuid,
    filename: String,
    content_hash: Option<Vec<u8>>,
    size_bytes: i64,
    mtime: DateTime<Utc>,
    inode: Option<i64>,
    kind: String,
    status: String,
    taken_at_utc: Option<DateTime<Utc>>,
    width: Option<i32>,
    height: Option<i32>,
    thumbhash: Option<Vec<u8>>,
    created_at: DateTime<Utc>,
    stack_size: i16,
    raw_kind: Option<String>,
}

impl AssetStackRow {
    pub(crate) fn into_domain(self) -> Result<AssetWithStack, DbError> {
        let stack_size = u16::try_from(self.stack_size).unwrap_or(1).max(1);
        let raw_kind = self.raw_kind;
        let asset = AssetRow::from_raw(
            self.id,
            self.folder_id,
            self.filename,
            self.content_hash,
            self.size_bytes,
            self.mtime,
            self.inode,
            self.kind,
            self.status,
            self.taken_at_utc,
            self.width,
            self.height,
            self.thumbhash,
            self.created_at,
        )
        .into_domain()?;
        Ok(AssetWithStack {
            asset,
            stack: StackBadge {
                stack_size,
                raw_kind,
            },
        })
    }
}

pub struct StackRepo<'a> {
    db: &'a Db,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackMember {
    pub asset: Asset,
    pub is_primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackDetails {
    pub stack_id: StackId,
    pub primary_asset_id: AssetId,
    pub members: Vec<StackMember>,
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    id: Uuid,
    filename: String,
    kind: String,
    stack_id: Option<Uuid>,
}

impl<'a> StackRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Raggruppa gli asset non cestinati di una cartella per nome base
    /// (spec §5, regola 1): `DSC_0042.ARW` e `DSC_0042.JPG` finiscono nello
    /// stesso stack, con il RAW come primario quando presente. Un file da
    /// solo, per quanto il suo nome base sia unico nella cartella, non
    /// forma mai uno stack.
    ///
    /// Idempotente: rieseguirla sugli stessi file riusa lo stack già
    /// esistente invece di crearne uno nuovo — è la proprietà critica per
    /// le riscansioni (senza, ogni scansione produrrebbe uno stack nuovo,
    /// vedi il piano di Fase 2, Task 6). La cancellazione o lo
    /// spostamento fuori dallo stack di un membro — incluso il primario —
    /// è gestita dal trigger `assets_promote_stack_primary` (migrazione
    /// 0013), non da questo metodo: un `DELETE` fatto altrove (cestino,
    /// scanner) deve tenere l'invariante senza dover richiamare
    /// `StackRepo`.
    ///
    /// Non prende un `AuthContext`: la chiamerà lo scanner su un'intera
    /// cartella dopo averne scritto gli asset, come
    /// `LibraryRepo::mark_scanned`.
    ///
    /// # Errors
    /// `Connection` se una query fallisce.
    pub async fn regroup_folder(&self, folder_id: FolderId) -> Result<(), DbError> {
        let members: Vec<MemberRow> = sqlx::query_as(
            "SELECT id, filename, kind, stack_id FROM assets \
              WHERE folder_id = $1 AND status <> 'trashed'",
        )
        .bind(folder_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;

        let mut groups: HashMap<String, Vec<MemberRow>> = HashMap::new();
        for member in members {
            groups
                .entry(basename_key(&member.filename))
                .or_default()
                .push(member);
        }

        let mut tx = self.db.pool().begin().await?;
        for mut group in groups.into_values() {
            // Ordine deterministico: decide il primario di riserva quando
            // non c'è un RAW, e rende il raggruppamento riproducibile.
            group.sort_by(|a, b| a.filename.cmp(&b.filename));

            if group.len() < 2 {
                unstack_lone_member(&mut tx, &group).await?;
                continue;
            }

            let primary = group
                .iter()
                .find(|m| m.kind == RAW_IMAGE)
                .unwrap_or(&group[0])
                .id;

            let mut existing_ids: Vec<Uuid> = group.iter().filter_map(|m| m.stack_id).collect();
            existing_ids.sort_unstable();
            existing_ids.dedup();

            // Un solo id preesistente fra i membri: è lo stesso gruppo di
            // un raggruppamento precedente, va riusato — non ricreato.
            // Zero o più di uno: un nuovo stack (più di uno è un caso
            // anomalo che questo task non prova a riconciliare, vedi
            // ledger).
            let stack_id = if let [only] = existing_ids.as_slice() {
                *only
            } else {
                let id = StackId::new().as_uuid();
                sqlx::query("INSERT INTO stacks (id, primary_asset_id) VALUES ($1, $2)")
                    .bind(id)
                    .bind(primary)
                    .execute(&mut *tx)
                    .await?;
                id
            };

            sqlx::query(
                "UPDATE stacks SET primary_asset_id = $2 \
                  WHERE id = $1 AND primary_asset_id IS DISTINCT FROM $2",
            )
            .bind(stack_id)
            .bind(primary)
            .execute(&mut *tx)
            .await?;

            let member_ids: Vec<Uuid> = group.iter().map(|m| m.id).collect();
            sqlx::query(
                "UPDATE assets SET stack_id = $2, updated_at = now() \
                  WHERE id = ANY($1) AND stack_id IS DISTINCT FROM $2",
            )
            .bind(&member_ids)
            .bind(stack_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Membri dello stack a cui appartiene `asset_id`. `None` se l'asset non
    /// è in uno stack.
    ///
    /// # Errors
    /// `Forbidden` se l'asset non è visibile al chiamante (anche
    /// inesistente). `Connection` se una query fallisce.
    pub async fn members(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
    ) -> Result<Option<StackDetails>, DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;

        let stack_id: Option<Uuid> =
            sqlx::query_scalar("SELECT stack_id FROM assets WHERE id = $1")
                .bind(asset_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;

        let Some(stack_id) = stack_id else {
            return Ok(None);
        };

        let primary: Uuid = sqlx::query_scalar("SELECT primary_asset_id FROM stacks WHERE id = $1")
            .bind(stack_id)
            .fetch_one(self.db.pool())
            .await?;

        let sql = format!(
            "SELECT {A_COLUMNS} FROM assets a \
              WHERE a.stack_id = $1 AND a.status <> 'trashed' \
              ORDER BY a.filename"
        );
        let rows: Vec<AssetRow> = sqlx::query_as(&sql)
            .bind(stack_id)
            .fetch_all(self.db.pool())
            .await?;

        let mut members = Vec::with_capacity(rows.len());
        for row in rows {
            let row_id = row.id();
            let is_primary = row_id == primary;
            members.push(StackMember {
                asset: row.into_domain()?,
                is_primary,
            });
        }

        Ok(Some(StackDetails {
            stack_id: StackId::from_uuid(stack_id),
            primary_asset_id: AssetId::from_uuid(primary),
            members,
        }))
    }

    /// Imposta `asset_id` come primario del suo stack.
    ///
    /// # Errors
    /// `Forbidden` come [`Self::members`]. `Conflict` se l'asset non è in
    /// uno stack. `Connection` se una query fallisce.
    pub async fn set_primary(&self, ctx: &AuthContext, asset_id: AssetId) -> Result<(), DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;

        let stack_id: Option<Uuid> =
            sqlx::query_scalar("SELECT stack_id FROM assets WHERE id = $1 AND status <> 'trashed'")
                .bind(asset_id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;

        let Some(stack_id) = stack_id else {
            return Err(DbError::Conflict("asset is not in a stack".to_owned()));
        };

        sqlx::query("UPDATE stacks SET primary_asset_id = $2 WHERE id = $1")
            .bind(stack_id)
            .bind(asset_id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}

/// Un nome base ormai unico nella cartella (era in uno stack, ma non lo
/// giustifica più — l'ultimo altro membro è sparito) si scollega. Il
/// trigger `assets_promote_stack_primary` fa il resto: se questo membro
/// era il primario e non ne resta un altro, cancella la riga di `stacks`
/// invece di lasciarla orfana.
async fn unstack_lone_member(
    tx: &mut sqlx::PgConnection,
    group: &[MemberRow],
) -> Result<(), DbError> {
    for member in group {
        if member.stack_id.is_some() {
            sqlx::query("UPDATE assets SET stack_id = NULL, updated_at = now() WHERE id = $1")
                .bind(member.id)
                .execute(&mut *tx)
                .await?;
        }
    }
    Ok(())
}

/// Nome base per il raggruppamento: il nome del file senza l'ultima
/// estensione, case-insensitive. `DSC_0042.ARW` e `dsc_0042.jpg` sono lo
/// stesso scatto anche se maiuscole/minuscole differiscono fra fotocamera
/// e software che ha scritto il JPEG.
fn basename_key(filename: &str) -> String {
    filename
        .rsplit_once('.')
        .map_or(filename, |(base, _ext)| base)
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::basename_key;

    #[test]
    fn strips_the_last_extension_case_insensitively() {
        assert_eq!(basename_key("DSC_0042.ARW"), "dsc_0042");
        assert_eq!(basename_key("DSC_0042.JPG"), "dsc_0042");
    }

    #[test]
    fn a_filename_without_an_extension_is_its_own_basename() {
        assert_eq!(basename_key("README"), "readme");
    }
}
