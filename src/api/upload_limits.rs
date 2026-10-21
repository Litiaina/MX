//! One upload policy for the browser, request-body guard and file reader.
//! Overrides live in SQLite; mx.config remains the startup fallback.
use axum::{
    Extension, Json,
    body::Body,
    extract::{Request, multipart::Field, rejection::JsonRejection},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use super::{
    deployment::{UploadSettings, read_config},
    live::publish_live_event,
};
use crate::{
    config::load_config::CONFIG, db::connector::with_sql_connection, middleware::auth::Claims,
};

const MIB: usize = 1024 * 1024;

#[derive(Clone, Copy)]
pub struct UploadLimit {
    pub bytes: usize,
    pub name: &'static str,
}

impl UploadLimit {
    pub fn rejection(self) -> Response {
        (StatusCode::PAYLOAD_TOO_LARGE, Json(json!({"response":format!(
            "This upload exceeds the current {} limit of {} MiB. An administrator can change it in Administration → Deployment → Upload limits. For a larger collection, send one file per request or use MX Drive.", self.name, self.bytes / MIB
        ), "max_size_bytes":self.bytes}))).into_response()
    }
}

fn valid_mb(value: usize) -> bool {
    value
        .checked_mul(MIB)
        .is_some_and(|bytes| (bytes as u128) <= 9_007_199_254_740_991)
}

fn validate(settings: &UploadSettings) -> Result<(), &'static str> {
    for value in [
        settings.record_max_size_mb,
        settings.collaboration_max_size_mb,
    ]
    .into_iter()
    .flatten()
    {
        if !valid_mb(value) {
            return Err(
                "Use a positive whole MiB limit, 0 for No file-size limit, or Server default to remove an override.",
            );
        }
    }
    Ok(())
}

pub(crate) fn effective(
    settings: &UploadSettings,
    record_default: usize,
    chat_default: usize,
) -> (usize, usize) {
    (
        settings
            .record_max_size_mb
            .unwrap_or(record_default)
            .saturating_mul(MIB),
        settings
            .collaboration_max_size_mb
            .unwrap_or(chat_default)
            .saturating_mul(MIB),
    )
}

pub(crate) fn limits_db(connection: &Connection) -> rusqlite::Result<(usize, usize)> {
    let (_, config, _) = read_config(connection)?;
    Ok(effective(
        &config.uploads,
        CONFIG.n1.attachment_max_size_mb,
        CONFIG.n1.collaboration_file_max_size_mb,
    ))
}

fn snapshot(connection: &Connection) -> rusqlite::Result<Value> {
    let (revision, config, _) = read_config(connection)?;
    let (record, chat) = effective(
        &config.uploads,
        CONFIG.n1.attachment_max_size_mb,
        CONFIG.n1.collaboration_file_max_size_mb,
    );
    Ok(json!({"revision":revision,"settings":config.uploads,
        "defaults":{"record_max_size_mb":CONFIG.n1.attachment_max_size_mb,"collaboration_max_size_mb":CONFIG.n1.collaboration_file_max_size_mb},
        "record_max_size_bytes":record,"collaboration_max_size_bytes":chat}))
}

