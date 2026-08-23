use std::path::{Component, Path, PathBuf};

use keeppix_domain::{
    AuthContext, CullingRole, Folder, FolderId, FolderPath, Library, LibraryId, ObjectRole,
};
use sqlx::PgConnection;

use crate::visibility::VisibilityScope;
use crate::{Db, DbError, LibraryRepo};

pub struct FolderRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct FolderRow {
    id: uuid::Uuid,
    library_id: uuid::Uuid,
    parent_id: Option<uuid::Uuid>,
    name: String,
    path: String,
    depth: i32,
    culling_role: Option<String>,
}

impl FolderRow {
    fn into_domain(self) -> Result<Folder, DbError> {
        Ok(Folder {
            id: FolderId::from_uuid(self.id),
            library_id: LibraryId::from_uuid(self.library_id),
            parent_id: self.parent_id.map(FolderId::from_uuid),
            name: self.name,
            path: FolderPath::parse(&self.path)
                .map_err(|e| crate::row::corrupted("folder path", e))?,
            depth: self.depth,
            culling_role: parse_culling_role(self.culling_role.as_deref())?,
        })
    }
}

fn parse_culling_role(raw: Option<&str>) -> Result<Option<CullingRole>, DbError> {
    match raw {
        None => Ok(None),
        Some("taken") => Ok(Some(CullingRole::Taken)),
        Some("skipped") => Ok(Some(CullingRole::Skipped)),
        Some(other) => Err(crate::row::corrupted("folders.culling_role", other)),
    }
}

// `path::text` perché `ltree` non ha una decodifica sqlx: la riga porta una
// String che `FolderPath::parse` valida.
const COLUMNS: &str = "id, library_id, parent_id, name, path::text AS path, depth, culling_role";

async fn load(conn: &mut PgConnection, id: FolderId) -> Result<Option<Folder>, DbError> {
    let row: Option<FolderRow> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM folders WHERE id = $1"))
            .bind(id.as_uuid())
            .fetch_optional(&mut *conn)
            .await?;

    row.map(FolderRow::into_domain).transpose()
}

/// Crea la radice della libreria se non c'è, e la restituisce.
///
/// L'etichetta `ltree` viene dal contatore della libreria, nella stessa
/// istruzione dell'inserimento: l'`UPDATE` blocca la riga di `libraries`, per
/// cui due scansioni concorrenti della stessa libreria si serializzano invece
/// di litigare sull'indice.
async fn ensure_root_on(conn: &mut PgConnection, library_id: LibraryId) -> Result<Folder, DbError> {
    sqlx::query(
        "WITH label AS ( \
             UPDATE libraries SET next_folder_seq = next_folder_seq + 1 \
              WHERE id = $2 \
             RETURNING next_folder_seq - 1 AS seq \
         ) \
         INSERT INTO folders (id, library_id, parent_id, name, path, depth) \
         SELECT $1, $2, NULL::uuid, ''::text, label.seq::text::ltree, 1 FROM label \
         ON CONFLICT (library_id) WHERE parent_id IS NULL DO NOTHING",
    )
    .bind(FolderId::new().as_uuid())
    .bind(library_id.as_uuid())
    .execute(&mut *conn)
    .await?;

    // Rilettura invece del `RETURNING`: con `DO NOTHING` la riga esistente non
    // viene restituita, e l'esistente è esattamente quella che serve.
    let row: Option<FolderRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM folders WHERE library_id = $1 AND parent_id IS NULL"
    ))
    .bind(library_id.as_uuid())
    .fetch_optional(&mut *conn)
    .await?;

    row.ok_or(DbError::NotFound)?.into_domain()
}

