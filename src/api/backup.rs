use std::{
    fs,
    path::{Path as FsPath, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use axum::{
    body::Body,
    extract::Path,
    http::{
        header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
    Json,
};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    api::aris::handler::{
        n1_access_token, n1_download, n1_ensure_directory,
        n1_soft_delete, n1_upload_one_shot,
    },
    db::connector::{with_sql_connection, SqliteDatabaseError},
    middleware::auth::Claims,
};

static BACKUP_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
pub struct BackupEntry {
    pub uid: String,
    pub file_name: String,
    pub object_key: String,
    pub size: u64,
    pub status: String,
    pub integrity_check: String,
    pub created_at: i64,
    pub verified_at: Option<i64>,
}

struct BackupRunGuard;

impl BackupRunGuard {
    fn acquire() -> Result<Self, ()> {
        BACKUP_RUNNING
            .compare_exchange(
                false,
                true,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| Self)
            .map_err(|_| ())
    }
}

impl Drop for BackupRunGuard {
    fn drop(&mut self) {
        BACKUP_RUNNING.store(false, Ordering::Release);
    }
}

fn backup_access_denied() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "response": "Administrator access is required for ARIS backups."
        })),
    )
        .into_response()
}

fn backup_error(
    status: StatusCode,
    message: impl Into<String>,
) -> Response {
    (
        status,
        Json(json!({
            "response": message.into()
        })),
    )
        .into_response()
}

fn ensure_backup_schema(
    connection: &rusqlite::Connection,
) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS aris_backups (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            uid              TEXT NOT NULL UNIQUE,
            file_name        TEXT NOT NULL,
            object_key       TEXT NOT NULL UNIQUE,
            size             INTEGER NOT NULL CHECK(size >= 0),
            status           TEXT NOT NULL,
            integrity_check  TEXT NOT NULL,
            created_at       INTEGER NOT NULL,
            verified_at      INTEGER
        );

        CREATE INDEX IF NOT EXISTS
            idx_aris_backups_created_at
        ON aris_backups (
            created_at DESC
        );
        "#,
    )
}

fn backup_temp_directory() -> Result<PathBuf, String> {
    let path = std::env::temp_dir()
        .join("aris-database-backups");

    fs::create_dir_all(&path)
        .map_err(|error| {
            format!(
                "failed to create ARIS backup temporary directory '{}': {error}",
                path.display()
            )
        })?;

    Ok(path)
}

fn sqlite_literal(value: &str) -> String {
    value.replace('\'', "''")
}

async fn create_sqlite_snapshot(
    destination: PathBuf,
) -> Result<(), String> {
    tokio::task::spawn_blocking(
        move || -> Result<(), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_backup_schema(connection)?;

                let destination =
                    destination.to_string_lossy();

                let sql = format!(
                    "VACUUM INTO '{}';",
                    sqlite_literal(&destination),
                );

                connection.execute_batch(&sql)?;

                Ok(())
            })
        },
    )
    .await
    .map_err(|error| {
        format!(
            "ARIS backup snapshot task failed: {error}"
        )
    })?
    .map_err(|error| {
        format!(
            "SQLite could not create the ARIS backup snapshot: {error}"
        )
    })
}

fn verify_sqlite_file(
    path: &FsPath,
) -> Result<String, String> {
    let connection =
        rusqlite::Connection::open(path)
            .map_err(|error| {
                format!(
                    "failed to open backup snapshot '{}': {error}",
                    path.display()
                )
            })?;

    let mut statement =
        connection
            .prepare("PRAGMA integrity_check;")
            .map_err(|error| {
                format!(
                    "failed to prepare backup integrity check: {error}"
                )
            })?;

    let mut rows =
        statement
            .query([])
            .map_err(|error| {
                format!(
                    "failed to execute backup integrity check: {error}"
                )
            })?;

    let mut messages = Vec::new();

    while let Some(row) =
        rows
            .next()
            .map_err(|error| {
                format!(
                    "failed to read backup integrity result: {error}"
                )
            })?
    {
        messages.push(
            row
                .get::<_, String>(0)
                .map_err(|error| {
                    format!(
                        "failed to decode backup integrity result: {error}"
                    )
                })?
        );
    }

    let result = messages.join("; ");

    if result.eq_ignore_ascii_case("ok") {
        Ok("ok".to_string())
    } else {
        Err(
            if result.is_empty() {
                "SQLite integrity_check returned no result."
                    .to_string()
            } else {
                format!(
                    "SQLite integrity_check failed: {result}"
                )
            }
        )
    }
}

