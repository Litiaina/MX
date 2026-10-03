use axum::{
    Json,
    body::Body,
    extract::Multipart,
    http::StatusCode,
    http::header::{CACHE_CONTROL, CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    api::{
        live::publish_live_event,
        mx::handler::{n1_access_token, n1_download, n1_ensure_directory, n1_upload},
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

const DEFAULT_PRIMARY: &str = "#1d4ed8";
const DEFAULT_SIDEBAR: &str = "#0f172a";
const DEFAULT_DISPLAY_NAME: &str = "MX";
const DEFAULT_SUBTITLE: &str = "Litiaina's General-Purpose System";
const DEFAULT_LOGO_URL: &str = "images/system-icon.png";
const LEGACY_DGS_SUBTITLE: &str = "DGS Information System";
const IDENTITY_DEFAULTS_VERSION: i64 = 1;

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
            display_name: DEFAULT_DISPLAY_NAME.to_string(),
            subtitle: DEFAULT_SUBTITLE.to_string(),
            organization_name: String::new(),
            logo_url: DEFAULT_LOGO_URL.to_string(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CollaborationConfig {
    pub message_page_size: usize,
}

impl Default for CollaborationConfig {
    fn default() -> Self {
        Self {
            message_page_size: 256,
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
    pub collaboration: CollaborationConfig,
}

#[derive(Debug, Deserialize)]
pub struct SaveDeploymentConfigRequest {
    pub config: Value,
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn deployment_table_has_column(
    connection: &Connection,
    column_name: &str,
) -> rusqlite::Result<bool> {
    Ok(connection
        .prepare("PRAGMA table_info(mx_deployment_config)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|name| name == column_name))
}

fn ensure_deployment_schema(connection: &Connection) -> rusqlite::Result<()> {
    let default_json =
        serde_json::to_string(&DeploymentConfig::default()).unwrap_or_else(|_| "{}".to_string());

    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_deployment_config (
            id                        INTEGER PRIMARY KEY NOT NULL CHECK(id = 1),
            revision                  INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
            config_json               TEXT NOT NULL,
            updated_at                INTEGER NOT NULL DEFAULT 0,
            identity_defaults_version INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS mx_branding_assets (
            asset_key   TEXT PRIMARY KEY NOT NULL,
            object_key  TEXT NOT NULL,
            file_name   TEXT NOT NULL,
            mime_type   TEXT NOT NULL,
            size        INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL,
            updated_by  TEXT NOT NULL
        );
        "#,
    )?;

    if !deployment_table_has_column(connection, "identity_defaults_version")? {
        let alter_result = connection.execute(
            "ALTER TABLE mx_deployment_config ADD COLUMN identity_defaults_version INTEGER NOT NULL DEFAULT 0",
            [],
        );
        if let Err(error) = alter_result
            && !deployment_table_has_column(connection, "identity_defaults_version")?
        {
            return Err(error);
        }
    }

    connection.execute(
        r#"
        INSERT OR IGNORE INTO mx_deployment_config (
            id, revision, config_json, updated_at, identity_defaults_version
        ) VALUES (1, 0, ?1, 0, ?2)
        "#,
        params![default_json, IDENTITY_DEFAULTS_VERSION],
    )?;

    migrate_legacy_identity(connection)?;

    Ok(())
}

fn logo_type(mime_type: &str, file_name: &str) -> Option<(&'static str, &'static str)> {
    match (
        mime_type.trim().to_ascii_lowercase().as_str(),
        file_name.to_ascii_lowercase(),
    ) {
        ("image/png", _) => Some(("png", "image/png")),
        ("image/jpeg", _) => Some(("jpg", "image/jpeg")),
        ("image/webp", _) => Some(("webp", "image/webp")),
        ("image/gif", _) => Some(("gif", "image/gif")),
        (_, name) if name.ends_with(".png") => Some(("png", "image/png")),
        (_, name) if name.ends_with(".jpg") || name.ends_with(".jpeg") => {
            Some(("jpg", "image/jpeg"))
        }
        (_, name) if name.ends_with(".webp") => Some(("webp", "image/webp")),
        (_, name) if name.ends_with(".gif") => Some(("gif", "image/gif")),
        _ => None,
    }
}

pub async fn upload_deployment_logo(claims: Claims, mut multipart: Multipart) -> Response {
    if !claims.can_manage_accounts() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"administrator access is required"}),
        );
    }
    let field = match multipart.next_field().await {
        Ok(Some(field)) => field,
        _ => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"select a logo image to upload"}),
            );
        }
    };
    let file_name = field.file_name().unwrap_or("logo").to_string();
    let supplied_type = field.content_type().unwrap_or("").to_string();
    let Some((extension, mime_type)) = logo_type(&supplied_type, &file_name) else {
        return api_json(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            json!({"response":"logo must be PNG, JPEG, WebP, or GIF"}),
        );
    };
    let bytes = match field.bytes().await {
        Ok(bytes) if !bytes.is_empty() && bytes.len() <= 5 * 1024 * 1024 => bytes.to_vec(),
        Ok(_) => {
            return api_json(
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({"response":"logo must be between 1 byte and 5 MiB"}),
            );
        }
        Err(_) => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"logo upload could not be read"}),
            );
        }
    };
    let object_key = format!("__mx/branding/logo.{extension}");
    let token = match n1_access_token().await {
        Ok(token) => token,
        Err(_) => {
            return api_json(
                StatusCode::BAD_GATEWAY,
                json!({"response":"N1 is unavailable; the logo was not changed"}),
            );
        }
    };
    if n1_ensure_directory("__mx", &token).await.is_err()
        || n1_ensure_directory("__mx/branding", &token).await.is_err()
        || n1_upload(&object_key, mime_type, bytes.clone(), &token)
            .await
            .is_err()
    {
        return api_json(
            StatusCode::BAD_GATEWAY,
            json!({"response":"N1 could not store the logo"}),
        );
    }
    let actor_uid = claims.uid.clone();
    let stored_key = object_key.clone();
    let stored_name = file_name.clone();
    let stored_type = mime_type.to_string();
    let size = bytes.len() as i64;
    let now = chrono::Utc::now().timestamp_millis();
    let result = tokio::task::spawn_blocking(move || with_sql_connection(|connection| {
        ensure_deployment_schema(connection)?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO mx_branding_assets(asset_key,object_key,file_name,mime_type,size,updated_at,updated_by) VALUES ('logo',?1,?2,?3,?4,?5,?6) ON CONFLICT(asset_key) DO UPDATE SET object_key=excluded.object_key,file_name=excluded.file_name,mime_type=excluded.mime_type,size=excluded.size,updated_at=excluded.updated_at,updated_by=excluded.updated_by",
            params![stored_key,stored_name,stored_type,size,now,actor_uid],
        )?;
        let (revision, mut config, _) = read_config(&transaction)?;
        config.branding.logo_url = format!("/mx/v1/deployment/logo?v={}", revision + 1);
        transaction.execute("UPDATE mx_deployment_config SET revision=revision+1,config_json=?1,updated_at=?2 WHERE id=1",params![serde_json::to_string(&config).unwrap_or_default(),now])?;
        transaction.commit()?;
        Ok(config)
    })).await;
    match result {
        Ok(Ok(config)) => {
            publish_live_event(
                "deployment.updated",
                Some(&claims.uid),
                json!({"branding":"logo"}),
            );
            api_json(
                StatusCode::OK,
                json!({"response":"logo uploaded to N1","logo_url":config.branding.logo_url,"config":config}),
            )
        }
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"logo was stored but its deployment metadata could not be saved"}),
        ),
    }
}

