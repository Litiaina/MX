use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    api::{
        live::publish_user_event,
        modules::ensure_module_schema,
        preferences::{UserPreferences, ensure_preferences_schema},
    },
    db::connector::with_sql_connection,
    middleware::auth::Claims,
};

#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub uid: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub actor_uid: Option<String>,
    pub actor_name: Option<String>,
    pub target_type: Option<String>,
    pub target_uid: Option<String>,
    pub module_uid: Option<String>,
    pub data: Value,
    pub created_at: i64,
    pub read_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewNotification {
    pub recipient_uid: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub actor_uid: Option<String>,
    pub target_type: Option<String>,
    pub target_uid: Option<String>,
    pub module_uid: Option<String>,
    pub data: Value,
    pub dedup_key: Option<String>,
    pub mandatory: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct NotificationQuery {
    pub unread_only: Option<bool>,
    pub before: Option<i64>,
    pub limit: Option<usize>,
}

pub(crate) fn ensure_notification_schema(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_notifications (
            uid            TEXT PRIMARY KEY NOT NULL,
            recipient_uid  TEXT NOT NULL,
            kind           TEXT NOT NULL,
            title          TEXT NOT NULL,
            body           TEXT NOT NULL DEFAULT '',
            actor_uid      TEXT,
            target_type    TEXT,
            target_uid     TEXT,
            module_uid     TEXT,
            data_json      TEXT NOT NULL DEFAULT '{}',
            dedup_key      TEXT,
            created_at     INTEGER NOT NULL,
            read_at        INTEGER,
            FOREIGN KEY(recipient_uid) REFERENCES users(uid) ON DELETE CASCADE,
            FOREIGN KEY(actor_uid) REFERENCES users(uid) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_mx_notifications_recipient_created
            ON mx_notifications(recipient_uid, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_mx_notifications_recipient_unread
            ON mx_notifications(recipient_uid, read_at, created_at DESC);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_mx_notifications_dedup
            ON mx_notifications(recipient_uid, dedup_key)
            WHERE dedup_key IS NOT NULL;
        "#,
    )
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn optional_notification_preference(preferences: &UserPreferences, kind: &str) -> Option<bool> {
    let notification = &preferences.notifications;
    if kind.starts_with("message.") {
        notification.messages
    } else if kind.starts_with("mention.") {
        notification.mentions
    } else if kind.starts_with("channel.") {
        notification.channel_activity
    } else if kind == "record.created" {
        notification.record_created
    } else if kind == "record.updated" || kind == "record.restored" || kind == "record.deleted" {
        notification.record_updated
    } else if kind == "record.assigned" {
        notification.record_assigned
    } else if kind.starts_with("attachment.") || kind.starts_with("file.") {
        notification.attachment_received
    } else if kind.starts_with("workflow.") {
        notification.workflow_changes
    } else if kind.starts_with("task.") {
        notification.task_activity
    } else {
        None
    }
}

fn notifications_enabled(
    connection: &rusqlite::Connection,
    user_uid: &str,
    kind: &str,
) -> rusqlite::Result<bool> {
    ensure_preferences_schema(connection)?;
    let text = connection
        .query_row(
            "SELECT preferences_json FROM mx_user_preferences WHERE user_uid = ?1",
            params![user_uid],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let preferences = text
        .as_deref()
        .and_then(|value| serde_json::from_str::<UserPreferences>(value).ok())
        .unwrap_or_default();
    Ok(optional_notification_preference(&preferences, kind).unwrap_or(true))
}

fn notification_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Notification> {
    let data_text = row.get::<_, String>(9)?;
    Ok(Notification {
        uid: row.get(0)?,
        kind: row.get(1)?,
        title: row.get(2)?,
        body: row.get(3)?,
        actor_uid: row.get(4)?,
        actor_name: row.get(5)?,
        target_type: row.get(6)?,
        target_uid: row.get(7)?,
        module_uid: row.get(8)?,
        data: serde_json::from_str(&data_text).unwrap_or(Value::Object(Default::default())),
        created_at: row.get(10)?,
        read_at: row.get(11)?,
    })
}

fn select_columns() -> &'static str {
    r#"
        n.uid, n.kind, n.title, n.body, n.actor_uid, u.name,
        n.target_type, n.target_uid, n.module_uid, n.data_json,
        n.created_at, n.read_at
    "#
}

fn insert_notification(
    connection: &rusqlite::Connection,
    request: &NewNotification,
) -> rusqlite::Result<Option<Notification>> {
    ensure_notification_schema(connection)?;
    if !request.mandatory
        && !notifications_enabled(connection, &request.recipient_uid, &request.kind)?
    {
        return Ok(None);
    }

    let uid = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().timestamp_millis();
    let data_json = serde_json::to_string(&request.data).unwrap_or_else(|_| "{}".to_string());
    let inserted = connection.execute(
        r#"
        INSERT OR IGNORE INTO mx_notifications(
            uid, recipient_uid, kind, title, body, actor_uid, target_type,
            target_uid, module_uid, data_json, dedup_key, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
        params![
            uid,
            request.recipient_uid,
            request.kind,
            request.title,
            request.body,
            request.actor_uid,
            request.target_type,
            request.target_uid,
            request.module_uid,
            data_json,
            request.dedup_key,
            created_at,
        ],
    )?;
    if inserted == 0 {
        return Ok(None);
    }

    let sql = format!(
        "SELECT {} FROM mx_notifications n LEFT JOIN users u ON u.uid = n.actor_uid WHERE n.uid = ?1",
        select_columns()
    );
    connection
        .query_row(&sql, params![uid], notification_from_row)
        .map(Some)
}

/// Persist a notification before attempting real-time delivery. Callers do not
/// depend on an active WebSocket: the notification center remains authoritative
/// and reconnecting clients fetch anything they missed.
pub async fn notify_user(request: NewNotification) -> Result<Option<Notification>, String> {
    let request_for_db = request.clone();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| insert_notification(connection, &request_for_db))
    })
    .await
    .map_err(|error| format!("notification task failed: {error}"))?
    .map_err(|error| format!("notification database operation failed: {error}"))?;

    if let Some(notification) = result.as_ref() {
        publish_user_event(
            &request.recipient_uid,
            "notification.created",
            request.actor_uid.as_deref(),
            serde_json::to_value(notification).unwrap_or_else(|_| json!({"uid":notification.uid})),
        );
    }
    Ok(result)
}

/// Notify every account that can read a module, excluding the actor. Each
/// recipient is filtered through their own notification preferences before a
/// durable notification is written and privately delivered over WebSocket.
pub async fn notify_module_readers(
    module_uid: String,
    actor_uid: String,
    kind: String,
    title: String,
    body: String,
    target_uid: String,
    data: Value,
) {
    let lookup_module = module_uid.clone();
    let lookup_actor = actor_uid.clone();
    let recipients = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_module_schema(connection)?;
            let mut statement = connection.prepare(
                r#"SELECT u.uid FROM users u
                   JOIN mx_module_permissions p ON p.access_level=u.access_level
                   WHERE p.module_uid=?1 AND p.can_read=1 AND u.uid<>?2"#,
            )?;
            statement
                .query_map(params![lookup_module, lookup_actor], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()
        })
    })
    .await;
    let Ok(Ok(recipients)) = recipients else {
        return;
    };
    for recipient_uid in recipients {
        let _ = notify_user(NewNotification {
            recipient_uid,
            kind: kind.clone(),
            title: title.clone(),
            body: body.clone(),
            actor_uid: Some(actor_uid.clone()),
            target_type: Some("record".to_string()),
            target_uid: Some(target_uid.clone()),
            module_uid: Some(module_uid.clone()),
            data: data.clone(),
            dedup_key: None,
            mandatory: false,
        })
        .await;
    }
}

