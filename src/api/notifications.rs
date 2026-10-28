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

mod outbox;
pub(crate) use outbox::{queue_notifications, queue_record_activity, start_delivery_worker};

#[cfg(test)]
pub(crate) fn drain_notifications_for_test(c: &rusqlite::Connection) -> rusqlite::Result<usize> {
    outbox::drain(c).map(|notifications| notifications.len())
}

use crate::{
    api::{
        live::publish_user_event,
        modules::ensure_module_schema,
        preferences::{UserPreferences, ensure_preferences_schema},
    },
    db::connector::{with_sql_connection, with_sql_write},
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub after: Option<i64>,
    pub after_uid: Option<String>,
}

pub(crate) fn ensure_notification_schema(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    if crate::db::schema::initialized(connection) {
        return Ok(());
    }
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

        CREATE TABLE IF NOT EXISTS mx_notification_read_state (
            recipient_uid TEXT PRIMARY KEY NOT NULL REFERENCES users(uid) ON DELETE CASCADE,
            through_created_at INTEGER NOT NULL,
            read_at INTEGER NOT NULL
        );
        "#,
    )?;
    outbox::ensure(connection)
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
        .prepare_cached("SELECT preferences_json FROM mx_user_preferences WHERE user_uid = ?1")?
        .query_row(params![user_uid], |row| row.get::<_, String>(0))
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
        n.created_at, COALESCE(n.read_at,
            CASE WHEN n.created_at <= read_state.through_created_at THEN read_state.read_at END)
    "#
}

pub(crate) fn insert_notification(
    connection: &rusqlite::Connection,
    request: &NewNotification,
) -> rusqlite::Result<Option<Notification>> {
    NotificationBatch::default().insert(connection, request)
}

/// Caches are confined to ONE transaction/snapshot. Never retain a batch across
/// commits: changed user preferences and names must take effect on the next job.
#[derive(Default)]
struct NotificationBatch {
    preferences: std::collections::HashMap<(String, String), bool>,
    actors: std::collections::HashMap<String, Option<String>>,
}