async fn verify_sqlite_file_async(
    path: PathBuf,
) -> Result<String, String> {
    tokio::task::spawn_blocking(
        move || verify_sqlite_file(&path),
    )
    .await
    .map_err(|error| {
        format!(
            "ARIS backup verification task failed: {error}"
        )
    })?
}

async fn read_backup_bytes(
    path: PathBuf,
) -> Result<Vec<u8>, String> {
    tokio::task::spawn_blocking(
        move || fs::read(&path),
    )
    .await
    .map_err(|error| {
        format!(
            "ARIS backup read task failed: {error}"
        )
    })?
    .map_err(|error| {
        format!(
            "failed to read ARIS backup snapshot: {error}"
        )
    })
}

fn backup_row(
    row: &rusqlite::Row<'_>,
) -> Result<BackupEntry, rusqlite::Error> {
    let size: i64 = row.get("size")?;

    Ok(BackupEntry {
        uid: row.get("uid")?,
        file_name: row.get("file_name")?,
        object_key: row.get("object_key")?,
        size: size.max(0) as u64,
        status: row.get("status")?,
        integrity_check:
            row.get("integrity_check")?,
        created_at: row.get("created_at")?,
        verified_at: row.get("verified_at")?,
    })
}

async fn insert_backup_entry(
    entry: BackupEntry,
) -> Result<(), String> {
    tokio::task::spawn_blocking(
        move || -> Result<(), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_backup_schema(connection)?;

                connection.execute(
                    r#"
                    INSERT INTO aris_backups (
                        uid,
                        file_name,
                        object_key,
                        size,
                        status,
                        integrity_check,
                        created_at,
                        verified_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4,
                        ?5, ?6, ?7, ?8
                    )
                    "#,
                    params![
                        entry.uid,
                        entry.file_name,
                        entry.object_key,
                        entry.size as i64,
                        entry.status,
                        entry.integrity_check,
                        entry.created_at,
                        entry.verified_at,
                    ],
                )?;

                Ok(())
            })
        },
    )
    .await
    .map_err(|error| {
        format!(
            "ARIS backup metadata task failed: {error}"
        )
    })?
    .map_err(|error| {
        format!(
            "failed to store ARIS backup metadata: {error}"
        )
    })
}

async fn get_backup_entry(
    uid: String,
) -> Result<Option<BackupEntry>, String> {
    tokio::task::spawn_blocking(
        move || -> Result<
            Option<BackupEntry>,
            SqliteDatabaseError,
        > {
            with_sql_connection(|connection| {
                ensure_backup_schema(connection)?;

                connection
                    .query_row(
                        r#"
                        SELECT
                            uid,
                            file_name,
                            object_key,
                            size,
                            status,
                            integrity_check,
                            created_at,
                            verified_at
                        FROM aris_backups
                        WHERE uid = ?1
                        LIMIT 1
                        "#,
                        params![uid],
                        backup_row,
                    )
                    .optional()
            })
        },
    )
    .await
    .map_err(|error| {
        format!(
            "ARIS backup lookup task failed: {error}"
        )
    })?
    .map_err(|error| {
        format!(
            "failed to read ARIS backup metadata: {error}"
        )
    })
}

async fn update_backup_verification(
    uid: String,
    status: String,
    integrity_check: String,
    verified_at: i64,
) -> Result<(), String> {
    tokio::task::spawn_blocking(
        move || -> Result<(), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_backup_schema(connection)?;

                connection.execute(
                    r#"
                    UPDATE aris_backups
                    SET
                        status = ?1,
                        integrity_check = ?2,
                        verified_at = ?3
                    WHERE uid = ?4
                    "#,
                    params![
                        status,
                        integrity_check,
                        verified_at,
                        uid,
                    ],
                )?;

                Ok(())
            })
        },
    )
    .await
    .map_err(|error| {
        format!(
            "ARIS backup verification metadata task failed: {error}"
        )
    })?
    .map_err(|error| {
        format!(
            "failed to update ARIS backup verification metadata: {error}"
        )
    })
}

