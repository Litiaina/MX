use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    sync::OnceLock,
    time::{Duration, Instant},
};

use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path},
    http::{
        HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use reqwest::Client;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    api::{
        api_error::SqliteError,
        aris::{
            model::FileAttachment,
            storage::{ResolvedRecordStorage, StorageResolutionError, resolve_record_storage},
        },
    },
    config::load_config::CONFIG,
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

/*
ARIS/N1 design

SQLite:
- owns ARIS business records and attachment metadata;
- generates control numbers atomically per year;
- control numbers never get reused after a committed entry is created.

N1:
- owns the actual attachment bytes;
- uses a dedicated ARIS fragment;
- stores files in a human-navigable layout selected by an Administrator;
- any active dynamic ARIS field can be used as an ordered folder segment;
- an optional dynamic field can be used as the human-readable file prefix.

Example configuration:

    folders: Division/Office -> Date
    prefix:  Control No.

Example object:

    records/MISO/2026-09-17/000125__purchase_request.pdf

The resolved namespace is frozen per record when its N1 storage identity is
first needed, so later edits or layout changes do not split one record across
multiple N1 directories.

Required configuration:

config.ini

[n1]
base_url=https://127.0.0.1:50001
fragment=<ARIS_FRAGMENT_HASH>
insecure_tls=true
attachment_max_size_mb=50

.env

N1_ARIS_SECRET=<ARIS_FRAGMENT_SECRET>
*/

static N1_CLIENT: OnceLock<Client> = OnceLock::new();
static N1_TOKEN_CACHE: OnceLock<RwLock<Option<CachedN1Token>>> = OnceLock::new();

#[derive(Debug)]
pub enum ArisOperationError {
    Sqlite(SqliteError),
    N1(String),
    InvalidRequest(String),
}

impl From<SqliteError> for ArisOperationError {
    fn from(value: SqliteError) -> Self {
        Self::Sqlite(value)
    }
}

#[derive(Debug, Serialize)]
struct N1AuthRequest<'a> {
    fragment: &'a str,
    secret: &'a str,
    read: bool,
    write: bool,
    modify: bool,
    delete: bool,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct N1AuthResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug)]
struct CachedN1Token {
    access_token: String,
    expires_at: Instant,
}

/* -------------------------------------------------------------------------- */
/* API helpers                                                                */
/* -------------------------------------------------------------------------- */

fn api_json(status: StatusCode, body: JsonValue) -> Response {
    (status, Json(body)).into_response()
}

fn aris_access_denied() -> Response {
    api_json(
        StatusCode::FORBIDDEN,
        json!({
            "response": "your account does not have permission for this ARIS operation"
        }),
    )
}

fn map_aris_error(error: ArisOperationError) -> Response {
    match error {
        ArisOperationError::Sqlite(SqliteError::Conflict) => api_json(
            StatusCode::CONFLICT,
            json!({
                "response": "ARIS entry or attachment conflicts with an existing record."
            }),
        ),

        ArisOperationError::Sqlite(SqliteError::NotFound) => api_json(
            StatusCode::NOT_FOUND,
            json!({
                "response": "ARIS entry or attachment was not found."
            }),
        ),

        ArisOperationError::InvalidRequest(reason) => {
            api_json(StatusCode::BAD_REQUEST, json!({ "response": reason }))
        }

        ArisOperationError::N1(reason) => {
            crate::report_error!(reason.clone(), "n1", "ARIS attachment operation");

            api_json(
                StatusCode::BAD_GATEWAY,
                json!({
                    "response":
                        "The ARIS database is available, but N1 could not complete the attachment operation."
                }),
            )
        }

        ArisOperationError::Sqlite(_) => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({
                "response": "SQLite database operation failed."
            }),
        ),
    }
}

/* -------------------------------------------------------------------------- */
/* N1 configuration/auth                                                      */
/* -------------------------------------------------------------------------- */

fn n1_base_url() -> Result<String, ArisOperationError> {
    let value = CONFIG.n1.base_url.trim();

    if value.is_empty() {
        return Err(ArisOperationError::N1(
            "config.ini [n1].base_url is empty.".to_string(),
        ));
    }

    Ok(value.trim_end_matches('/').to_string())
}

fn n1_fragment() -> Result<String, ArisOperationError> {
    let value = CONFIG.n1.fragment.trim();

    if value.is_empty() {
        return Err(ArisOperationError::N1(
            "config.ini [n1].fragment is empty.".to_string(),
        ));
    }

    Ok(value.to_string())
}

fn n1_secret() -> Result<String, ArisOperationError> {
    env::var("N1_ARIS_SECRET")
        .map(|value| value.trim().to_string())
        .map_err(|_| {
            ArisOperationError::N1(
                "N1_ARIS_SECRET is missing from the ARIS environment/.env.".to_string(),
            )
        })
        .and_then(|value| {
            if value.is_empty() {
                Err(ArisOperationError::N1(
                    "N1_ARIS_SECRET is empty.".to_string(),
                ))
            } else {
                Ok(value)
            }
        })
}

