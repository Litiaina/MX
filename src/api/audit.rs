use axum::{
    Json,
    extract::{FromRequestParts, Query, Request},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

const DEFAULT_AUDIT_LIMIT: usize = 100;
const MAX_AUDIT_LIMIT: usize = 200;

#[derive(Debug)]
struct AuditClassification {
    action: &'static str,
    target_type: Option<&'static str>,
    target_uid: Option<String>,
}

#[derive(Debug)]
struct AuditWrite {
    event_uid: String,
    actor_uid: String,
    actor_email: String,
    access_level: i64,
    action: String,
    method: String,
    path: String,
    target_type: Option<String>,
    target_uid: Option<String>,
    status_code: u16,
    success: bool,
    user_agent: Option<String>,
    created_at: i64,
}

#[derive(Debug, Deserialize, Default)]
pub struct AuditQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub user: Option<String>,
    pub action: Option<String>,
    pub result: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub event_uid: String,
    pub actor_uid: String,
    pub actor_name: String,
    pub actor_email: String,
    pub access_level: i64,
    pub action: String,
    pub method: String,
    pub path: String,
    pub target_type: Option<String>,
    pub target_uid: Option<String>,
    pub status_code: i64,
    pub success: bool,
    pub user_agent: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct AuditPage {
    pub data: Vec<AuditEntry>,
    pub page: usize,
    pub limit: usize,
    pub total: usize,
    pub total_pages: usize,
    pub has_next: bool,
}

pub async fn audit_request(request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();

    let method = parts.method.clone();
    let path = parts.uri.path().to_string();

    let user_agent = parts
        .headers
        .get("user-agent")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let claims = Claims::from_request_parts(&mut parts, &()).await.ok();

    let request = Request::from_parts(parts, body);
    let response = next.run(request).await;

    let Some(claims) = claims else {
        return response;
    };

    let Some(classification) = classify_action(&method, &path) else {
        return response;
    };

    let status_code = response.status().as_u16();

    let write = AuditWrite {
        event_uid: Uuid::new_v4().to_string(),
        actor_uid: claims.uid,
        actor_email: claims.email,
        access_level: claims.access_level,
        action: classification.action.to_string(),
        method: method.as_str().to_string(),
        path,
        target_type: classification.target_type.map(str::to_string),
        target_uid: classification.target_uid,
        status_code,
        success: status_code < 400,
        user_agent,
        created_at: chrono::Utc::now().timestamp_millis(),
    };

    if let Err(error) = record_audit_event(write).await {
        crate::report_error!(error, "audit", "audit_request()");
    }

    response
}

pub async fn get_audit_log(claims: Claims, Query(query): Query<AuditQuery>) -> Response {
    if !claims.can_manage_accounts() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "response": "access denied"
            })),
        )
            .into_response();
    }

    let page = query.page.unwrap_or(1).max(1);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_AUDIT_LIMIT)
        .clamp(1, MAX_AUDIT_LIMIT);

    let user_filter = query.user.unwrap_or_default().trim().to_string();

    let action_filter = query.action.unwrap_or_default().trim().to_string();

    let result_filter = match query
        .result
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None | Some("all") => None,
        Some("success") => Some(1_i64),
        Some("failed") => Some(0_i64),

        Some(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "response":
                        "result must be 'all', 'success', or 'failed'"
                })),
            )
                .into_response();
        }
    };

    let offset = page.saturating_sub(1).saturating_mul(limit);

    let database_result =
        tokio::task::spawn_blocking(move || -> Result<AuditPage, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_audit_schema(connection)?;

                let user_enabled = if user_filter.is_empty() { 0_i64 } else { 1_i64 };

                let action_enabled = if action_filter.is_empty() {
                    0_i64
                } else {
                    1_i64
                };

                let user_like = format!("%{}%", user_filter);

                let action_like = format!("%{}%", action_filter);

                let total: i64 = connection.query_row(
                    r#"
                            SELECT COUNT(*)
                            FROM mx_audit_log
                            WHERE (
                                ?1 = 0
                                OR actor_uid LIKE ?2 COLLATE NOCASE
                                OR actor_name LIKE ?2 COLLATE NOCASE
                                OR actor_email LIKE ?2 COLLATE NOCASE
                            )
                            AND (
                                ?3 = 0
                                OR action LIKE ?4 COLLATE NOCASE
                                OR path LIKE ?4 COLLATE NOCASE
                                OR COALESCE(target_uid, '') LIKE ?4 COLLATE NOCASE
                            )
                            AND (
                                ?5 IS NULL
                                OR success = ?5
                            )
                            "#,
                    params![
                        user_enabled,
                        user_like,
                        action_enabled,
                        action_like,
                        result_filter,
                    ],
                    |row| row.get(0),
                )?;

                let mut statement = connection.prepare(
                    r#"
                            SELECT
                                id,
                                event_uid,
                                actor_uid,
                                actor_name,
                                actor_email,
                                access_level,
                                action,
                                method,
                                path,
                                target_type,
                                target_uid,
                                status_code,
                                success,
                                user_agent,
                                created_at
                            FROM mx_audit_log
                            WHERE (
                                ?1 = 0
                                OR actor_uid LIKE ?2 COLLATE NOCASE
                                OR actor_name LIKE ?2 COLLATE NOCASE
                                OR actor_email LIKE ?2 COLLATE NOCASE
                            )
                            AND (
                                ?3 = 0
                                OR action LIKE ?4 COLLATE NOCASE
                                OR path LIKE ?4 COLLATE NOCASE
                                OR COALESCE(target_uid, '') LIKE ?4 COLLATE NOCASE
                            )
                            AND (
                                ?5 IS NULL
                                OR success = ?5
                            )
                            ORDER BY id DESC
                            LIMIT ?6 OFFSET ?7
                            "#,
                )?;

                let rows = statement.query_map(
                    params![
                        user_enabled,
                        user_like,
                        action_enabled,
                        action_like,
                        result_filter,
                        limit as i64,
                        offset as i64,
                    ],
                    |row| {
                        let success: i64 = row.get("success")?;

                        Ok(AuditEntry {
                            id: row.get("id")?,
                            event_uid: row.get("event_uid")?,
                            actor_uid: row.get("actor_uid")?,
                            actor_name: row.get("actor_name")?,
                            actor_email: row.get("actor_email")?,
                            access_level: row.get("access_level")?,
                            action: row.get("action")?,
                            method: row.get("method")?,
                            path: row.get("path")?,
                            target_type: row.get("target_type")?,
                            target_uid: row.get("target_uid")?,
                            status_code: row.get("status_code")?,
                            success: success != 0,
                            user_agent: row.get("user_agent")?,
                            created_at: row.get("created_at")?,
                        })
                    },
                )?;

                let mut data = Vec::new();

                for row in rows {
                    data.push(row?);
                }

                let total = total.max(0) as usize;
                let total_pages = if total == 0 { 0 } else { total.div_ceil(limit) };

                Ok(AuditPage {
                    data,
                    page,
                    limit,
                    total,
                    total_pages,
                    has_next: page < total_pages,
                })
            })
        })
        .await;

    match database_result {
        Ok(Ok(result)) => (StatusCode::OK, Json(json!(result))).into_response(),

        Ok(Err(error)) => {
            crate::report_error!(error, "audit", "get_audit_log()");

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "response":
                        "failed to read audit log"
                })),
            )
                .into_response()
        }

        Err(error) => {
            crate::report_error!(
                format!("audit log blocking task failed: {error}"),
                "audit",
                "get_audit_log()"
            );

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "response":
                        "failed to read audit log"
                })),
            )
                .into_response()
        }
    }
}

