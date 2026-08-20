//! TOTP two-factor authentication (Fase 6 Task 5). Provisioning, status,
//! recovery-code regeneration, and disable. Login verification lives in
//! `auth::login` so the password + TOTP challenge stays a single request.

use axum::extract::State;
use axum::http::StatusCode;
use keeppix_db::TotpRepo;
use keeppix_db::UserRepo;
use qrcode::QrCode;
use qrcode::render::svg;
use serde::{Deserialize, Serialize};

use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct TotpStatusView {
    pub enabled: bool,
    pub pending: bool,
    pub unused_recovery_codes: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TotpSetupView {
    pub otpauth_uri: String,
    pub secret_base32: String,
    /// Inline SVG QR of `otpauth_uri` — the SPA renders it without a QR dependency.
    pub qr_svg: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TotpCodeRequest {
    pub code: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TotpRecoveryCodesView {
    pub recovery_codes: Vec<String>,
}

/// # Errors
/// `401` if not authenticated.
#[utoipa::path(
    get,
    path = "/api/v1/auth/totp",
    tag = "auth",
    operation_id = "totp_status",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "Current TOTP enrolment state", body = TotpStatusView),
        (status = 401, description = "Not authenticated", body = Problem)
    )
)]
pub async fn status(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<TotpStatusView>, Problem> {
    let s = TotpRepo::new(&state.db).status(&ctx).await?;
    Ok(Json(TotpStatusView {
        enabled: s.enabled,
        pending: s.pending,
        unused_recovery_codes: s.unused_recovery_codes,
    }))
}

/// Starts provisioning: stores an encrypted secret with `enabled_at` NULL.
/// The current session is untouched.
///
/// # Errors
/// `401` if not authenticated; `409` if TOTP is already enabled.
#[utoipa::path(
    post,
    path = "/api/v1/auth/totp/setup",
    tag = "auth",
    operation_id = "totp_setup",
    security(("session_cookie" = [])),
    responses(
        (status = 200, description = "otpauth URI and QR for the authenticator app", body = TotpSetupView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 409, description = "TOTP already enabled", body = Problem)
    )
)]
pub async fn setup(
    State(state): State<AppState>,
    Auth(ctx): Auth,
) -> Result<Json<TotpSetupView>, Problem> {
    let user_id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    let user = UserRepo::new(&state.db).find_by_id(&ctx, user_id).await?;
    let begun = TotpRepo::new(&state.db)
        .begin_setup(&ctx, user.username.as_str())
        .await?;
    let qr_svg = render_qr_svg(&begun.otpauth_uri)?;
    Ok(Json(TotpSetupView {
        otpauth_uri: begun.otpauth_uri,
        secret_base32: begun.secret_base32,
        qr_svg,
    }))
}

/// Confirms the first authenticator code and enables TOTP. Returns the ten
/// recovery codes once. Does not revoke the current session.
///
/// # Errors
/// `401` if not authenticated; `409` if nothing is pending or the code is wrong.
#[utoipa::path(
    post,
    path = "/api/v1/auth/totp/confirm",
    tag = "auth",
    operation_id = "totp_confirm",
    security(("session_cookie" = [])),
    request_body = TotpCodeRequest,
    responses(
        (status = 200, description = "TOTP enabled; recovery codes shown once", body = TotpRecoveryCodesView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 409, description = "No pending setup or invalid code", body = Problem)
    )
)]
pub async fn confirm(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<TotpCodeRequest>,
) -> Result<Json<TotpRecoveryCodesView>, Problem> {
    let confirmed = TotpRepo::new(&state.db).confirm(&ctx, &body.code).await?;
    Ok(Json(TotpRecoveryCodesView {
        recovery_codes: confirmed.recovery_codes,
    }))
}

/// Regenerates all recovery codes after a valid authenticator code. Use when
/// the previous set is exhausted (or lost).
///
/// # Errors
/// `401` if not authenticated; `409` if TOTP is off or the code is wrong.
#[utoipa::path(
    post,
    path = "/api/v1/auth/totp/recovery-codes",
    tag = "auth",
    operation_id = "totp_regenerate_recovery",
    security(("session_cookie" = [])),
    request_body = TotpCodeRequest,
    responses(
        (status = 200, description = "New recovery codes", body = TotpRecoveryCodesView),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 409, description = "TOTP not enabled or invalid code", body = Problem)
    )
)]
pub async fn regenerate_recovery(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<TotpCodeRequest>,
) -> Result<Json<TotpRecoveryCodesView>, Problem> {
    let codes = TotpRepo::new(&state.db)
        .regenerate_recovery_codes(&ctx, &body.code)
        .await?;
    Ok(Json(TotpRecoveryCodesView {
        recovery_codes: codes,
    }))
}

/// Disables TOTP after a valid authenticator or recovery code.
///
/// # Errors
/// `401` if not authenticated; `409` if not enabled or the code is wrong.
#[utoipa::path(
    delete,
    path = "/api/v1/auth/totp",
    tag = "auth",
    operation_id = "totp_disable",
    security(("session_cookie" = [])),
    request_body = TotpCodeRequest,
    responses(
        (status = 204, description = "TOTP disabled"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 409, description = "TOTP not enabled or invalid code", body = Problem)
    )
)]
pub async fn disable(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<TotpCodeRequest>,
) -> Result<StatusCode, Problem> {
    TotpRepo::new(&state.db).disable(&ctx, &body.code).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn render_qr_svg(otpauth_uri: &str) -> Result<String, Problem> {
    let code = QrCode::new(otpauth_uri.as_bytes()).map_err(|e| {
        tracing::error!(error = %e, "totp qr encode failed");
        Problem::internal()
    })?;
    Ok(code
        .render::<svg::Color<'_>>()
        .min_dimensions(200, 200)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build())
}