async fn prepare_backup_namespace(
    token: &str,
    year: &str,
    month: &str,
    day: &str,
) -> Result<(), String> {
    for path in [
        "__aris",
        "__aris/backups",
        "__aris/backups/database",
        &format!("__aris/backups/database/{year}"),
        &format!("__aris/backups/database/{year}/{month}"),
        &format!(
            "__aris/backups/database/{year}/{month}/{day}"
        ),
    ] {
        n1_ensure_directory(
            path,
            token,
        )
        .await
        .map_err(|error| {
            format!(
                "N1 could not prepare backup directory '{path}': {error:?}"
            )
        })?;
    }

    Ok(())
}

pub async fn list_backups(
    claims: Claims,
) -> Response {
    if !claims.can_manage_accounts() {
        return backup_access_denied();
    }

    let result =
        tokio::task::spawn_blocking(
            move || -> Result<
                Vec<BackupEntry>,
                SqliteDatabaseError,
            > {
                with_sql_connection(|connection| {
                    ensure_backup_schema(
                        connection
                    )?;

                    let mut statement =
                        connection.prepare(
                            r#"
                            SELECT
                                uid,
                                file_name,
                                object_key,
                                size,
                                status,
                                integrity_check,
                                created_at,
                                verified_at
                            FROM aris_backups
                            ORDER BY created_at DESC
                            LIMIT 200
                            "#,
                        )?;

                    let rows =
                        statement.query_map(
                            [],
                            backup_row,
                        )?;

                    let mut backups =
                        Vec::new();

                    for row in rows {
                        backups.push(row?);
                    }

                    Ok(backups)
                })
            },
        )
        .await;

    match result {
        Ok(Ok(backups)) => (
            StatusCode::OK,
            Json(json!({
                "backups": backups,
                "backup_running":
                    BACKUP_RUNNING.load(
                        Ordering::Acquire
                    )
            })),
        )
            .into_response(),

        Ok(Err(error)) => {
            crate::report_error!(
                format!("{error}"),
                "backup",
                "list_backups()"
            );

            backup_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ARIS could not read backup history.",
            )
        }

        Err(error) => {
            crate::report_error!(
                format!(
                    "backup list blocking task failed: {error}"
                ),
                "backup",
                "list_backups()"
            );

            backup_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ARIS backup history task failed.",
            )
        }
    }
}