fn max_attachment_size_bytes() -> usize {
    CONFIG
        .n1
        .attachment_max_size_mb
        .max(1)
        .saturating_mul(1024 * 1024)
}

fn n1_client() -> Result<&'static Client, ArisOperationError> {
    if let Some(client) = N1_CLIENT.get() {
        return Ok(client);
    }

    let client = Client::builder()
        .danger_accept_invalid_certs(CONFIG.n1.insecure_tls)
        .build()
        .map_err(|error| {
            ArisOperationError::N1(format!("failed to build N1 HTTP client: {error}"))
        })?;

    let _ = N1_CLIENT.set(client);

    N1_CLIENT
        .get()
        .ok_or_else(|| ArisOperationError::N1("failed to initialize N1 HTTP client".to_string()))
}

fn n1_token_cache() -> &'static RwLock<Option<CachedN1Token>> {
    N1_TOKEN_CACHE.get_or_init(|| RwLock::new(None))
}

pub(crate) async fn n1_access_token() -> Result<String, ArisOperationError> {
    let safety_window = Duration::from_secs(30);

    {
        let guard = n1_token_cache().read().await;

        if let Some(cached) = guard.as_ref() {
            if cached.expires_at > Instant::now() + safety_window {
                return Ok(cached.access_token.clone());
            }
        }
    }

    let mut guard = n1_token_cache().write().await;

    // Another request may have refreshed the token while this request waited.
    if let Some(cached) = guard.as_ref() {
        if cached.expires_at > Instant::now() + safety_window {
            return Ok(cached.access_token.clone());
        }
    }

    let base_url = n1_base_url()?;
    let fragment = n1_fragment()?;
    let secret = n1_secret()?;

    let response = n1_client()?
        .post(format!("{base_url}/noa/v1/auth/fragment"))
        .json(&N1AuthRequest {
            fragment: &fragment,
            secret: &secret,
            read: true,
            write: true,
            modify: true,
            delete: true,
            expires_in: 3600,
        })
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!("failed to authenticate to N1: {error}"))
        })?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(ArisOperationError::N1(format!(
            "N1 fragment authentication failed with HTTP {status}: {text}"
        )));
    }

    let auth: N1AuthResponse = serde_json::from_str(&text).map_err(|error| {
        ArisOperationError::N1(format!("invalid N1 authentication response: {error}"))
    })?;

    let usable_seconds = auth.expires_in.saturating_sub(30).max(1);

    *guard = Some(CachedN1Token {
        access_token: auth.access_token.clone(),
        expires_at: Instant::now() + Duration::from_secs(usable_seconds),
    });

    Ok(auth.access_token)
}

/* -------------------------------------------------------------------------- */
/* N1 namespace                                                               */
/* -------------------------------------------------------------------------- */

fn sanitize_namespace_component(value: &str, fallback: &str) -> String {
    let mut output = String::with_capacity(value.len());

    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric()
            || character == '.'
            || character == '-'
            || character == '_'
        {
            output.push(character);
        } else if character.is_whitespace() {
            output.push('_');
        } else {
            output.push('_');
        }
    }

    while output.contains("__") {
        output = output.replace("__", "_");
    }

    let output = output
        .trim_matches(|character| character == '.' || character == '_' || character == '-')
        .to_string();

    if output.is_empty() {
        fallback.to_string()
    } else {
        output
    }
}

fn sanitize_file_name(file_name: &str) -> String {
    sanitize_namespace_component(file_name, "attachment.bin")
}

fn attachment_object_key(storage: &ResolvedRecordStorage, file_name: &str) -> String {
    let file_name = sanitize_file_name(file_name);

    let visible_name = match storage.file_prefix.as_deref() {
        Some(prefix) if !prefix.trim().is_empty() => format!(
            "{}__{}",
            sanitize_namespace_component(prefix, "record"),
            file_name
        ),
        _ => file_name,
    };

    format!(
        "{}/{}",
        storage.directory_path.trim_end_matches('/'),
        visible_name
    )
}

async fn n1_prepare_directory_path(
    directory_path: &str,
    token: &str,
) -> Result<(), ArisOperationError> {
    let mut current = String::new();

    for component in directory_path
        .split('/')
        .map(str::trim)
        .filter(|component| !component.is_empty())
    {
        if !current.is_empty() {
            current.push('/');
        }

        current.push_str(component);
        n1_ensure_directory(&current, token).await?;
    }

    Ok(())
}

