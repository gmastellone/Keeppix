mod harness;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use harness::{TestServer, assert_security_headers};
use tower::ServiceExt as _;

/// The document doesn't touch the database: it's generated from
/// annotations on the types. Careful: this router only mounts `/health`
/// and `/api/openapi.json`, not `/api/v1` — querying the real routes
/// requires `TestServer`.
fn app() -> axum::Router {
    keeppix_api::router_without_state()
}

/// The methods that count as operations in an `OpenAPI` 3.1 Path Item
/// Object; the other keys (`summary`, `parameters`, `servers`, …) don't.
const HTTP_METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::too_many_lines)]
async fn openapi_document_is_served_and_complete() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(doc["openapi"].as_str().unwrap(), "3.1.0");
    assert_eq!(doc["info"]["title"], "Keeppix API");
    assert_eq!(doc["info"]["version"], "1.0.0");

    for path in [
        "/api/v1/setup/status",
        "/api/v1/setup",
        "/api/v1/auth/login",
        "/api/v1/auth/refresh",
        "/api/v1/auth/logout",
        "/api/v1/auth/me",
        "/api/v1/auth/totp",
        "/api/v1/auth/totp/setup",
        "/api/v1/auth/totp/confirm",
        "/api/v1/auth/totp/recovery-codes",
        "/api/v1/timeline/buckets",
        "/api/v1/timeline/geometry",
        "/api/v1/timeline",
        "/api/v1/folders/tree",
        "/api/v1/folders/{id}",
        "/api/v1/folders/{id}/children",
        "/media/thumb/{hash}",
        "/media/preview/{hash}",
        "/media/full/{hash}",
        "/media/original/{id}",
        "/media/video/{id}/playback",
        "/media/video/{id}/hls/{file}",
        "/media/video/{id}/poster",
        "/api/v1/viewport",
        "/api/v1/search",
        "/api/v1/search/suggest",
        "/api/v1/places/reverse",
        "/api/v1/places/suggest",
        "/api/v1/map/clusters",
        "/api/v1/saved-searches",
        "/api/v1/ws",
        "/api/v1/ws/ticket",
        "/api/v1/sync/delta",
        "/api/v1/problems",
        "/api/v1/duplicates",
        "/api/v1/duplicates/{content_hash}",
        "/api/v1/duplicates/{content_hash}/resolve",
        "/api/v1/assets/{id}",
        "/api/v1/assets/{id}/restore",
        "/api/v1/assets/{id}/stack",
        "/api/v1/assets/{id}/stack/primary",
        "/api/v1/trash",
        "/api/v1/trash/empty",
        "/api/v1/assets/{id}/metadata",
        "/api/v1/metadata/batch",
        "/api/v1/metadata/batch/copy-location",
        "/api/v1/metadata/batch/import-gpx",
        "/api/v1/metadata/batch/recalculate-timezones",
        "/api/v1/metadata/batch/recalculate-timezones/preview",
        "/api/v1/metadata/batch/shift-taken-at",
        "/api/v1/metadata/batch/{batch_id}/undo",
        "/api/v1/assets/{id}/flags",
        "/api/v1/flags/batch",
        "/api/v1/users/me/app-passwords",
        "/api/v1/users/me/app-passwords/{id}",
        "/api/v1/libraries/{id}/probe",
        "/health",
        "/api/v1/bootstrap",
        "/api/v1/operations/{id}/cancel",
        "/api/v1/albums",
        "/api/v1/albums/{id}",
        "/api/v1/albums/{id}/assets",
        "/api/v1/albums/{id}/assets/{asset_id}",
        "/api/v1/albums/{id}/assets/{asset_id}/position",
        "/api/v1/albums/{id}/refresh",
        "/api/v1/groups",
        "/api/v1/groups/{id}",
        "/api/v1/groups/{id}/members",
        "/api/v1/groups/{group_id}/members/{user_id}",
        "/api/v1/permissions",
        "/api/v1/permissions/explain",
        "/api/v1/permissions/{id}",
        "/api/v1/shared-with-me",
        "/api/v1/share/links",
        "/api/v1/share/links/{id}",
        "/api/v1/guest-uploads/{id}/approve",
        "/api/v1/share/{token}",
        "/api/v1/share/{token}/assets",
        "/api/v1/share/{token}/auth",
        "/api/v1/share/{token}/uploads",
        "/api/v1/audit",
        "/api/v1/backup/preferences",
        "/api/v1/backup/destinations",
        "/api/v1/backup/destinations/{id}",
        "/api/v1/backup/destinations/{id}/test",
        "/api/v1/backup/runs",
        "/api/v1/backup/run",
        "/api/v1/restore/inspect",
        "/api/v1/restore",
        "/api/v1/upload/check",
        "/api/v1/upload",
        "/api/v1/upload/{id}",
    ] {
        assert!(doc["paths"][path].is_object(), "missing path {path}");
    }

    assert!(doc["components"]["schemas"]["UserView"].is_object());
    assert_eq!(
        doc["paths"]["/api/v1/auth/login"]["post"]["responses"]["200"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/LoginResponse"
    );
    assert_eq!(
        doc["paths"]["/api/v1/auth/me"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/MeResponse"
    );
    assert_eq!(
        doc["paths"]["/api/v1/setup"]["post"]["responses"]["201"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/SetupResponse"
    );
}

/// Pins the spot that's easy to get wrong: the route must be mounted
/// **inside** `common_layers`'s argument, not chained after its call. In
/// the latter case the `.layer(...)` calls wouldn't wrap it and the
/// document would come out without CSP, nosniff, referrer-policy, and
/// permissions-policy — the same bug the 404 fallback already pins in
/// `tests/health.rs`.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn openapi_document_carries_the_security_headers() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_security_headers(response.headers());
}

/// Ties the document to the routes that are actually mounted. Each
/// operation's path and method are strings written by hand inside
/// `#[utoipa::path]`: nothing in the compiler checks them against
/// `lib.rs`'s `Router::route(...)` calls, so without this test a typo in
/// the attribute would publish an operation that doesn't exist — or
/// exists under a different method — and the whole suite would stay
/// green. Every operation is called against the **real, stateful**
/// router: a 404 status means the path doesn't exist, 405 means the path
/// is right but the method is wrong. Any other outcome is fine: this test
/// doesn't verify handler logic, only that the described HTTP surface
/// exists.
///
/// **Direction not covered: a route mounted but undocumented.** That
/// would require enumerating the router's routes, and axum 0.8 doesn't
/// expose its own table (`Router` has no introspection API). Someone
/// adding a `route(...)` without the matching `#[utoipa::path]` gets no
/// warning here: that check has to happen in review, or in CI by
/// comparing the document against the route list once axum makes that
/// readable.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn documented_operations_are_all_mounted() {
    let server = TestServer::start().await;

    let doc: serde_json::Value = server
        .client
        .get(server.url("/api/openapi.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let mut checked = 0_usize;
    for (path, item) in doc["paths"].as_object().unwrap() {
        for (method, _) in item.as_object().unwrap() {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }

            let verb = reqwest::Method::from_bytes(method.to_uppercase().as_bytes()).unwrap();
            let status = server
                .client
                .request(verb, server.url(path))
                .send()
                .await
                .unwrap()
                .status();

            assert_ne!(
                status,
                reqwest::StatusCode::NOT_FOUND,
                "the document declares {method} {path}, but that path isn't mounted"
            );
            assert_ne!(
                status,
                reqwest::StatusCode::METHOD_NOT_ALLOWED,
                "the document declares {method} {path}, but the route doesn't accept that method"
            );
            checked += 1;
        }
    }

    // Without this, an empty document — or a `paths` that stops being an
    // object of operations — would make the test pass with a loop that
    // never ran.
    assert_eq!(
        checked, 184,
        "the document must describe one hundred eighty-four operations"
    );
}

/// Security scheme names are literals inside `#[utoipa::path]` and cannot
/// reference `openapi::SESSION_SCHEME`: if the two spellings diverge, the
/// document declares `security` against a nonexistent scheme, and a
/// client generator won't know which credential to send. This checks
/// that every requirement points to a declared scheme, that the protected
/// routes carry it, and that the cookie described is really the one the
/// extractor reads.
#[test]
#[allow(clippy::unwrap_used, clippy::too_many_lines)]
fn security_requirements_name_a_declared_scheme() {
    let doc =
        serde_json::to_value(<keeppix_api::openapi::ApiDoc as utoipa::OpenApi>::openapi()).unwrap();

    let schemes = doc["components"]["securitySchemes"].as_object().unwrap();
    let session = &schemes[keeppix_api::openapi::SESSION_SCHEME];
    assert_eq!(session["type"], "apiKey");
    assert_eq!(session["in"], "cookie");
    assert_eq!(session["name"], keeppix_api::SESSION_COOKIE);

    let mut protected = Vec::new();
    for (path, item) in doc["paths"].as_object().unwrap() {
        for (method, operation) in item.as_object().unwrap() {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }
            let Some(requirements) = operation["security"].as_array() else {
                continue;
            };
            for requirement in requirements {
                for name in requirement.as_object().unwrap().keys() {
                    assert!(
                        schemes.contains_key(name),
                        "{method} {path} requires scheme {name}, which is not declared in components"
                    );
                }
            }
            protected.push(path.clone());
        }
    }

    protected.sort();
    assert_eq!(
        protected,
        [
            "/api/v1/albums",
            "/api/v1/albums",
            "/api/v1/albums/{id}",
            "/api/v1/albums/{id}",
            "/api/v1/albums/{id}",
            "/api/v1/albums/{id}/assets",
            "/api/v1/albums/{id}/assets/{asset_id}",
            "/api/v1/albums/{id}/assets/{asset_id}",
            "/api/v1/albums/{id}/assets/{asset_id}/position",
            "/api/v1/albums/{id}/refresh",
            "/api/v1/assets/batch/delete",
            "/api/v1/assets/batch/move",
            "/api/v1/assets/batch/rename",
            "/api/v1/assets/batch/rename/preview",
            "/api/v1/assets/batch/rename/{batch_id}/undo",
            "/api/v1/assets/{id}",
            "/api/v1/assets/{id}",
            "/api/v1/assets/{id}/albums",
            "/api/v1/assets/{id}/faces",
            "/api/v1/assets/{id}/flags",
            "/api/v1/assets/{id}/flags",
            "/api/v1/assets/{id}/metadata",
            "/api/v1/assets/{id}/metadata",
            "/api/v1/assets/{id}/pick",
            "/api/v1/assets/{id}/restore",
            "/api/v1/assets/{id}/stack",
            "/api/v1/assets/{id}/stack/primary",
            "/api/v1/assets/{id}/tags",
            "/api/v1/audit",
            "/api/v1/auth/me",
            "/api/v1/auth/refresh",
            "/api/v1/auth/totp",
            "/api/v1/auth/totp",
            "/api/v1/auth/totp/confirm",
            "/api/v1/auth/totp/recovery-codes",
            "/api/v1/auth/totp/setup",
            "/api/v1/backup/destinations",
            "/api/v1/backup/destinations",
            "/api/v1/backup/destinations/{id}",
            "/api/v1/backup/destinations/{id}/test",
            "/api/v1/backup/preferences",
            "/api/v1/backup/preferences",
            "/api/v1/backup/run",
            "/api/v1/backup/runs",
            "/api/v1/bootstrap",
            "/api/v1/culling/lots/{id}/empty-skipped",
            "/api/v1/duplicates",
            "/api/v1/duplicates/{content_hash}",
            "/api/v1/duplicates/{content_hash}/resolve",
            "/api/v1/faces/data",
            "/api/v1/faces/proposals",
            "/api/v1/faces/{id}/assign",
            "/api/v1/faces/{id}/confirm",
            "/api/v1/faces/{id}/reject",
            "/api/v1/flags/batch",
            "/api/v1/folders/tree",
            "/api/v1/folders/{id}",
            "/api/v1/folders/{id}/children",
            "/api/v1/groups",
            "/api/v1/groups",
            "/api/v1/groups/{group_id}/members/{user_id}",
            "/api/v1/groups/{group_id}/members/{user_id}",
            "/api/v1/groups/{id}",
            "/api/v1/groups/{id}",
            "/api/v1/groups/{id}/members",
            "/api/v1/guest-uploads/{id}/approve",
            "/api/v1/libraries",
            "/api/v1/libraries",
            "/api/v1/libraries/preview",
            "/api/v1/libraries/{id}",
            "/api/v1/libraries/{id}",
            "/api/v1/libraries/{id}",
            "/api/v1/libraries/{id}/culling-root",
            "/api/v1/libraries/{id}/culling/lots",
            "/api/v1/libraries/{id}/probe",
            "/api/v1/libraries/{id}/scan",
            "/api/v1/libraries/{id}/scan",
            "/api/v1/libraries/{id}/storage",
            "/api/v1/map/clusters",
            "/api/v1/map/regions",
            "/api/v1/map/regions",
            "/api/v1/map/regions/{id}",
            "/api/v1/map/regions/{id}/cancel",
            "/api/v1/map/tiles/{region}/{z}/{x}/{y}",
            "/api/v1/metadata/batch",
            "/api/v1/metadata/batch/copy-location",
            "/api/v1/metadata/batch/import-gpx",
            "/api/v1/metadata/batch/recalculate-timezones",
            "/api/v1/metadata/batch/recalculate-timezones/preview",
            "/api/v1/metadata/batch/shift-taken-at",
            "/api/v1/metadata/batch/{batch_id}/undo",
            "/api/v1/operations/{id}/cancel",
            "/api/v1/permissions",
            "/api/v1/permissions",
            "/api/v1/permissions/explain",
            "/api/v1/permissions/{id}",
            "/api/v1/permissions/{id}",
            "/api/v1/person-groups",
            "/api/v1/person-groups",
            "/api/v1/person-groups/{id}",
            "/api/v1/person-groups/{id}",
            "/api/v1/person-groups/{id}/members",
            "/api/v1/person-groups/{id}/members/{person_id}",
            "/api/v1/person-groups/{id}/members/{person_id}",
            "/api/v1/persons",
            "/api/v1/persons",
            "/api/v1/persons/{id}",
            "/api/v1/persons/{id}",
            "/api/v1/persons/{id}",
            "/api/v1/persons/{id}/merge",
            "/api/v1/persons/{id}/proposals/confirm",
            "/api/v1/persons/{id}/proposals/reject",
            "/api/v1/persons/{id}/separate",
            "/api/v1/places/reverse",
            "/api/v1/places/suggest",
            "/api/v1/problems",
            "/api/v1/restore",
            "/api/v1/restore/inspect",
            "/api/v1/saved-searches",
            "/api/v1/saved-searches",
            "/api/v1/search",
            "/api/v1/search/suggest",
            "/api/v1/share/links",
            "/api/v1/share/links",
            "/api/v1/share/links/{id}",
            "/api/v1/shared-with-me",
            "/api/v1/sync/delta",
            "/api/v1/tags",
            "/api/v1/tags",
            "/api/v1/tags/proposals",
            "/api/v1/tags/{id}",
            "/api/v1/tags/{id}",
            "/api/v1/tags/{id}",
            "/api/v1/tags/{id}/assets/batch",
            "/api/v1/tags/{id}/assets/batch/remove",
            "/api/v1/tags/{id}/assets/{asset_id}/confirm",
            "/api/v1/tags/{id}/assets/{asset_id}/reject",
            "/api/v1/tags/{id}/assets/{asset_id}/remove",
            "/api/v1/tags/{id}/proposals/confirm",
            "/api/v1/tags/{id}/proposals/reject",
            "/api/v1/timeline",
            "/api/v1/timeline/buckets",
            "/api/v1/timeline/geometry",
            "/api/v1/trash",
            "/api/v1/trash/empty",
            "/api/v1/upload",
            "/api/v1/upload/check",
            "/api/v1/upload/{id}",
            "/api/v1/upload/{id}",
            "/api/v1/users",
            "/api/v1/users",
            "/api/v1/users/me/app-passwords",
            "/api/v1/users/me/app-passwords",
            "/api/v1/users/me/app-passwords/{id}",
            "/api/v1/users/me/home",
            "/api/v1/users/me/home",
            "/api/v1/users/me/password",
            "/api/v1/users/me/preferences",
            "/api/v1/users/me/preferences",
            "/api/v1/users/me/sessions",
            "/api/v1/users/me/sessions/revoke-others",
            "/api/v1/users/me/sessions/{id}",
            "/api/v1/users/{id}",
            "/api/v1/users/{id}/disable",
            "/api/v1/users/{id}/enable",
            "/api/v1/viewport",
            "/api/v1/ws",
            "/api/v1/ws/ticket",
            "/media/full/{hash}",
            "/media/original/{id}",
            "/media/preview/{hash}",
            "/media/thumb/{hash}",
            "/media/video/{id}/hls/{file}",
            "/media/video/{id}/playback",
            "/media/video/{id}/poster"
        ]
    );
}

