use std::{
    collections::{HashMap, HashSet},
    env,
    fs,
    path::PathBuf,
    process::Command,
    sync::OnceLock,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{Multipart, Path, Query},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
    Json,
};
use reqwest::Client;
use rusqlite::{
    params,
    params_from_iter,
    types::Value as SqlValue,
    OptionalExtension,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    api::{
        api_error::SqliteError,
        aris::model::{
            CreateEntryRequest,
            Entry,
            FileAttachment,
            UpdateEntryRequest,
        },
    },
    config::load_config::CONFIG,
    db::connector::{
        with_sql_connection,
        SqliteDatabaseError,
    },
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
- stores files in a human-navigable layout:

    <office>/<date>/<zero-padded-control-no>__<file-name>

Example:

    MISO/2026-09-17/000125__purchase_request.pdf

The UI never supplies control_no. ARIS generates it.

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

const DEFAULT_PAGE_LIMIT: usize = 256;
const MAX_PAGE_LIMIT: usize = 256;

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

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ArisListQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,

    // Universal search.
    pub q: Option<String>,

    #[serde(rename = "match")]
    pub match_mode: Option<String>,

    // Advanced filters.
    pub control_no: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub office: Option<String>,
    pub requestor: Option<String>,
    pub subject: Option<String>,
    pub routed_to_div: Option<String>,
    pub remarks: Option<String>,
    pub file_name: Option<String>,
    pub attachments: Option<String>,

    // Sorting.
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArisPage {
    pub data: Vec<Entry>,
    pub page: usize,
    pub limit: usize,
    pub has_next: bool,
    pub total: usize,
    pub total_pages: usize,
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

#[derive(Debug, Clone)]
struct StoredEntry {
    entry: Entry,
    control_year: i32,
}

#[derive(Debug, Clone)]
struct StorageIdentity {
    control_no: u64,
    date: String,
    office: String,
}

#[derive(Debug, Clone)]
struct AttachmentMove {
    attachment_uid: String,
    old_key: String,
    new_key: String,
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

        ArisOperationError::InvalidRequest(reason) => api_json(
            StatusCode::BAD_REQUEST,
            json!({ "response": reason }),
        ),

        ArisOperationError::N1(reason) => {
            crate::report_error!(
                reason.clone(),
                "n1",
                "ARIS attachment operation"
            );

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

fn validate_required_fields(
    date: &str,
    office: &str,
    requestor: &str,
    subject: &str,
    routed_to_div: &str,
) -> Result<(), ArisOperationError> {
    if date.trim().is_empty()
        || office.trim().is_empty()
        || requestor.trim().is_empty()
        || subject.trim().is_empty()
        || routed_to_div.trim().is_empty()
    {
        return Err(ArisOperationError::InvalidRequest(
            "Date, office, requestor, subject, and routed-to division are required."
                .to_string(),
        ));
    }

    parse_entry_year(date)?;

    Ok(())
}

fn parse_entry_year(date: &str) -> Result<i32, ArisOperationError> {
    let bytes = date.as_bytes();

    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[0..4].iter().all(|byte| byte.is_ascii_digit())
        || !bytes[5..7].iter().all(|byte| byte.is_ascii_digit())
        || !bytes[8..10].iter().all(|byte| byte.is_ascii_digit())
    {
        return Err(ArisOperationError::InvalidRequest(
            "Date must use YYYY-MM-DD format.".to_string(),
        ));
    }

    let year = date[0..4]
        .parse::<i32>()
        .map_err(|_| {
            ArisOperationError::InvalidRequest(
                "Invalid ARIS entry year.".to_string(),
            )
        })?;

    if !(1900..=9999).contains(&year) {
        return Err(ArisOperationError::InvalidRequest(
            "Invalid ARIS entry year.".to_string(),
        ));
    }

    Ok(year)
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
                "N1_ARIS_SECRET is missing from the ARIS environment/.env."
                    .to_string(),
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
            ArisOperationError::N1(format!(
                "failed to build N1 HTTP client: {error}"
            ))
        })?;

    let _ = N1_CLIENT.set(client);

    N1_CLIENT.get().ok_or_else(|| {
        ArisOperationError::N1(
            "failed to initialize N1 HTTP client".to_string(),
        )
    })
}

fn n1_token_cache() -> &'static RwLock<Option<CachedN1Token>> {
    N1_TOKEN_CACHE.get_or_init(|| RwLock::new(None))
}

async fn n1_access_token() -> Result<String, ArisOperationError> {
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
            ArisOperationError::N1(format!(
                "failed to authenticate to N1: {error}"
            ))
        })?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(ArisOperationError::N1(format!(
            "N1 fragment authentication failed with HTTP {status}: {text}"
        )));
    }

    let auth: N1AuthResponse =
        serde_json::from_str(&text).map_err(|error| {
            ArisOperationError::N1(format!(
                "invalid N1 authentication response: {error}"
            ))
        })?;

    let usable_seconds = auth.expires_in.saturating_sub(30).max(1);

    *guard = Some(CachedN1Token {
        access_token: auth.access_token.clone(),
        expires_at: Instant::now()
            + Duration::from_secs(usable_seconds),
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
        .trim_matches(|character| {
            character == '.'
                || character == '_'
                || character == '-'
        })
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

fn office_folder(office: &str) -> String {
    sanitize_namespace_component(office, "UNKNOWN_OFFICE")
}

fn attachment_object_key(
    identity: &StorageIdentity,
    file_name: &str,
) -> String {
    format!(
        "{}/{}/{:06}__{}",
        office_folder(&identity.office),
        identity.date,
        identity.control_no,
        sanitize_file_name(file_name)
    )
}

async fn n1_ensure_directory(
    path: &str,
    token: &str,
) -> Result<(), ArisOperationError> {
    let base_url = n1_base_url()?;

    let response = n1_client()?
        .get(format!("{base_url}/noa/v1/posix/stat"))
        .bearer_auth(token)
        .query(&[("path", path)])
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!(
                "failed to stat N1 directory '{path}': {error}"
            ))
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
            ArisOperationError::N1(format!(
                "failed to create N1 directory '{path}': {error}"
            ))
        })?;

    if response.status().is_success()
        || response.status() == StatusCode::CONFLICT
    {
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

async fn n1_prepare_attachment_directory(
    office: &str,
    date: &str,
    token: &str,
) -> Result<(), ArisOperationError> {
    let office = office_folder(office);
    let date_path = format!("{office}/{date}");

    n1_ensure_directory(&office, token).await?;
    n1_ensure_directory(&date_path, token).await?;

    Ok(())
}

async fn n1_upload_one_shot(
    object_key: &str,
    mime_type: &str,
    bytes: Vec<u8>,
    token: &str,
) -> Result<(), ArisOperationError> {
    let base_url = n1_base_url()?;

    let response = n1_client()?
        .post(format!(
            "{base_url}/noa/v1/upload/one-shot/{object_key}"
        ))
        .bearer_auth(token)
        .header(CONTENT_TYPE, mime_type)
        .body(bytes)
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!(
                "failed to upload '{object_key}' to N1: {error}"
            ))
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

async fn n1_soft_delete(
    object_key: &str,
) -> Result<(), ArisOperationError> {
    let base_url = n1_base_url()?;
    let token = n1_access_token().await?;

    let response = n1_client()?
        .delete(format!(
            "{base_url}/noa/v1/objects/{object_key}"
        ))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!(
                "failed to soft-delete '{object_key}' from N1: {error}"
            ))
        })?;

    if response.status().is_success()
        || response.status() == StatusCode::NOT_FOUND
    {
        return Ok(());
    }

    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    Err(ArisOperationError::N1(format!(
        "N1 soft-delete failed for '{object_key}' with HTTP {status}: {text}"
    )))
}