pub async fn create_backup(
    claims: Claims,
) -> Response {
    if !claims.can_manage_accounts() {
        return backup_access_denied();
    }

    let _guard =
        match BackupRunGuard::acquire() {
            Ok(guard) => guard,

            Err(()) => {
                return backup_error(
                    StatusCode::CONFLICT,
                    "An ARIS database backup is already running.",
                );
            }
        };

    let now = chrono::Utc::now();
    let uid = Uuid::new_v4().to_string();

    let file_name = format!(
        "aris-{}.db",
        now.format("%Y%m%dT%H%M%SZ")
    );

    let year =
        now.format("%Y").to_string();
    let month =
        now.format("%m").to_string();
    let day =
        now.format("%d").to_string();

    let object_key = format!(
        "__aris/backups/database/{year}/{month}/{day}/{file_name}"
    );

    let temp_directory =
        match backup_temp_directory() {
            Ok(path) => path,

            Err(error) => {
                return backup_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error,
                );
            }
        };

    let temp_path =
        temp_directory.join(
            format!(
                "{}-{}",
                uid,
                file_name
            )
        );

    if let Err(error) =
        create_sqlite_snapshot(
            temp_path.clone()
        )
        .await
    {
        crate::report_error!(
            error.clone(),
            "backup",
            "create_backup()"
        );

        let _ =
            fs::remove_file(
                &temp_path
            );

        return backup_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            error,
        );
    }

    let integrity_check =
        match verify_sqlite_file_async(
            temp_path.clone()
        )
        .await
        {
            Ok(result) => result,

            Err(error) => {
                crate::report_error!(
                    error.clone(),
                    "backup",
                    "create_backup()"
                );

                let _ =
                    fs::remove_file(
                        &temp_path
                    );

                return backup_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error,
                );
            }
        };

    let size =
        match fs::metadata(
            &temp_path
        ) {
            Ok(metadata) =>
                metadata.len(),

            Err(error) => {
                let _ =
                    fs::remove_file(
                        &temp_path
                    );

                return backup_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!(
                        "failed to read ARIS backup size: {error}"
                    ),
                );
            }
        };

    let bytes =
        match read_backup_bytes(
            temp_path.clone()
        )
        .await
        {
            Ok(bytes) => bytes,

            Err(error) => {
                let _ =
                    fs::remove_file(
                        &temp_path
                    );

                return backup_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error,
                );
            }
        };

    let token =
        match n1_access_token()
            .await
        {
            Ok(token) => token,

            Err(error) => {
                let _ =
                    fs::remove_file(
                        &temp_path
                    );

                crate::report_error!(
                    format!("{error:?}"),
                    "backup",
                    "create_backup()"
                );

                return backup_error(
                    StatusCode::BAD_GATEWAY,
                    "ARIS created a local backup snapshot, but N1 authentication failed. The temporary snapshot was removed.",
                );
            }
        };

    if let Err(error) =
        prepare_backup_namespace(
            &token,
            &year,
            &month,
            &day,
        )
        .await
    {
        let _ =
            fs::remove_file(
                &temp_path
            );

        crate::report_error!(
            error.clone(),
            "backup",
            "create_backup()"
        );

        return backup_error(
            StatusCode::BAD_GATEWAY,
            error,
        );
    }

    if let Err(error) =
        n1_upload_one_shot(
            &object_key,
            "application/vnd.sqlite3",
            bytes,
            &token,
        )
        .await
    {
        let _ =
            fs::remove_file(
                &temp_path
            );

        crate::report_error!(
            format!("{error:?}"),
            "backup",
            "create_backup()"
        );

        return backup_error(
            StatusCode::BAD_GATEWAY,
            "ARIS verified the SQLite snapshot, but N1 could not store the backup.",
        );
    }

    let created_at =
        now.timestamp_millis();

    let entry =
        BackupEntry {
            uid: uid.clone(),
            file_name:
                file_name.clone(),
            object_key:
                object_key.clone(),
            size,
            status:
                "verified".to_string(),
            integrity_check,
            created_at,
            verified_at:
                Some(created_at),
        };

    if let Err(error) =
        insert_backup_entry(
            entry.clone()
        )
        .await
    {
        /*
         * N1 accepted the backup but ARIS could not commit its metadata.
         * Soft-delete the N1 object so the backup system does not
         * intentionally leave an untracked database snapshot.
         */
        let _ =
            n1_soft_delete(
                &object_key
            )
            .await;

        let _ =
            fs::remove_file(
                &temp_path
            );

        crate::report_error!(
            error.clone(),
            "backup",
            "create_backup()"
        );

        return backup_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            error,
        );
    }

    let _ =
        fs::remove_file(
            &temp_path
        );

    (
        StatusCode::CREATED,
        Json(json!({
            "response":
                "ARIS database backup created and verified.",
            "backup":
                entry
        })),
    )
        .into_response()
}

