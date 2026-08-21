//! Per-user presentation preferences (Fase 10 Task 9).

use keeppix_domain::AuthContext;
use serde::{Deserialize, Serialize};

use crate::{Db, DbError};

const THEME_CHIARO: &str = "chiaro";
const THEME_SCURO: &str = "scuro";
const THEME_SISTEMA: &str = "sistema";

const LANG_IT: &str = "it";
const LANG_EN: &str = "en";

const GRID_DESKTOP_MIN: u8 = 2;
const GRID_DESKTOP_MAX: u8 = 12;
const GRID_MOBILE_MIN: u8 = 2;
const GRID_MOBILE_MAX: u8 = 6;

const TOP_LEVEL_KEYS: &[&str] = &["theme", "grid_density", "notifications", "language"];
const GRID_KEYS: &[&str] = &["desktop", "mobile"];
const NOTIF_KEYS: &[&str] = &["digest", "condivisioni", "problemi"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridDensity {
    pub desktop: u8,
    pub mobile: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPreferences {
    pub digest: bool,
    pub condivisioni: bool,
    pub problemi: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserPreferences {
    pub theme: String,
    pub grid_density: GridDensity,
    pub notifications: NotificationPreferences,
    pub language: String,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            theme: THEME_CHIARO.to_owned(),
            grid_density: GridDensity {
                desktop: 4,
                mobile: 3,
            },
            notifications: NotificationPreferences {
                digest: true,
                condivisioni: true,
                problemi: true,
            },
            language: LANG_IT.to_owned(),
        }
    }
}

impl UserPreferences {
    fn from_stored(raw: &serde_json::Value) -> Result<Self, DbError> {
        if raw.is_null() || raw.as_object().is_some_and(serde_json::Map::is_empty) {
            return Ok(Self::default());
        }
        let mut prefs = Self::default();
        prefs
            .apply_patch(raw)
            .map_err(|e| DbError::Corrupted(e.to_string()))?;
        Ok(prefs)
    }

    /// Merge a partial patch into the current document. Rejects unknown keys.
    ///
    /// # Errors
    /// `PreferencesPatchError` when a field name or value is not allowed.
    pub fn apply_patch(&mut self, patch: &serde_json::Value) -> Result<(), PreferencesPatchError> {
        let Some(obj) = patch.as_object() else {
            return Err(PreferencesPatchError::InvalidValue(
                "patch body must be a JSON object".to_owned(),
            ));
        };

        for key in obj.keys() {
            if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
                return Err(PreferencesPatchError::UnknownField(key.clone()));
            }
        }

        if let Some(theme) = obj.get("theme") {
            self.theme = parse_theme(theme)?;
        }

        if let Some(grid) = obj.get("grid_density") {
            apply_grid_patch(&mut self.grid_density, grid)?;
        }

        if let Some(notif) = obj.get("notifications") {
            apply_notifications_patch(&mut self.notifications, notif)?;
        }

        if let Some(language) = obj.get("language") {
            self.language = parse_language(language)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferencesPatchError {
    UnknownField(String),
    InvalidValue(String),
}

impl std::fmt::Display for PreferencesPatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownField(key) => write!(f, "unknown preference field: {key}"),
            Self::InvalidValue(msg) => f.write_str(msg),
        }
    }
}

fn apply_grid_patch(
    target: &mut GridDensity,
    patch: &serde_json::Value,
) -> Result<(), PreferencesPatchError> {
    let Some(obj) = patch.as_object() else {
        return Err(PreferencesPatchError::InvalidValue(
            "grid_density must be an object".to_owned(),
        ));
    };
    for key in obj.keys() {
        if !GRID_KEYS.contains(&key.as_str()) {
            return Err(PreferencesPatchError::UnknownField(format!(
                "grid_density.{key}"
            )));
        }
    }
    if let Some(desktop) = obj.get("desktop") {
        target.desktop = parse_grid_value(desktop, GRID_DESKTOP_MIN, GRID_DESKTOP_MAX, "desktop")?;
    }
    if let Some(mobile) = obj.get("mobile") {
        target.mobile = parse_grid_value(mobile, GRID_MOBILE_MIN, GRID_MOBILE_MAX, "mobile")?;
    }
    Ok(())
}

