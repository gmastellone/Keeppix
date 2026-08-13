//! Tipi ed entità pure di Keeppix. Nessun I/O, nessun SQL, nessuna rete.

pub mod auth;
pub mod error;
pub mod ids;
pub mod password;
pub mod token;
pub mod user;

pub use auth::{Actor, AuthContext};
pub use error::DomainError;
pub use ids::{GroupId, UserId};
pub use password::{Password, PasswordHash, hash_password, verify_password};
pub use token::SessionToken;
pub use user::{NewUser, SystemRole, User, Username};
