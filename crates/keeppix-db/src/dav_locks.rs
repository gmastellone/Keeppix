//! Lock `WebDAV` Class 2 (Task 8, Fase 5). Nessun `AuthContext`: un lock
//! non appartiene a un utente nel senso in cui lo fanno gli asset o le
//! cartelle — è un contratto opaco col client (Finder, Windows Explorer),
//! identificato dal token stesso, non da chi l'ha creato. Lo stesso motivo
//! per cui `LibraryRepo::mark_scanned` non ne prende uno.

use crate::{Db, DbError};

/// TTL di default per un lock nuovo o rinnovato (spec del task: 3600s).
const LOCK_TTL_SECONDS: i64 = 3600;

pub struct DavLockRepo<'a> {
    db: &'a Db,
}

impl<'a> DavLockRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Crea un lock con timeout a [`LOCK_TTL_SECONDS`] da ora.
    ///
    /// # Errors
    /// `Connection` se l'inserimento fallisce (compreso un token duplicato,
    /// impossibile per costruzione perché il token è un `Uuid` v7 nuovo ad
    /// ogni chiamata).
    pub async fn create(
        &self,
        token: &str,
        resource_path: &str,
        owner: Option<&str>,
        depth: &str,
    ) -> Result<(), DbError> {
        // `LOCK_TTL_SECONDS` è una costante del codice, mai un valore
        // arrivato dal client: l'interpolazione qui non riapre la porta
        // alla concatenazione di stringhe che l'invariante di `AGENTS.md`
        // vieta per i valori esterni.
        sqlx::query(&format!(
            "INSERT INTO dav_locks (token, resource_path, owner, depth, timeout_at) \
             VALUES ($1, $2, $3, $4, now() + interval '{LOCK_TTL_SECONDS} seconds')"
        ))
        .bind(token)
        .bind(resource_path)
        .bind(owner)
        .bind(depth)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Rinnova il timeout di un lock ancora attivo. `false` se il token non
    /// esiste o è già scaduto — un lock scaduto non si rinnova, si ricrea
    /// (spec del task: "un lock scaduto è come se non esistesse").
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn refresh(&self, token: &str) -> Result<bool, DbError> {
        let result = sqlx::query(&format!(
            "UPDATE dav_locks SET timeout_at = now() + interval '{LOCK_TTL_SECONDS} seconds' \
              WHERE token = $1 AND timeout_at > now()"
        ))
        .bind(token)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Elimina un lock, esistente o non. Incondizionata: il chiamante che
    /// deve distinguere "c'era ed era attivo" da "non c'era" usa
    /// [`Self::refresh`] prima di chiamare questa (vedi `dav::lock::unlock`,
    /// che sfrutta `refresh` come test-and-set senza dover aggiungere un
    /// quinto metodo non richiesto dalla spec).
    ///
    /// # Errors
    /// `Connection` se la cancellazione fallisce.
    pub async fn delete(&self, token: &str) -> Result<(), DbError> {
        sqlx::query("DELETE FROM dav_locks WHERE token = $1")
            .bind(token)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// C'è un lock attivo (non scaduto) su questo percorso? Usata da `LOCK`
    /// senza `If:` per decidere se creare un nuovo lock o rispondere `423
    /// Locked` — un lock scaduto conta come assente.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn is_locked(&self, resource_path: &str) -> Result<bool, DbError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM dav_locks WHERE resource_path = $1 AND timeout_at > now())",
        )
        .bind(resource_path)
        .fetch_one(self.db.pool())
        .await?;
        Ok(exists)
    }
}