pub(crate) async fn n1_ensure_directory(path: &str, token: &str) -> Result<(), ArisOperationError> {
    let base_url = n1_base_url()?;

    let response = n1_client()?
        .get(format!("{base_url}/noa/v1/posix/stat"))
        .bearer_auth(token)
        .query(&[("path", path)])
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!("failed to stat N1 directory '{path}': {error}"))
        })?;

    if response.status().is_success() {
        return Ok(());
    }

    if response.status() != StatusCode::NOT_FOUND {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        return Err(ArisOperationError::N1(format!(
            "N1 stat failed for '{path}' with HTTP {status}: {text}"
        )));
    }

    let response = n1_client()?
        .post(format!("{base_url}/noa/v1/posix/mkdir"))
        .bearer_auth(token)
        .json(&json!({ "path": path }))
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!("failed to create N1 directory '{path}': {error}"))
        })?;

    if response.status().is_success() || response.status() == StatusCode::CONFLICT {
        // 409 is acceptable here because another concurrent request may have
        // created the same office/date directory after our stat request.
        return Ok(());
    }

    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    Err(ArisOperationError::N1(format!(
        "N1 mkdir failed for '{path}' with HTTP {status}: {text}"
    )))
}

pub(crate) async fn n1_upload_one_shot(
    object_key: &str,
    mime_type: &str,
    bytes: Vec<u8>,
    token: &str,
) -> Result<(), ArisOperationError> {
    let base_url = n1_base_url()?;

    let response = n1_client()?
        .post(format!("{base_url}/noa/v1/upload/one-shot/{object_key}"))
        .bearer_auth(token)
        .header(CONTENT_TYPE, mime_type)
        .body(bytes)
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!("failed to upload '{object_key}' to N1: {error}"))
        })?;

    let status = response.status();

    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();

        return Err(ArisOperationError::N1(format!(
            "N1 upload failed for '{object_key}' with HTTP {status}: {text}"
        )));
    }

    Ok(())
}

pub(crate) async fn n1_soft_delete(object_key: &str) -> Result<(), ArisOperationError> {
    let base_url = n1_base_url()?;
    let token = n1_access_token().await?;

    let response = n1_client()?
        .delete(format!("{base_url}/noa/v1/objects/{object_key}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!(
                "failed to soft-delete '{object_key}' from N1: {error}"
            ))
        })?;

    if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
        return Ok(());
    }

    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    Err(ArisOperationError::N1(format!(
        "N1 soft-delete failed for '{object_key}' with HTTP {status}: {text}"
    )))
}

async fn n1_recover(object_key: &str) -> Result<(), ArisOperationError> {
    let base_url = n1_base_url()?;
    let token = n1_access_token().await?;

    let response = n1_client()?
        .post(format!("{base_url}/noa/v1/objects/recover/{object_key}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!("failed to recover '{object_key}' in N1: {error}"))
        })?;

    let status = response.status();

    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();

        return Err(ArisOperationError::N1(format!(
            "N1 recovery failed for '{object_key}' with HTTP {status}: {text}"
        )));
    }

    Ok(())
}

pub(crate) async fn n1_download(
    object_key: &str,
    file_name: &str,
) -> Result<Vec<u8>, ArisOperationError> {
    let base_url = n1_base_url()?;
    let token = n1_access_token().await?;

    let response = n1_client()?
        .get(format!("{base_url}/noa/v1/objects/download"))
        .bearer_auth(token)
        .query(&[("object_key", object_key), ("filename", file_name)])
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!(
                "failed to download '{object_key}' from N1: {error}"
            ))
        })?;

    let status = response.status();

    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();

        return Err(ArisOperationError::N1(format!(
            "N1 download failed for '{object_key}' with HTTP {status}: {text}"
        )));
    }

    let bytes = response.bytes().await.map_err(|error| {
        ArisOperationError::N1(format!(
            "failed to read N1 download body for '{object_key}': {error}"
        ))
    })?;

    Ok(bytes.to_vec())
}

fn attachment_extension(file_name: &str) -> Option<String> {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.trim().to_ascii_lowercase())?;

    if extension.is_empty() {
        None
    } else {
        Some(extension)
    }
}

fn office_preview_supported(file_name: &str) -> bool {
    matches!(
        attachment_extension(file_name).as_deref(),
        Some(
            "doc"
                | "docx"
                | "docm"
                | "rtf"
                | "odt"
                | "xls"
                | "xlsx"
                | "xlsm"
                | "ods"
                | "ppt"
                | "pptx"
                | "pptm"
                | "odp"
        )
    )
}

fn safe_cache_component(value: &str) -> String {
    let mut result = String::with_capacity(value.len());

    for character in value.chars() {
        if character.is_ascii_alphanumeric()
            || character == '-'
            || character == '_'
            || character == '.'
        {
            result.push(character);
        } else {
            result.push('_');
        }
    }

    if result.is_empty() {
        "unknown".to_string()
    } else {
        result
    }
}

