//! Convenzioni di mapping fra righe di database e tipi di dominio.
//!
//! Ogni tabella ha una struct `…Row` con `#[derive(sqlx::FromRow)]`, i cui
//! campi portano lo stesso nome delle colonne, e una `into_domain()` che
//! costruisce il tipo di dominio validando ciò che il database non può
//! garantire da solo. Le due responsabilità restano separate: `FromRow` non
//! sa nulla del dominio, `into_domain` non sa nulla di SQL.

use crate::DbError;

/// Errore uniforme per un valore memorizzato che il dominio rifiuta.
/// Usare sempre questo invece di costruire `DbError::Corrupted` a mano, così
/// i messaggi hanno la stessa forma ovunque.
pub(crate) fn corrupted(field: &str, detail: impl std::fmt::Display) -> DbError {
    DbError::Corrupted(format!("stored {field} is invalid: {detail}"))
}