/// `operationId` becomes the method name in generated clients and must be
/// unique across the whole document. Deriving it from the Rust function
/// name isn't enough: `setup::create` and a future `albums::create` would
/// both produce `create`. `operation_id`s are therefore explicit and
/// prefixed by area; this test fails if two operations end up with the
/// same name.
#[test]
#[allow(clippy::unwrap_used, clippy::too_many_lines)]
fn operation_ids_are_explicit_and_unique() {
    let doc =
        serde_json::to_value(<keeppix_api::openapi::ApiDoc as utoipa::OpenApi>::openapi()).unwrap();

    let mut ids = Vec::new();
    for item in doc["paths"].as_object().unwrap().values() {
        for (method, operation) in item.as_object().unwrap() {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }
            ids.push(operation["operationId"].as_str().unwrap().to_owned());
        }
    }

    ids.sort();
    let unique = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), unique, "duplicate operationId: {ids:?}");
    assert_eq!(
        ids,
        [
            "albums_add_asset",
            "albums_create",
            "albums_delete",
            "albums_get",
            "albums_list",
            "albums_list_assets",
            "albums_patch",
            "albums_refresh",
            "albums_remove_asset",
            "albums_reorder_asset",
            "app_passwords_create",
            "app_passwords_list",
            "app_passwords_revoke",
            "assets_batch_delete",
            "assets_batch_move",
            "assets_delete",
            "assets_get",
            "assets_list_albums",
            "assets_list_faces",
            "assets_list_tags",
            "assets_restore",
            "assets_stack_get",
            "assets_stack_set_primary",
            "audit_list",
            "auth_login",
            "auth_logout",
            "auth_me",
            "auth_refresh",
            "backup_destinations_create",
            "backup_destinations_delete",
            "backup_destinations_list",
            "backup_destinations_test",
            "backup_preferences",
            "backup_preferences_put",
            "backup_run_now",
            "backup_runs_list",
            "bootstrap_get",
            "culling_empty_skipped",
            "culling_list_lots",
            "culling_set_pick",
            "delta",
            "duplicates_list",
            "duplicates_members",
            "duplicates_resolve",
            "faces_assign",
            "faces_confirm_proposal",
            "faces_delete_all_data",
            "faces_list_proposals",
            "faces_reject",
            "flags_batch_set",
            "flags_get",
            "flags_set",
            "folders_children",
            "folders_move",
            "folders_tree",
            "groups_add_member",
            "groups_create",
            "groups_delete",
            "groups_list",
            "groups_list_members",
            "groups_patch",
            "groups_remove_member",
            "guest_uploads_approve",
            "health_get",
            "libraries_create",
            "libraries_delete",
            "libraries_get",
            "libraries_list",
            "libraries_patch",
            "libraries_preview",
            "libraries_probe",
            "libraries_scan_start",
            "libraries_scan_status",
            "libraries_set_culling_root",
            "libraries_storage",
            "map_clusters",
            "map_regions_cancel",
            "map_regions_delete",
            "map_regions_download",
            "map_regions_list",
            "map_tile_archive",
            "media_full",
            "media_original",
            "media_preview",
            "media_thumb",
            "media_video_hls",
            "media_video_playback",
            "media_video_poster",
            "metadata_apply",
            "metadata_apply_batch",
            "metadata_copy_location",
            "metadata_effective",
            "metadata_import_gpx",
            "metadata_recalculate_timezones_apply",
            "metadata_recalculate_timezones_preview",
            "metadata_shift_taken_at",
            "metadata_undo_batch",
            "operations_cancel",
            "permissions_explain",
            "permissions_grant",
            "permissions_list",
            "permissions_patch",
            "permissions_revoke",
            "permissions_shared_with_me",
            "person_groups_add_member",
            "person_groups_create",
            "person_groups_delete",
            "person_groups_list",
            "person_groups_list_members",
            "person_groups_patch",
            "person_groups_remove_member",
            "persons_confirm_all_proposals",
            "persons_create",
            "persons_delete",
            "persons_get",
            "persons_list",
            "persons_merge",
            "persons_patch",
            "persons_reject_all_proposals",
            "persons_separate",
            "places_reverse",
            "places_suggest",
            "problems_list",
            "rename_apply_batch",
            "rename_preview",
            "rename_undo_batch",
            "restore_inspect",
            "restore_restore",
            "saved_searches_create",
            "saved_searches_list",
            "search_run",
            "search_suggest",
            "sessions_list",
            "sessions_revoke",
            "sessions_revoke_others",
            "setup_create",
            "setup_status",
            "share_links_create",
            "share_links_list",
            "share_links_revoke",
            "share_public_assets",
            "share_public_auth",
            "share_public_info",
            "share_public_upload",
            "tags_assign_batch",
            "tags_confirm_all_proposals",
            "tags_confirm_proposal",
            "tags_create",
            "tags_delete",
            "tags_get",
            "tags_list",
            "tags_list_proposals",
            "tags_patch",
            "tags_reject_all_proposals",
            "tags_reject_proposal",
            "tags_remove_confirmed",
            "tags_unassign_batch",
            "timeline_buckets",
            "timeline_geometry",
            "timeline_page",
            "totp_confirm",
            "totp_disable",
            "totp_regenerate_recovery",
            "totp_setup",
            "totp_status",
            "trash_empty",
            "trash_list",
            "upload_check",
            "upload_create_session",
            "upload_session_head",
            "upload_session_patch",
            "users_change_password",
            "users_create",
            "users_delete_home",
            "users_disable",
            "users_enable",
            "users_list",
            "users_patch",
            "users_preferences_get",
            "users_preferences_patch",
            "users_set_home",
            "viewport_promote",
            "ws_connect",
            "ws_ticket"
        ]
    );
}