async fn n1_recover(
    object_key: &str,
) -> Result<(), ArisOperationError> {
    let base_url = n1_base_url()?;
    let token = n1_access_token().await?;

    let response = n1_client()?
        .post(format!(
            "{base_url}/noa/v1/objects/recover/{object_key}"
        ))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!(
                "failed to recover '{object_key}' in N1: {error}"
            ))
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

async fn n1_rename(
    source_key: &str,
    destination_key: &str,
    token: &str,
) -> Result<(), ArisOperationError> {
    if source_key == destination_key {
        return Ok(());
    }

    let base_url = n1_base_url()?;
    let fragment = n1_fragment()?;

    let response = n1_client()?
        .post(format!(
            "{base_url}/noa/v1/objects/rename"
        ))
        .bearer_auth(token)
        .json(&json!({
            "source_fragment": fragment,
            "source_key": source_key,
            "destination_fragment": fragment,
            "destination_key": destination_key,
        }))
        .send()
        .await
        .map_err(|error| {
            ArisOperationError::N1(format!(
                "failed to move N1 object '{source_key}' to '{destination_key}': {error}"
            ))
        })?;

    let status = response.status();

    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();

        return Err(ArisOperationError::N1(format!(
            "N1 move failed from '{source_key}' to '{destination_key}' with HTTP {status}: {text}"
        )));
    }

    Ok(())
}

async fn n1_download(
    object_key: &str,
    file_name: &str,
) -> Result<Vec<u8>, ArisOperationError> {
    let base_url = n1_base_url()?;
    let token = n1_access_token().await?;

    let response = n1_client()?
        .get(format!(
            "{base_url}/noa/v1/objects/download"
        ))
        .bearer_auth(token)
        .query(&[
            ("object_key", object_key),
            ("filename", file_name),
        ])
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

fn aris_preview_cache_path(
    attachment: &FileAttachment,
) -> PathBuf {
    let cache_root = env::temp_dir()
        .join("aris-preview-cache");

    let version = attachment
        .version_id
        .as_deref()
        .unwrap_or("original");

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
    let cache_path =
        aris_preview_cache_path(&attachment);

    if cache_path.is_file() {
        return fs::read(&cache_path).map_err(|error| {
            format!(
                "failed to read cached ARIS preview '{}': {error}",
                cache_path.display()
            )
        });
    }

    let extension =
        attachment_extension(&attachment.file_name)
            .ok_or_else(|| {
                "attachment has no supported Office extension"
                    .to_string()
            })?;

    let work_dir = env::temp_dir().join(format!(
        "aris-preview-work-{}",
        Uuid::new_v4()
    ));

    let profile_dir = work_dir.join("lo-profile");
    let source_path =
        work_dir.join(format!("source.{extension}"));
    let generated_pdf = work_dir.join("source.pdf");

    let result = (|| -> Result<Vec<u8>, String> {
        fs::create_dir_all(&profile_dir).map_err(
            |error| {
                format!(
                    "failed to create ARIS preview work directory: {error}"
                )
            },
        )?;

        fs::write(&source_path, original_bytes).map_err(
            |error| {
                format!(
                    "failed to stage Office attachment for preview: {error}"
                )
            },
        )?;

        let profile_uri = format!(
            "file://{}",
            profile_dir.to_string_lossy()
        );

        let output = Command::new("libreoffice")
            .arg("--headless")
            .arg("--nologo")
            .arg("--nodefault")
            .arg("--nolockcheck")
            .arg("--nofirststartwizard")
            .arg(format!(
                "-env:UserInstallation={profile_uri}"
            ))
            .arg("--convert-to")
            .arg("pdf")
            .arg("--outdir")
            .arg(&work_dir)
            .arg(&source_path)
            .output()
            .map_err(|error| {
                if error.kind()
                    == std::io::ErrorKind::NotFound
                {
                    "LibreOffice is not installed or is not available in PATH"
                        .to_string()
                } else {
                    format!(
                        "failed to start LibreOffice: {error}"
                    )
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(
                &output.stderr
            );
            let stdout = String::from_utf8_lossy(
                &output.stdout
            );

            return Err(format!(
                "LibreOffice preview conversion failed. stdout='{}' stderr='{}'",
                stdout.trim(),
                stderr.trim(),
            ));
        }

        if !generated_pdf.is_file() {
            return Err(
                "LibreOffice completed without producing a PDF preview"
                    .to_string(),
            );
        }

        let pdf_bytes = fs::read(&generated_pdf)
            .map_err(|error| {
                format!(
                    "failed to read generated PDF preview: {error}"
                )
            })?;

        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).map_err(
                |error| {
                    format!(
                        "failed to create ARIS preview cache: {error}"
                    )
                },
            )?;
        }

        fs::write(&cache_path, &pdf_bytes).map_err(
            |error| {
                format!(
                    "failed to cache generated ARIS preview: {error}"
                )
            },
        )?;

        Ok(pdf_bytes)
    })();

    let _ = fs::remove_dir_all(&work_dir);

    result
}

fn inline_attachment_response(
    bytes: Vec<u8>,
    content_type: &str,
    file_name: &str,
) -> Response {
    let safe_name = file_name.replace('"', "_");

    let mut response =
        Response::new(Body::from(bytes));

    *response.status_mut() = StatusCode::OK;

    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| {
                HeaderValue::from_static(
                    "application/octet-stream"
                )
            }),
    );

    if let Ok(value) = HeaderValue::from_str(
        &format!("inline; filename=\"{safe_name}\"")
    ) {
        response.headers_mut().insert(
            CONTENT_DISPOSITION,
            value,
        );
    }

    response
}