/// Come sopra per un figlio: il percorso del genitore viene riletto dal
/// database, così un `Folder` in mano al chiamante da prima di uno spostamento
/// non produce un percorso sbagliato.
async fn ensure_child_on(
    conn: &mut PgConnection,
    parent: &Folder,
    name: &str,
) -> Result<Folder, DbError> {
    sqlx::query(
        "WITH label AS ( \
             UPDATE libraries SET next_folder_seq = next_folder_seq + 1 \
              WHERE id = (SELECT library_id FROM folders WHERE id = $2) \
             RETURNING next_folder_seq - 1 AS seq \
         ) \
         INSERT INTO folders (id, library_id, parent_id, name, path, depth) \
         SELECT $1, p.library_id, p.id, $3::text, p.path || label.seq::text::ltree, p.depth + 1 \
           FROM folders p, label WHERE p.id = $2 \
         ON CONFLICT (parent_id, name) WHERE parent_id IS NOT NULL DO NOTHING",
    )
    .bind(FolderId::new().as_uuid())
    .bind(parent.id.as_uuid())
    .bind(name)
    .execute(&mut *conn)
    .await?;

    let row: Option<FolderRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM folders WHERE parent_id = $1 AND name = $2"
    ))
    .bind(parent.id.as_uuid())
    .bind(name)
    .fetch_optional(&mut *conn)
    .await?;

    row.ok_or(DbError::NotFound)?.into_domain()
}

/// Come `ensure_child_on`, ma marca la cartella con `culling_role` (Fase 9
/// Task 2): `_taken`/`_skipped` non sono riconosciute dal nome, dalla
/// colonna. Auto-guarigione se la cartella esisteva già senza ruolo (creata
/// a mano prima che Keeppix ne avesse bisogno come speciale, o da una
/// versione precedente di questa funzione): l'`UPDATE` dopo l'`INSERT`
/// ignorato la marca comunque, invece di lasciare una `_taken` che si
/// comporta come una cartella qualunque.
async fn ensure_culling_child_on(
    conn: &mut PgConnection,
    parent: &Folder,
    name: &str,
    role: CullingRole,
) -> Result<Folder, DbError> {
    sqlx::query(
        "WITH label AS ( \
             UPDATE libraries SET next_folder_seq = next_folder_seq + 1 \
              WHERE id = (SELECT library_id FROM folders WHERE id = $2) \
             RETURNING next_folder_seq - 1 AS seq \
         ) \
         INSERT INTO folders (id, library_id, parent_id, name, path, depth, culling_role) \
         SELECT $1, p.library_id, p.id, $3::text, p.path || label.seq::text::ltree, p.depth + 1, $4 \
           FROM folders p, label WHERE p.id = $2 \
         ON CONFLICT (parent_id, name) WHERE parent_id IS NOT NULL DO NOTHING",
    )
    .bind(FolderId::new().as_uuid())
    .bind(parent.id.as_uuid())
    .bind(name)
    .bind(role.as_str())
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "UPDATE folders SET culling_role = $3 \
          WHERE parent_id = $1 AND name = $2 AND culling_role IS NULL",
    )
    .bind(parent.id.as_uuid())
    .bind(name)
    .bind(role.as_str())
    .execute(&mut *conn)
    .await?;

    let row: Option<FolderRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM folders WHERE parent_id = $1 AND name = $2"
    ))
    .bind(parent.id.as_uuid())
    .bind(name)
    .fetch_optional(&mut *conn)
    .await?;

    row.ok_or(DbError::NotFound)?.into_domain()
}