/// `utoipa` takes the summary from rustdoc's first line if `summary =` is
/// missing. When the doc comment starts with `/// # Errors`, that heading
/// ends up in the public document and in generated clients. Every
/// operation must have an explicit English summary (or rustdoc that
/// isn't an Errors section).
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn openapi_summaries_do_not_contain_errors_heading() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let mut checked = 0_usize;
    for (path, item) in doc["paths"].as_object().unwrap() {
        for (method, operation) in item.as_object().unwrap() {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }
            let summary = operation["summary"].as_str().unwrap_or("");
            assert!(
                !summary.contains("# Errors"),
                "{method} {path} (operationId={:?}) summary must not contain `# Errors`: {summary:?}",
                operation.get("operationId")
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked, 184,
        "the document must describe one hundred eighty-four operations"
    );
}

/// Extracts `.route("...", VERB(...))` calls from `lib.rs`'s source text,
/// balancing parentheses (a single `.route(...)` can chain multiple
/// verbs, e.g. `get(a).post(b)`, and can span multiple lines). This is
/// not a Rust parser: it's the pragmatic counterpart to axum 0.8 not
/// offering introspection on `Router` (see the comment above
/// `documented_operations_are_all_mounted`) — the only source of truth
/// for routes actually mounted is the text that declares them.
fn extract_route_calls(source: &str) -> Vec<(String, Vec<&'static str>)> {
    const VERBS: [(&str, &str); 6] = [
        ("get(", "get"),
        ("post(", "post"),
        ("put(", "put"),
        ("patch(", "patch"),
        ("delete(", "delete"),
        ("head(", "head"),
    ];

    fn is_word_boundary_before(s: &str, idx: usize) -> bool {
        match s[..idx].chars().next_back() {
            None => true,
            Some(c) => !(c.is_ascii_alphanumeric() || c == '_'),
        }
    }

    fn verbs_in(block: &str) -> Vec<&'static str> {
        let mut found = Vec::new();
        for (needle, verb) in VERBS {
            let mut from = 0;
            while let Some(rel) = block[from..].find(needle) {
                let abs = from + rel;
                if is_word_boundary_before(block, abs) {
                    found.push(verb);
                }
                from = abs + needle.len();
            }
        }
        found
    }

    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = source[search_from..].find(".route(") {
        let call_start = search_from + rel;
        let paren_open = call_start + ".route".len();
        let mut depth = 0_i32;
        let mut i = paren_open;
        let mut close = None;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(i);
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let close = close
            .unwrap_or_else(|| panic!("unbalanced .route( call at byte {call_start} of lib.rs"));
        let block = &source[paren_open + 1..close];
        if let Some(q1) = block.find('"') {
            if let Some(q2_rel) = block[q1 + 1..].find('"') {
                let path = block[q1 + 1..q1 + 1 + q2_rel].to_owned();
                out.push((path, verbs_in(block)));
            }
        }
        search_from = close + 1;
    }
    out
}