/* -------------------------------------------------------------------------- */
/* SQLite: ARIS record schema                                                 */
/* -------------------------------------------------------------------------- */

fn ensure_aris_record_schema(
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
        "#,
    )
}

/* -------------------------------------------------------------------------- */
/* Search SQL                                                                 */
/* -------------------------------------------------------------------------- */

fn normalize_match_mode(value: Option<&str>) -> &'static str {
    match value
        .unwrap_or("contains")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "exact" => "exact",
        "prefix" => "prefix",
        _ => "contains",
    }
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn build_text_condition(
    column: &str,
    value: &str,
    match_mode: &str,
    sql_params: &mut Vec<SqlValue>,
) -> String {
    match match_mode {
        "exact" => {
            sql_params.push(SqlValue::Text(value.to_string()));

            format!(
                "LOWER({column}) = LOWER(?)"
            )
        }

        "prefix" => {
            sql_params.push(SqlValue::Text(format!(
                "{}%",
                escape_like(value)
            )));

            format!(
                "LOWER({column}) LIKE LOWER(?) ESCAPE '\\'"
            )
        }

        _ => {
            sql_params.push(SqlValue::Text(format!(
                "%{}%",
                escape_like(value)
            )));

            format!(
                "LOWER({column}) LIKE LOWER(?) ESCAPE '\\'"
            )
        }
    }
}

fn build_attachment_name_condition(
    value: &str,
    match_mode: &str,
    sql_params: &mut Vec<SqlValue>,
) -> String {
    let inner = build_text_condition(
        "af.file_name",
        value,
        match_mode,
        sql_params,
    );

    format!(
        "EXISTS (
            SELECT 1
            FROM aris_attachments af
            WHERE af.entry_uid = e.uid
              AND {inner}
        )"
    )
}

fn build_aris_where_clause(
    query: &ArisListQuery,
) -> (String, Vec<SqlValue>) {
    let mut conditions: Vec<String> = Vec::new();
    let mut sql_params: Vec<SqlValue> = Vec::new();

    let match_mode =
        normalize_match_mode(query.match_mode.as_deref());

    if let Some(value) = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let mut universal: Vec<String> = Vec::new();

        for column in [
            "CAST(e.control_no AS TEXT)",
            "CAST(e.control_year AS TEXT)",
            "e.date",
            "e.office",
            "e.requestor",
            "e.subject",
            "e.routed_to_div",
            "e.remarks",
        ] {
            universal.push(build_text_condition(
                column,
                value,
                match_mode,
                &mut sql_params,
            ));
        }

        universal.push(build_attachment_name_condition(
            value,
            match_mode,
            &mut sql_params,
        ));

        conditions.push(format!(
            "({})",
            universal.join(" OR ")
        ));
    }

    if let Some(value) = query
        .control_no
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(build_text_condition(
            "CAST(e.control_no AS TEXT)",
            value,
            match_mode,
            &mut sql_params,
        ));
    }

    if let Some(value) = query
        .office
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(build_text_condition(
            "e.office",
            value,
            match_mode,
            &mut sql_params,
        ));
    }

    if let Some(value) = query
        .requestor
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(build_text_condition(
            "e.requestor",
            value,
            match_mode,
            &mut sql_params,
        ));
    }

    if let Some(value) = query
        .subject
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(build_text_condition(
            "e.subject",
            value,
            match_mode,
            &mut sql_params,
        ));
    }

    if let Some(value) = query
        .routed_to_div
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(build_text_condition(
            "e.routed_to_div",
            value,
            match_mode,
            &mut sql_params,
        ));
    }

    if let Some(value) = query
        .remarks
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(build_text_condition(
            "e.remarks",
            value,
            match_mode,
            &mut sql_params,
        ));
    }

    if let Some(value) = query
        .file_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        conditions.push(build_attachment_name_condition(
            value,
            match_mode,
            &mut sql_params,
        ));
    }

    if let Some(value) = query
        .date_from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        sql_params.push(SqlValue::Text(value.to_string()));
        conditions.push("e.date >= ?".to_string());
    }

    if let Some(value) = query
        .date_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        sql_params.push(SqlValue::Text(value.to_string()));
        conditions.push("e.date <= ?".to_string());
    }

    match query
        .attachments
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "with" => {
            conditions.push(
                "EXISTS (
                    SELECT 1
                    FROM aris_attachments af
                    WHERE af.entry_uid = e.uid
                )"
                .to_string(),
            );
        }

        "without" => {
            conditions.push(
                "NOT EXISTS (
                    SELECT 1
                    FROM aris_attachments af
                    WHERE af.entry_uid = e.uid
                )"
                .to_string(),
            );
        }

        _ => {}
    }

    if conditions.is_empty() {
        ("".to_string(), sql_params)
    } else {
        (
            format!("WHERE {}", conditions.join(" AND ")),
            sql_params,
        )
    }
}

fn build_order_clause(query: &ArisListQuery) -> String {
    let direction = match query
        .sort_dir
        .as_deref()
        .unwrap_or("desc")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "asc" => "ASC",
        _ => "DESC",
    };

    match query
        .sort_by
        .as_deref()
        .unwrap_or("date")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "control_no" => format!(
            "ORDER BY e.control_year {direction}, e.control_no {direction}"
        ),

        "office" => format!(
            "ORDER BY e.office {direction}, e.date DESC, e.control_no DESC"
        ),

        "requestor" => format!(
            "ORDER BY e.requestor {direction}, e.date DESC, e.control_no DESC"
        ),

        "subject" => format!(
            "ORDER BY e.subject {direction}, e.date DESC, e.control_no DESC"
        ),

        "routed_to_div" => format!(
            "ORDER BY e.routed_to_div {direction}, e.date DESC, e.control_no DESC"
        ),

        _ => format!(
            "ORDER BY e.date {direction}, e.control_no {direction}"
        ),
    }
}

