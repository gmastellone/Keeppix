use uuid::Uuid;

use crate::ids::UserId;
use crate::user::SystemRole;

/// Chi sta effettuando la richiesta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    User {
        id: UserId,
        role: SystemRole,
    },
    /// Accesso tramite link pubblico (Fase 3). Il contesto è confinato
    /// all'oggetto del link.
    ShareLink {
        link_id: Uuid,
        object_type: String,
        object_id: Uuid,
        allow_download: bool,
        allow_original: bool,
        hide_metadata: bool,
        allow_upload: bool,
    },
}

/// Parametri per costruire un `Actor::ShareLink`. Raggruppa i booleani per
/// evitare funzioni con più di 3 bool (clippy `clippy::too_many_bool_params`).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareLinkParams {
    pub object_type: String,
    pub object_id: Uuid,
    pub allow_download: bool,
    pub allow_original: bool,
    pub hide_metadata: bool,
    pub allow_upload: bool,
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
    pub fn share_link(link_id: Uuid, params: ShareLinkParams) -> Self {
        Self {
            actor: Actor::ShareLink {
                link_id,
                object_type: params.object_type,
                object_id: params.object_id,
                allow_download: params.allow_download,
                allow_original: params.allow_original,
                hide_metadata: params.hide_metadata,
                allow_upload: params.allow_upload,
            },
        }
    }

    /// `Some` solo per attori `User`; un link pubblico non ha `user_id`.
    #[must_use]
    pub const fn user_id(&self) -> Option<UserId> {
        match self.actor {
            Actor::User { id, .. } => Some(id),
            Actor::ShareLink { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_admin(&self) -> bool {
        match self.actor {
            Actor::User { role, .. } => role.is_admin(),
            Actor::ShareLink { .. } => false,
        }
    }

    #[must_use]
    pub const fn is_share_link(&self) -> bool {
        matches!(self.actor, Actor::ShareLink { .. })
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

    #[test]
    fn share_link_is_not_admin() {
        let ctx = AuthContext::share_link(
            Uuid::now_v7(),
            ShareLinkParams {
                object_type: "folder".into(),
                object_id: Uuid::now_v7(),
                allow_download: true,
                allow_original: false,
                hide_metadata: true,
                allow_upload: false,
            },
        );
        assert!(!ctx.is_admin());
        assert!(ctx.is_share_link());
        assert!(ctx.user_id().is_none());
    }
}