pub async fn get_limits(_claims: Claims) -> Response {
    match tokio::task::spawn_blocking(|| with_sql_connection(snapshot)).await {
        Ok(Ok(value)) => ([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response(),
        _ => unavailable(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveLimits {
    revision: i64,
    settings: UploadSettings,
}

pub async fn save_limits(
    claims: Claims,
    payload: Result<Json<SaveLimits>, JsonRejection>,
) -> Response {
    if !claims.can_manage_accounts() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"response":"Only an administrator can change upload limits."})),
        )
            .into_response();
    }
    let request = match payload {
        Ok(Json(request)) => request,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({"response":"Enter positive whole MiB values, 0 for No file-size limit, or null for Server default, together with the loaded settings revision."}))).into_response(),
    };
    if let Err(message) = validate(&request.settings) {
        return (StatusCode::BAD_REQUEST, Json(json!({"response":message}))).into_response();
    }
    let result = tokio::task::spawn_blocking(move || with_sql_connection(|connection| {
        // Acquire the writer before reading; never upgrade a deferred read transaction.
        let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
        let (revision, mut config, _) = read_config(&transaction)?;
        if revision != request.revision { return Ok(None); }
        config.uploads = request.settings;
        transaction.execute("UPDATE mx_deployment_config SET config_json=?1, revision=revision+1, updated_at=?2 WHERE id=1",
            params![serde_json::to_string(&config).unwrap(), chrono::Utc::now().timestamp_millis()])?;
        let value = snapshot(&transaction)?;
        transaction.commit()?;
        Ok(Some(value))
    })).await;
    match result {
        Ok(Ok(Some(value))) => {
            publish_live_event("deployment.updated",Some(&claims.uid),json!({"revision":value["revision"]}));
            (StatusCode::OK,Json(value)).into_response()
        }
        Ok(Ok(None)) => (StatusCode::CONFLICT,Json(json!({"response":"Deployment settings changed in another session. Refresh the current limits, review your entries and save again. Nothing was overwritten."}))).into_response(),
        _ => unavailable(),
    }
}

fn unavailable() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE,Json(json!({"response":"MX could not load the upload policy. Retry when connected; no file was accepted."}))).into_response()
}

pub async fn record_body(request: Request, next: Next) -> Response {
    guard(request, next, true).await
}
pub async fn chat_body(request: Request, next: Next) -> Response {
    guard(request, next, false).await
}
async fn guard(mut request: Request, next: Next, record: bool) -> Response {
    let limits = match tokio::task::spawn_blocking(|| with_sql_connection(limits_db)).await {
        Ok(Ok(limits)) => limits,
        _ => return unavailable(),
    };
    let limit = UploadLimit {
        bytes: if record { limits.0 } else { limits.1 },
        name: if record {
            "record attachment"
        } else {
            "Collaboration file"
        },
    };
    request.extensions_mut().insert(limit);
    if limit.bytes == 0 {
        return next.run(request).await;
    }
    let body_limit = limit.bytes.saturating_add(MIB); // multipart headers, not additional file allowance
    if request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size > body_limit as u64)
    {
        return limit.rejection();
    }
    let (parts, body) = request.into_parts();
    let response = next
        .run(Request::from_parts(
            parts,
            Body::new(http_body_util::Limited::new(body, body_limit)),
        ))
        .await;
    if response.status() == StatusCode::PAYLOAD_TOO_LARGE {
        limit.rejection()
    } else {
        response
    }
}

/// A private spool, not a whole-file RAM buffer. On Unix the open file is
/// immediately unlinked, so cancellation and process death cannot leave it behind.
#[derive(Debug)]
pub struct UploadFile {
    pub file: tokio::fs::File,
    size: usize,
    path: Option<std::path::PathBuf>,
}

impl UploadFile {
    pub fn len(&self) -> usize {
        self.size
    }
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn create() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("mx-upload-{}", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x04000000); // FILE_FLAG_DELETE_ON_CLOSE
        }
        let file = options.open(&path)?;
        let retained = match std::fs::remove_file(&path) {
            Ok(()) => None,
            Err(_) => Some(path),
        };
        Ok(Self {
            file: tokio::fs::File::from_std(file),
            size: 0,
            path: retained,
        })
    }
}