fn aris_preview_cache_path(attachment: &FileAttachment) -> PathBuf {
    let cache_root = env::temp_dir().join("aris-preview-cache");

    let version = attachment.version_id.as_deref().unwrap_or("original");

    cache_root.join(format!(
        "{}__{}.pdf",
        safe_cache_component(&attachment.uid),
        safe_cache_component(version),
    ))
}

fn generate_office_pdf_preview(
    attachment: FileAttachment,
    original_bytes: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let cache_path = aris_preview_cache_path(&attachment);

    if cache_path.is_file() {
        return fs::read(&cache_path).map_err(|error| {
            format!(
                "failed to read cached ARIS preview '{}': {error}",
                cache_path.display()
            )
        });
    }

    let extension = attachment_extension(&attachment.file_name)
        .ok_or_else(|| "attachment has no supported Office extension".to_string())?;

    let work_dir = env::temp_dir().join(format!("aris-preview-work-{}", Uuid::new_v4()));

    let profile_dir = work_dir.join("lo-profile");
    let source_path = work_dir.join(format!("source.{extension}"));
    let generated_pdf = work_dir.join("source.pdf");

    let result = (|| -> Result<Vec<u8>, String> {
        fs::create_dir_all(&profile_dir)
            .map_err(|error| format!("failed to create ARIS preview work directory: {error}"))?;

        fs::write(&source_path, original_bytes)
            .map_err(|error| format!("failed to stage Office attachment for preview: {error}"))?;

        let profile_uri = format!("file://{}", profile_dir.to_string_lossy());

        let output = Command::new("libreoffice")
            .arg("--headless")
            .arg("--nologo")
            .arg("--nodefault")
            .arg("--nolockcheck")
            .arg("--nofirststartwizard")
            .arg(format!("-env:UserInstallation={profile_uri}"))
            .arg("--convert-to")
            .arg("pdf")
            .arg("--outdir")
            .arg(&work_dir)
            .arg(&source_path)
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    "LibreOffice is not installed or is not available in PATH".to_string()
                } else {
                    format!("failed to start LibreOffice: {error}")
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            return Err(format!(
                "LibreOffice preview conversion failed. stdout='{}' stderr='{}'",
                stdout.trim(),
                stderr.trim(),
            ));
        }

        if !generated_pdf.is_file() {
            return Err("LibreOffice completed without producing a PDF preview".to_string());
        }

        let pdf_bytes = fs::read(&generated_pdf)
            .map_err(|error| format!("failed to read generated PDF preview: {error}"))?;

        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create ARIS preview cache: {error}"))?;
        }

        fs::write(&cache_path, &pdf_bytes)
            .map_err(|error| format!("failed to cache generated ARIS preview: {error}"))?;

        Ok(pdf_bytes)
    })();

    let _ = fs::remove_dir_all(&work_dir);

    result
}

fn inline_attachment_response(bytes: Vec<u8>, content_type: &str, file_name: &str) -> Response {
    let safe_name = file_name.replace('"', "_");

    let mut response = Response::new(Body::from(bytes));

    *response.status_mut() = StatusCode::OK;

    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );

    if let Ok(value) = HeaderValue::from_str(&format!("inline; filename=\"{safe_name}\"")) {
        response.headers_mut().insert(CONTENT_DISPOSITION, value);
    }

    response
}

/* -------------------------------------------------------------------------- */
/* SQLite: ARIS record schema                                                 */
/* -------------------------------------------------------------------------- */

