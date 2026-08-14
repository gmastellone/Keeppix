use crate::ids::UserId;
use crate::user::SystemRole;

/// Chi sta effettuando la richiesta. In Fase 0 esiste solo `User`;
/// `ShareLink` arriva in Fase 3 e passerà per lo stesso `AuthContext`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    User { id: UserId, role: SystemRole },
}

/// Contesto richiesto da ogni repository che legge dati di un utente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    pub actor: Actor,
}

impl AuthContext {
    #[must_use]
    pub const fn user(id: UserId, role: SystemRole) -> Self {
        Self {
            actor: Actor::User { id, role },
        }
    }

    #[must_use]
    #[allow(clippy::unnecessary_wraps)]
    pub const fn user_id(&self) -> Option<UserId> {
        match self.actor {
            Actor::User { id, .. } => Some(id),
        }
    }

    #[must_use]
    pub const fn is_admin(&self) -> bool {
        match self.actor {
            Actor::User { role, .. } => role.is_admin(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_context_reports_admin() {
        let ctx = AuthContext::user(UserId::new(), SystemRole::Admin);
        assert!(ctx.is_admin());
    }

    #[test]
    fn plain_user_context_is_not_admin() {
        let ctx = AuthContext::user(UserId::new(), SystemRole::User);
        assert!(!ctx.is_admin());
    }
}