async fn record_audit_event(write: AuditWrite) -> Result<(), String> {
    let database_result =
        tokio::task::spawn_blocking(move || -> Result<(), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_audit_schema(connection)?;

                let actor_name: Option<String> = connection
                    .query_row(
                        r#"
                                SELECT name
                                FROM users
                                WHERE uid = ?1
                                LIMIT 1
                                "#,
                        params![&write.actor_uid],
                        |row| row.get(0),
                    )
                    .optional()?;

                let actor_name = actor_name.unwrap_or_else(|| write.actor_email.clone());

                connection.execute(
                    r#"
                        INSERT INTO mx_audit_log (
                            event_uid,
                            actor_uid,
                            actor_name,
                            actor_email,
                            access_level,
                            action,
                            method,
                            path,
                            target_type,
                            target_uid,
                            status_code,
                            success,
                            user_agent,
                            created_at
                        ) VALUES (
                            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                            ?8, ?9, ?10, ?11, ?12, ?13, ?14
                        )
                        "#,
                    params![
                        write.event_uid,
                        write.actor_uid,
                        actor_name,
                        write.actor_email,
                        write.access_level,
                        write.action,
                        write.method,
                        write.path,
                        write.target_type,
                        write.target_uid,
                        write.status_code as i64,
                        if write.success { 1_i64 } else { 0_i64 },
                        write.user_agent,
                        write.created_at,
                    ],
                )?;

                Ok(())
            })
        })
        .await;

    match database_result {
        Ok(Ok(())) => Ok(()),

        Ok(Err(error)) => Err(format!("failed to write MX audit event: {error}")),

        Err(error) => Err(format!("audit write blocking task failed: {error}")),
    }
}

