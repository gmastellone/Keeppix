use std::collections::HashMap;

use keeppix_domain::{FolderId, StackId};
use uuid::Uuid;

use crate::{Db, DbError};

/// Vale il valore di `assets.kind` scritto dalla migrazione 0005: usato per
/// preferire il RAW come primario senza tirare in ballo `AssetKind` di
/// dominio solo per un confronto di stringa.
const RAW_IMAGE: &str = "raw_image";

pub struct StackRepo<'a> {
    db: &'a Db,
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