fn apply_notifications_patch(
    target: &mut NotificationPreferences,
    patch: &serde_json::Value,
) -> Result<(), PreferencesPatchError> {
    let Some(obj) = patch.as_object() else {
        return Err(PreferencesPatchError::InvalidValue(
            "notifications must be an object".to_owned(),
        ));
    };
    for key in obj.keys() {
        if !NOTIF_KEYS.contains(&key.as_str()) {
            return Err(PreferencesPatchError::UnknownField(format!(
                "notifications.{key}"
            )));
        }
    }
    if let Some(v) = obj.get("digest") {
        target.digest = parse_bool(v, "notifications.digest")?;
    }
    if let Some(v) = obj.get("condivisioni") {
        target.condivisioni = parse_bool(v, "notifications.condivisioni")?;
    }
    if let Some(v) = obj.get("problemi") {
        target.problemi = parse_bool(v, "notifications.problemi")?;
    }
    Ok(())
}

fn parse_theme(value: &serde_json::Value) -> Result<String, PreferencesPatchError> {
    let Some(raw) = value.as_str() else {
        return Err(PreferencesPatchError::InvalidValue(
            "theme must be a string".to_owned(),
        ));
    };
    match raw {
        THEME_CHIARO | THEME_SCURO | THEME_SISTEMA => Ok(raw.to_owned()),
        _ => Err(PreferencesPatchError::InvalidValue(format!(
            "theme must be {THEME_CHIARO}, {THEME_SCURO}, or {THEME_SISTEMA}"
        ))),
    }
}

fn parse_language(value: &serde_json::Value) -> Result<String, PreferencesPatchError> {
    let Some(raw) = value.as_str() else {
        return Err(PreferencesPatchError::InvalidValue(
            "language must be a string".to_owned(),
        ));
    };
    match raw {
        LANG_IT | LANG_EN => Ok(raw.to_owned()),
        _ => Err(PreferencesPatchError::InvalidValue(format!(
            "language must be {LANG_IT} or {LANG_EN}"
        ))),
    }
}

fn parse_grid_value(
    value: &serde_json::Value,
    min: u8,
    max: u8,
    field: &str,
) -> Result<u8, PreferencesPatchError> {
    let Some(n) = value.as_u64() else {
        return Err(PreferencesPatchError::InvalidValue(format!(
            "grid_density.{field} must be an integer"
        )));
    };
    let Ok(n) = u8::try_from(n) else {
        return Err(PreferencesPatchError::InvalidValue(format!(
            "grid_density.{field} is out of range"
        )));
    };
    if !(min..=max).contains(&n) {
        return Err(PreferencesPatchError::InvalidValue(format!(
            "grid_density.{field} must be between {min} and {max}"
        )));
    }
    Ok(n)
}

fn parse_bool(value: &serde_json::Value, field: &str) -> Result<bool, PreferencesPatchError> {
    value
        .as_bool()
        .ok_or_else(|| PreferencesPatchError::InvalidValue(format!("{field} must be a boolean")))
}

pub struct PreferencesRepo<'a> {
    db: &'a Db,
}

impl<'a> PreferencesRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// `DbError::Forbidden` without a logged-in user; `DbError::Connection` on
    /// query failure; `DbError::Corrupted` if stored json is invalid.
    pub async fn get(&self, ctx: &AuthContext) -> Result<UserPreferences, DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let raw: serde_json::Value =
            sqlx::query_scalar("SELECT preferences FROM users WHERE id = $1")
                .bind(user_id.as_uuid())
                .fetch_one(self.db.pool())
                .await?;
        UserPreferences::from_stored(&raw)
    }

    /// Replace the stored document with `prefs`.
    ///
    /// # Errors
    /// Same as [`Self::get`].
    pub async fn set(&self, ctx: &AuthContext, prefs: &UserPreferences) -> Result<(), DbError> {
        let user_id = ctx.user_id().ok_or(DbError::Forbidden)?;
        let stored = serde_json::to_value(prefs)
            .map_err(|e| DbError::Corrupted(format!("serialize preferences: {e}")))?;
        sqlx::query("UPDATE users SET preferences = $2, updated_at = now() WHERE id = $1")
            .bind(user_id.as_uuid())
            .bind(stored)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}