fn ensure_audit_schema(connection: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    crate::api::mx::migration::migrate_legacy_schema(connection)?;
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_audit_log (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            event_uid       TEXT NOT NULL UNIQUE,
            actor_uid       TEXT NOT NULL,
            actor_name      TEXT NOT NULL,
            actor_email     TEXT NOT NULL,
            access_level    INTEGER NOT NULL,
            action          TEXT NOT NULL,
            method          TEXT NOT NULL,
            path            TEXT NOT NULL,
            target_type     TEXT,
            target_uid      TEXT,
            status_code     INTEGER NOT NULL,
            success         INTEGER NOT NULL CHECK(success IN (0, 1)),
            user_agent      TEXT,
            created_at      INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS
            idx_mx_audit_created_at
        ON mx_audit_log (
            created_at DESC
        );

        CREATE INDEX IF NOT EXISTS
            idx_mx_audit_actor_uid
        ON mx_audit_log (
            actor_uid,
            created_at DESC
        );

        CREATE INDEX IF NOT EXISTS
            idx_mx_audit_action
        ON mx_audit_log (
            action,
            created_at DESC
        );

        CREATE INDEX IF NOT EXISTS
            idx_mx_audit_target
        ON mx_audit_log (
            target_type,
            target_uid,
            created_at DESC
        );

        CREATE TRIGGER IF NOT EXISTS
            mx_audit_log_no_update
        BEFORE UPDATE ON mx_audit_log
        BEGIN
            SELECT RAISE(
                ABORT,
                'MX audit log is append-only'
            );
        END;

        CREATE TRIGGER IF NOT EXISTS
            mx_audit_log_no_delete
        BEFORE DELETE ON mx_audit_log
        BEGIN
            SELECT RAISE(
                ABORT,
                'MX audit log is append-only'
            );
        END;
        "#,
    )
}