pub async fn list_notifications(
    claims: Claims,
    Query(query): Query<NotificationQuery>,
) -> Response {
    let uid = claims.uid;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let unread_only = query.unread_only.unwrap_or(false);
    let before = query.before.unwrap_or(i64::MAX);
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_notification_schema(connection)?;
            let sql = format!(
                r#"
                SELECT {} FROM mx_notifications n
                LEFT JOIN users u ON u.uid = n.actor_uid
                WHERE n.recipient_uid = ?1
                  AND n.created_at < ?2
                  AND (?3 = 0 OR n.read_at IS NULL)
                ORDER BY n.created_at DESC
                LIMIT ?4
                "#,
                select_columns()
            );
            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map(
                params![uid, before, if unread_only { 1 } else { 0 }, limit as i64],
                notification_from_row,
            )?;
            let notifications = rows.collect::<Result<Vec<_>, _>>()?;
            let unread: i64 = connection.query_row(
                "SELECT COUNT(*) FROM mx_notifications WHERE recipient_uid = ?1 AND read_at IS NULL",
                params![uid],
                |row| row.get(0),
            )?;
            Ok((notifications, unread.max(0)))
        })
    })
    .await;

    match result {
        Ok(Ok((notifications, unread))) => api_json(
            StatusCode::OK,
            json!({"notifications":notifications,"unread":unread}),
        ),
        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "notifications", "list_notifications()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to load notifications"}),
            )
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "notifications", "list_notifications()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"notification task failed"}),
            )
        }
    }
}

pub async fn mark_notification_read(claims: Claims, Path(uid): Path<String>) -> Response {
    let user_uid = claims.uid.clone();
    let event_user_uid = claims.uid;
    let notification_uid = uid.clone();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_notification_schema(connection)?;
            connection.execute(
                "UPDATE mx_notifications SET read_at = COALESCE(read_at, ?1) WHERE uid = ?2 AND recipient_uid = ?3",
                params![chrono::Utc::now().timestamp_millis(), notification_uid, user_uid],
            )
        })
    })
    .await;
    match result {
        Ok(Ok(1)) => {
            publish_user_event(
                &event_user_uid,
                "notification.read",
                Some(&event_user_uid),
                json!({"uid":uid}),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Ok(_)) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"notification was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to update notification"}),
        ),
    }
}

pub async fn mark_all_notifications_read(claims: Claims) -> Response {
    let uid = claims.uid.clone();
    let event_uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_notification_schema(connection)?;
            connection.execute(
                "UPDATE mx_notifications SET read_at = ?1 WHERE recipient_uid = ?2 AND read_at IS NULL",
                params![chrono::Utc::now().timestamp_millis(), uid],
            )
        })
    })
    .await;
    match result {
        Ok(Ok(updated)) => {
            publish_user_event(
                &event_uid,
                "notification.read_all",
                Some(&event_uid),
                json!({"updated":updated}),
            );
            api_json(
                StatusCode::OK,
                json!({"response":"notifications marked as read","updated":updated}),
            )
        }
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to update notifications"}),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_preference_routes_known_events() {
        let mut preferences = UserPreferences::default();
        preferences.notifications.messages = Some(false);
        preferences.notifications.record_created = Some(true);
        assert_eq!(
            optional_notification_preference(&preferences, "message.created"),
            Some(false)
        );
        assert_eq!(
            optional_notification_preference(&preferences, "record.created"),
            Some(true)
        );
        assert_eq!(
            optional_notification_preference(&preferences, "security.password_changed"),
            None
        );
    }
}