impl<'a> FolderRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Radice della libreria, creandola se non esiste.
    ///
    /// Non prende un `AuthContext` perché la chiama lo scanner, che non agisce
    /// per conto di un utente — come `LibraryRepo::mark_scanned`.
    ///
    /// # Errors
    /// `NotFound` se la libreria non esiste.
    pub async fn ensure_root(&self, library_id: LibraryId) -> Result<Folder, DbError> {
        let mut conn = self.db.pool().acquire().await?;
        ensure_root_on(&mut conn, library_id).await
    }

    /// Figlio di nome `name`, creandolo se non esiste.
    ///
    /// Idempotente anche sotto concorrenza: `ON CONFLICT DO NOTHING` più
    /// rilettura, non `SELECT` seguito da `INSERT`. Senza `AuthContext` per la
    /// stessa ragione di `ensure_root`.
    ///
    /// # Errors
    /// `NotFound` se il genitore non esiste più.
    pub async fn ensure_child(&self, parent: &Folder, name: &str) -> Result<Folder, DbError> {
        let mut conn = self.db.pool().acquire().await?;
        ensure_child_on(&mut conn, parent, name).await
    }

    /// `_taken`/`_skipped` dentro un lotto di culling (Fase 9 Task 2),
    /// creata se non c'è, sempre marcata dal `culling_role` giusto — mai
    /// riconosciuta dal nome.
    ///
    /// # Errors
    /// Come `ensure_child`.
    pub async fn ensure_culling_child(
        &self,
        parent: &Folder,
        role: CullingRole,
    ) -> Result<Folder, DbError> {
        let name = match role {
            CullingRole::Taken => "_taken",
            CullingRole::Skipped => "_skipped",
        };
        let mut conn = self.db.pool().acquire().await?;
        ensure_culling_child_on(&mut conn, parent, name, role).await
    }

    /// Crea l'intera catena `relative` sotto la radice della libreria e
    /// restituisce l'ultima cartella. Senza `AuthContext`: la chiama lo
    /// scanner.
    ///
    /// Tutto in una transazione: una scansione interrotta a metà non lascia
    /// rami a cui manca un pezzo.
    ///
    /// # Errors
    /// `NotFound` se la libreria non esiste.
    pub async fn ensure_path(
        &self,
        library_id: LibraryId,
        relative: &[&str],
    ) -> Result<Folder, DbError> {
        let mut tx = self.db.pool().begin().await?;

        let mut current = ensure_root_on(&mut tx, library_id).await?;
        for name in relative {
            current = ensure_child_on(&mut tx, &current, name).await?;
        }

        tx.commit().await?;
        Ok(current)
    }

    /// # Errors
    /// `Forbidden` se il chiamante non vede la libreria della cartella — anche
    /// quando l'id non esiste, per non offrire un oracolo di esistenza.
    /// `NotFound` solo a un admin che chiede un id inesistente.
    pub async fn find_by_id(&self, ctx: &AuthContext, id: FolderId) -> Result<Folder, DbError> {
        Ok(self.visible(ctx, id).await?.0)
    }

    /// Come `find_by_id`, più la libreria che la contiene — stesso cancello
    /// di visibilità, non quello più stretto owner/admin di
    /// `LibraryRepo::find_by_id`. Serve a `CullingRepo::set_pick` (Fase 9
    /// Task 4) per leggere `culling_root_folder_id` senza restringere a
    /// owner/admin chi può impostare scelto/scartato: quel permesso resta
    /// quello di sempre (visibilità), lo spostamento fisico dentro un lotto
    /// lo stringe da sé tramite `AssetRepo::move_asset`.
    ///
    /// # Errors
    /// Come `find_by_id`.
    pub async fn find_with_library(
        &self,
        ctx: &AuthContext,
        id: FolderId,
    ) -> Result<(Folder, Library), DbError> {
        self.visible(ctx, id).await
    }

    /// Permesso di scrittura sulla cartella: owner della libreria, admin,
    /// oppure `editor` esplicito via `PermissionRepo::effective_role` — lo
    /// stesso cancello di `move_subtree` e di `UploadSessionRepo::create`,
    /// ora riusabile senza duplicarlo una terza volta per `WebDAV`
    /// `PUT`/`MKCOL`/`MOVE`/`COPY` (Task 7, Fase 5).
    ///
    /// # Errors
    /// `Forbidden` se il chiamante vede la cartella ma non può scriverci (un
    /// viewer, o nessun permesso) — mai `NotFound`, per non offrire un
    /// oracolo di esistenza a chi sonda un id che non gli appartiene.
    /// Altrimenti come `find_by_id`.
    pub async fn assert_editor(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<(Folder, Library), DbError> {
        let (folder, library) = self.visible(ctx, folder_id).await?;
        if !ctx.is_admin() && ctx.user_id() != Some(library.owner_id) {
            match crate::PermissionRepo::new(self.db)
                .effective_role(ctx, folder_id)
                .await?
            {
                Some(ObjectRole::Editor) => {}
                _ => return Err(DbError::Forbidden),
            }
        }
        Ok((folder, library))
    }

    /// Albero visibile al chiamante, in ordine di `path`.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn tree(&self, ctx: &AuthContext) -> Result<Vec<Folder>, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("folders.path", "folders.library_id", "NULL::uuid", 1);
        let rows: Vec<FolderRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM folders WHERE {} ORDER BY path",
            filter.sql()
        ))
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(FolderRow::into_domain).collect()
    }

    /// Radici della foresta visibile: librerie possedute per l'owner,
    /// cartelle concesse per chi ha uno share. Non l'albero intero.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn roots(&self, ctx: &AuthContext) -> Result<Vec<Folder>, DbError> {
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("folders.path", "folders.library_id", "NULL::uuid", 1);
        let sql = if scope.is_unrestricted() {
            format!(
                "SELECT {COLUMNS} FROM folders WHERE parent_id IS NULL AND {} ORDER BY name",
                filter.sql()
            )
        } else {
            format!(
                "SELECT {COLUMNS} FROM folders WHERE id = ANY($1::uuid[]) AND {} ORDER BY name",
                filter.sql()
            )
        };
        let rows: Vec<FolderRow> = sqlx::query_as(&sql)
            .bind(filter.bind())
            .bind(filter.holes())
            .bind(filter.assets())
            .fetch_all(self.db.pool())
            .await?;

        rows.into_iter().map(FolderRow::into_domain).collect()
    }

    /// Figli diretti, in ordine di nome.
    ///
    /// # Errors
    /// Come `find_by_id`.
    pub async fn children(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<Vec<Folder>, DbError> {
        self.visible(ctx, folder_id).await?;

        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("folders.path", "folders.library_id", "NULL::uuid", 2);
        let rows: Vec<FolderRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM folders WHERE parent_id = $1 AND {} ORDER BY name",
            filter.sql()
        ))
        .bind(folder_id.as_uuid())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(FolderRow::into_domain).collect()
    }

    /// La cartella e tutti i suoi discendenti: una condizione indicizzata
    /// (`path <@ prefisso`) invece di una ricorsione.
    ///
    /// # Errors
    /// Come `find_by_id`.
    pub async fn subtree(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<Vec<Folder>, DbError> {
        let (folder, _) = self.visible(ctx, folder_id).await?;
        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        let filter = scope.filter("folders.path", "folders.library_id", "NULL::uuid", 3);

        let rows: Vec<FolderRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM folders \
              WHERE library_id = $1 AND path <@ $2::text::ltree AND {} \
              ORDER BY path",
            filter.sql()
        ))
        .bind(folder.library_id.as_uuid())
        .bind(folder.path.as_str())
        .bind(filter.bind())
        .bind(filter.holes())
        .bind(filter.assets())
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(FolderRow::into_domain).collect()
    }

    /// Sposta la cartella, e con lei tutto il sottoalbero, sotto `new_parent`.
    ///
    /// I percorsi dei discendenti si riscrivono con **una** UPDATE: spostare
    /// una cartella con 40.000 foto tocca le righe di `folders`, non quelle di
    /// `assets`, ed è la ragione per cui nessun asset porta un percorso
    /// assoluto denormalizzato.
    ///
    /// # Errors
    /// `Conflict` se `new_parent` sta nel sottoalbero da spostare (compresa la
    /// cartella stessa): il sottoalbero si scollegherebbe e non sarebbe più
    /// raggiungibile da nessuna radice. `Conflict` anche se le due cartelle
    /// stanno in librerie diverse, o se il nuovo genitore ha già un figlio con
    /// quel nome. Altrimenti come `find_by_id`.
    pub async fn move_subtree(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
        new_parent: FolderId,
    ) -> Result<(), DbError> {
        let (folder, library) = self.visible(ctx, folder_id).await?;
        self.visible(ctx, new_parent).await?;
        if !ctx.is_admin() && ctx.user_id() != Some(library.owner_id) {
            match crate::PermissionRepo::new(self.db)
                .effective_role(ctx, folder_id)
                .await?
            {
                Some(keeppix_domain::ObjectRole::Editor) => {}
                _ => return Err(DbError::Forbidden),
            }
        }

        let mut tx = self.db.pool().begin().await?;

        // La riga di `libraries` è il lucchetto dell'albero: la prendono anche
        // gli `ensure_*`, incrementando il contatore. Senza, fra il controllo
        // del ciclo e l'UPDATE un altro spostamento potrebbe rendere
        // `new_parent` un discendente, e il sottoalbero si scollegherebbe.
        // Un solo lucchetto per libreria, quindi nessun deadlock.
        sqlx::query("SELECT 1 FROM libraries WHERE id = $1 FOR UPDATE")
            .bind(folder.library_id.as_uuid())
            .execute(&mut *tx)
            .await?;

        // Riletti sotto lucchetto: i percorsi visti prima possono essere già
        // vecchi.
        let folder = match load(&mut tx, folder_id).await? {
            Some(folder) => folder,
            None if ctx.is_admin() => return Err(DbError::NotFound),
            None => return Err(DbError::Forbidden),
        };
        let parent = match load(&mut tx, new_parent).await? {
            Some(parent) => parent,
            None if ctx.is_admin() => return Err(DbError::NotFound),
            None => return Err(DbError::Forbidden),
        };

        if parent.library_id != folder.library_id {
            return Err(DbError::Conflict(
                "a folder cannot be moved to another library".to_owned(),
            ));
        }
        // Il controllo va prima dell'UPDATE. `is_descendant_of` ha la
        // semantica di `<@`, quindi copre anche lo spostamento su se stessa;
        // e siccome ogni cartella discende dalla radice, la radice non è
        // spostabile.
        if parent.path.is_descendant_of(&folder.path) {
            return Err(DbError::Conflict(
                "a folder cannot be moved inside its own subtree".to_owned(),
            ));
        }

        // `subpath(path, nlevel(old) - 1)` tiene l'etichetta della cartella
        // spostata e tutto ciò che le sta sotto, e la riattacca al nuovo
        // genitore. Le etichette sono uniche per libreria, quindi il nuovo
        // percorso non può collidere.
        sqlx::query(
            "UPDATE folders \
                SET path  = $3::text::ltree || subpath(path, nlevel($2::text::ltree) - 1), \
                    depth = nlevel($3::text::ltree) + nlevel(path) \
                            - nlevel($2::text::ltree) + 1, \
                    parent_id = CASE WHEN id = $4 THEN $5 ELSE parent_id END \
              WHERE library_id = $1 AND path <@ $2::text::ltree",
        )
        .bind(folder.library_id.as_uuid())
        .bind(folder.path.as_str())
        .bind(parent.path.as_str())
        .bind(folder.id.as_uuid())
        .bind(parent.id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sibling_name_conflict)?;

        tx.commit().await?;
        Ok(())
    }

    /// Rimuove la cartella e tutto il suo sottoalbero **solo dal
    /// database** (`WebDAV DELETE`, Task 8 Fase 5): il chiamante deve aver
    /// già cestinato ogni asset del sottoalbero (`TrashRepo::choose` con
    /// `DiskAction::MovedToTrash` — mai un `rm -rf`) prima di chiamare
    /// questo metodo, e rimuove la directory fisica separatamente dopo,
    /// come `dav::delete::folder`.
    ///
    /// # Errors
    /// `403` se il chiamante non è editor sulla cartella. Altrimenti come
    /// `assert_editor`.
    pub async fn delete_subtree(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<(), DbError> {
        let (folder, _library) = self.assert_editor(ctx, folder_id).await?;
        sqlx::query("DELETE FROM folders WHERE library_id = $1 AND path <@ $2::text::ltree")
            .bind(folder.library_id.as_uuid())
            .bind(folder.path.as_str())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Percorso su disco della cartella, ricostruito risalendo l'albero.
    ///
    /// I nomi vengono dal database, mai dal client.
    ///
    /// # Errors
    /// `Corrupted` se un nome memorizzato non è un singolo componente di
    /// percorso. Altrimenti come `find_by_id`.
    pub async fn absolute_path(
        &self,
        ctx: &AuthContext,
        folder_id: FolderId,
    ) -> Result<PathBuf, DbError> {
        let (_folder, library) = self.visible(ctx, folder_id).await?;
        self.join_folder_path(library.root_path, folder_id).await
    }

    /// Come `absolute_path`, per lo scanner: niente `AuthContext`.
    ///
    /// # Errors
    /// `NotFound` se la cartella non esiste; `Corrupted` su un nome illegale.
    pub async fn absolute_path_for_scan(&self, folder_id: FolderId) -> Result<PathBuf, DbError> {
        let mut conn = self.db.pool().acquire().await?;
        let Some(folder) = load(&mut conn, folder_id).await? else {
            return Err(DbError::NotFound);
        };
        drop(conn);
        let library = LibraryRepo::new(self.db)
            .load_for_scan(folder.library_id)
            .await?;
        self.join_folder_path(library.root_path, folder_id).await
    }

    async fn join_folder_path(
        &self,
        mut path: PathBuf,
        folder_id: FolderId,
    ) -> Result<PathBuf, DbError> {
        let names: Vec<String> = sqlx::query_scalar(
            "WITH RECURSIVE up AS ( \
                 SELECT id, parent_id, name, 0 AS lvl FROM folders WHERE id = $1 \
                 UNION ALL \
                 SELECT p.id, p.parent_id, p.name, up.lvl + 1 \
                   FROM folders p JOIN up ON p.id = up.parent_id \
             ) \
             SELECT name FROM up WHERE name <> '' ORDER BY lvl DESC",
        )
        .bind(folder_id.as_uuid())
        .fetch_all(self.db.pool())
        .await?;
        for name in names {
            if !is_single_component(&name) {
                return Err(crate::row::corrupted("folder name", name));
            }
            path.push(name);
        }
        Ok(path)
    }

    /// Risolve la visibilità dallo scope (prefissi di cartella), non dalla
    /// sola proprietà della libreria: una cartella condivisa deve essere
    /// raggiungibile. `Forbidden` prima di `NotFound`.
    /// Restituisce anche la libreria, che serve a `absolute_path`.
    async fn visible(&self, ctx: &AuthContext, id: FolderId) -> Result<(Folder, Library), DbError> {
        let mut conn = self.db.pool().acquire().await?;

        let Some(folder) = load(&mut conn, id).await? else {
            return Err(if ctx.is_admin() {
                DbError::NotFound
            } else {
                DbError::Forbidden
            });
        };
        drop(conn);

        let scope = VisibilityScope::resolve(self.db, ctx).await?;
        if !scope.allows(folder.library_id, folder.path.as_str()) {
            return Err(if ctx.is_admin() {
                DbError::NotFound
            } else {
                DbError::Forbidden
            });
        }

        let library = LibraryRepo::new(self.db)
            .load_for_scan(folder.library_id)
            .await?;
        Ok((folder, library))
    }
}

/// Un nome di cartella deve essere un solo componente: né vuoto, né `.`, né
/// `..`, né contenente separatori.
fn is_single_component(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}

fn map_sibling_name_conflict(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("the destination already has a folder with this name".to_owned());
    }
    DbError::Connection(err)
}

#[cfg(test)]
mod tests {
    use super::is_single_component;

    #[test]
    fn a_plain_name_is_a_single_component() {
        assert!(is_single_component("Matrimonio Rossi"));
        assert!(is_single_component("2024"));
    }

    #[test]
    fn traversal_and_separators_are_rejected() {
        // Se uno di questi passasse, `absolute_path` uscirebbe dalla libreria.
        assert!(!is_single_component(""));
        assert!(!is_single_component("."));
        assert!(!is_single_component(".."));
        assert!(!is_single_component("../etc"));
        assert!(!is_single_component("a/b"));
        assert!(!is_single_component("/etc"));
    }
}