pub async fn get_deployment_logo() -> Response {
    let result = tokio::task::spawn_blocking(move || with_sql_connection(|connection| {
        ensure_deployment_schema(connection)?;
        connection.query_row("SELECT object_key,file_name,mime_type FROM mx_branding_assets WHERE asset_key='logo'",[],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))
    })).await;
    let Ok(Ok((object_key, file_name, mime_type))) = result else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match n1_download(&object_key, &file_name).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, mime_type)
            .header(CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

/// Remove the government-specific identity that shipped in pre-1.0 development
/// builds. The version marker makes this a one-time data migration: once an
/// installation has been checked, administrators remain free to configure any
/// identity (including these exact strings) without MX rewriting it later.
fn migrate_legacy_identity(connection: &Connection) -> rusqlite::Result<()> {
    let (version, config_text): (i64, String) = connection.query_row(
        "SELECT identity_defaults_version, config_json FROM mx_deployment_config WHERE id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    if version >= IDENTITY_DEFAULTS_VERSION {
        return Ok(());
    }

    let migrated_config = serde_json::from_str::<DeploymentConfig>(&config_text)
        .ok()
        .filter(|config| {
            config
                .branding
                .display_name
                .trim()
                .eq_ignore_ascii_case(DEFAULT_DISPLAY_NAME)
                && config
                    .branding
                    .subtitle
                    .trim()
                    .eq_ignore_ascii_case(LEGACY_DGS_SUBTITLE)
        })
        .and_then(|mut config| {
            config.branding.subtitle = DEFAULT_SUBTITLE.to_string();
            config.branding.organization_name.clear();
            config.branding.logo_url = DEFAULT_LOGO_URL.to_string();
            serde_json::to_string(&normalize_config(config)).ok()
        });

    if let Some(config_text) = migrated_config {
        connection.execute(
            r#"
            UPDATE mx_deployment_config
            SET revision = revision + 1,
                config_json = ?1,
                updated_at = CAST(STRFTIME('%s','now') AS INTEGER) * 1000,
                identity_defaults_version = ?2
            WHERE id = 1 AND identity_defaults_version < ?2
            "#,
            params![config_text, IDENTITY_DEFAULTS_VERSION],
        )?;
    } else {
        connection.execute(
            "UPDATE mx_deployment_config SET identity_defaults_version = ?1 WHERE id = 1 AND identity_defaults_version < ?1",
            params![IDENTITY_DEFAULTS_VERSION],
        )?;
    }

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
    config.branding.display_name =
        clean_text(config.branding.display_name, DEFAULT_DISPLAY_NAME, 80);
    config.branding.subtitle = clean_text(config.branding.subtitle, DEFAULT_SUBTITLE, 160);
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

    config.collaboration.message_page_size = config.collaboration.message_page_size.clamp(32, 512);

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

pub(crate) fn collaboration_message_page_size(connection: &Connection) -> rusqlite::Result<usize> {
    let (_, config, _) = read_config(connection)?;
    Ok(config.collaboration.message_page_size)
}

/// Public, non-secret deployment presentation configuration.
///
/// This endpoint is intentionally available before authentication so the
/// login/bootstrap screen can use deployment branding and appearance.
pub async fn get_deployment_config() -> Response {
    let result = tokio::task::spawn_blocking(
        move || -> Result<(i64, DeploymentConfig, i64), SqliteDatabaseError> {
            with_sql_connection(read_config)
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
        Ok(Ok(revision)) => {
            publish_live_event(
                "deployment.updated",
                Some(&claims.uid),
                json!({"revision": revision}),
            );
            api_json(
                StatusCode::OK,
                json!({
                    "response":"deployment configuration saved",
                    "revision":revision,
                    "config":parsed
                }),
            )
        }
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to save deployment configuration"}),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_dgs_identity_is_migrated_only_once() {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(
                r#"
                CREATE TABLE mx_deployment_config (
                    id          INTEGER PRIMARY KEY NOT NULL CHECK(id = 1),
                    revision    INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
                    config_json TEXT NOT NULL,
                    updated_at  INTEGER NOT NULL DEFAULT 0
                );
                "#,
            )
            .expect("create legacy schema");

        let mut legacy = DeploymentConfig::default();
        legacy.branding.subtitle = LEGACY_DGS_SUBTITLE.to_string();
        legacy.branding.organization_name = "DGS".to_string();
        legacy.branding.logo_url = "https://example.test/government-seal.png".to_string();
        connection
            .execute(
                "INSERT INTO mx_deployment_config (id, revision, config_json, updated_at) VALUES (1, 8, ?1, 0)",
                params![serde_json::to_string(&legacy).expect("serialize legacy config")],
            )
            .expect("insert legacy config");

        ensure_deployment_schema(&connection).expect("migrate legacy identity");
        let (revision, config, _) = read_config(&connection).expect("read migrated config");
        assert_eq!(revision, 9);
        assert_eq!(config.branding.display_name, DEFAULT_DISPLAY_NAME);
        assert_eq!(config.branding.subtitle, DEFAULT_SUBTITLE);
        assert!(config.branding.organization_name.is_empty());
        assert_eq!(config.branding.logo_url, DEFAULT_LOGO_URL);

        let mut deliberately_customized = config;
        deliberately_customized.branding.subtitle = LEGACY_DGS_SUBTITLE.to_string();
        deliberately_customized.branding.logo_url = "https://example.test/custom.png".to_string();
        connection
            .execute(
                "UPDATE mx_deployment_config SET revision = 10, config_json = ?1 WHERE id = 1",
                params![
                    serde_json::to_string(&deliberately_customized)
                        .expect("serialize customized config")
                ],
            )
            .expect("save deliberate customization");

        ensure_deployment_schema(&connection).expect("recheck current schema");
        let (revision, config, _) = read_config(&connection).expect("read customized config");
        assert_eq!(revision, 10);
        assert_eq!(config.branding.subtitle, LEGACY_DGS_SUBTITLE);
        assert_eq!(config.branding.logo_url, "https://example.test/custom.png");
    }

    #[test]
    fn collaboration_page_size_defaults_and_stays_within_safe_bounds() {
        let legacy = serde_json::from_str::<DeploymentConfig>("{}").unwrap();
        assert_eq!(
            normalize_config(legacy).collaboration.message_page_size,
            256
        );

        let mut too_small = DeploymentConfig::default();
        too_small.collaboration.message_page_size = 1;
        assert_eq!(
            normalize_config(too_small).collaboration.message_page_size,
            32
        );

        let mut too_large = DeploymentConfig::default();
        too_large.collaboration.message_page_size = 10_000;
        assert_eq!(
            normalize_config(too_large).collaboration.message_page_size,
            512
        );
    }
}
