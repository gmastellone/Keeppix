use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use keeppix_domain::{AssetId, AuthContext, DiskAction, TrashEntry, TrashEntryId, UserId};
use sqlx::PgConnection;

use crate::{AssetRepo, Db, DbError, FolderRepo};

/// Nome della cartella del cestino, dentro la radice della libreria.
/// **Deve** restare identico a quello escluso dal walker
/// (`keeppix_media::walk::is_excluded_name`): un disallineamento qui
/// produrrebbe un ciclo infinito di reindicizzazione su una libreria grande.
pub const TRASH_DIR_NAME: &str = ".keeppix-trash";

const COLUMNS: &str = "id, asset_id, deleted_by, deleted_at, original_path, trash_path, \
                       disk_action, restored_at";

#[derive(sqlx::FromRow)]
struct EntryRow {
    id: uuid::Uuid,
    asset_id: uuid::Uuid,
    deleted_by: Option<uuid::Uuid>,
    deleted_at: DateTime<Utc>,
    original_path: String,
    trash_path: Option<String>,
    disk_action: String,
    restored_at: Option<DateTime<Utc>>,
}

impl EntryRow {
    fn into_domain(self) -> Result<TrashEntry, DbError> {
        Ok(TrashEntry {
            id: TrashEntryId::from_uuid(self.id),
            asset_id: AssetId::from_uuid(self.asset_id),
            deleted_by: self.deleted_by.map(UserId::from_uuid),
            deleted_at: self.deleted_at,
            original_path: self.original_path,
            trash_path: self.trash_path,
            disk_action: DiskAction::parse(&self.disk_action)
                .map_err(|e| crate::row::corrupted("trash_entries.disk_action", e))?,
            restored_at: self.restored_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    id: uuid::Uuid,
    asset_id: uuid::Uuid,
    original_path: String,
    trash_path: Option<String>,
}

#[derive(sqlx::FromRow)]
struct LibraryInfo {
    root_path: String,
    owner_id: uuid::Uuid,
}

pub struct TrashRepo<'a> {
    db: &'a Db,
}

impl<'a> TrashRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Applica una delle tre opzioni di cancellazione (spec §6) a un asset,
    /// e registra sempre una riga di audit in `trash_entries`.
    ///
    /// # Errors
    /// `Forbidden` se il chiamante non vede l'asset — anche quando l'id non
    /// esiste — o se chiede [`DiskAction::Purged`] senza essere il
    /// proprietario della libreria o un admin. `Io` se l'operazione sul
    /// filesystem fallisce.
    pub async fn choose(
        &self,
        ctx: &AuthContext,
        asset_id: AssetId,
        action: DiskAction,
    ) -> Result<TrashEntry, DbError> {
        // Cancello comune alle tre opzioni: senza visibilità sull'asset
        // nessuna delle tre è ammessa. È l'aggancio che la Fase 3 estenderà
        // a chi ha visibilità condivisa (editor/viewer) senza toccare
        // questo metodo.
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;

        let asset = AssetRepo::new(self.db).get_for_scan(asset_id).await?;
        let library = library_info_for_folder(self.db, asset.folder_id).await?;

        // Secondo cancello, più stretto e solo per `Purged`: la cancellazione
        // dal disco resta di owner/admin anche quando la Fase 3 concederà ad
        // altri la visibilità sull'asset (design §4.2: «un editor non può
        // distruggere file»). Oggi, senza condivisione, coincide con il
        // primo controllo — ma è un cancello distinto apposta, non lo stesso
        // scritto due volte.
        if matches!(action, DiskAction::Purged)
            && !may_purge(ctx, UserId::from_uuid(library.owner_id))
        {
            return Err(DbError::Forbidden);
        }

        let folder_abs = FolderRepo::new(self.db)
            .absolute_path_for_scan(asset.folder_id)
            .await?;
        let original_path = folder_abs.join(asset.filename.as_str());

        let entry_id = TrashEntryId::new();
        let deleted_by = ctx.user_id().map(|id| id.as_uuid());

        let row = match action {
            DiskAction::Kept => {
                let mut tx = self.db.pool().begin().await?;
                let row = insert_entry(
                    &mut tx,
                    entry_id,
                    asset_id,
                    deleted_by,
                    &original_path,
                    None,
                    action,
                )
                .await?;
                sqlx::query("DELETE FROM assets WHERE id = $1")
                    .bind(asset_id.as_uuid())
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                row
            }
            DiskAction::Purged => {
                remove_file_tolerant(&original_path)?;
                let mut tx = self.db.pool().begin().await?;
                let row = insert_entry(
                    &mut tx,
                    entry_id,
                    asset_id,
                    deleted_by,
                    &original_path,
                    None,
                    action,
                )
                .await?;
                sqlx::query("DELETE FROM assets WHERE id = $1")
                    .bind(asset_id.as_uuid())
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                row
            }
            DiskAction::MovedToTrash => {
                let root = PathBuf::from(&library.root_path);
                let trash_path = move_into_trash(
                    &root,
                    &folder_abs,
                    &original_path,
                    entry_id,
                    asset.filename.as_str(),
                )?;
                let mut tx = self.db.pool().begin().await?;
                let row = insert_entry(
                    &mut tx,
                    entry_id,
                    asset_id,
                    deleted_by,
                    &original_path,
                    Some(&trash_path),
                    action,
                )
                .await?;
                sqlx::query(
                    "UPDATE assets SET status = 'trashed', updated_at = now() WHERE id = $1",
                )
                .bind(asset_id.as_uuid())
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                row
            }
        };

        row.into_domain()
    }

    /// Riporta al percorso originale il file più recentemente spostato nel
    /// cestino per questo asset, e l'asset a `indexed`.
    ///
    /// **Non sovrascrive**: se il percorso originale è di nuovo occupato —
    /// da un altro file, o da un ripristino concorrente — l'operazione
    /// fallisce con `Conflict` senza toccare nulla.
    ///
    /// # Errors
    /// `Forbidden` come [`Self::choose`]. `Conflict` se l'asset non ha un
    /// cestinamento pendente, o se il percorso originale è occupato. `Io` se
    /// il `rename()` fallisce.
    pub async fn restore(&self, ctx: &AuthContext, asset_id: AssetId) -> Result<(), DbError> {
        AssetRepo::new(self.db)
            .assert_visible(ctx, std::slice::from_ref(&asset_id))
            .await?;

        let pending: Option<PendingRow> = sqlx::query_as(
            "SELECT id, asset_id, original_path, trash_path FROM trash_entries \
              WHERE asset_id = $1 AND disk_action = 'moved_to_trash' AND restored_at IS NULL \
              ORDER BY deleted_at DESC LIMIT 1",
        )
        .bind(asset_id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;

        let Some(pending) = pending else {
            return Err(DbError::Conflict("asset is not in the trash".to_owned()));
        };
        let Some(trash_path) = pending.trash_path else {
            return Err(crate::row::corrupted(
                "trash_entries.trash_path",
                "missing on a moved_to_trash row",
            ));
        };

        let original = PathBuf::from(&pending.original_path);
        if original.exists() {
            return Err(DbError::Conflict(
                "the original location is occupied by another file".to_owned(),
            ));
        }

        std::fs::rename(&trash_path, &original).map_err(|e| {
            DbError::Io(format!(
                "restoring {trash_path} to {}: {e}",
                original.display()
            ))
        })?;

        let mut tx = self.db.pool().begin().await?;
        sqlx::query("UPDATE assets SET status = 'indexed', updated_at = now() WHERE id = $1")
            .bind(asset_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        // La condizione `restored_at IS NULL` chiude la corsa fra due
        // ripristini concorrenti sullo stesso cestinamento: solo il primo a
        // arrivare qui lo marca, ma entrambi hanno già superato il controllo
        // "il percorso originale è libero" sopra — un residuo noto e
        // documentato, non risolto in questo task (vedi ledger).
        sqlx::query(
            "UPDATE trash_entries SET restored_at = now() \
              WHERE id = $1 AND restored_at IS NULL",
        )
        .bind(pending.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Pulizia notturna del cestino (spec §6): ogni `moved_to_trash` ancora
    /// pendente più vecchio di `before` viene cancellato dal disco e la sua
    /// riga rimossa. Non ancora agganciata a un job schedulato (fuori dai
    /// file di questo task) — chi la chiamerà passerà `Utc::now() -
    /// Duration::days(30)`.
    ///
    /// Non prende un `AuthContext`: è manutenzione di sistema su tutte le
    /// librerie, come `LibraryRepo::mark_scanned`.
    ///
    /// # Errors
    /// `Connection` se una query fallisce. Un singolo file che non si
    /// riesce a cancellare (permessi, disco già staccato) non abortisce il
    /// giro: viene loggato e quella riga resta per il prossimo tentativo.
    pub async fn cleanup_expired(&self, before: DateTime<Utc>) -> Result<u64, DbError> {
        let rows: Vec<PendingRow> = sqlx::query_as(
            "SELECT id, asset_id, original_path, trash_path FROM trash_entries \
              WHERE disk_action = 'moved_to_trash' AND restored_at IS NULL AND deleted_at < $1",
        )
        .bind(before)
        .fetch_all(self.db.pool())
        .await?;

        let mut cleaned = 0u64;
        for row in rows {
            if let Some(trash_path) = &row.trash_path
                && let Err(e) = std::fs::remove_file(trash_path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(error = %e, path = %trash_path, "trash cleanup: cannot remove file, retrying later");
                continue;
            }

            let mut tx = self.db.pool().begin().await?;
            sqlx::query("DELETE FROM assets WHERE id = $1 AND status = 'trashed'")
                .bind(row.asset_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM trash_entries WHERE id = $1")
                .bind(row.id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            cleaned += 1;
        }
        Ok(cleaned)
    }
}

async fn library_info_for_folder(
    db: &Db,
    folder_id: keeppix_domain::FolderId,
) -> Result<LibraryInfo, DbError> {
    let row: Option<LibraryInfo> = sqlx::query_as(
        "SELECT l.root_path, l.owner_id FROM folders f JOIN libraries l ON l.id = f.library_id \
          WHERE f.id = $1",
    )
    .bind(folder_id.as_uuid())
    .fetch_optional(db.pool())
    .await?;
    row.ok_or(DbError::NotFound)
}

async fn insert_entry(
    tx: &mut PgConnection,
    id: TrashEntryId,
    asset_id: AssetId,
    deleted_by: Option<uuid::Uuid>,
    original_path: &Path,
    trash_path: Option<&Path>,
    action: DiskAction,
) -> Result<EntryRow, DbError> {
    let row: EntryRow = sqlx::query_as(&format!(
        "INSERT INTO trash_entries (id, asset_id, deleted_by, original_path, trash_path, disk_action) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLUMNS}"
    ))
    .bind(id.as_uuid())
    .bind(asset_id.as_uuid())
    .bind(deleted_by)
    .bind(original_path.to_string_lossy().as_ref())
    .bind(trash_path.map(|p| p.to_string_lossy().into_owned()))
    .bind(action.as_str())
    .fetch_one(&mut *tx)
    .await?;
    Ok(row)
}

/// Solo owner e admin possono chiedere [`DiskAction::Purged`] (spec §4.2:
/// «un editor non può distruggere file»). Estratta come funzione pura,
/// separata dalla risoluzione async di libreria/visibilità, così la regola
/// si può pinnare con un test diretto senza passare dal database — la
/// visibilità odierna (solo owner o admin, nessuna condivisione prima della
/// Fase 3) renderebbe altrimenti questo cancello indistinguibile dal
/// controllo di visibilità che lo precede in [`TrashRepo::choose`].
fn may_purge(ctx: &AuthContext, library_owner: UserId) -> bool {
    ctx.is_admin() || ctx.user_id() == Some(library_owner)
}

fn remove_file_tolerant(path: &Path) -> Result<(), DbError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(DbError::Io(format!("removing {}: {e}", path.display()))),
    }
}

/// Sposta `original_path` dentro `<library_root>/.keeppix-trash/`,
/// preservando il sottopercorso relativo (così il cestino resta navigabile
/// a mano) e prefissando il nome con `entry_id` — univoco per costruzione,
/// quindi nessuna collisione possibile con un file già cestinato con lo
/// stesso nome, senza bisogno di un controllo "esiste già?" a rischio di
/// corsa.
///
/// `rename()`, non copia: `.keeppix-trash/` sta sempre dentro la stessa
/// libreria, quindi sempre sullo stesso filesystem — l'inode non cambia.
fn move_into_trash(
    library_root: &Path,
    folder_abs: &Path,
    original_path: &Path,
    entry_id: TrashEntryId,
    filename: &str,
) -> Result<PathBuf, DbError> {
    let relative_dir = folder_abs
        .strip_prefix(library_root)
        .unwrap_or(Path::new(""));
    let target_dir = library_root.join(TRASH_DIR_NAME).join(relative_dir);
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| DbError::Io(format!("creating {}: {e}", target_dir.display())))?;

    let trash_path = target_dir.join(format!("{entry_id}__{filename}"));
    std::fs::rename(original_path, &trash_path).map_err(|e| {
        DbError::Io(format!(
            "moving {} to {}: {e}",
            original_path.display(),
            trash_path.display()
        ))
    })?;
    Ok(trash_path)
}

#[cfg(test)]
mod tests {
    use keeppix_domain::SystemRole;

    use super::*;

    #[test]
    fn an_admin_may_purge_a_library_they_do_not_own() {
        let stranger_admin = AuthContext::user(UserId::new(), SystemRole::Admin);
        assert!(may_purge(&stranger_admin, UserId::new()));
    }

    #[test]
    fn the_owner_may_purge_their_own_library_even_without_the_admin_role() {
        let owner = UserId::new();
        let ctx = AuthContext::user(owner, SystemRole::User);
        assert!(may_purge(&ctx, owner));
    }

    #[test]
    fn a_plain_user_who_is_neither_owner_nor_admin_may_not_purge() {
        let ctx = AuthContext::user(UserId::new(), SystemRole::User);
        assert!(!may_purge(&ctx, UserId::new()));
    }
}
