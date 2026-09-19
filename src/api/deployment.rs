use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

const DEFAULT_PRIMARY: &str = "#1d4ed8";
const DEFAULT_SIDEBAR: &str = "#0f172a";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrandingConfig {
    pub display_name: String,
    pub subtitle: String,
    pub organization_name: String,
    pub logo_url: String,
}

impl Default for BrandingConfig {
    fn default() -> Self {
        Self {
            display_name: "MX".to_string(),
            subtitle: "General-Purpose Information System".to_string(),
            organization_name: String::new(),
            logo_url: "images/system-icon.png".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub preset: String,
    pub primary_color: String,
    pub sidebar_color: String,
    pub radius: String,
    pub density: String,
    pub default_theme: String,
    pub content_width: String,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            preset: "blue".to_string(),
            primary_color: DEFAULT_PRIMARY.to_string(),
            sidebar_color: DEFAULT_SIDEBAR.to_string(),
            radius: "rounded".to_string(),
            density: "normal".to_string(),
            default_theme: "light".to_string(),
            content_width: "wide".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminologyConfig {
    pub record_singular: String,
    pub record_plural: String,
    pub dashboard_label: String,
    pub administration_label: String,
}

impl Default for TerminologyConfig {
    fn default() -> Self {
        Self {
            record_singular: "Record".to_string(),
            record_plural: "Records".to_string(),
            dashboard_label: "Dashboard".to_string(),
            administration_label: "Administration".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NavigationConfig {
    pub show_dashboard: bool,
    pub show_records: bool,
    pub show_quick_actions: bool,
    pub default_workspace: String,
}

impl Default for NavigationConfig {
    fn default() -> Self {
        Self {
            show_dashboard: true,
            show_records: true,
            show_quick_actions: true,
            default_workspace: "dashboard".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DeploymentConfig {
    pub branding: BrandingConfig,
    pub appearance: AppearanceConfig,
    pub terminology: TerminologyConfig,
    pub navigation: NavigationConfig,
}

#[derive(Debug, Deserialize)]
pub struct SaveDeploymentConfigRequest {
    pub config: Value,
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn ensure_deployment_schema(connection: &Connection) -> rusqlite::Result<()> {
    let default_json =
        serde_json::to_string(&DeploymentConfig::default()).unwrap_or_else(|_| "{}".to_string());

    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_deployment_config (
            id          INTEGER PRIMARY KEY NOT NULL CHECK(id = 1),
            revision    INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
            config_json TEXT NOT NULL,
            updated_at  INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )?;

    connection.execute(
        r#"
        INSERT OR IGNORE INTO mx_deployment_config (
            id, revision, config_json, updated_at
        ) VALUES (1, 0, ?1, 0)
        "#,
        params![default_json],
    )?;

    Ok(())
}

fn clean_text(value: String, fallback: &str, max_len: usize) -> String {
    let clean = value
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .trim()
        .to_string();

    if clean.is_empty() {
        fallback.to_string()
    } else {
        clean.chars().take(max_len).collect()
    }
}

fn clean_optional_text(value: String, max_len: usize) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(max_len)
        .collect()
}

fn valid_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7
        && bytes.first() == Some(&b'#')
        && bytes[1..].iter().all(|byte| byte.is_ascii_hexdigit())
}

fn clean_hex_color(value: String, fallback: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    if valid_hex_color(&value) {
        value
    } else {
        fallback.to_string()
    }
}

fn clean_logo_url(value: String) -> String {
    let value = clean_optional_text(value, 512);
    if value.is_empty()
        || value.starts_with('/')
        || value.starts_with("images/")
        || value.starts_with("./")
        || value.starts_with("https://")
        || value.starts_with("http://")
    {
        value
    } else {
        String::new()
    }
}

fn normalize_config(mut config: DeploymentConfig) -> DeploymentConfig {
    config.branding.display_name = clean_text(config.branding.display_name, "MX", 80);
    config.branding.subtitle = clean_text(
        config.branding.subtitle,
        "General-Purpose Information System",
        160,
    );
    config.branding.organization_name = clean_optional_text(config.branding.organization_name, 120);
    config.branding.logo_url = clean_logo_url(config.branding.logo_url);

    config.appearance.preset = match config.appearance.preset.as_str() {
        "blue" | "emerald" | "violet" | "amber" | "rose" | "slate" | "custom" => {
            config.appearance.preset
        }
        _ => "blue".to_string(),
    };

    config.appearance.primary_color =
        clean_hex_color(config.appearance.primary_color, DEFAULT_PRIMARY);
    config.appearance.sidebar_color =
        clean_hex_color(config.appearance.sidebar_color, DEFAULT_SIDEBAR);

    config.appearance.radius = match config.appearance.radius.as_str() {
        "square" | "subtle" | "rounded" | "soft" => config.appearance.radius,
        _ => "rounded".to_string(),
    };

    config.appearance.density = match config.appearance.density.as_str() {
        "compact" | "normal" | "comfortable" => config.appearance.density,
        _ => "normal".to_string(),
    };

    config.appearance.default_theme = match config.appearance.default_theme.as_str() {
        "light" | "dark" | "system" => config.appearance.default_theme,
        _ => "light".to_string(),
    };

    config.appearance.content_width = match config.appearance.content_width.as_str() {
        "standard" | "wide" | "full" => config.appearance.content_width,
        _ => "wide".to_string(),
    };

    config.terminology.record_singular =
        clean_text(config.terminology.record_singular, "Record", 48);
    config.terminology.record_plural = clean_text(config.terminology.record_plural, "Records", 48);
    config.terminology.dashboard_label =
        clean_text(config.terminology.dashboard_label, "Dashboard", 48);
    config.terminology.administration_label = clean_text(
        config.terminology.administration_label,
        "Administration",
        48,
    );

    config.navigation.default_workspace = match config.navigation.default_workspace.as_str() {
        "dashboard" if config.navigation.show_dashboard => "dashboard".to_string(),
        "records" if config.navigation.show_records => "records".to_string(),
        _ if config.navigation.show_dashboard => "dashboard".to_string(),
        _ if config.navigation.show_records => "records".to_string(),
        _ => "dashboard".to_string(),
    };

    // At least one ordinary workspace must remain visible.
    if !config.navigation.show_dashboard && !config.navigation.show_records {
        config.navigation.show_dashboard = true;
        config.navigation.default_workspace = "dashboard".to_string();
    }

    config
}

fn read_config(connection: &Connection) -> rusqlite::Result<(i64, DeploymentConfig, i64)> {
    ensure_deployment_schema(connection)?;

    let (revision, text, updated_at): (i64, String, i64) = connection.query_row(
        "SELECT revision, config_json, updated_at FROM mx_deployment_config WHERE id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    let config = serde_json::from_str::<DeploymentConfig>(&text)
        .map(normalize_config)
        .unwrap_or_default();

    Ok((revision, config, updated_at))
}

/// Public, non-secret deployment presentation configuration.
///
/// This endpoint is intentionally available before authentication so the
/// login/bootstrap screen can use deployment branding and appearance.
pub async fn get_deployment_config() -> Response {
    let result = tokio::task::spawn_blocking(
        move || -> Result<(i64, DeploymentConfig, i64), SqliteDatabaseError> {
            with_sql_connection(|connection| read_config(connection))
        },
    )
    .await;

    match result {
        Ok(Ok((revision, config, updated_at))) => api_json(
            StatusCode::OK,
            json!({
                "revision": revision,
                "config": config,
                "updated_at": updated_at
            }),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to load deployment configuration"}),
        ),
    }
}

pub async fn save_deployment_config(
    claims: Claims,
    Json(request): Json<SaveDeploymentConfigRequest>,
) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }

    let parsed = match serde_json::from_value::<DeploymentConfig>(request.config) {
        Ok(config) => normalize_config(config),
        Err(_) => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"invalid deployment configuration"}),
            );
        }
    };

    let config_text = match serde_json::to_string(&parsed) {
        Ok(value) => value,
        Err(_) => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"invalid deployment configuration"}),
            );
        }
    };

    let result = tokio::task::spawn_blocking(move || -> Result<i64, SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_deployment_schema(connection)?;
            connection.execute(
                r#"
                    UPDATE mx_deployment_config
                    SET revision = revision + 1,
                        config_json = ?1,
                        updated_at = CAST(STRFTIME('%s','now') AS INTEGER) * 1000
                    WHERE id = 1
                    "#,
                params![config_text],
            )?;

            connection.query_row(
                "SELECT revision FROM mx_deployment_config WHERE id = 1",
                [],
                |row| row.get(0),
            )
        })
    })
    .await;

    match result {
        Ok(Ok(revision)) => api_json(
            StatusCode::OK,
            json!({
                "response":"deployment configuration saved",
                "revision":revision,
                "config":parsed
            }),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to save deployment configuration"}),
        ),
    }
}
