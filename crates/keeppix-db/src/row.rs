//! Conventions for mapping database rows to domain types.
//!
//! Every table has a `…Row` struct with `#[derive(sqlx::FromRow)]`, whose
//! fields carry the same names as the columns, and an `into_domain()`
//! that builds the domain type, validating what the database cannot
//! guarantee on its own. The two responsibilities stay separate:
//! `FromRow` knows nothing about the domain, `into_domain` knows nothing
//! about SQL.

use crate::DbError;

/// Uniform error for a stored value the domain rejects. Always use this
/// instead of building `DbError::Corrupted` by hand, so messages have the
/// same shape everywhere.
pub(crate) fn corrupted(field: &str, detail: impl std::fmt::Display) -> DbError {
    DbError::Corrupted(format!("stored {field} is invalid: {detail}"))
}