/* -------------------------------------------------------------------------- */
/* SQLite: create/update/list                                                 */
/* -------------------------------------------------------------------------- */

pub async fn execute_create_aris_record(
    request: CreateEntryRequest,
    year: i32,
    content_type: &str,
    function_name: &str,
) -> Result<Entry, SqliteError> {
    let uid = Uuid::new_v4().to_string();

    let content_type = content_type.to_string();
    let function_name = function_name.to_string();

    let database_result =
        tokio::task::spawn_blocking(
            move || -> Result<Entry, SqliteDatabaseError> {
                with_sql_connection(|connection| {
                    ensure_aris_record_schema(connection)?;
                    /*
                    The sequence increment and the new ARIS row live in the same
                    SQLite transaction.

                    SQLite serializes writers. With WAL + busy_timeout this is a
                    very short write section, so concurrent creates cannot receive
                    the same number.

                    If the entry insert fails, the transaction rolls back the
                    sequence increment too.
                    */
                    let transaction =
                        connection.unchecked_transaction()?;

                    transaction.execute(
                        r#"
                        INSERT OR IGNORE INTO aris_control_sequence (
                            year,
                            last_value
                        ) VALUES (?1, 0)
                        "#,
                        params![year],
                    )?;

                    transaction.execute(
                        r#"
                        UPDATE aris_control_sequence
                        SET last_value = last_value + 1
                        WHERE year = ?1
                        "#,
                        params![year],
                    )?;

                    let control_no: i64 = transaction.query_row(
                        r#"
                        SELECT last_value
                        FROM aris_control_sequence
                        WHERE year = ?1
                        "#,
                        params![year],
                        |row| row.get(0),
                    )?;

                    transaction.execute(
                        r#"
                        INSERT INTO aris_records (
                            uid,
                            control_year,
                            control_no,
                            date,
                            office,
                            requestor,
                            subject,
                            routed_to_div,
                            remarks
                        ) VALUES (
                            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
                        )
                        "#,
                        params![
                            &uid,
                            year,
                            control_no,
                            &request.date,
                            &request.office,
                            &request.requestor,
                            &request.subject,
                            &request.routed_to_div,
                            &request.remarks,
                        ],
                    )?;

                    transaction.commit()?;

                    Ok(Entry {
                        uid,
                        control_no: control_no.max(0) as u64,
                        date: request.date,
                        office: request.office,
                        requestor: request.requestor,
                        subject: request.subject,
                        routed_to_div: request.routed_to_div,
                        attached_files: Vec::new(),
                        remarks: request.remarks,
                    })
                })
            },
        )
        .await;

    match database_result {
        Ok(Ok(entry)) => Ok(entry),

        Ok(Err(SqliteDatabaseError::Sqlite(
            rusqlite::Error::SqliteFailure(error, _),
        ))) if error.code
            == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(SqliteError::Conflict)
        }

        Ok(Err(error)) => {
            crate::report_error!(
                format!("{error}"),
                &content_type,
                &function_name
            );

            Err(SqliteError::SqliteDatabaseError)
        }

        Err(error) => {
            crate::report_error!(
                format!("{error}"),
                &content_type,
                &function_name
            );

            Err(SqliteError::JoinError)
        }
    }
}

async fn execute_update_aris_record_db(
    uid: String,
    request: UpdateEntryRequest,
    attachment_moves: Vec<AttachmentMove>,
) -> Result<(), SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(
            move || -> Result<(), SqliteDatabaseError> {
                with_sql_connection(|connection| {
                    ensure_aris_record_schema(connection)?;
                    let transaction =
                        connection.unchecked_transaction()?;

                    let affected = transaction.execute(
                        r#"
                        UPDATE aris_records
                        SET
                            date = ?2,
                            office = ?3,
                            requestor = ?4,
                            subject = ?5,
                            routed_to_div = ?6,
                            remarks = ?7
                        WHERE uid = ?1
                        "#,
                        params![
                            &uid,
                            &request.date,
                            &request.office,
                            &request.requestor,
                            &request.subject,
                            &request.routed_to_div,
                            &request.remarks,
                        ],
                    )?;

                    if affected == 0 {
                        return Err(
                            rusqlite::Error::QueryReturnedNoRows
                        );
                    }

                    for moved in attachment_moves {
                        transaction.execute(
                            r#"
                            UPDATE aris_attachments
                            SET object_key = ?2
                            WHERE uid = ?1
                            "#,
                            params![
                                moved.attachment_uid,
                                moved.new_key,
                            ],
                        )?;
                    }

                    transaction.commit()?;

                    Ok(())
                })
            },
        )
        .await;

    match database_result {
        Ok(Ok(())) => Ok(()),

        Ok(Err(SqliteDatabaseError::Sqlite(
            rusqlite::Error::QueryReturnedNoRows,
        ))) => Err(SqliteError::NotFound),

        Ok(Err(SqliteDatabaseError::Sqlite(
            rusqlite::Error::SqliteFailure(error, _),
        ))) if error.code
            == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(SqliteError::Conflict)
        }

        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