impl NotificationBatch {
    fn insert(
        &mut self,
        connection: &rusqlite::Connection,
        request: &NewNotification,
    ) -> rusqlite::Result<Option<Notification>> {
        ensure_notification_schema(connection)?;
        if !request.mandatory {
            let key = (request.recipient_uid.clone(), request.kind.clone());
            let enabled = if let Some(enabled) = self.preferences.get(&key) {
                *enabled
            } else {
                let enabled =
                    notifications_enabled(connection, &request.recipient_uid, &request.kind)?;
                self.preferences.insert(key, enabled);
                enabled
            };
            if !enabled {
                return Ok(None);
            }
        }

        let uid = Uuid::new_v4().to_string();
        let created_at = notification_created_at(
            connection,
            &request.recipient_uid,
            chrono::Utc::now().timestamp_millis(),
        )?;
        let data_json = serde_json::to_string(&request.data).unwrap_or_else(|_| "{}".to_string());
        let inserted = connection
            .prepare_cached(
                r#"
        INSERT OR IGNORE INTO mx_notifications(
            uid, recipient_uid, kind, title, body, actor_uid, target_type,
            target_uid, module_uid, data_json, dedup_key, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
            )?
            .execute(params![
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
            ])?;
        if inserted == 0 {
            return Ok(None);
        }

        let actor_name = if let Some(actor) = &request.actor_uid {
            if !self.actors.contains_key(actor) {
                let name = connection
                    .prepare_cached("SELECT name FROM users WHERE uid=?1")?
                    .query_row([actor], |r| r.get::<_, String>(0))
                    .optional()?;
                self.actors.insert(actor.clone(), name);
            }
            self.actors.get(actor).cloned().flatten()
        } else {
            None
        };
        Ok(Some(Notification {
            uid,
            kind: request.kind.clone(),
            title: request.title.clone(),
            body: request.body.clone(),
            actor_uid: request.actor_uid.clone(),
            actor_name,
            target_type: request.target_type.clone(),
            target_uid: request.target_uid.clone(),
            module_uid: request.module_uid.clone(),
            data: request.data.clone(),
            created_at,
            read_at: None,
        }))
    }
}

// A read-all receipt is one account-scoped watermark, not hundreds/thousands
// of rewrites of notification payload/index pages. New deliveries must be
// newer than that watermark, including within the same millisecond and after
// a wall-clock correction. This is per-recipient, not a global clock/cache.
fn notification_created_at(
    c: &rusqlite::Connection,
    recipient: &str,
    now: i64,
) -> rusqlite::Result<i64> {
    let through = c
        .prepare_cached(
            "SELECT through_created_at FROM mx_notification_read_state WHERE recipient_uid=?1",
        )?
        .query_row([recipient], |r| r.get::<_, i64>(0))
        .optional()?;
    Ok(through.map_or(now, |through| now.max(through.saturating_add(1))))
}

fn mark_all_read(c: &rusqlite::Connection, recipient: &str, now: i64) -> rusqlite::Result<i64> {
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let updated: i64 = tx.query_row(
        "SELECT count(*) FROM mx_notifications WHERE recipient_uid=?1 AND read_at IS NULL
          AND created_at > COALESCE((SELECT through_created_at FROM mx_notification_read_state WHERE recipient_uid=?1),-1)",
        [recipient], |r| r.get(0),
    )?;
    let through: Option<i64> = tx.query_row(
        "SELECT max(created_at) FROM mx_notifications WHERE recipient_uid=?1",
        [recipient],
        |r| r.get(0),
    )?;
    if let Some(through) = through {
        tx.execute(
            "INSERT INTO mx_notification_read_state(recipient_uid,through_created_at,read_at) VALUES(?1,?2,?3)
             ON CONFLICT(recipient_uid) DO UPDATE SET
                read_at=CASE WHEN excluded.through_created_at>through_created_at THEN excluded.read_at ELSE read_at END,
                through_created_at=max(through_created_at,excluded.through_created_at)",
            params![recipient, through, now],
        )?;
    }
    tx.commit()?;
    Ok(updated)
}

/// Persist a notification before attempting real-time delivery. Callers do not
/// depend on an active WebSocket: the notification center remains authoritative
/// and reconnecting clients fetch anything they missed.
pub async fn notify_user(request: NewNotification) -> Result<Option<Notification>, String> {
    let request_for_db = request.clone();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_write(|connection| insert_notification(connection, &request_for_db))
    })
    .await
    .map_err(|error| format!("notification task failed: {error}"))?
    .map_err(|error| format!("notification database operation failed: {error}"))?;

    if let Some(notification) = result.as_ref() {
        deliver_notification(&request.recipient_uid, notification);
    }
    Ok(result)
}

/// Call only after the transaction containing this notification has committed.
pub(crate) fn deliver_notification(recipient: &str, notification: &Notification) {
    publish_user_event(
        recipient,
        "notification.created",
        notification.actor_uid.as_deref(),
        serde_json::to_value(notification).unwrap_or_else(|_| json!({"uid":notification.uid})),
    );
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
    let dedup_key = if kind == "record.updated" || kind.starts_with("attachment.") {
        Some(format!(
            "record-activity:{kind}:{module_uid}:{target_uid}:{}:{actor_uid}:{}",
            data.get("attachment_field_uid")
                .and_then(Value::as_str)
                .unwrap_or(""),
            chrono::Utc::now().timestamp() / 300
        ))
    } else {
        None
    };
    let request = NewNotification {
        recipient_uid: String::new(),
        kind,
        title,
        body,
        actor_uid: Some(actor_uid),
        target_type: Some("record".to_owned()),
        target_uid: Some(target_uid),
        module_uid: Some(module_uid),
        data,
        dedup_key,
        mandatory: false,
    };
    let result = tokio::task::spawn_blocking(move || {
        with_sql_write(|connection| outbox::enqueue_module(connection, request))
    })
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::error!(%error, "Module notification fan-out failed"),
        Err(error) => tracing::error!(%error, "Module notification task failed"),
    }
}

// One activity is one atomic batch, not one pool checkout + autocommit per
// recipient. At 100 active editors the previous fan-out could contend with
// foreground saves through tens of thousands of individual write transactions.
// Authorization and preferences are rechecked in the SAME write snapshot.
// Publish only after the whole batch commits; never announce rolled-back rows.
#[cfg(test)]
fn insert_module_notifications(
    connection: &rusqlite::Connection,
    request: &NewNotification,
) -> rusqlite::Result<Vec<(String, Notification)>> {
    ensure_module_schema(connection)?;
    ensure_notification_schema(connection)?;
    ensure_preferences_schema(connection)?;
    let transaction =
        rusqlite::Transaction::new_unchecked(connection, rusqlite::TransactionBehavior::Immediate)?;
    let notifications = insert_module_notifications_in_transaction(
        &transaction,
        request,
        &mut NotificationBatch::default(),
    )?;
    transaction.commit()?;
    Ok(notifications)
}