fn classify_action(method: &Method, path: &str) -> Option<AuditClassification> {
    let segments = path.trim_matches('/').split('/').collect::<Vec<_>>();

    if path == "/mx/v1/admin/audit" && method == Method::GET {
        return Some(AuditClassification {
            action: "audit.view",
            target_type: None,
            target_uid: None,
        });
    }

    if path == "/mx/v1/admin/schema/order" && method == Method::PUT {
        return Some(AuditClassification {
            action: "schema.order.update",
            target_type: Some("record-structure"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/admin/schema/fields" && method == Method::POST {
        return Some(AuditClassification {
            action: "schema.field.create",
            target_type: Some("record-field"),
            target_uid: None,
        });
    }

    if segments.len() == 6
        && segments[0] == "mx"
        && segments[1] == "v1"
        && segments[2] == "admin"
        && segments[3] == "schema"
        && segments[4] == "fields"
        && method == Method::PUT
    {
        return Some(AuditClassification {
            action: "schema.field.update",
            target_type: Some("record-field"),
            target_uid: segments.get(5).map(|value| value.to_string()),
        });
    }

    if path == "/mx/v1/admin/attachment-fields" && method == Method::POST {
        return Some(AuditClassification {
            action: "attachment-field.create",
            target_type: Some("attachment-field"),
            target_uid: None,
        });
    }

    if segments.len() == 5
        && segments[0] == "mx"
        && segments[1] == "v1"
        && segments[2] == "admin"
        && segments[3] == "attachment-fields"
        && method == Method::PUT
    {
        return Some(AuditClassification {
            action: "attachment-field.update",
            target_type: Some("attachment-field"),
            target_uid: segments.get(4).map(|value| value.to_string()),
        });
    }

    if path == "/mx/v1/admin/dashboard-config" && method == Method::PUT {
        return Some(AuditClassification {
            action: "dashboard.config.update",
            target_type: Some("dashboard-config"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/reports/export.csv" && method == Method::GET {
        return Some(AuditClassification {
            action: "report.export",
            target_type: Some("report"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/admin/storage-layout" && method == Method::PUT {
        return Some(AuditClassification {
            action: "storage.layout.update",
            target_type: Some("n1-storage-layout"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/admin/backups" && method == Method::POST {
        return Some(AuditClassification {
            action: "backup.create",
            target_type: Some("database-backup"),
            target_uid: None,
        });
    }

    if segments.len() == 6
        && segments[0] == "mx"
        && segments[1] == "v1"
        && segments[2] == "admin"
        && segments[3] == "backups"
    {
        let backup_uid = segments.get(4).map(|value| value.to_string());

        if segments[5] == "verify" && method == Method::POST {
            return Some(AuditClassification {
                action: "backup.verify",
                target_type: Some("database-backup"),
                target_uid: backup_uid,
            });
        }

        if segments[5] == "download" && method == Method::GET {
            return Some(AuditClassification {
                action: "backup.download",
                target_type: Some("database-backup"),
                target_uid: backup_uid,
            });
        }
    }

    if path == "/mx/v1/db/query" && method == Method::POST {
        return Some(AuditClassification {
            action: "database.query",
            target_type: Some("database"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/auth/get" && method == Method::POST {
        return Some(AuditClassification {
            action: "account.list",
            target_type: Some("account"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/auth/modify" && method == Method::PATCH {
        return Some(AuditClassification {
            action: "account.modify",
            target_type: Some("account"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/auth/delete" && method == Method::DELETE {
        return Some(AuditClassification {
            action: "account.delete",
            target_type: Some("account"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/auth/enable_2fa" && method == Method::POST {
        return Some(AuditClassification {
            action: "account.2fa.enable",
            target_type: Some("account"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/auth/disable_2fa" && method == Method::POST {
        return Some(AuditClassification {
            action: "account.2fa.disable",
            target_type: Some("account"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/user/create" && method == Method::POST {
        return Some(AuditClassification {
            action: "account.create",
            target_type: Some("account"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/user/modify" && method == Method::PATCH {
        return Some(AuditClassification {
            action: "account.self.modify",
            target_type: Some("account"),
            target_uid: None,
        });
    }

    if path == "/mx/v1/user/delete" && method == Method::DELETE {
        return Some(AuditClassification {
            action: "account.self.delete",
            target_type: Some("account"),
            target_uid: None,
        });
    }

    if segments.len() < 3 || segments[0] != "mx" || segments[1] != "v1" || segments[2] != "records"
    {
        return None;
    }

    if segments.len() == 3 {
        if method == Method::POST {
            return Some(AuditClassification {
                action: "record.create",
                target_type: Some("record"),
                target_uid: None,
            });
        }

        return None;
    }

    let record_uid = segments.get(3).map(|value| value.to_string());

    if segments.len() == 4 {
        if method == Method::PUT || method == Method::PATCH {
            return Some(AuditClassification {
                action: "record.update",
                target_type: Some("record"),
                target_uid: record_uid,
            });
        }

        if method == Method::DELETE {
            return Some(AuditClassification {
                action: "record.delete",
                target_type: Some("record"),
                target_uid: record_uid,
            });
        }

        return None;
    }

    if segments.get(4) != Some(&"attachments") {
        return None;
    }

    if segments.len() == 5 && method == Method::POST {
        return Some(AuditClassification {
            action: "attachment.upload",
            target_type: Some("record"),
            target_uid: record_uid,
        });
    }

    // File Attachment field upload:
    // /mx/v1/records/{record}/attachments/fields/{field}
    if segments.len() == 7 && segments.get(5) == Some(&"fields") && method == Method::POST {
        return Some(AuditClassification {
            action: "attachment.upload",
            target_type: Some("record"),
            target_uid: record_uid,
        });
    }

    let attachment_uid = segments.get(5).map(|value| value.to_string());

    if segments.len() == 6 && method == Method::DELETE {
        return Some(AuditClassification {
            action: "attachment.delete",
            target_type: Some("attachment"),
            target_uid: attachment_uid,
        });
    }

    if segments.len() == 7 && method == Method::GET {
        return match segments[6] {
            "preview" => Some(AuditClassification {
                action: "attachment.preview",
                target_type: Some("attachment"),
                target_uid: attachment_uid,
            }),

            "download" => Some(AuditClassification {
                action: "attachment.download",
                target_type: Some("attachment"),
                target_uid: attachment_uid,
            }),

            _ => None,
        };
    }

    None
}
