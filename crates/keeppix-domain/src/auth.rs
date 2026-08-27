use uuid::Uuid;

use crate::ids::UserId;
use crate::user::SystemRole;

/// Who is making the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    User {
        id: UserId,
        role: SystemRole,
    },
    /// Access via a public link. The context is confined to the link's
    /// object.
    ShareLink {
        link_id: Uuid,
        object_type: String,
        object_id: Uuid,
        allow_download: bool,
        allow_original: bool,
        hide_metadata: bool,
        allow_upload: bool,
        upload_quota_bytes: Option<i64>,
    },
}

/// Parameters for building an `Actor::ShareLink`. Groups the booleans to
/// avoid functions with more than 3 bools (clippy
/// `clippy::too_many_bool_params`).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareLinkParams {
    pub object_type: String,
    pub object_id: Uuid,
    pub allow_download: bool,
    pub allow_original: bool,
    pub hide_metadata: bool,
    pub allow_upload: bool,
    pub upload_quota_bytes: Option<i64>,
}

/// Context required by every repository that reads a user's data.
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
                upload_quota_bytes: params.upload_quota_bytes,
            },
        }
    }

    /// `Some` only for `User` actors; a public link has no `user_id`.
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
                upload_quota_bytes: None,
            },
        );
        assert!(!ctx.is_admin());
        assert!(ctx.is_share_link());
        assert!(ctx.user_id().is_none());
    }
}