pub async fn verify_backup(
    claims: Claims,
    Path(uid): Path<String>,
) -> Response {
    if !claims.can_manage_accounts() {
        return backup_access_denied();
    }

    let backup =
        match get_backup_entry(
            uid.clone()
        )
        .await
        {
            Ok(Some(backup)) =>
                backup,

            Ok(None) => {
                return backup_error(
                    StatusCode::NOT_FOUND,
                    "ARIS backup was not found.",
                );
            }

            Err(error) => {
                crate::report_error!(
                    error.clone(),
                    "backup",
                    "verify_backup()"
                );

                return backup_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error,
                );
            }
        };

    let bytes =
        match n1_download(
            &backup.object_key,
            &backup.file_name,
        )
        .await
        {
            Ok(bytes) => bytes,

            Err(error) => {
                crate::report_error!(
                    format!("{error:?}"),
                    "backup",
                    "verify_backup()"
                );

                return backup_error(
                    StatusCode::BAD_GATEWAY,
                    "N1 could not return the selected ARIS backup for verification.",
                );
            }
        };

    if bytes.len() as u64
        != backup.size
    {
        let message = format!(
            "Backup size mismatch: expected {} bytes, received {} bytes.",
            backup.size,
            bytes.len()
        );

        let _ =
            update_backup_verification(
                uid,
                "failed".to_string(),
                message.clone(),
                chrono::Utc::now()
                    .timestamp_millis(),
            )
            .await;

        return backup_error(
            StatusCode::CONFLICT,
            message,
        );
    }

    let temp_directory =
        match backup_temp_directory() {
            Ok(path) => path,

            Err(error) => {
                return backup_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error,
                );
            }
        };

    let temp_path =
        temp_directory.join(
            format!(
                "verify-{}-{}",
                backup.uid,
                backup.file_name
            )
        );

    if let Err(error) =
        fs::write(
            &temp_path,
            &bytes,
        )
    {
        return backup_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "failed to stage ARIS backup verification: {error}"
            ),
        );
    }

    let verification =
        verify_sqlite_file_async(
            temp_path.clone()
        )
        .await;

    let _ =
        fs::remove_file(
            &temp_path
        );

    let verified_at =
        chrono::Utc::now()
            .timestamp_millis();

    match verification {
        Ok(integrity_check) => {
            if let Err(error) =
                update_backup_verification(
                    backup.uid.clone(),
                    "verified".to_string(),
                    integrity_check.clone(),
                    verified_at,
                )
                .await
            {
                return backup_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error,
                );
            }

            (
                StatusCode::OK,
                Json(json!({
                    "response":
                        "ARIS backup passed SQLite integrity verification.",
                    "backup_uid":
                        backup.uid,
                    "integrity_check":
                        integrity_check,
                    "verified_at":
                        verified_at
                })),
            )
                .into_response()
        }

        Err(error) => {
            let _ =
                update_backup_verification(
                    backup.uid,
                    "failed".to_string(),
                    error.clone(),
                    verified_at,
                )
                .await;

            crate::report_error!(
                error.clone(),
                "backup",
                "verify_backup()"
            );

            backup_error(
                StatusCode::CONFLICT,
                error,
            )
        }
    }
}

pub async fn download_backup(
    claims: Claims,
    Path(uid): Path<String>,
) -> Response {
    if !claims.can_manage_accounts() {
        return backup_access_denied();
    }

    let backup =
        match get_backup_entry(uid)
            .await
        {
            Ok(Some(backup)) =>
                backup,

            Ok(None) => {
                return backup_error(
                    StatusCode::NOT_FOUND,
                    "ARIS backup was not found.",
                );
            }

            Err(error) => {
                return backup_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error,
                );
            }
        };

    let bytes =
        match n1_download(
            &backup.object_key,
            &backup.file_name,
        )
        .await
        {
            Ok(bytes) => bytes,

            Err(error) => {
                crate::report_error!(
                    format!("{error:?}"),
                    "backup",
                    "download_backup()"
                );

                return backup_error(
                    StatusCode::BAD_GATEWAY,
                    "N1 could not return the selected ARIS backup.",
                );
            }
        };

    let safe_name =
        backup
            .file_name
            .replace('"', "_");

    let mut response =
        Response::new(
            Body::from(bytes)
        );

    *response.status_mut() =
        StatusCode::OK;

    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static(
            "application/vnd.sqlite3"
        ),
    );

    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static(
            "no-store"
        ),
    );

    if let Ok(value) =
        HeaderValue::from_str(
            &format!(
                "attachment; filename=\"{safe_name}\""
            )
        )
    {
        response
            .headers_mut()
            .insert(
                CONTENT_DISPOSITION,
                value,
            );
    }

    response
}