pub(crate) fn ensure_aris_record_schema(
    connection: &rusqlite::Connection,
) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS aris_control_sequence (
            year        INTEGER PRIMARY KEY NOT NULL,
            last_value  INTEGER NOT NULL DEFAULT 0 CHECK(last_value >= 0)
        );

        CREATE TABLE IF NOT EXISTS aris_records (
            uid             TEXT PRIMARY KEY NOT NULL,
            control_year    INTEGER NOT NULL,
            control_no      INTEGER NOT NULL CHECK(control_no > 0),
            date            TEXT NOT NULL,
            office          TEXT NOT NULL,
            requestor       TEXT NOT NULL,
            subject         TEXT NOT NULL,
            routed_to_div   TEXT NOT NULL,
            remarks         TEXT NOT NULL,
            UNIQUE(control_year, control_no)
        );

        CREATE TABLE IF NOT EXISTS aris_attachments (
            uid         TEXT PRIMARY KEY NOT NULL,
            entry_uid   TEXT NOT NULL,
            file_name   TEXT NOT NULL COLLATE NOCASE,
            mime_type   TEXT NOT NULL,
            size        INTEGER NOT NULL CHECK(size >= 0),
            object_key  TEXT NOT NULL UNIQUE,
            version_id  TEXT,
            FOREIGN KEY (entry_uid)
                REFERENCES aris_records(uid)
                ON DELETE CASCADE,
            UNIQUE(entry_uid, file_name)
        );

        CREATE INDEX IF NOT EXISTS idx_aris_records_date
            ON aris_records(date DESC, control_no DESC);

        CREATE INDEX IF NOT EXISTS idx_aris_records_office
            ON aris_records(office COLLATE NOCASE);

        CREATE INDEX IF NOT EXISTS idx_aris_records_requestor
            ON aris_records(requestor COLLATE NOCASE);

        CREATE INDEX IF NOT EXISTS idx_aris_attachments_entry_uid
            ON aris_attachments(entry_uid);

        CREATE INDEX IF NOT EXISTS idx_aris_attachments_file_name
            ON aris_attachments(file_name COLLATE NOCASE);

        CREATE INDEX IF NOT EXISTS idx_aris_records_office_date
            ON aris_records(office COLLATE NOCASE, date DESC, control_no DESC);

        CREATE INDEX IF NOT EXISTS idx_aris_records_route_date
            ON aris_records(routed_to_div COLLATE NOCASE, date DESC, control_no DESC);

        CREATE INDEX IF NOT EXISTS idx_aris_records_requestor_date
            ON aris_records(requestor COLLATE NOCASE, date DESC, control_no DESC);

        CREATE TABLE IF NOT EXISTS aris_revision (
            id               INTEGER PRIMARY KEY NOT NULL CHECK(id = 1),
            records_revision INTEGER NOT NULL DEFAULT 0 CHECK(records_revision >= 0)
        );

        INSERT OR IGNORE INTO aris_revision (
            id,
            records_revision
        ) VALUES (
            1,
            0
        );

        CREATE TRIGGER IF NOT EXISTS trg_aris_records_revision_insert
        AFTER INSERT ON aris_records
        BEGIN
            UPDATE aris_revision
            SET records_revision = records_revision + 1
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_aris_records_revision_update
        AFTER UPDATE ON aris_records
        BEGIN
            UPDATE aris_revision
            SET records_revision = records_revision + 1
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_aris_records_revision_delete
        AFTER DELETE ON aris_records
        BEGIN
            UPDATE aris_revision
            SET records_revision = records_revision + 1
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_aris_attachments_revision_insert
        AFTER INSERT ON aris_attachments
        BEGIN
            UPDATE aris_revision
            SET records_revision = records_revision + 1
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_aris_attachments_revision_update
        AFTER UPDATE ON aris_attachments
        BEGIN
            UPDATE aris_revision
            SET records_revision = records_revision + 1
            WHERE id = 1;
        END;

        CREATE TRIGGER IF NOT EXISTS trg_aris_attachments_revision_delete
        AFTER DELETE ON aris_attachments
        BEGIN
            UPDATE aris_revision
            SET records_revision = records_revision + 1
            WHERE id = 1;
        END;
        "#,
    )
}

/* -------------------------------------------------------------------------- */
/* SQLite: entry/attachment lookups                                           */
/* -------------------------------------------------------------------------- */