/// **The CI check for the reverse direction**: fails if a route
/// registered in the router (`crates/keeppix-api/src/lib.rs`) doesn't
/// appear in `openapi.json` with the same method. Covers the direction
/// that `documented_operations_are_all_mounted` explicitly leaves
/// uncovered (a route mounted but undocumented) — the two together close
/// the loop.
///
/// `lib.rs`'s source text is the source of truth: `fn api_routes` mounts
/// under `/api/v1`, `fn all_routes` mounts with no prefix (except
/// `/dav/{*path}`, deliberately out of contract — see the comment in
/// `lib.rs` — and `/api/openapi.json`, which is the document itself, not
/// an operation).
#[test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
fn router_registered_routes_are_all_documented() {
    let source = include_str!("../src/lib.rs");

    let api_routes_start = source
        .find("fn api_routes(")
        .expect("lib.rs must define fn api_routes");
    let all_routes_start = source
        .find("fn all_routes(")
        .expect("lib.rs must define fn all_routes");
    let not_found_start = source
        .find("async fn not_found()")
        .expect("lib.rs must define async fn not_found");
    assert!(api_routes_start < all_routes_start);
    assert!(all_routes_start < not_found_start);

    let api_routes_body = &source[api_routes_start..all_routes_start];
    let all_routes_body = &source[all_routes_start..not_found_start];

    let mut registered: Vec<(String, &'static str)> = Vec::new();
    for (path, verbs) in extract_route_calls(api_routes_body) {
        let full_path = format!("/api/v1{path}");
        for verb in verbs {
            registered.push((full_path.clone(), verb));
        }
    }
    for (path, verbs) in extract_route_calls(all_routes_body) {
        if path.starts_with("/dav") || path == "/api/openapi.json" {
            continue;
        }
        for verb in verbs {
            registered.push((path.clone(), verb));
        }
    }

    // Regression guard on the parser itself: if it stopped finding routes
    // (e.g. from a reformatting of `lib.rs` that breaks the assumption
    // about `.route(`), this test would pass vacuously without checking
    // anything.
    assert!(
        registered.len() > 100,
        "the parser found only {} routes in lib.rs: it probably broke, routes didn't just shrink",
        registered.len()
    );

    let doc =
        serde_json::to_value(<keeppix_api::openapi::ApiDoc as utoipa::OpenApi>::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();

    let mut missing = Vec::new();
    for (path, verb) in &registered {
        let documented = paths
            .get(path)
            .and_then(|item| item.as_object())
            .is_some_and(|item| item.contains_key(*verb));
        if !documented {
            missing.push(format!("{} {path}", verb.to_uppercase()));
        }
    }

    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "routes mounted in lib.rs but missing from openapi.json (missing #[utoipa::path] or \
         the registration in ApiDoc::paths): {missing:#?}"
    );
}

/// Pins the on-disk spec: mobile clients are generated from this file, so
/// a change must be seen before it's published, not after.
#[test]
#[allow(clippy::unwrap_used)]
fn openapi_snapshot_matches_the_committed_file() {
    let generated =
        serde_json::to_string_pretty(&<keeppix_api::openapi::ApiDoc as utoipa::OpenApi>::openapi())
            .unwrap();

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/api/openapi.json");

    if std::env::var("UPDATE_OPENAPI").as_deref() == Ok("1") {
        std::fs::write(&path, generated.trim_end().to_string() + "\n").unwrap();
        return;
    }

    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        committed.trim(),
        generated.trim(),
        "the public contract changed: the code no longer produces the document \
         committed in docs/api/openapi.json. The Kotlin, Swift, Dart, and \
         TypeScript clients are generated from that file, and it's declared \
         frozen (additions only within /api/v1). Don't regenerate it just to \
         make the test green: look at what changed and decide. If the change \
         is unintended, fix the code; if it's intended and compatible, update \
         the committed file deliberately and explain why in the commit message."
    );
}