pub async fn execute_list_aris_records(
    query: ArisListQuery,
    content_type: &str,
    function_name: &str,
) -> Result<ArisPage, SqliteError> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);

    let offset = page
        .saturating_sub(1)
        .saturating_mul(limit);

    let content_type = content_type.to_string();
    let function_name = function_name.to_string();

    let database_result =
        tokio::task::spawn_blocking(
            move || -> Result<ArisPage, SqliteDatabaseError> {
                with_sql_connection(|connection| {
                    ensure_aris_record_schema(connection)?;
                    let (where_clause, base_params) =
                        build_aris_where_clause(&query);

                    let count_sql = format!(
                        r#"
                        SELECT COUNT(*)
                        FROM aris_records e
                        {where_clause}
                        "#
                    );

                    let total: i64 = connection.query_row(
                        &count_sql,
                        params_from_iter(base_params.iter()),
                        |row| row.get(0),
                    )?;

                    let order_clause =
                        build_order_clause(&query);

                    let data_sql = format!(
                        r#"
                        SELECT
                            e.uid,
                            e.control_no,
                            e.date,
                            e.office,
                            e.requestor,
                            e.subject,
                            e.routed_to_div,
                            e.remarks
                        FROM aris_records e
                        {where_clause}
                        {order_clause}
                        LIMIT ? OFFSET ?
                        "#
                    );

                    let mut data_params = base_params.clone();
                    data_params.push(
                        SqlValue::Integer(limit as i64)
                    );
                    data_params.push(
                        SqlValue::Integer(offset as i64)
                    );

                    let mut statement =
                        connection.prepare(&data_sql)?;

                    let mut entries = statement
                        .query_map(
                            params_from_iter(data_params.iter()),
                            |row| {
                                let control_no: i64 =
                                    row.get(1)?;

                                Ok(Entry {
                                    uid: row.get(0)?,
                                    control_no:
                                        control_no.max(0) as u64,
                                    date: row.get(2)?,
                                    office: row.get(3)?,
                                    requestor: row.get(4)?,
                                    subject: row.get(5)?,
                                    routed_to_div: row.get(6)?,
                                    attached_files: Vec::new(),
                                    remarks: row.get(7)?,
                                })
                            },
                        )?
                        .collect::<
                            Result<Vec<_>, rusqlite::Error>
                        >()?;

                    /*
                    Fetch all attachments for the current 256-row page in one
                    SQL query instead of one query per ARIS entry.
                    */
                    if !entries.is_empty() {
                        let placeholders =
                            std::iter::repeat("?")
                                .take(entries.len())
                                .collect::<Vec<_>>()
                                .join(",");

                        let attachment_sql = format!(
                            r#"
                            SELECT
                                entry_uid,
                                uid,
                                file_name,
                                mime_type,
                                size,
                                object_key,
                                version_id
                            FROM aris_attachments
                            WHERE entry_uid IN ({placeholders})
                            ORDER BY entry_uid, rowid ASC
                            "#
                        );

                        let attachment_params =
                            entries
                                .iter()
                                .map(|entry| {
                                    SqlValue::Text(
                                        entry.uid.clone()
                                    )
                                })
                                .collect::<Vec<_>>();

                        let mut attachment_statement =
                            connection.prepare(
                                &attachment_sql
                            )?;

                        let attachment_rows =
                            attachment_statement.query_map(
                                params_from_iter(
                                    attachment_params.iter()
                                ),
                                |row| {
                                    let size: i64 =
                                        row.get(4)?;

                                    Ok((
                                        row.get::<_, String>(0)?,
                                        FileAttachment {
                                            uid: row.get(1)?,
                                            file_name:
                                                row.get(2)?,
                                            mime_type:
                                                row.get(3)?,
                                            size:
                                                size.max(0)
                                                    as u64,
                                            object_key:
                                                row.get(5)?,
                                            version_id:
                                                row.get(6)?,
                                        },
                                    ))
                                },
                            )?;

                        let mut attachment_map:
                            HashMap<
                                String,
                                Vec<FileAttachment>,
                            > = HashMap::new();

                        for row in attachment_rows {
                            let (
                                entry_uid,
                                attachment,
                            ) = row?;

                            attachment_map
                                .entry(entry_uid)
                                .or_default()
                                .push(attachment);
                        }

                        for entry in &mut entries {
                            entry.attached_files =
                                attachment_map
                                    .remove(&entry.uid)
                                    .unwrap_or_default();
                        }
                    }

                    let total = total.max(0) as usize;

                    let total_pages = if total == 0 {
                        1
                    } else {
                        (total + limit - 1) / limit
                    };

                    let has_next =
                        page < total_pages;

                    Ok(ArisPage {
                        data: entries,
                        page,
                        limit,
                        has_next,
                        total,
                        total_pages,
                    })
                })
            },
        )
        .await;

    match database_result {
        Ok(Ok(result)) => Ok(result),

        Ok(Err(error)) => {
            crate::report_error!(
                format!("{error}"),
                &content_type,
                &function_name
            );

            Err(SqliteError::SqliteDatabaseError)
        }

        Err(error) => {
            crate::report_error!(
                format!("{error}"),
                &content_type,
                &function_name
            );

            Err(SqliteError::JoinError)
        }
    }
}

/* -------------------------------------------------------------------------- */
/* SQLite: entry/attachment lookups                                           */
/* -------------------------------------------------------------------------- */