async fn execute_record_exists(entry_uid: String) -> Result<(), SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<(), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_aris_record_schema(connection)?;

                connection.query_row(
                    r#"
                        SELECT 1
                        FROM aris_records
                        WHERE uid = ?1
                        LIMIT 1
                        "#,
                    params![entry_uid],
                    |_row| Ok(()),
                )
            })
        })
        .await;

    match database_result {
        Ok(Ok(())) => Ok(()),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => {
            Err(SqliteError::NotFound)
        }

        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_attachment_name_exists(
    entry_uid: String,
    file_name: String,
) -> Result<bool, SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<bool, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_aris_record_schema(connection)?;
                let exists = connection
                    .query_row(
                        r#"
                            SELECT 1
                            FROM aris_attachments
                            WHERE entry_uid = ?1
                              AND file_name = ?2 COLLATE NOCASE
                            LIMIT 1
                            "#,
                        params![entry_uid, file_name,],
                        |_row| Ok(true),
                    )
                    .optional()?
                    .unwrap_or(false);

                Ok(exists)
            })
        })
        .await;

    match database_result {
        Ok(Ok(exists)) => Ok(exists),
        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_attachment_object_key_exists(object_key: String) -> Result<bool, SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<bool, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_aris_record_schema(connection)?;

                let exists = connection
                    .query_row(
                        r#"
                            SELECT 1
                            FROM aris_attachments
                            WHERE object_key = ?1
                            LIMIT 1
                            "#,
                        params![object_key],
                        |_row| Ok(true),
                    )
                    .optional()?
                    .unwrap_or(false);

                Ok(exists)
            })
        })
        .await;

    match database_result {
        Ok(Ok(exists)) => Ok(exists),
        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_insert_attachment(
    entry_uid: String,
    attachment: FileAttachment,
) -> Result<(), SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<(), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_aris_record_schema(connection)?;
                connection.execute(
                    r#"
                        INSERT INTO aris_attachments (
                            uid,
                            entry_uid,
                            file_name,
                            mime_type,
                            size,
                            object_key,
                            version_id
                        ) VALUES (
                            ?1, ?2, ?3, ?4, ?5, ?6, ?7
                        )
                        "#,
                    params![
                        attachment.uid,
                        entry_uid,
                        attachment.file_name,
                        attachment.mime_type,
                        attachment.size as i64,
                        attachment.object_key,
                        attachment.version_id,
                    ],
                )?;

                Ok(())
            })
        })
        .await;

    match database_result {
        Ok(Ok(())) => Ok(()),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))))
            if error.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(SqliteError::Conflict)
        }

        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_get_attachment(
    entry_uid: String,
    attachment_uid: String,
) -> Result<FileAttachment, SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<FileAttachment, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_aris_record_schema(connection)?;
                let attachment = connection.query_row(
                    r#"
                            SELECT
                                uid,
                                file_name,
                                mime_type,
                                size,
                                object_key,
                                version_id
                            FROM aris_attachments
                            WHERE uid = ?1
                              AND entry_uid = ?2
                            "#,
                    params![attachment_uid, entry_uid,],
                    |row| {
                        let size: i64 = row.get(3)?;

                        Ok(FileAttachment {
                            uid: row.get(0)?,
                            file_name: row.get(1)?,
                            mime_type: row.get(2)?,
                            size: size.max(0) as u64,
                            object_key: row.get(4)?,
                            version_id: row.get(5)?,
                        })
                    },
                )?;

                Ok(attachment)
            })
        })
        .await;

    match database_result {
        Ok(Ok(attachment)) => Ok(attachment),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => {
            Err(SqliteError::NotFound)
        }

        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_list_attachments_for_entry(
    entry_uid: String,
) -> Result<Vec<FileAttachment>, SqliteError> {
    let database_result = tokio::task::spawn_blocking(
        move || -> Result<Vec<FileAttachment>, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_aris_record_schema(connection)?;
                let mut statement = connection.prepare(
                    r#"
                            SELECT
                                uid,
                                file_name,
                                mime_type,
                                size,
                                object_key,
                                version_id
                            FROM aris_attachments
                            WHERE entry_uid = ?1
                            ORDER BY rowid ASC
                            "#,
                )?;

                let attachments = statement
                    .query_map(params![entry_uid], |row| {
                        let size: i64 = row.get(3)?;

                        Ok(FileAttachment {
                            uid: row.get(0)?,
                            file_name: row.get(1)?,
                            mime_type: row.get(2)?,
                            size: size.max(0) as u64,
                            object_key: row.get(4)?,
                            version_id: row.get(5)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;

                Ok(attachments)
            })
        },
    )
    .await;

    match database_result {
        Ok(Ok(attachments)) => Ok(attachments),
        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_delete_attachment_metadata(
    entry_uid: String,
    attachment_uid: String,
) -> Result<(), SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<(), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_aris_record_schema(connection)?;
                let affected = connection.execute(
                    r#"
                        DELETE FROM aris_attachments
                        WHERE uid = ?1
                          AND entry_uid = ?2
                        "#,
                    params![attachment_uid, entry_uid,],
                )?;

                if affected == 0 {
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                }

                Ok(())
            })
        })
        .await;

    match database_result {
        Ok(Ok(())) => Ok(()),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => {
            Err(SqliteError::NotFound)
        }

        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_delete_entry_metadata(entry_uid: String) -> Result<(), SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<(), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_aris_record_schema(connection)?;
                crate::api::aris::schema::ensure_dynamic_schema(connection)?;

                let transaction = connection.unchecked_transaction()?;

                transaction.execute(
                    "DELETE FROM aris_unique_values WHERE record_uid = ?1",
                    params![&entry_uid],
                )?;

                transaction.execute(
                    "DELETE FROM aris_record_values WHERE record_uid = ?1",
                    params![&entry_uid],
                )?;

                transaction.execute(
                    r#"
                        DELETE FROM aris_attachments
                        WHERE entry_uid = ?1
                        "#,
                    params![&entry_uid],
                )?;

                let affected = transaction.execute(
                    r#"
                        DELETE FROM aris_records
                        WHERE uid = ?1
                        "#,
                    params![&entry_uid],
                )?;

                if affected == 0 {
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                }

                transaction.commit()?;

                Ok(())
            })
        })
        .await;

    match database_result {
        Ok(Ok(())) => Ok(()),

        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => {
            Err(SqliteError::NotFound)
        }

        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

/* -------------------------------------------------------------------------- */
/* Attachment operations                                                      */
/* -------------------------------------------------------------------------- */

pub async fn execute_upload_aris_attachment(
    entry_uid: String,
    file_name: String,
    mime_type: String,
    bytes: Vec<u8>,
) -> Result<FileAttachment, ArisOperationError> {
    if bytes.is_empty() {
        return Err(ArisOperationError::InvalidRequest(
            "Attachment is empty.".to_string(),
        ));
    }

    let max_size = max_attachment_size_bytes();

    if bytes.len() > max_size {
        return Err(ArisOperationError::InvalidRequest(format!(
            "Attachment exceeds the configured {} MiB ARIS limit.",
            CONFIG.n1.attachment_max_size_mb.max(1)
        )));
    }

    if execute_attachment_name_exists(entry_uid.clone(), file_name.clone()).await? {
        return Err(ArisOperationError::InvalidRequest(format!(
            "The entry already has an attachment named '{file_name}'. Remove the old attachment first if you want to replace it."
        )));
    }

    let attachment_uid = Uuid::new_v4().to_string();

    let storage = match resolve_record_storage(entry_uid.clone()).await {
        Ok(storage) => storage,

        Err(StorageResolutionError::MissingFieldValue(label)) => {
            return Err(ArisOperationError::InvalidRequest(format!(
                "N1 storage layout uses '{label}', but this record has no value for that field. Set the field before uploading attachments."
            )));
        }

        Err(StorageResolutionError::NotFound) => {
            return Err(ArisOperationError::Sqlite(SqliteError::NotFound));
        }

        Err(StorageResolutionError::Database) => {
            return Err(ArisOperationError::Sqlite(SqliteError::SqliteDatabaseError));
        }
    };

    let object_key = attachment_object_key(&storage, &file_name);

    if execute_attachment_object_key_exists(object_key.clone()).await? {
        return Err(ArisOperationError::InvalidRequest(format!(
            "N1 storage layout collision for '{object_key}'. Configure a unique file-prefix field or add another folder field before uploading this attachment."
        )));
    }

    let size = bytes.len() as u64;
    let token = n1_access_token().await?;

    n1_prepare_directory_path(&storage.directory_path, &token).await?;

    n1_upload_one_shot(&object_key, &mime_type, bytes, &token).await?;

    let attachment = FileAttachment {
        uid: attachment_uid,
        file_name,
        mime_type,
        size,
        object_key: object_key.clone(),
        version_id: None,
    };

    if let Err(error) = execute_insert_attachment(entry_uid, attachment.clone()).await {
        /*
        SQLite failed after N1 committed the object. Move the object into N1
        trash so ARIS does not intentionally leave a live unreferenced ARIS file.
        */
        let _ = n1_soft_delete(&object_key).await;

        return Err(ArisOperationError::Sqlite(error));
    }

    Ok(attachment)
}

pub async fn execute_delete_aris_attachment(
    entry_uid: String,
    attachment_uid: String,
) -> Result<(), ArisOperationError> {
    let attachment = execute_get_attachment(entry_uid.clone(), attachment_uid.clone()).await?;

    n1_soft_delete(&attachment.object_key).await?;

    if let Err(error) = execute_delete_attachment_metadata(entry_uid, attachment_uid).await {
        let _ = n1_recover(&attachment.object_key).await;

        return Err(ArisOperationError::Sqlite(error));
    }

    Ok(())
}

pub async fn execute_delete_aris_record_with_attachments(
    entry_uid: String,
) -> Result<(), ArisOperationError> {
    // Ensure the entry exists before touching N1.
    execute_record_exists(entry_uid.clone()).await?;

    let attachments = execute_list_attachments_for_entry(entry_uid.clone()).await?;

    let mut deleted_object_keys: Vec<String> = Vec::new();

    for attachment in &attachments {
        match n1_soft_delete(&attachment.object_key).await {
            Ok(()) => {
                deleted_object_keys.push(attachment.object_key.clone());
            }

            Err(error) => {
                for object_key in deleted_object_keys.iter().rev() {
                    let _ = n1_recover(object_key).await;
                }

                return Err(error);
            }
        }
    }

    if let Err(error) = execute_delete_entry_metadata(entry_uid).await {
        for object_key in deleted_object_keys.iter().rev() {
            let _ = n1_recover(object_key).await;
        }

        return Err(ArisOperationError::Sqlite(error));
    }

    // The yearly sequence is intentionally NOT decremented.
    // Deleted control numbers are never reused.
    Ok(())
}

/* -------------------------------------------------------------------------- */
/* Axum handlers                                                              */
/* -------------------------------------------------------------------------- */

pub async fn delete_aris_record(claims: Claims, Path(uid): Path<String>) -> Response {
    if !claims.can_delete_records() {
        return aris_access_denied();
    }

    match execute_delete_aris_record_with_attachments(uid).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),

        Err(error) => map_aris_error(error),
    }
}