fn insert_module_notifications_in_transaction(
    transaction: &rusqlite::Connection,
    request: &NewNotification,
    batch: &mut NotificationBatch,
) -> rusqlite::Result<Vec<(String, Notification)>> {
    let lookup_module = request.module_uid.as_deref().unwrap_or("");
    let lookup_actor = request.actor_uid.as_deref().unwrap_or("");
    let lookup_record = request.target_uid.as_deref().unwrap_or("");
    let lookup_field = request
        .data
        .get("attachment_field_uid")
        .and_then(Value::as_str)
        .map(str::to_owned);
    // Load shared record/field policy once per activity, not once per account.
    // The whole batch still uses the same current authorization snapshot.
    let policy: Option<(String, Option<String>)> = transaction.prepare_cached(
        "SELECT COALESCE(json_extract(m.config_json,'$.record_visibility'),'module'),o.creator_uid FROM mx_records r JOIN mx_modules m ON m.uid=r.module_uid LEFT JOIN mx_record_owners o ON o.record_uid=r.uid WHERE r.uid=?1 AND m.uid=?2",
        )?.query_row(params![lookup_record,lookup_module],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((record_scope, creator)) = policy else {
        return Ok(Vec::new());
    };
    let field_scope = if let Some(field) = lookup_field.as_deref().filter(|f| !f.is_empty()) {
        let scope: Option<String> = transaction.query_row("SELECT COALESCE(json_extract(config_json,'$.access_scope'),'module') FROM mx_fields WHERE uid=?1 AND module_uid=?2",params![field,lookup_module],|r|r.get(0)).optional()?;
        let Some(scope) = scope else {
            return Ok(Vec::new());
        };
        scope
    } else {
        "module".to_owned()
    };
    let recipients = {
        let mut statement = transaction.prepare_cached(
            r#"SELECT u.uid,u.access_level FROM users u
                   JOIN mx_module_permissions role
                     ON role.access_level=u.access_level AND role.module_uid=?1
                   LEFT JOIN mx_module_account_permissions account
                     ON account.module_uid=?1 AND account.user_uid=u.uid
                   JOIN mx_modules module ON module.uid=?1 AND module.active=1
                   WHERE role.can_read=1
                     AND (u.access_level=0 OR account.can_read=1)
                     AND u.uid<>?2
                     AND (?3 IS NULL OR NOT EXISTS(
                         SELECT 1 FROM mx_notifications existing
                         WHERE existing.recipient_uid=u.uid AND existing.dedup_key=?3
                     ))"#,
        )?;
        let recipients = statement
            .query_map(
                params![lookup_module, lookup_actor, request.dedup_key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        recipients
    };
    let mut notifications = Vec::new();
    for (recipient_uid, level) in recipients {
        let owns = creator.as_deref() == Some(recipient_uid.as_str());
        if level != 0
            && ((record_scope != "module" && !owns)
                || (field_scope != "module" && !(field_scope == "creator" && owns)))
        {
            continue;
        }
        let mut request = request.clone();
        request.recipient_uid = recipient_uid.clone();
        if let Some(notification) = batch.insert(&transaction, &request)? {
            notifications.push((recipient_uid, notification));
        }
    }
    Ok(notifications)
}

// Keep ordering indexable. A parameter-dependent CASE in ORDER BY forces
// SQLite to visit, authorize and sort an entire account's inbox before LIMIT.
// With 100 reconnecting accounts that can occupy every read connection and
// even prevent authentication from checking its tiny auth-version row.
fn notification_page_sql(privacy_scope: &str, catch_up: bool, unread_only: bool) -> String {
    let order = if catch_up {
        "n.created_at ASC,n.uid ASC"
    } else {
        "n.created_at DESC,n.uid DESC"
    };
    let cursor = if catch_up {
        "n.created_at>=?6 AND (n.created_at>?6 OR n.uid>?7)"
    } else {
        "(?6 IS NULL OR n.created_at>?6 OR (n.created_at=?6 AND n.uid>?7))"
    };
    let unread = if unread_only {
        "n.read_at IS NULL AND n.created_at>COALESCE((SELECT through_created_at FROM mx_notification_read_state WHERE recipient_uid=?1),-1)"
    } else {
        "?3=0"
    };
    format!(
        r#"SELECT {} FROM mx_notifications n
        LEFT JOIN users u ON u.uid=n.actor_uid
        LEFT JOIN mx_notification_read_state read_state ON read_state.recipient_uid=n.recipient_uid
        WHERE n.recipient_uid=?1 AND n.created_at<?2
          AND {unread}
          AND {cursor}
          AND {privacy_scope}
          AND (n.module_uid IS NULL OR ?5=0 OR n.module_uid IN (
            SELECT role.module_uid FROM mx_module_permissions role
            JOIN mx_module_account_permissions account ON account.module_uid=role.module_uid AND account.user_uid=?1
            JOIN mx_modules module ON module.uid=role.module_uid AND module.active=1
            WHERE role.access_level=?5
              AND role.can_read=1 AND account.can_read=1))
        ORDER BY {order} LIMIT ?4"#,
        select_columns()
    )
}

fn notification_unread_sql(privacy_scope: &str) -> String {
    // An account-constant scalar boundary lets the existing unread index seek
    // past acknowledged history, rather than looking up a joined watermark
    // for every historical row. Access checks remain current, never cached.
    format!(
        r#"SELECT COUNT(*) FROM mx_notifications n
        WHERE n.recipient_uid=?1 AND n.read_at IS NULL
          AND n.created_at>COALESCE((SELECT through_created_at FROM mx_notification_read_state WHERE recipient_uid=?1),-1)
          AND {privacy_scope}
          AND (n.module_uid IS NULL OR ?2=0 OR n.module_uid IN (
            SELECT role.module_uid FROM mx_module_permissions role
            JOIN mx_module_account_permissions account ON account.module_uid=role.module_uid AND account.user_uid=?1
            JOIN mx_modules module ON module.uid=role.module_uid AND module.active=1
            WHERE role.access_level=?2
              AND role.can_read=1 AND account.can_read=1))"#
    )
}

fn notification_privacy_sql(uid: &str, access_level: i64) -> String {
    let record_scope = crate::api::mx::privacy::record_sql("nr", uid, access_level);
    let attachment_scope = crate::api::mx::privacy::field_sql("nf", "nr", uid, access_level);
    format!("(n.target_type IS NOT 'record' OR EXISTS(SELECT 1 FROM mx_records nr WHERE nr.uid=n.target_uid AND {record_scope} AND (json_extract(n.data_json,'$.attachment_field_uid') IS NULL OR EXISTS(SELECT 1 FROM mx_fields nf WHERE nf.uid=json_extract(n.data_json,'$.attachment_field_uid') AND {attachment_scope}))))")
}

pub async fn list_notifications(
    claims: Claims,
    Query(query): Query<NotificationQuery>,
) -> Response {
    let uid = claims.uid;
    let access_level = claims.access_level;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let unread_only = query.unread_only.unwrap_or(false);
    let before = query.before.unwrap_or(i64::MAX);
    let after = query.after;
    let after_uid = query.after_uid.unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_notification_schema(connection)?;
            ensure_module_schema(connection)?;
            crate::api::mx::privacy::ensure(connection)?;
            let privacy_scope=notification_privacy_sql(&uid,access_level);
            let sql = notification_page_sql(&privacy_scope,after.is_some(),unread_only);
            let mut statement = connection.prepare_cached(&sql)?;
            let rows = statement.query_map(
                params![
                    uid,
                    before,
                    if unread_only { 1 } else { 0 },
                    limit as i64,
                    access_level,after,after_uid
                ],
                notification_from_row,
            )?;
            let notifications = rows.collect::<Result<Vec<_>, _>>()?;
            let unread: i64 = connection.query_row(
                &notification_unread_sql(&privacy_scope),
                params![uid, access_level],
                |row| row.get(0),
            )?;
            Ok((notifications, unread.max(0)))
        })
    })
    .await;

    match result {
        Ok(Ok((notifications, unread))) => api_json(
            StatusCode::OK,
            json!({"notifications":notifications,"unread":unread,"server_time":chrono::Utc::now().timestamp_millis()}),
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
        with_sql_write(|connection| {
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
        with_sql_write(|connection| {
            ensure_notification_schema(connection)?;
            mark_all_read(connection, &uid, chrono::Utc::now().timestamp_millis())
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

    pub(super) fn fanout_fixture() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE users(uid TEXT PRIMARY KEY,access_level INTEGER NOT NULL,name TEXT NOT NULL,email TEXT NOT NULL);").unwrap();
        crate::db::schema::initialize(&c).unwrap();
        c.execute_batch("INSERT INTO mx_modules(uid,slug,name,singular_name,created_at,updated_at) VALUES('capacity','capacity','Capacity','Record',0,0);
            INSERT INTO mx_module_permissions(module_uid,access_level,can_read) VALUES('capacity',0,1),('capacity',2,1);
            INSERT INTO users VALUES('actor',2,'Actor','actor@test'),('admin',0,'Admin','admin@test'),('reader',2,'Reader','reader@test'),('muted',2,'Muted','muted@test'),('revoked',2,'Revoked','revoked@test');
            INSERT INTO mx_module_account_permissions(module_uid,user_uid,can_read,updated_at) VALUES('capacity','reader',1,0),('capacity','muted',1,0),('capacity','revoked',0,0);
            INSERT INTO mx_user_preferences(user_uid,preferences_json,updated_at) VALUES('muted','{\"notifications\":{\"record_created\":false}}',0);
            INSERT INTO mx_records(uid,module_uid,control_year,control_no,date,office,requestor,subject,routed_to_div,remarks) VALUES('record','capacity',0,1,'','','','','','');
            INSERT INTO mx_record_owners VALUES('record','reader','test');").unwrap();
        c
    }

    fn activity() -> NewNotification {
        NewNotification {
            recipient_uid: String::new(),
            kind: "record.created".into(),
            title: "Created".into(),
            body: "A record changed".into(),
            actor_uid: Some("actor".into()),
            target_type: Some("record".into()),
            target_uid: Some("record".into()),
            module_uid: Some("capacity".into()),
            data: json!({}),
            dedup_key: Some("test-event".into()),
            mandatory: false,
        }
    }

    fn inbox_page(
        c: &rusqlite::Connection,
        before: i64,
        after: Option<i64>,
        after_uid: &str,
        unread: bool,
    ) -> (Vec<Notification>, i32) {
        let sql = notification_page_sql(&notification_privacy_sql("reader", 2), after.is_some(), unread);
        let mut statement = c.prepare(&sql).unwrap();
        let rows = statement
            .query_map(
                params!["reader", before, unread as i64, 100, 2, after, after_uid],
                notification_from_row,
            )
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        (rows, statement.get_status(rusqlite::StatementStatus::VmStep))
    }

    #[test]
    fn inbox_page_and_unread_count_seek_past_large_acknowledged_history() {
        let c = fanout_fixture();
        c.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<20000)
            INSERT INTO mx_notifications(uid,recipient_uid,kind,title,target_type,target_uid,module_uid,created_at)
            SELECT printf('notice-%05d',x),'reader','record.created','Saved','record','record','capacity',x FROM n;").unwrap();
        let (latest, steps) = inbox_page(&c, i64::MAX, None, "", false);
        assert_eq!(latest.len(), 100);
        assert_eq!(latest[0].created_at, 20000);
        assert_eq!(latest[99].created_at, 19901);
        assert!(steps < 30000, "A page must not authorize and sort all 20,000 rows: {steps} VM steps");
        let (next, _) = inbox_page(&c, 19901, None, "", false);
        assert_eq!(next[0].created_at, 19900);
        let (catch_up, steps) = inbox_page(&c, i64::MAX, Some(19990), "notice-19990", false);
        assert_eq!(catch_up.len(), 10);
        assert_eq!(catch_up[0].created_at, 19991);
        assert_eq!(catch_up[9].created_at, 20000);
        assert!(steps < 30000, "Catch-up must seek, not rescan acknowledged history: {steps}");
        assert_eq!(mark_all_read(&c, "reader", 30000).unwrap(), 20000);
        let mut count = c.prepare(&notification_unread_sql(&notification_privacy_sql("reader", 2))).unwrap();
        assert_eq!(count.query_row(params!["reader", 2], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert!(count.get_status(rusqlite::StatementStatus::VmStep) < 1000, "An empty unread range must not scan 20,000 acknowledged rows");
        let (read, _) = inbox_page(&c, i64::MAX, None, "", false);
        assert!(read.iter().all(|n| n.read_at == Some(30000)));
        let (unread, steps) = inbox_page(&c, i64::MAX, None, "", true);
        assert!(unread.is_empty());
        assert!(steps < 1000, "An unread page must seek past acknowledged history too: {steps}");
    }

    #[test]
    fn inbox_optimization_preserves_tied_cursors_and_current_access_checks() {
        let c = fanout_fixture();
        c.execute_batch("INSERT INTO mx_notifications(uid,recipient_uid,kind,title,target_type,target_uid,module_uid,created_at)
            VALUES('a','reader','record.created','Saved','record','record','capacity',100),
                  ('b','reader','record.created','Saved','record','record','capacity',100),
                  ('c','reader','record.created','Saved','record','record','capacity',101);").unwrap();
        let ids = |rows: Vec<Notification>| rows.into_iter().map(|n| n.uid).collect::<Vec<_>>();
        assert_eq!(ids(inbox_page(&c, i64::MAX, None, "", false).0), ["c", "b", "a"]);
        assert_eq!(ids(inbox_page(&c, i64::MAX, Some(100), "a", false).0), ["b", "c"]);
        assert_eq!(ids(inbox_page(&c, 101, Some(100), "a", false).0), ["b"]);
        let unread = || c.query_row(&notification_unread_sql(&notification_privacy_sql("reader", 2)), params!["reader", 2], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(unread(), 3);
        for mutation in [
            "UPDATE mx_module_account_permissions SET can_read=0 WHERE user_uid='reader'",
            "UPDATE mx_module_account_permissions SET can_read=1 WHERE user_uid='reader'; UPDATE mx_module_permissions SET can_read=0 WHERE access_level=2",
            "UPDATE mx_module_permissions SET can_read=1 WHERE access_level=2; UPDATE mx_modules SET active=0",
            "UPDATE mx_modules SET active=1,config_json='{\"record_visibility\":\"creator\"}'; UPDATE mx_record_owners SET creator_uid='actor'",
        ] {
            c.execute_batch(mutation).unwrap();
            assert!(inbox_page(&c, i64::MAX, None, "", false).0.is_empty(), "Revocation must apply immediately: {mutation}");
            assert_eq!(unread(), 0);
        }
        c.execute_batch("UPDATE mx_record_owners SET creator_uid='reader'").unwrap();
        assert_eq!(unread(), 3);
        assert_eq!(inbox_page(&c, i64::MAX, None, "", false).0.len(), 3);
        c.execute_batch("INSERT INTO mx_fields(uid,field_key,label,field_type,module_uid,config_json)
            VALUES('restricted-file','restricted_file','Restricted file','attachments','capacity','{\"access_scope\":\"administrators\"}');
            UPDATE mx_notifications SET data_json='{\"attachment_field_uid\":\"restricted-file\"}' WHERE uid='c';").unwrap();
        assert_eq!(unread(), 2, "Restricted attachment notifications must not leak through counts");
        assert_eq!(ids(inbox_page(&c, i64::MAX, None, "", false).0), ["b", "a"]);
        c.execute_batch("UPDATE mx_fields SET config_json='{\"access_scope\":\"creator\"}' WHERE uid='restricted-file'").unwrap();
        assert_eq!(unread(), 3, "Creator attachment access is evaluated on every read");
        c.execute_batch("DELETE FROM mx_fields WHERE uid='restricted-file'").unwrap();
        assert_eq!(unread(), 2, "A missing attachment definition never grants visibility");
    }

    #[test]
    fn read_all_is_account_scoped_and_does_not_rewrite_notification_payloads() {
        let c = fanout_fixture();
        c.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<2500)
            INSERT INTO mx_notifications(uid,recipient_uid,kind,title,created_at,read_at)
            SELECT 'notice-'||x,'reader','record.created','Saved',x,CASE WHEN x=1 THEN 77 END FROM n;
            INSERT INTO mx_notifications(uid,recipient_uid,kind,title,created_at) VALUES('other','admin','record.created','Other',100);").unwrap();
        let before = c.total_changes();
        assert_eq!(mark_all_read(&c, "reader", 5000).unwrap(), 2499);
        assert_eq!(
            c.total_changes() - before,
            1,
            "Only one watermark row is written, regardless of inbox size"
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM mx_notifications WHERE read_at IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT read_at FROM mx_notifications WHERE uid='notice-1'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            77
        );
        assert_eq!(mark_all_read(&c, "reader", 6000).unwrap(), 0);
        assert_eq!(
            c.query_row(
                "SELECT read_at FROM mx_notification_read_state WHERE recipient_uid='reader'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            5000,
            "Repeated acknowledgement retains its original time"
        );
        assert_eq!(mark_all_read(&c, "admin", 6000).unwrap(), 1);
        assert_eq!(mark_all_read(&c, "muted", 6000).unwrap(), 0);
    }

    #[test]
    fn deliveries_after_read_all_remain_unread_even_with_a_clock_correction() {
        let c = fanout_fixture();
        let mut request = activity();
        request.recipient_uid = "reader".into();
        let first = insert_notification(&c, &request).unwrap().unwrap();
        assert_eq!(mark_all_read(&c, "reader", first.created_at).unwrap(), 1);
        assert_eq!(
            notification_created_at(&c, "reader", first.created_at).unwrap(),
            first.created_at + 1
        );
        assert_eq!(
            notification_created_at(&c, "reader", first.created_at - 10_000).unwrap(),
            first.created_at + 1
        );
        assert!(
            insert_notification(&c, &request).unwrap().is_none(),
            "Replay never creates a fresh unread event"
        );
        request.dedup_key = Some("next-event".into());
        let second = insert_notification(&c, &request).unwrap().unwrap();
        assert!(second.created_at > first.created_at);
        let notices = c.prepare(&format!("SELECT {} FROM mx_notifications n LEFT JOIN users u ON u.uid=n.actor_uid LEFT JOIN mx_notification_read_state read_state ON read_state.recipient_uid=n.recipient_uid WHERE n.recipient_uid='reader' ORDER BY n.created_at", select_columns())).unwrap()
            .query_map([], notification_from_row).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(notices.len(), 2);
        assert_eq!(notices[0].read_at, Some(first.created_at));
        assert_eq!(notices[1].read_at, None);
        assert_eq!(mark_all_read(&c, "reader", second.created_at).unwrap(), 1);
    }

    #[test]
    fn failed_read_all_cannot_advance_the_watermark() {
        let c = fanout_fixture();
        let mut request = activity();
        request.recipient_uid = "reader".into();
        insert_notification(&c, &request).unwrap();
        c.execute_batch("CREATE TRIGGER fail_read_receipt BEFORE INSERT ON mx_notification_read_state BEGIN SELECT RAISE(ABORT,'injected read receipt failure'); END;").unwrap();
        assert!(mark_all_read(&c, "reader", 5000).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM mx_notification_read_state", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        c.execute_batch("DROP TRIGGER fail_read_receipt;").unwrap();
        assert_eq!(mark_all_read(&c, "reader", 6000).unwrap(), 1);
    }

    #[test]
    fn module_fanout_is_atomic_private_preference_aware_and_deduplicated() {
        let c = fanout_fixture();
        let sent = insert_module_notifications(&c, &activity()).unwrap();
        let mut recipients = sent.iter().map(|(uid, _)| uid.as_str()).collect::<Vec<_>>();
        recipients.sort_unstable();
        assert_eq!(recipients, ["admin", "reader"]);
        assert!(
            insert_module_notifications(&c, &activity())
                .unwrap()
                .is_empty()
        );
        c.execute_batch(
            "DELETE FROM mx_notifications;
            UPDATE mx_modules SET config_json='{\"record_visibility\":\"creator\"}';
            UPDATE mx_record_owners SET creator_uid='muted';",
        )
        .unwrap();
        assert_eq!(
            insert_module_notifications(&c, &activity())
                .unwrap()
                .iter()
                .map(|(u, _)| u.as_str())
                .collect::<Vec<_>>(),
            ["admin"]
        );
    }

    #[test]
    fn failed_module_fanout_rolls_back_every_recipient() {
        let c = fanout_fixture();
        c.execute_batch("CREATE TRIGGER fail_notification BEFORE INSERT ON mx_notifications WHEN NEW.recipient_uid='reader' BEGIN SELECT RAISE(ABORT,'injected fanout failure'); END;").unwrap();
        assert!(insert_module_notifications(&c, &activity()).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM mx_notifications", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        c.execute_batch("DROP TRIGGER fail_notification;").unwrap();
        assert_eq!(
            insert_module_notifications(&c, &activity()).unwrap().len(),
            2
        );
    }

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