async fn execute_get_stored_entry(
    entry_uid: String,
) -> Result<StoredEntry, SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(
            move || -> Result<StoredEntry, SqliteDatabaseError> {
                with_sql_connection(|connection| {
                    ensure_aris_record_schema(connection)?;
                    let (
                        uid,
                        control_year,
                        control_no,
                        date,
                        office,
                        requestor,
                        subject,
                        routed_to_div,
                        remarks,
                    ): (
                        String,
                        i32,
                        i64,
                        String,
                        String,
                        String,
                        String,
                        String,
                        String,
                    ) = connection.query_row(
                        r#"
                        SELECT
                            uid,
                            control_year,
                            control_no,
                            date,
                            office,
                            requestor,
                            subject,
                            routed_to_div,
                            remarks
                        FROM aris_records
                        WHERE uid = ?1
                        "#,
                        params![entry_uid],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                                row.get(6)?,
                                row.get(7)?,
                                row.get(8)?,
                            ))
                        },
                    )?;

                    let mut statement =
                        connection.prepare(
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

                    let attached_files = statement
                        .query_map(
                            params![uid],
                            |row| {
                                let size: i64 =
                                    row.get(3)?;

                                Ok(FileAttachment {
                                    uid: row.get(0)?,
                                    file_name:
                                        row.get(1)?,
                                    mime_type:
                                        row.get(2)?,
                                    size:
                                        size.max(0) as u64,
                                    object_key:
                                        row.get(4)?,
                                    version_id:
                                        row.get(5)?,
                                })
                            },
                        )?
                        .collect::<
                            Result<Vec<_>, rusqlite::Error>
                        >()?;

                    Ok(StoredEntry {
                        control_year,
                        entry: Entry {
                            uid,
                            control_no:
                                control_no.max(0) as u64,
                            date,
                            office,
                            requestor,
                            subject,
                            routed_to_div,
                            attached_files,
                            remarks,
                        },
                    })
                })
            },
        )
        .await;

    match database_result {
        Ok(Ok(entry)) => Ok(entry),

        Ok(Err(SqliteDatabaseError::Sqlite(
            rusqlite::Error::QueryReturnedNoRows,
        ))) => Err(SqliteError::NotFound),

        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_get_storage_identity(
    entry_uid: String,
) -> Result<StorageIdentity, SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(
            move || -> Result<
                StorageIdentity,
                SqliteDatabaseError,
            > {
                with_sql_connection(|connection| {
                    ensure_aris_record_schema(connection)?;
                    connection
                        .query_row(
                            r#"
                            SELECT
                                control_year,
                                control_no,
                                date,
                                office
                            FROM aris_records
                            WHERE uid = ?1
                            "#,
                            params![entry_uid],
                            |row| {
                                let control_no: i64 =
                                    row.get(1)?;

                                let _control_year: i32 = row.get(0)?;

                                Ok(StorageIdentity {
                                    control_no:
                                        control_no.max(0)
                                            as u64,
                                    date: row.get(2)?,
                                    office: row.get(3)?,
                                })
                            },
                        )
                })
            },
        )
        .await;

    match database_result {
        Ok(Ok(identity)) => Ok(identity),

        Ok(Err(SqliteDatabaseError::Sqlite(
            rusqlite::Error::QueryReturnedNoRows,
        ))) => Err(SqliteError::NotFound),

        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_attachment_name_exists(
    entry_uid: String,
    file_name: String,
) -> Result<bool, SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(
            move || -> Result<bool, SqliteDatabaseError> {
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
                            params![
                                entry_uid,
                                file_name,
                            ],
                            |_row| Ok(true),
                        )
                        .optional()?
                        .unwrap_or(false);

                    Ok(exists)
                })
            },
        )
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
        tokio::task::spawn_blocking(
            move || -> Result<(), SqliteDatabaseError> {
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
            },
        )
        .await;

    match database_result {
        Ok(Ok(())) => Ok(()),

        Ok(Err(SqliteDatabaseError::Sqlite(
            rusqlite::Error::SqliteFailure(error, _),
        ))) if error.code
            == rusqlite::ErrorCode::ConstraintViolation =>
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
        tokio::task::spawn_blocking(
            move || -> Result<
                FileAttachment,
                SqliteDatabaseError,
            > {
                with_sql_connection(|connection| {
                    ensure_aris_record_schema(connection)?;
                    let attachment =
                        connection.query_row(
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
                            params![
                                attachment_uid,
                                entry_uid,
                            ],
                            |row| {
                                let size: i64 =
                                    row.get(3)?;

                                Ok(FileAttachment {
                                    uid: row.get(0)?,
                                    file_name:
                                        row.get(1)?,
                                    mime_type:
                                        row.get(2)?,
                                    size:
                                        size.max(0) as u64,
                                    object_key:
                                        row.get(4)?,
                                    version_id:
                                        row.get(5)?,
                                })
                            },
                        )?;

                    Ok(attachment)
                })
            },
        )
        .await;

    match database_result {
        Ok(Ok(attachment)) => Ok(attachment),

        Ok(Err(SqliteDatabaseError::Sqlite(
            rusqlite::Error::QueryReturnedNoRows,
        ))) => Err(SqliteError::NotFound),

        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_list_attachments_for_entry(
    entry_uid: String,
) -> Result<Vec<FileAttachment>, SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(
            move || -> Result<
                Vec<FileAttachment>,
                SqliteDatabaseError,
            > {
                with_sql_connection(|connection| {
                    ensure_aris_record_schema(connection)?;
                    let mut statement =
                        connection.prepare(
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
                        .query_map(
                            params![entry_uid],
                            |row| {
                                let size: i64 =
                                    row.get(3)?;

                                Ok(FileAttachment {
                                    uid: row.get(0)?,
                                    file_name:
                                        row.get(1)?,
                                    mime_type:
                                        row.get(2)?,
                                    size:
                                        size.max(0) as u64,
                                    object_key:
                                        row.get(4)?,
                                    version_id:
                                        row.get(5)?,
                                })
                            },
                        )?
                        .collect::<
                            Result<Vec<_>, rusqlite::Error>
                        >()?;

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
        tokio::task::spawn_blocking(
            move || -> Result<(), SqliteDatabaseError> {
                with_sql_connection(|connection| {
                    ensure_aris_record_schema(connection)?;
                    let affected = connection.execute(
                        r#"
                        DELETE FROM aris_attachments
                        WHERE uid = ?1
                          AND entry_uid = ?2
                        "#,
                        params![
                            attachment_uid,
                            entry_uid,
                        ],
                    )?;

                    if affected == 0 {
                        return Err(
                            rusqlite::Error::QueryReturnedNoRows
                        );
                    }

                    Ok(())
                })
            },
        )
        .await;

    match database_result {
        Ok(Ok(())) => Ok(()),

        Ok(Err(SqliteDatabaseError::Sqlite(
            rusqlite::Error::QueryReturnedNoRows,
        ))) => Err(SqliteError::NotFound),

        Ok(Err(_)) => Err(SqliteError::SqliteDatabaseError),
        Err(_) => Err(SqliteError::JoinError),
    }
}

async fn execute_delete_entry_metadata(
    entry_uid: String,
) -> Result<(), SqliteError> {
    let database_result =
        tokio::task::spawn_blocking(
            move || -> Result<(), SqliteDatabaseError> {
                with_sql_connection(|connection| {
                    ensure_aris_record_schema(connection)?;
                    let transaction =
                        connection.unchecked_transaction()?;

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
                        return Err(
                            rusqlite::Error::QueryReturnedNoRows
                        );
                    }

                    transaction.commit()?;

                    Ok(())
                })
            },
        )
        .await;

    match database_result {
        Ok(Ok(())) => Ok(()),

        Ok(Err(SqliteDatabaseError::Sqlite(
            rusqlite::Error::QueryReturnedNoRows,
        ))) => Err(SqliteError::NotFound),

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

    if execute_attachment_name_exists(
        entry_uid.clone(),
        file_name.clone(),
    )
    .await?
    {
        return Err(ArisOperationError::InvalidRequest(format!(
            "The entry already has an attachment named '{file_name}'. Remove the old attachment first if you want to replace it."
        )));
    }

    let identity =
        execute_get_storage_identity(entry_uid.clone())
            .await?;

    let attachment_uid =
        Uuid::new_v4().to_string();

    let object_key =
        attachment_object_key(&identity, &file_name);

    let size = bytes.len() as u64;
    let token = n1_access_token().await?;

    n1_prepare_attachment_directory(
        &identity.office,
        &identity.date,
        &token,
    )
    .await?;

    n1_upload_one_shot(
        &object_key,
        &mime_type,
        bytes,
        &token,
    )
    .await?;

    let attachment = FileAttachment {
        uid: attachment_uid,
        file_name,
        mime_type,
        size,
        object_key: object_key.clone(),
        version_id: None,
    };

    if let Err(error) =
        execute_insert_attachment(
            entry_uid,
            attachment.clone(),
        )
        .await
    {
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
    let attachment =
        execute_get_attachment(
            entry_uid.clone(),
            attachment_uid.clone(),
        )
        .await?;

    n1_soft_delete(
        &attachment.object_key
    )
    .await?;

    if let Err(error) =
        execute_delete_attachment_metadata(
            entry_uid,
            attachment_uid,
        )
        .await
    {
        let _ =
            n1_recover(&attachment.object_key).await;

        return Err(ArisOperationError::Sqlite(error));
    }

    Ok(())
}

pub async fn execute_delete_aris_record_with_attachments(
    entry_uid: String,
) -> Result<(), ArisOperationError> {
    // Ensure the entry exists before touching N1.
    execute_get_storage_identity(
        entry_uid.clone()
    )
    .await?;

    let attachments =
        execute_list_attachments_for_entry(
            entry_uid.clone()
        )
        .await?;

    let mut deleted_object_keys: Vec<String> =
        Vec::new();

    for attachment in &attachments {
        match n1_soft_delete(
            &attachment.object_key
        )
        .await
        {
            Ok(()) => {
                deleted_object_keys.push(
                    attachment.object_key.clone()
                );
            }

            Err(error) => {
                for object_key in
                    deleted_object_keys.iter().rev()
                {
                    let _ =
                        n1_recover(object_key).await;
                }

                return Err(error);
            }
        }
    }

    if let Err(error) =
        execute_delete_entry_metadata(entry_uid).await
    {
        for object_key in
            deleted_object_keys.iter().rev()
        {
            let _ =
                n1_recover(object_key).await;
        }

        return Err(
            ArisOperationError::Sqlite(error)
        );
    }

    // The yearly sequence is intentionally NOT decremented.
    // Deleted control numbers are never reused.
    Ok(())
}

/* -------------------------------------------------------------------------- */
/* Update entry + keep N1 console hierarchy synchronized                      */
/* -------------------------------------------------------------------------- */

async fn execute_update_entry_with_n1_moves(
    entry_uid: String,
    request: UpdateEntryRequest,
) -> Result<Entry, ArisOperationError> {
    validate_required_fields(
        &request.date,
        &request.office,
        &request.requestor,
        &request.subject,
        &request.routed_to_div,
    )?;

    let new_year =
        parse_entry_year(&request.date)?;

    let existing =
        execute_get_stored_entry(
            entry_uid.clone()
        )
        .await?;

    /*
    The control number belongs to the year in which it was assigned.

    Changing 2026 -> 2027 would require assigning a brand-new 2027 control
    number and would make an "edit" behave like a new record. We reject that
    operation instead of silently renumbering an existing ARIS record.
    */
    if new_year != existing.control_year {
        return Err(ArisOperationError::InvalidRequest(
            format!(
                "The entry year cannot be changed from {} to {} after control number {} has been assigned. Create a new ARIS entry instead.",
                existing.control_year,
                new_year,
                existing.entry.control_no
            ),
        ));
    }

    let storage_changed =
        request.date != existing.entry.date
            || request.office
                != existing.entry.office;

    let mut moves: Vec<AttachmentMove> =
        Vec::new();

    if storage_changed
        && !existing.entry.attached_files.is_empty()
    {
        let token = n1_access_token().await?;

        n1_prepare_attachment_directory(
            &request.office,
            &request.date,
            &token,
        )
        .await?;

        let new_identity = StorageIdentity {
            control_no:
                existing.entry.control_no,
            date:
                request.date.clone(),
            office:
                request.office.clone(),
        };

        let mut unique_destinations:
            HashSet<String> = HashSet::new();

        for attachment in
            &existing.entry.attached_files
        {
            let new_key = attachment_object_key(
                &new_identity,
                &attachment.file_name,
            );

            if !unique_destinations.insert(
                new_key.clone()
            ) {
                return Err(
                    ArisOperationError::InvalidRequest(
                        "Two attachment names resolve to the same N1 destination path."
                            .to_string(),
                    ),
                );
            }

            moves.push(AttachmentMove {
                attachment_uid:
                    attachment.uid.clone(),
                old_key:
                    attachment.object_key.clone(),
                new_key,
            });
        }

        let mut moved_count = 0usize;

        for moved in &moves {
            if moved.old_key == moved.new_key {
                moved_count += 1;
                continue;
            }

            if let Err(error) =
                n1_rename(
                    &moved.old_key,
                    &moved.new_key,
                    &token,
                )
                .await
            {
                for rollback in
                    moves[..moved_count]
                        .iter()
                        .rev()
                {
                    if rollback.old_key
                        != rollback.new_key
                    {
                        let _ =
                            n1_rename(
                                &rollback.new_key,
                                &rollback.old_key,
                                &token,
                            )
                            .await;
                    }
                }

                return Err(error);
            }

            moved_count += 1;
        }
    }

    if let Err(error) =
        execute_update_aris_record_db(
            entry_uid.clone(),
            request.clone(),
            moves.clone(),
        )
        .await
    {
        if !moves.is_empty() {
            if let Ok(token) =
                n1_access_token().await
            {
                for moved in moves.iter().rev() {
                    if moved.old_key != moved.new_key {
                        let _ =
                            n1_rename(
                                &moved.new_key,
                                &moved.old_key,
                                &token,
                            )
                            .await;
                    }
                }
            }
        }

        return Err(
            ArisOperationError::Sqlite(error)
        );
    }

    let mut result = existing.entry;

    result.date = request.date;
    result.office = request.office;
    result.requestor = request.requestor;
    result.subject = request.subject;
    result.routed_to_div =
        request.routed_to_div;
    result.remarks = request.remarks;

    if !moves.is_empty() {
        let move_map = moves
            .into_iter()
            .map(|moved| {
                (
                    moved.attachment_uid,
                    moved.new_key,
                )
            })
            .collect::<HashMap<_, _>>();

        for attachment in
            &mut result.attached_files
        {
            if let Some(new_key) =
                move_map.get(&attachment.uid)
            {
                attachment.object_key =
                    new_key.clone();
            }
        }
    }

    Ok(result)
}

/* -------------------------------------------------------------------------- */
/* Axum handlers                                                              */
/* -------------------------------------------------------------------------- */

pub async fn create_aris_record(
    claims: Claims,
    Json(request): Json<CreateEntryRequest>,
) -> Response {
    if !claims.can_write_records() {
        return aris_access_denied();
    }

    if let Err(error) =
        validate_required_fields(
            &request.date,
            &request.office,
            &request.requestor,
            &request.subject,
            &request.routed_to_div,
        )
    {
        return map_aris_error(error);
    }

    let year = match parse_entry_year(
        &request.date
    ) {
        Ok(year) => year,
        Err(error) => {
            return map_aris_error(error);
        }
    };

    match execute_create_aris_record(
        request,
        year,
        "api",
        "create_aris_record()",
    )
    .await
    {
        Ok(entry) => {
            api_json(
                StatusCode::CREATED,
                json!(entry),
            )
        }

        Err(error) => {
            map_aris_error(
                ArisOperationError::Sqlite(error)
            )
        }
    }
}

pub async fn update_aris_record(
    claims: Claims,
    Path(uid): Path<String>,
    Json(request): Json<UpdateEntryRequest>,
) -> Response {
    if !claims.can_write_records() {
        return aris_access_denied();
    }

    match execute_update_entry_with_n1_moves(
        uid,
        request,
    )
    .await
    {
        Ok(entry) => {
            api_json(
                StatusCode::OK,
                json!(entry),
            )
        }

        Err(error) => map_aris_error(error),
    }
}

pub async fn list_aris_records(
    claims: Claims,
    Query(query): Query<ArisListQuery>,
) -> Response {
    if !claims.can_read_records() {
        return aris_access_denied();
    }

    match execute_list_aris_records(
        query,
        "api",
        "list_aris_records()",
    )
    .await
    {
        Ok(result) => {
            api_json(
                StatusCode::OK,
                json!(result),
            )
        }

        Err(error) => {
            map_aris_error(
                ArisOperationError::Sqlite(error)
            )
        }
    }
}

pub async fn delete_aris_record(
    claims: Claims,
    Path(uid): Path<String>,
) -> Response {
    if !claims.can_delete_records() {
        return aris_access_denied();
    }

    match execute_delete_aris_record_with_attachments(
        uid
    )
    .await
    {
        Ok(()) => {
            StatusCode::NO_CONTENT.into_response()
        }

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

    let mut uploaded:
        Vec<FileAttachment> = Vec::new();

    loop {
        let field =
            match multipart.next_field().await {
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
            .unwrap_or_else(|| {
                "attachment.bin".to_string()
            });

        let mime_type = field
            .content_type()
            .map(str::to_string)
            .unwrap_or_else(|| {
                "application/octet-stream"
                    .to_string()
            });

        let bytes =
            match field.bytes().await {
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

        match execute_upload_aris_attachment(
            entry_uid.clone(),
            file_name,
            mime_type,
            bytes,
        )
        .await
        {
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
    Path((entry_uid, attachment_uid)):
        Path<(String, String)>,
) -> Response {
    if !claims.can_delete_records() {
        return aris_access_denied();
    }

    match execute_delete_aris_attachment(
        entry_uid,
        attachment_uid,
    )
    .await
    {
        Ok(()) => {
            StatusCode::NO_CONTENT.into_response()
        }

        Err(error) => map_aris_error(error),
    }
}

pub async fn preview_aris_attachment(
    claims: Claims,
    Path((entry_uid, attachment_uid)):
        Path<(String, String)>,
) -> Response {
    if !claims.can_read_records() {
        return aris_access_denied();
    }

    let attachment =
        match execute_get_attachment(
            entry_uid,
            attachment_uid,
        )
        .await
        {
            Ok(attachment) => attachment,

            Err(error) => {
                return map_aris_error(
                    ArisOperationError::Sqlite(error)
                );
            }
        };

    let original_bytes =
        match n1_download(
            &attachment.object_key,
            &attachment.file_name,
        )
        .await
        {
            Ok(bytes) => bytes,
            Err(error) => {
                return map_aris_error(error);
            }
        };

    if office_preview_supported(
        &attachment.file_name
    ) {
        let attachment_for_preview =
            attachment.clone();

        let preview_result =
            tokio::task::spawn_blocking(move || {
                generate_office_pdf_preview(
                    attachment_for_preview,
                    original_bytes,
                )
            })
            .await;

        let pdf_bytes = match preview_result {
            Ok(Ok(bytes)) => bytes,

            Ok(Err(error)) => {
                crate::report_error!(
                    error,
                    "preview",
                    "preview_aris_attachment()"
                );

                return api_json(
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({
                        "response": "ARIS could not generate the Office preview. Verify that LibreOffice Writer, Calc, and Impress are installed on the ARIS server."
                    }),
                );
            }

            Err(error) => {
                crate::report_error!(
                    format!(
                        "Office preview blocking task failed: {error}"
                    ),
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
            .map(|(stem, _)| {
                format!("{stem}.pdf")
            })
            .unwrap_or_else(|| {
                format!(
                    "{}.pdf",
                    attachment.file_name
                )
            });

        return inline_attachment_response(
            pdf_bytes,
            "application/pdf",
            &preview_name,
        );
    }

    inline_attachment_response(
        original_bytes,
        &attachment.mime_type,
        &attachment.file_name,
    )
}

pub async fn download_aris_attachment(
    claims: Claims,
    Path((entry_uid, attachment_uid)):
        Path<(String, String)>,
) -> Response {
    if !claims.can_read_records() {
        return aris_access_denied();
    }

    let attachment =
        match execute_get_attachment(
            entry_uid,
            attachment_uid,
        )
        .await
        {
            Ok(attachment) => attachment,

            Err(error) => {
                return map_aris_error(
                    ArisOperationError::Sqlite(error)
                );
            }
        };

    let bytes =
        match n1_download(
            &attachment.object_key,
            &attachment.file_name,
        )
        .await
        {
            Ok(bytes) => bytes,
            Err(error) => {
                return map_aris_error(error);
            }
        };

    let safe_download_name =
        attachment.file_name.replace('"', "_");

    let mut response =
        Response::new(Body::from(bytes));

    *response.status_mut() =
        StatusCode::OK;

    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(
            &attachment.mime_type
        )
        .unwrap_or_else(|_| {
            HeaderValue::from_static(
                "application/octet-stream"
            )
        }),
    );

    if let Ok(value) =
        HeaderValue::from_str(
            &format!(
                "attachment; filename=\"{safe_download_name}\""
            )
        )
    {
        response.headers_mut().insert(
            CONTENT_DISPOSITION,
            value,
        );
    }

    response
}