pub async fn upload_aris_attachments(
    claims: Claims,
    Path(entry_uid): Path<String>,
    mut multipart: Multipart,
) -> Response {
    if !claims.can_write_records() {
        return aris_access_denied();
    }

    let mut uploaded: Vec<FileAttachment> = Vec::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,

            Err(error) => {
                return api_json(
                    StatusCode::BAD_REQUEST,
                    json!({
                        "response":
                            format!(
                                "Invalid multipart upload: {error}"
                            )
                    }),
                );
            }
        };

        if field.name() != Some("files") {
            continue;
        }

        let file_name = field
            .file_name()
            .map(str::to_string)
            .unwrap_or_else(|| "attachment.bin".to_string());

        let mime_type = field
            .content_type()
            .map(str::to_string)
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let bytes = match field.bytes().await {
            Ok(bytes) => bytes.to_vec(),

            Err(error) => {
                return api_json(
                    StatusCode::BAD_REQUEST,
                    json!({
                        "response":
                            format!(
                                "Failed to read attachment '{file_name}': {error}"
                            )
                    }),
                );
            }
        };

        match execute_upload_aris_attachment(entry_uid.clone(), file_name, mime_type, bytes).await {
            Ok(attachment) => {
                uploaded.push(attachment);
            }

            Err(error) => {
                return map_aris_error(error);
            }
        }
    }

    if uploaded.is_empty() {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({
                "response":
                    "No files were provided. Multipart field name must be 'files'."
            }),
        );
    }

    api_json(
        StatusCode::CREATED,
        json!({
            "attachments": uploaded
        }),
    )
}

