mod harness;

use harness::TestDb;
use keeppix_db::{
    FolderRepo, GroupRepo, LibraryRepo, NewGrant, ObjectType, PermissionRepo, SubjectType,
    UserRepo, VisibilityScope,
};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};

fn new_library(name: &str, path: &str, owner: keeppix_domain::UserId) -> NewLibrary {
    NewLibrary {
        name: name.to_owned(),
        owner_id: owner,
        root_path: std::path::PathBuf::from(path),
        exclude_patterns: vec![],
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn an_admin_has_unrestricted_scope() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let scope = VisibilityScope::resolve(test.db(), &ctx).await.unwrap();

    assert!(scope.is_unrestricted());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_plain_user_sees_only_its_libraries() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());
    let folders = FolderRepo::new(test.db());

    let admin_lib = repo
        .create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();
    let mario_lib = repo
        .create(&admin_ctx, new_library("Mario", "/mnt/m", mario))
        .await
        .unwrap();
    let admin_root = folders.ensure_path(admin_lib.id, &[]).await.unwrap();
    let mario_root = folders.ensure_path(mario_lib.id, &[]).await.unwrap();

    let scope = VisibilityScope::resolve(test.db(), &AuthContext::user(mario, SystemRole::User))
        .await
        .unwrap();

    assert!(!scope.is_unrestricted());
    assert!(scope.allows(mario_lib.id, mario_root.path.as_str()));
    assert!(!scope.allows(admin_lib.id, admin_root.path.as_str()));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_user_with_no_libraries_matches_zero_rows() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let admin_lib = LibraryRepo::new(test.db())
        .create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();
    FolderRepo::new(test.db())
        .ensure_path(admin_lib.id, &[])
        .await
        .unwrap();

    let scope = VisibilityScope::resolve(test.db(), &AuthContext::user(mario, SystemRole::User))
        .await
        .unwrap();

    assert!(
        scope
            .filter("folders.path", "folders.library_id", "NULL::uuid", 1)
            .bind()
            .is_some_and(<[uuid::Uuid]>::is_empty)
    );
    assert!(!scope.is_unrestricted());

    let filter = scope.filter("folders.path", "folders.library_id", "NULL::uuid", 1);
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM folders WHERE {}",
        filter.sql()
    ))
    .bind(filter.bind())
    .bind(filter.holes())
    .bind(filter.assets())
    .fetch_one(test.db().pool())
    .await
    .expect("an empty scope is not an error");
    assert_eq!(n, 0);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn scope_updates_when_a_library_is_created() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);

    let before = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(
        before
            .filter("folders.path", "folders.library_id", "NULL::uuid", 1)
            .bind()
            .is_some_and(<[uuid::Uuid]>::is_empty)
    );

    let created = LibraryRepo::new(test.db())
        .create(&admin_ctx, new_library("Mario", "/mnt/m", mario))
        .await
        .unwrap();
    let root = FolderRepo::new(test.db())
        .ensure_path(created.id, &[])
        .await
        .unwrap();

    let after = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert_eq!(
        after
            .filter("folders.path", "folders.library_id", "NULL::uuid", 1)
            .bind()
            .unwrap(),
        [root.id.as_uuid()].as_slice()
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn scope_drops_a_revoked_permission_even_after_the_scope_was_cached() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let library = LibraryRepo::new(test.db())
        .create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &["Privato"])
        .await
        .unwrap();
    let granted = PermissionRepo::new(test.db())
        .grant(
            &admin_ctx,
            NewGrant {
                subject: SubjectType::User,
                subject_id: mario.as_uuid(),
                object: ObjectType::Folder,
                object_id: folder.id.as_uuid(),
                role: keeppix_domain::ObjectRole::Viewer,
                inherit: true,
            },
        )
        .await
        .unwrap();

    let before = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(before.allows(library.id, folder.path.as_str()));

    sqlx::query("DELETE FROM permissions WHERE id = $1")
        .bind(granted.id)
        .execute(test.db().pool())
        .await
        .unwrap();

    let stale = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(
        stale.allows(library.id, folder.path.as_str()),
        "bypassing the repo must leave the cached scope stale until explicit invalidation"
    );

    let granted = PermissionRepo::new(test.db())
        .grant(
            &admin_ctx,
            NewGrant {
                subject: SubjectType::User,
                subject_id: mario.as_uuid(),
                object: ObjectType::Folder,
                object_id: folder.id.as_uuid(),
                role: keeppix_domain::ObjectRole::Viewer,
                inherit: true,
            },
        )
        .await
        .unwrap();
    PermissionRepo::new(test.db())
        .revoke(&admin_ctx, granted.id)
        .await
        .unwrap();

    let after = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(
        !after.allows(library.id, folder.path.as_str()),
        "a revoked permission must disappear even if the previous scope was cached"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn scope_updates_when_a_user_role_changes_after_being_cached() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let library = LibraryRepo::new(test.db())
        .create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();
    let root = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();

    let before = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(!before.is_unrestricted());
    assert!(!before.allows(library.id, root.path.as_str()));

    sqlx::query("UPDATE users SET role = 'admin', updated_at = now() WHERE id = $1")
        .bind(mario.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();
    let stale = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(
        !stale.allows(library.id, root.path.as_str()),
        "the cached scope for a non-admin must stay stale until the role-change invalidation runs"
    );

    UserRepo::new(test.db())
        .update_profile(&admin_ctx, mario, None, None, Some(SystemRole::User))
        .await
        .unwrap();
    UserRepo::new(test.db())
        .update_profile(&admin_ctx, mario, None, None, Some(SystemRole::Admin))
        .await
        .unwrap();

    let after = VisibilityScope::resolve(test.db(), &AuthContext::user(mario, SystemRole::Admin))
        .await
        .unwrap();
    assert!(
        after.is_unrestricted(),
        "an admin role change must invalidate cached scope"
    );
    assert!(after.allows(library.id, root.path.as_str()));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn scope_drops_group_access_when_a_member_is_removed_after_being_cached() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let library = LibraryRepo::new(test.db())
        .create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &["Condivisa"])
        .await
        .unwrap();
    let group = GroupRepo::new(test.db())
        .create(&admin_ctx, "Famiglia")
        .await
        .unwrap();
    GroupRepo::new(test.db())
        .add_member(&admin_ctx, group.id, mario)
        .await
        .unwrap();
    PermissionRepo::new(test.db())
        .grant(
            &admin_ctx,
            NewGrant {
                subject: SubjectType::Group,
                subject_id: group.id.as_uuid(),
                object: ObjectType::Folder,
                object_id: folder.id.as_uuid(),
                role: keeppix_domain::ObjectRole::Viewer,
                inherit: true,
            },
        )
        .await
        .unwrap();

    let before = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(before.allows(library.id, folder.path.as_str()));

    sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
        .bind(group.id.as_uuid())
        .bind(mario.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let stale = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(
        stale.allows(library.id, folder.path.as_str()),
        "bypassing the repo must leave the cached membership-derived scope stale"
    );

    GroupRepo::new(test.db())
        .add_member(&admin_ctx, group.id, mario)
        .await
        .unwrap();
    GroupRepo::new(test.db())
        .remove_member(&admin_ctx, group.id, mario)
        .await
        .unwrap();

    let after = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(
        !after.allows(library.id, folder.path.as_str()),
        "group member removal must invalidate the cached scope"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn scope_for_a_deleted_user_is_evicted_before_their_id_can_be_reused() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let library = LibraryRepo::new(test.db())
        .create(&admin_ctx, new_library("Mario", "/mnt/m", mario))
        .await
        .unwrap();
    let root = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();

    let before = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(before.allows(library.id, root.path.as_str()));

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(mario.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let stale = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(
        stale.allows(library.id, root.path.as_str()),
        "bypassing the app must leave the cached user scope stale until explicit eviction"
    );

    test.db().invalidate_permission_cache_for_user(mario).await;
    let after = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(
        !after.allows(library.id, root.path.as_str()),
        "deleting a user must evict any cached scope tied to that user id"
    );
}