impl Drop for UploadFile {
    fn drop(&mut self) {
        // Best-effort fallback; Windows additionally uses delete-on-close.
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn spool_error(_: std::io::Error) -> Response {
    (StatusCode::INSUFFICIENT_STORAGE, Json(json!({"response":"MX could not stage this upload on its temporary disk. Check free space and permissions for the MX service's TMPDIR, then retry. The file was not saved."}))).into_response()
}

/// Bound RAM independently of administrator policy; stop oversized files before
/// N1 publication. Temporary disk capacity remains necessary even with no cap.
pub async fn read_file(
    mut field: Field<'_>,
    Extension(limit): Extension<UploadLimit>,
) -> Result<UploadFile, Response> {
    let mut upload = UploadFile::create().map_err(spool_error)?;
    loop {
        let chunk = field.chunk().await.map_err(|error| {
            if error.status() == StatusCode::PAYLOAD_TOO_LARGE {limit.rejection()}
            else {(StatusCode::BAD_REQUEST,Json(json!({"response":"The file transfer was incomplete. Your original file was not changed; retry the upload."}))).into_response()}
        })?;
        let Some(chunk) = chunk else {
            upload.file.flush().await.map_err(spool_error)?;
            upload.file.rewind().await.map_err(spool_error)?;
            return Ok(upload);
        };
        if limit.bytes > 0 && chunk.len() > limit.bytes.saturating_sub(upload.size) {
            return Err(limit.rejection());
        }
        upload.size = upload
            .size
            .checked_add(chunk.len())
            .filter(|size| *size as u128 <= i64::MAX as u128)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "File size cannot be represented by this server.",
                )
                    .into_response()
            })?;
        upload.file.write_all(&chunk).await.map_err(spool_error)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_defaults_and_independent_overrides_have_one_byte_contract() {
        let mut settings: UploadSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(effective(&settings, 100, 200), (100 * MIB, 200 * MIB));
        settings.record_max_size_mb = Some(80);
        assert_eq!(effective(&settings, 50, 100), (80 * MIB, 100 * MIB));
        settings.collaboration_max_size_mb = Some(120);
        assert_eq!(effective(&settings, 50, 100), (80 * MIB, 120 * MIB));
        settings.record_max_size_mb = None;
        assert_eq!(effective(&settings, 50, 100), (50 * MIB, 120 * MIB));
    }
    #[test]
    fn overrides_accept_unlimited_but_reject_overflow_and_non_integer_values() {
        for value in [usize::MAX] {
            assert!(
                validate(&UploadSettings {
                    record_max_size_mb: Some(value),
                    ..Default::default()
                })
                .is_err()
            );
        }
        assert!(
            validate(&UploadSettings {
                record_max_size_mb: Some(100),
                ..Default::default()
            })
            .is_ok()
        );
        let unlimited = UploadSettings {
            record_max_size_mb: Some(0),
            collaboration_max_size_mb: Some(0),
        };
        assert!(validate(&unlimited).is_ok());
        assert_eq!(effective(&unlimited, 50, 100), (0, 0));
        assert_eq!(effective(&UploadSettings::default(), 0, 0), (0, 0));
        for input in [
            r#"{"record_max_size_mb":1.5}"#,
            r#"{"record_max_size_mb":-1}"#,
            r#"{"record_max_size_mb":"100"}"#,
        ] {
            assert!(serde_json::from_str::<UploadSettings>(input).is_err());
        }
    }
    #[tokio::test]
    async fn streamed_file_guard_accepts_exact_limit_and_rejects_the_next_byte() {
        use axum::extract::{FromRequest, Multipart};
        for (cap, count, accepted) in [(4, 0, true), (4, 4, true), (4, 5, false), (0, 1024, true)] {
            let data = format!(
                "--boundary\r\nContent-Disposition: form-data; name=\"files\"; filename=\"test.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n{}\r\n--boundary--\r\n",
                "x".repeat(count)
            );
            let request = Request::builder()
                .method("POST")
                .header(
                    header::CONTENT_TYPE,
                    "multipart/form-data; boundary=boundary",
                )
                .body(Body::from(data))
                .unwrap();
            let mut multipart = Multipart::from_request(request, &()).await.unwrap();
            let field = multipart.next_field().await.unwrap().unwrap();
            let result = read_file(
                field,
                Extension(UploadLimit {
                    bytes: cap,
                    name: "record attachment",
                }),
            )
            .await;
            if accepted {
                use tokio::io::AsyncReadExt;
                let mut file = result.unwrap();
                assert_eq!(file.len(), count);
                #[cfg(unix)]
                assert!(
                    file.path.is_none(),
                    "Spools are anonymous and cannot survive process exit"
                );
                let mut bytes = Vec::new();
                file.file.read_to_end(&mut bytes).await.unwrap();
                assert_eq!(
                    bytes,
                    vec![b'x'; count],
                    "Spool is rewound with original bytes"
                );
            } else {
                assert_eq!(result.unwrap_err().status(), StatusCode::PAYLOAD_TOO_LARGE);
            }
        }
    }
}