pub async fn delete_aris_attachment(
    claims: Claims,
    Path((entry_uid, attachment_uid)): Path<(String, String)>,
) -> Response {
    if !claims.can_delete_records() {
        return aris_access_denied();
    }

    match execute_delete_aris_attachment(entry_uid, attachment_uid).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),

        Err(error) => map_aris_error(error),
    }
}

pub async fn preview_aris_attachment(
    claims: Claims,
    Path((entry_uid, attachment_uid)): Path<(String, String)>,
) -> Response {
    if !claims.can_read_records() {
        return aris_access_denied();
    }

    let attachment = match execute_get_attachment(entry_uid, attachment_uid).await {
        Ok(attachment) => attachment,

        Err(error) => {
            return map_aris_error(ArisOperationError::Sqlite(error));
        }
    };

    let original_bytes = match n1_download(&attachment.object_key, &attachment.file_name).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return map_aris_error(error);
        }
    };

    if office_preview_supported(&attachment.file_name) {
        let attachment_for_preview = attachment.clone();

        let preview_result = tokio::task::spawn_blocking(move || {
            generate_office_pdf_preview(attachment_for_preview, original_bytes)
        })
        .await;

        let pdf_bytes = match preview_result {
            Ok(Ok(bytes)) => bytes,

            Ok(Err(error)) => {
                crate::report_error!(error, "preview", "preview_aris_attachment()");

                return api_json(
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({
                        "response": "ARIS could not generate the Office preview. Verify that LibreOffice Writer, Calc, and Impress are installed on the ARIS server."
                    }),
                );
            }

            Err(error) => {
                crate::report_error!(
                    format!("Office preview blocking task failed: {error}"),
                    "preview",
                    "preview_aris_attachment()"
                );

                return api_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({
                        "response": "ARIS Office preview task failed."
                    }),
                );
            }
        };

        let preview_name = attachment
            .file_name
            .rsplit_once('.')
            .map(|(stem, _)| format!("{stem}.pdf"))
            .unwrap_or_else(|| format!("{}.pdf", attachment.file_name));

        return inline_attachment_response(pdf_bytes, "application/pdf", &preview_name);
    }

    inline_attachment_response(original_bytes, &attachment.mime_type, &attachment.file_name)
}

pub async fn download_aris_attachment(
    claims: Claims,
    Path((entry_uid, attachment_uid)): Path<(String, String)>,
) -> Response {
    if !claims.can_read_records() {
        return aris_access_denied();
    }

    let attachment = match execute_get_attachment(entry_uid, attachment_uid).await {
        Ok(attachment) => attachment,

        Err(error) => {
            return map_aris_error(ArisOperationError::Sqlite(error));
        }
    };

    let bytes = match n1_download(&attachment.object_key, &attachment.file_name).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return map_aris_error(error);
        }
    };

    let safe_download_name = attachment.file_name.replace('"', "_");

    let mut response = Response::new(Body::from(bytes));

    *response.status_mut() = StatusCode::OK;

    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&attachment.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );

    if let Ok(value) =
        HeaderValue::from_str(&format!("attachment; filename=\"{safe_download_name}\""))
    {
        response.headers_mut().insert(CONTENT_DISPOSITION, value);
    }

    response
}
