#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use harness::TestDb;
use keeppix_db::{GroupRepo, NewGrant, ObjectType, PermissionRepo, SubjectType};
use keeppix_domain::{AuthContext, ObjectRole, SystemRole, UserId};

async fn grant_folder(test: &TestDb, granter: UserId, group_id: uuid::Uuid, folder_id: uuid::Uuid) {
    PermissionRepo::new(test.db())
        .grant(
            &AuthContext::user(granter, SystemRole::Admin),
            NewGrant {
                subject: SubjectType::Group,
                subject_id: group_id,
                object: ObjectType::Folder,
                object_id: folder_id,
                role: ObjectRole::Viewer,
                inherit: true,
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn deleting_a_group_with_active_permissions_is_refused_or_confirmed() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let group = GroupRepo::new(test.db())
        .create(&ctx, "Famiglia")
        .await
        .unwrap();
    grant_folder(&test, admin, group.id.as_uuid(), uuid::Uuid::now_v7()).await;

    let err = GroupRepo::new(test.db())
        .delete(&ctx, group.id, false)
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Conflict(_)));

    GroupRepo::new(test.db())
        .delete(&ctx, group.id, true)
        .await
        .unwrap();
}

#[tokio::test]
async fn group_names_are_unique_case_insensitively() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    GroupRepo::new(test.db())
        .create(&ctx, "Famiglia")
        .await
        .unwrap();
    let err = GroupRepo::new(test.db())
        .create(&ctx, "famiglia")
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Conflict(_)));
}

#[tokio::test]
async fn removing_the_last_member_leaves_the_group_usable() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let user = harness::seed_user(&test, admin, "mario").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let group = GroupRepo::new(test.db())
        .create(&ctx, "Solo")
        .await
        .unwrap();
    GroupRepo::new(test.db())
        .add_member(&ctx, group.id, user)
        .await
        .unwrap();
    GroupRepo::new(test.db())
        .remove_member(&ctx, group.id, user)
        .await
        .unwrap();
    let listed = GroupRepo::new(test.db()).list(&ctx).await.unwrap();
    assert!(listed.iter().any(|g| g.id == group.id));
}
