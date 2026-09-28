use axum::{
    Json,
    extract::{Multipart, Path, Query},
    http::{
        HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use futures::future::join_all;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    api::{
        deployment::collaboration_message_page_size,
        lifecycle::ensure_record_lifecycle_schema,
        live::publish_user_event,
        modules::module_can,
        mx::handler::{
            generate_office_pdf_preview, inline_attachment_response, n1_access_token, n1_download,
            n1_ensure_directory, n1_soft_delete, n1_upload_one_shot, office_preview_supported,
        },
        mx::model::FileAttachment,
        notifications::{NewNotification, notify_user},
    },
    config::load_config::CONFIG,
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

type MessageInsertResult =
    Result<(ChatMessage, Vec<(String, String)>, Vec<String>), SqliteDatabaseError>;
type ChannelReadAdvance = (ChannelReadState, Vec<(String, String)>, bool);

#[derive(Debug, Serialize)]
struct ChannelSummary {
    uid: String,
    kind: String,
    name: String,
    description: String,
    created_by: String,
    created_at: i64,
    role: String,
    notification_level: String,
    member_count: i64,
    unread_count: i64,
    last_message_at: Option<i64>,
    direct_user_uid: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ChannelReadState {
    user_uid: String,
    user_name: String,
    last_read_at: i64,
    last_read_message_id: i64,
}

#[derive(Debug, Clone, Serialize)]
struct MessageFile {
    uid: String,
    file_name: String,
    mime_type: String,
    size: i64,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
struct MessageRecordLink {
    uid: String,
    module_uid: String,
    module_name: String,
    singular_name: String,
    label: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChatMessage {
    uid: String,
    channel_uid: String,
    sender_uid: String,
    sender_name: String,
    body: String,
    reply_to_uid: Option<String>,
    created_at: i64,
    edited_at: Option<i64>,
    deleted_at: Option<i64>,
    sequence: i64,
    files: Vec<MessageFile>,
    record_links: Vec<MessageRecordLink>,
}

#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub member_uids: Vec<String>,
    #[serde(default = "default_group_kind")]
    pub kind: String,
}

fn default_group_kind() -> String {
    "group".to_string()
}

#[derive(Debug, Deserialize)]
pub struct CreateDirectRequest {
    pub user_uid: String,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub body: String,
    pub reply_to_uid: Option<String>,
    #[serde(default)]
    pub mention_uids: Vec<String>,
    #[serde(default)]
    pub record_uids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct EditMessageRequest {
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_uid: String,
    #[serde(default = "default_member_role")]
    pub role: String,
}

fn default_member_role() -> String {
    "member".to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct MessageQuery {
    pub before: Option<i64>,
}

pub(crate) fn ensure_collaboration_schema(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    ensure_record_lifecycle_schema(connection)?;
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_channels (
            uid          TEXT PRIMARY KEY NOT NULL,
            kind         TEXT NOT NULL CHECK(kind IN ('channel','group','direct')),
            name         TEXT NOT NULL,
            description  TEXT NOT NULL DEFAULT '',
            direct_key   TEXT UNIQUE,
            created_by   TEXT NOT NULL,
            created_at   INTEGER NOT NULL,
            archived_at  INTEGER,
            FOREIGN KEY(created_by) REFERENCES users(uid) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mx_channel_members (
            channel_uid        TEXT NOT NULL,
            user_uid           TEXT NOT NULL,
            role               TEXT NOT NULL DEFAULT 'member' CHECK(role IN ('owner','admin','member')),
            notification_level TEXT NOT NULL DEFAULT 'all' CHECK(notification_level IN ('all','mentions','muted')),
            joined_at          INTEGER NOT NULL,
            last_read_at       INTEGER NOT NULL DEFAULT 0,
            last_read_message_id INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(channel_uid, user_uid),
            FOREIGN KEY(channel_uid) REFERENCES mx_channels(uid) ON DELETE CASCADE,
            FOREIGN KEY(user_uid) REFERENCES users(uid) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS mx_messages (
            uid           TEXT PRIMARY KEY NOT NULL,
            channel_uid   TEXT NOT NULL,
            sender_uid    TEXT NOT NULL,
            body          TEXT NOT NULL DEFAULT '',
            reply_to_uid  TEXT,
            created_at    INTEGER NOT NULL,
            edited_at     INTEGER,
            deleted_at    INTEGER,
            FOREIGN KEY(channel_uid) REFERENCES mx_channels(uid) ON DELETE CASCADE,
            FOREIGN KEY(sender_uid) REFERENCES users(uid) ON DELETE RESTRICT,
            FOREIGN KEY(reply_to_uid) REFERENCES mx_messages(uid) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS mx_message_files (
            uid          TEXT PRIMARY KEY NOT NULL,
            message_uid  TEXT NOT NULL,
            file_name    TEXT NOT NULL,
            mime_type    TEXT NOT NULL,
            size         INTEGER NOT NULL CHECK(size >= 0),
            object_key   TEXT NOT NULL UNIQUE,
            version_id   TEXT,
            created_at   INTEGER NOT NULL,
            deleted_at   INTEGER,
            FOREIGN KEY(message_uid) REFERENCES mx_messages(uid) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS mx_message_mentions (
            message_uid TEXT NOT NULL,
            user_uid    TEXT NOT NULL,
            PRIMARY KEY(message_uid, user_uid),
            FOREIGN KEY(message_uid) REFERENCES mx_messages(uid) ON DELETE CASCADE,
            FOREIGN KEY(user_uid) REFERENCES users(uid) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS mx_message_record_links (
            message_uid TEXT NOT NULL,
            record_uid  TEXT NOT NULL,
            PRIMARY KEY(message_uid, record_uid),
            FOREIGN KEY(message_uid) REFERENCES mx_messages(uid) ON DELETE CASCADE,
            FOREIGN KEY(record_uid) REFERENCES mx_records(uid) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_mx_channel_members_user ON mx_channel_members(user_uid, channel_uid);
        CREATE INDEX IF NOT EXISTS idx_mx_messages_channel_created ON mx_messages(channel_uid, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_mx_message_files_message ON mx_message_files(message_uid, created_at);
        "#,
    )?;

    // Databases created by earlier MX builds only have the timestamp marker.
    // Keep it for display, but use the message rowid for exact receipts so two
    // messages created in the same millisecond cannot share a read state.
    let has_exact_read_marker: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('mx_channel_members') WHERE name = 'last_read_message_id')",
        [],
        |row| row.get(0),
    )?;
    if !has_exact_read_marker {
        connection.execute(
            "ALTER TABLE mx_channel_members ADD COLUMN last_read_message_id INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    connection.execute(
        r#"
        UPDATE mx_channel_members
        SET last_read_message_id = COALESCE((
            SELECT MAX(message.rowid)
            FROM mx_messages message
            WHERE message.channel_uid = mx_channel_members.channel_uid
              AND message.created_at <= mx_channel_members.last_read_at
        ), 0)
        WHERE last_read_message_id = 0 AND last_read_at > 0
        "#,
        [],
    )?;
    Ok(())
}

pub async fn list_collaboration_people(claims: Claims) -> Response {
    let current_uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT uid,name FROM users WHERE uid<>?1 ORDER BY name COLLATE NOCASE LIMIT 1000",
            )?;
            statement
                .query_map(params![current_uid], |row| {
                    Ok(json!({"uid":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?}))
                })?
                .collect::<Result<Vec<_>, _>>()
        })
    })
    .await;
    match result {
        Ok(Ok(people)) => api_json(StatusCode::OK, json!({"people":people})),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to load collaboration people"}),
        ),
    }
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn clean_text(value: &str, maximum: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(maximum)
        .collect()
}

fn clean_message_text(value: &str, maximum: usize) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(maximum)
        .collect()
}

fn safe_component(value: &str, fallback: &str) -> String {
    let value = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let value = value.trim_matches(['.', '-', '_']).to_string();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn member_role(
    connection: &rusqlite::Connection,
    channel_uid: &str,
    user_uid: &str,
) -> rusqlite::Result<Option<String>> {
    connection
        .query_row(
            "SELECT role FROM mx_channel_members WHERE channel_uid = ?1 AND user_uid = ?2",
            params![channel_uid, user_uid],
            |row| row.get(0),
        )
        .optional()
}

fn channel_members(
    connection: &rusqlite::Connection,
    channel_uid: &str,
) -> rusqlite::Result<Vec<(String, String)>> {
    let mut statement = connection.prepare(
        "SELECT user_uid, notification_level FROM mx_channel_members WHERE channel_uid = ?1",
    )?;
    statement
        .query_map(params![channel_uid], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect()
}

fn channel_read_states(
    connection: &rusqlite::Connection,
    channel_uid: &str,
) -> rusqlite::Result<Vec<ChannelReadState>> {
    let mut statement = connection.prepare(
        r#"
        SELECT member.user_uid, user.name, member.last_read_at, member.last_read_message_id
        FROM mx_channel_members member
        JOIN users user ON user.uid = member.user_uid
        WHERE member.channel_uid = ?1
        ORDER BY user.name COLLATE NOCASE, member.user_uid
        "#,
    )?;
    statement
        .query_map(params![channel_uid], |row| {
            Ok(ChannelReadState {
                user_uid: row.get(0)?,
                user_name: row.get(1)?,
                last_read_at: row.get(2)?,
                last_read_message_id: row.get(3)?,
            })
        })?
        .collect()
}

fn advance_channel_read(
    connection: &rusqlite::Connection,
    channel_uid: &str,
    user_uid: &str,
) -> rusqlite::Result<ChannelReadAdvance> {
    ensure_collaboration_schema(connection)?;
    let transaction = connection.unchecked_transaction()?;
    let (latest_message_id, latest_message_at): (i64, i64) = transaction.query_row(
        "SELECT COALESCE(MAX(rowid), 0), COALESCE(MAX(created_at), 0) FROM mx_messages WHERE channel_uid = ?1",
        params![channel_uid],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let changed = transaction.execute(
        r#"
        UPDATE mx_channel_members
        SET last_read_at = MAX(last_read_at, ?1), last_read_message_id = ?2
        WHERE channel_uid = ?3 AND user_uid = ?4 AND last_read_message_id < ?2
        "#,
        params![latest_message_at, latest_message_id, channel_uid, user_uid],
    )? > 0;
    let state = transaction.query_row(
        r#"
        SELECT member.user_uid, user.name, member.last_read_at, member.last_read_message_id
        FROM mx_channel_members member
        JOIN users user ON user.uid = member.user_uid
        WHERE member.channel_uid = ?1 AND member.user_uid = ?2
        "#,
        params![channel_uid, user_uid],
        |row| {
            Ok(ChannelReadState {
                user_uid: row.get(0)?,
                user_name: row.get(1)?,
                last_read_at: row.get(2)?,
                last_read_message_id: row.get(3)?,
            })
        },
    )?;
    let members = if changed {
        channel_members(&transaction, channel_uid)?
    } else {
        Vec::new()
    };
    transaction.commit()?;
    Ok((state, members, changed))
}

fn load_files(
    connection: &rusqlite::Connection,
    message_uid: &str,
) -> rusqlite::Result<Vec<MessageFile>> {
    let mut statement = connection.prepare(
        "SELECT uid, file_name, mime_type, size, created_at FROM mx_message_files WHERE message_uid = ?1 AND deleted_at IS NULL ORDER BY created_at",
    )?;
    statement
        .query_map(params![message_uid], |row| {
            Ok(MessageFile {
                uid: row.get(0)?,
                file_name: row.get(1)?,
                mime_type: row.get(2)?,
                size: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect()
}

fn load_record_links(
    connection: &rusqlite::Connection,
    message_uid: &str,
    access_level: i64,
) -> rusqlite::Result<Vec<MessageRecordLink>> {
    let mut statement = connection.prepare(
        r#"
        SELECT
            record.uid,
            record.module_uid,
            module.name,
            module.singular_name,
            COALESCE((
                SELECT CASE field.field_type
                    WHEN 'integer' THEN CAST(value.value_integer AS TEXT)
                    WHEN 'auto_number' THEN CAST(value.value_integer AS TEXT)
                    WHEN 'decimal' THEN CAST(value.value_real AS TEXT)
                    WHEN 'boolean' THEN CASE value.value_boolean WHEN 1 THEN 'Yes' ELSE 'No' END
                    ELSE NULLIF(TRIM(value.value_text), '')
                END
                FROM mx_record_values value
                JOIN mx_fields field ON field.uid = value.field_uid
                WHERE value.record_uid = record.uid
                  AND field.active = 1
                  AND field.field_type <> 'attachments'
                ORDER BY field.position, field.rowid
                LIMIT 1
            ), record.uid)
        FROM mx_message_record_links link
        JOIN mx_records record ON record.uid = link.record_uid
        JOIN mx_modules module ON module.uid = record.module_uid
        WHERE link.message_uid = ?1
        ORDER BY module.name COLLATE NOCASE, record.rowid
        "#,
    )?;
    let candidates = statement
        .query_map(params![message_uid], |row| {
            Ok(MessageRecordLink {
                uid: row.get(0)?,
                module_uid: row.get(1)?,
                module_name: row.get(2)?,
                singular_name: row.get(3)?,
                label: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    candidates
        .into_iter()
        .filter_map(|record| {
            match module_can(connection, &record.module_uid, access_level, "read") {
                Ok(true) => Some(Ok(record)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect()
}

fn load_message(
    connection: &rusqlite::Connection,
    uid: &str,
    access_level: i64,
) -> rusqlite::Result<ChatMessage> {
    let mut message = connection.query_row(
        r#"
        SELECT m.uid, m.channel_uid, m.sender_uid, u.name, m.body, m.reply_to_uid,
               m.created_at, m.edited_at, m.deleted_at, m.rowid
        FROM mx_messages m JOIN users u ON u.uid = m.sender_uid WHERE m.uid = ?1
        "#,
        params![uid],
        |row| {
            Ok(ChatMessage {
                uid: row.get(0)?,
                channel_uid: row.get(1)?,
                sender_uid: row.get(2)?,
                sender_name: row.get(3)?,
                body: row.get(4)?,
                reply_to_uid: row.get(5)?,
                created_at: row.get(6)?,
                edited_at: row.get(7)?,
                deleted_at: row.get(8)?,
                sequence: row.get(9)?,
                files: Vec::new(),
                record_links: Vec::new(),
            })
        },
    )?;
    if message.deleted_at.is_some() {
        message.body.clear();
    } else {
        message.files = load_files(connection, uid)?;
        message.record_links = load_record_links(connection, uid, access_level)?;
    }
    Ok(message)
}

fn tombstone_message(
    connection: &rusqlite::Connection,
    message_uid: &str,
    deleted_at: i64,
) -> rusqlite::Result<usize> {
    connection.execute(
        "UPDATE mx_messages SET deleted_at = ?1 WHERE uid = ?2 AND deleted_at IS NULL",
        params![deleted_at, message_uid],
    )
}

pub async fn list_channels(claims: Claims) -> Response {
    let uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            let mut statement = connection.prepare(
                r#"
                SELECT c.uid, c.kind,
                       CASE WHEN c.kind = 'direct' THEN COALESCE(other.name, c.name) ELSE c.name END,
                       c.description, c.created_by, c.created_at, mine.role, mine.notification_level,
                       (SELECT COUNT(*) FROM mx_channel_members count_members WHERE count_members.channel_uid = c.uid),
                       (SELECT COUNT(*) FROM mx_messages unread WHERE unread.channel_uid = c.uid AND unread.rowid > mine.last_read_message_id AND unread.sender_uid <> ?1),
                       (SELECT MAX(created_at) FROM mx_messages latest WHERE latest.channel_uid = c.uid),
                       other.uid
                FROM mx_channel_members mine
                JOIN mx_channels c ON c.uid = mine.channel_uid
                LEFT JOIN mx_channel_members other_member ON c.kind = 'direct' AND other_member.channel_uid = c.uid AND other_member.user_uid <> ?1
                LEFT JOIN users other ON other.uid = other_member.user_uid
                WHERE mine.user_uid = ?1 AND c.archived_at IS NULL
                ORDER BY COALESCE((SELECT MAX(created_at) FROM mx_messages latest WHERE latest.channel_uid = c.uid), c.created_at) DESC
                "#,
            )?;
            statement.query_map(params![uid], |row| Ok(ChannelSummary {
                uid: row.get(0)?, kind: row.get(1)?, name: row.get(2)?, description: row.get(3)?,
                created_by: row.get(4)?, created_at: row.get(5)?, role: row.get(6)?, notification_level: row.get(7)?,
                member_count: row.get(8)?, unread_count: row.get(9)?, last_message_at: row.get(10)?,
                direct_user_uid: row.get(11)?,
            }))?.collect::<Result<Vec<_>, _>>()
        })
    }).await;
    match result {
        Ok(Ok(channels)) => api_json(StatusCode::OK, json!({"channels":channels})),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to load collaboration channels"}),
        ),
    }
}

pub async fn create_channel(claims: Claims, Json(request): Json<CreateChannelRequest>) -> Response {
    let name = clean_text(&request.name, 80);
    let description = clean_text(&request.description, 500);
    if name.is_empty() {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"channel name is required"}),
        );
    }
    if !matches!(request.kind.as_str(), "channel" | "group") {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"channel kind must be channel or group"}),
        );
    }
    let owner_uid = claims.uid.clone();
    let actor_uid = claims.uid;
    let channel_uid = Uuid::new_v4().to_string();
    let event_channel_uid = channel_uid.clone();
    let kind = request.kind;
    let member_uids = request.member_uids;
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<String>, SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            let transaction = connection.unchecked_transaction()?;
            let now = chrono::Utc::now().timestamp_millis();
            transaction.execute(
                "INSERT INTO mx_channels(uid, kind, name, description, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![channel_uid, kind, name, description, owner_uid, now],
            )?;
            transaction.execute(
                "INSERT INTO mx_channel_members(channel_uid, user_uid, role, joined_at) VALUES (?1, ?2, 'owner', ?3)",
                params![channel_uid, owner_uid, now],
            )?;
            let mut members = vec![owner_uid.clone()];
            for member_uid in member_uids {
                if member_uid == owner_uid || member_uid.trim().is_empty() { continue; }
                let exists: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE uid = ?1)", params![member_uid], |row| row.get(0))?;
                if !exists { continue; }
                transaction.execute(
                    "INSERT OR IGNORE INTO mx_channel_members(channel_uid, user_uid, role, joined_at) VALUES (?1, ?2, 'member', ?3)",
                    params![channel_uid, member_uid, now],
                )?;
                members.push(member_uid);
            }
            transaction.commit()?;
            Ok(members)
        })
    }).await;
    match result {
        Ok(Ok(members)) => {
            for member in &members {
                publish_user_event(
                    member,
                    "channel.created",
                    Some(&actor_uid),
                    json!({"channel_uid":event_channel_uid}),
                );
            }
            api_json(
                StatusCode::CREATED,
                json!({"response":"channel created","uid":event_channel_uid}),
            )
        }
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to create channel"}),
        ),
    }
}

pub async fn create_direct_channel(
    claims: Claims,
    Json(request): Json<CreateDirectRequest>,
) -> Response {
    let other_uid = request.user_uid.trim().to_string();
    if other_uid.is_empty() || other_uid == claims.uid {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"select another account"}),
        );
    }
    let mut pair = [claims.uid.clone(), other_uid.clone()];
    pair.sort();
    let direct_key = format!("{}:{}", pair[0], pair[1]);
    let actor_uid = claims.uid;
    let db_other_uid = other_uid.clone();
    let db_actor_uid = actor_uid.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<(String, bool), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            let other_exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE uid = ?1)", params![db_other_uid], |row| row.get(0))?;
            if !other_exists { return Err(rusqlite::Error::QueryReturnedNoRows); }
            if let Some(uid) = connection.query_row("SELECT uid FROM mx_channels WHERE direct_key = ?1", params![direct_key], |row| row.get::<_, String>(0)).optional()? {
                return Ok((uid, false));
            }
            let transaction = connection.unchecked_transaction()?;
            let uid = Uuid::new_v4().to_string();
            let now = chrono::Utc::now().timestamp_millis();
            transaction.execute(
                "INSERT INTO mx_channels(uid, kind, name, direct_key, created_by, created_at) VALUES (?1, 'direct', 'Direct message', ?2, ?3, ?4)",
                params![uid, direct_key, db_actor_uid, now],
            )?;
            for user_uid in [db_actor_uid.as_str(), db_other_uid.as_str()] {
                transaction.execute("INSERT INTO mx_channel_members(channel_uid, user_uid, role, joined_at) VALUES (?1, ?2, 'member', ?3)", params![uid, user_uid, now])?;
            }
            transaction.commit()?;
            Ok((uid, true))
        })
    }).await;
    match result {
        Ok(Ok((uid, created))) => {
            publish_user_event(
                &other_uid,
                "channel.created",
                Some(&actor_uid),
                json!({"channel_uid":uid,"kind":"direct"}),
            );
            api_json(
                if created {
                    StatusCode::CREATED
                } else {
                    StatusCode::OK
                },
                json!({"uid":uid,"created":created}),
            )
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"account was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to open direct conversation"}),
        ),
    }
}

pub async fn list_messages(
    claims: Claims,
    Path(channel_uid): Path<String>,
    Query(query): Query<MessageQuery>,
) -> Response {
    let user_uid = claims.uid;
    let access_level = claims.access_level;
    let before = query.before.unwrap_or(i64::MAX);
    let result = tokio::task::spawn_blocking(move || -> Result<(Vec<ChatMessage>, Vec<ChannelReadState>, bool, usize), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            if member_role(connection, &channel_uid, &user_uid)?.is_none() { return Err(rusqlite::Error::QueryReturnedNoRows); }
            let page_size = collaboration_message_page_size(connection)?;
            let mut statement = connection.prepare(
                "SELECT uid FROM mx_messages WHERE channel_uid = ?1 AND rowid < ?2 ORDER BY rowid DESC LIMIT ?3",
            )?;
            let mut ids = statement.query_map(params![channel_uid, before, page_size.saturating_add(1) as i64], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
            let has_more = ids.len() > page_size;
            ids.truncate(page_size);
            let mut messages = ids.iter().map(|uid| load_message(connection, uid, access_level)).collect::<Result<Vec<_>, _>>()?;
            messages.reverse();
            let read_states = channel_read_states(connection, &channel_uid)?;
            Ok((messages, read_states, has_more, page_size))
        })
    }).await;
    match result {
        Ok(Ok((messages, read_states, has_more, page_size))) => api_json(
            StatusCode::OK,
            json!({"messages":messages,"read_states":read_states,"has_more":has_more,"page_size":page_size}),
        ),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"you are not a member of this channel"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to load messages"}),
        ),
    }
}

pub async fn send_message(
    claims: Claims,
    Path(channel_uid): Path<String>,
    Json(request): Json<SendMessageRequest>,
) -> Response {
    let body = clean_message_text(&request.body, 20_000);
    if body.is_empty() {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"message cannot be empty"}),
        );
    }
    let sender_uid = claims.uid.clone();
    let actor_uid = claims.uid;
    let access_level = claims.access_level;
    let message_uid = Uuid::new_v4().to_string();
    let event_message_uid = message_uid.clone();
    let reply_to = request.reply_to_uid;
    let mention_uids = request.mention_uids;
    let record_uids = request.record_uids;
    let result = tokio::task::spawn_blocking(move || -> MessageInsertResult {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            if member_role(connection, &channel_uid, &sender_uid)?.is_none() { return Err(rusqlite::Error::QueryReturnedNoRows); }
            if let Some(reply_uid) = reply_to.as_deref() {
                let valid: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM mx_messages WHERE uid = ?1 AND channel_uid = ?2)", params![reply_uid, channel_uid], |row| row.get(0))?;
                if !valid { return Err(rusqlite::Error::InvalidParameterName("reply does not belong to this channel".to_string())); }
            }
            let transaction = connection.unchecked_transaction()?;
            let now = chrono::Utc::now().timestamp_millis();
            transaction.execute(
                "INSERT INTO mx_messages(uid, channel_uid, sender_uid, body, reply_to_uid, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![message_uid, channel_uid, sender_uid, body, reply_to, now],
            )?;
            let members = channel_members(&transaction, &channel_uid)?;
            let mut mentions = Vec::new();
            for mentioned_uid in mention_uids {
                if mentioned_uid == sender_uid || !members.iter().any(|(uid, _)| uid == &mentioned_uid) { continue; }
                transaction.execute("INSERT OR IGNORE INTO mx_message_mentions(message_uid, user_uid) VALUES (?1, ?2)", params![message_uid, mentioned_uid])?;
                mentions.push(mentioned_uid);
            }
            for record_uid in record_uids {
                let module_uid = transaction.query_row(
                    "SELECT module_uid FROM mx_records WHERE uid = ?1 AND deleted_at IS NULL",
                    params![record_uid],
                    |row| row.get::<_, String>(0),
                ).optional()?;
                let Some(module_uid) = module_uid else {
                    return Err(rusqlite::Error::InvalidParameterName("a linked record was not found".to_string()));
                };
                if !module_can(&transaction, &module_uid, access_level, "read")? {
                    return Err(rusqlite::Error::InvalidParameterName("you cannot share a record you cannot access".to_string()));
                }
                transaction.execute("INSERT OR IGNORE INTO mx_message_record_links(message_uid, record_uid) VALUES (?1, ?2)", params![message_uid, record_uid])?;
            }
            transaction.commit()?;
            let message = load_message(connection, &message_uid, access_level)?;
            Ok((message, members, mentions))
        })
    }).await;
    match result {
        Ok(Ok((message, members, mentions))) => {
            let message_json = serde_json::to_value(&message).unwrap_or_default();
            for (member_uid, _) in &members {
                publish_user_event(
                    member_uid,
                    "message.created",
                    Some(&actor_uid),
                    message_json.clone(),
                );
            }
            let mut deliveries = Vec::new();
            for (member_uid, level) in members {
                if member_uid == actor_uid || level == "muted" {
                    continue;
                }
                let mentioned = mentions.contains(&member_uid);
                if level == "mentions" && !mentioned {
                    continue;
                }
                deliveries.push(notify_user(NewNotification {
                    recipient_uid: member_uid,
                    kind: if mentioned {
                        "mention.created".to_string()
                    } else {
                        "message.created".to_string()
                    },
                    title: if mentioned {
                        format!("{} mentioned you", message.sender_name)
                    } else {
                        format!("New message from {}", message.sender_name)
                    },
                    body: message.body.chars().take(240).collect(),
                    actor_uid: Some(actor_uid.clone()),
                    target_type: Some("channel".to_string()),
                    target_uid: Some(message.channel_uid.clone()),
                    module_uid: None,
                    data: json!({"message_uid":message.uid,"channel_uid":message.channel_uid}),
                    dedup_key: Some(format!("message:{}:{}", message.uid, mentioned)),
                    mandatory: false,
                }));
            }
            let _ = join_all(deliveries).await;
            api_json(
                StatusCode::CREATED,
                json!({"message":message,"uid":event_message_uid}),
            )
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"you are not a member of this channel"}),
        ),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message)))) => {
            api_json(StatusCode::BAD_REQUEST, json!({"response":message}))
        }
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to send message"}),
        ),
    }
}

pub async fn edit_message(
    claims: Claims,
    Path(message_uid): Path<String>,
    Json(request): Json<EditMessageRequest>,
) -> Response {
    let body = clean_message_text(&request.body, 20_000);
    if body.is_empty() {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"message cannot be empty"}),
        );
    }
    let actor_uid = claims.uid.clone();
    let event_actor_uid = claims.uid;
    let access_level = claims.access_level;
    let result = tokio::task::spawn_blocking(move || -> Result<(ChatMessage, Vec<(String, String)>), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            let (channel_uid, sender_uid): (String, String) = connection.query_row("SELECT channel_uid, sender_uid FROM mx_messages WHERE uid = ?1 AND deleted_at IS NULL", params![message_uid], |row| Ok((row.get(0)?, row.get(1)?)))?;
            member_role(connection, &channel_uid, &actor_uid)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            if sender_uid != actor_uid { return Err(rusqlite::Error::InvalidParameterName("only the sender may edit this message".to_string())); }
            connection.execute("UPDATE mx_messages SET body = ?1, edited_at = ?2 WHERE uid = ?3", params![body, chrono::Utc::now().timestamp_millis(), message_uid])?;
            Ok((load_message(connection, &message_uid, access_level)?, channel_members(connection, &channel_uid)?))
        })
    }).await;
    match result {
        Ok(Ok((message, members))) => {
            let payload = serde_json::to_value(&message).unwrap_or_default();
            for (uid, _) in members {
                publish_user_event(
                    &uid,
                    "message.updated",
                    Some(&event_actor_uid),
                    payload.clone(),
                );
            }
            api_json(StatusCode::OK, json!({"message":message}))
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message)))) => {
            api_json(StatusCode::FORBIDDEN, json!({"response":message}))
        }
        _ => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"message was not found"}),
        ),
    }
}

pub async fn delete_message(claims: Claims, Path(message_uid): Path<String>) -> Response {
    let actor_uid = claims.uid.clone();
    let event_actor_uid = claims.uid;
    let db_message_uid = message_uid.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<(String, Vec<(String, String)>), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            let (channel_uid, sender_uid): (String, String) = connection.query_row("SELECT channel_uid, sender_uid FROM mx_messages WHERE uid = ?1 AND deleted_at IS NULL", params![db_message_uid], |row| Ok((row.get(0)?, row.get(1)?)))?;
            let role = member_role(connection, &channel_uid, &actor_uid)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            if sender_uid != actor_uid && !matches!(role.as_str(), "owner" | "admin") { return Err(rusqlite::Error::InvalidParameterName("only the sender or a channel moderator may delete this message".to_string())); }
            tombstone_message(connection, &db_message_uid, chrono::Utc::now().timestamp_millis())?;
            Ok((channel_uid.clone(), channel_members(connection, &channel_uid)?))
        })
    }).await;
    match result {
        Ok(Ok((channel_uid, members))) => {
            for (uid, _) in members {
                publish_user_event(
                    &uid,
                    "message.deleted",
                    Some(&event_actor_uid),
                    json!({"uid":message_uid,"channel_uid":channel_uid}),
                );
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message)))) => {
            api_json(StatusCode::FORBIDDEN, json!({"response":message}))
        }
        _ => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"message was not found"}),
        ),
    }
}

pub async fn mark_channel_read(claims: Claims, Path(channel_uid): Path<String>) -> Response {
    let uid = claims.uid;
    let actor_uid = uid.clone();
    let event_channel_uid = channel_uid.clone();
    let result =
        tokio::task::spawn_blocking(move || -> Result<ChannelReadAdvance, SqliteDatabaseError> {
            with_sql_connection(|connection| advance_channel_read(connection, &channel_uid, &uid))
        })
        .await;
    match result {
        Ok(Ok((state, members, changed))) => {
            if changed {
                let payload = json!({"channel_uid":event_channel_uid,"read_state":state.clone()});
                for (member_uid, _) in members {
                    publish_user_event(
                        &member_uid,
                        "channel.read",
                        Some(&actor_uid),
                        payload.clone(),
                    );
                }
            }
            api_json(StatusCode::OK, json!({"read_state":state}))
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"channel was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to update read state"}),
        ),
    }
}

pub async fn add_channel_member(
    claims: Claims,
    Path(channel_uid): Path<String>,
    Json(request): Json<AddMemberRequest>,
) -> Response {
    if !matches!(request.role.as_str(), "admin" | "member") {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"role must be admin or member"}),
        );
    }
    let actor_uid = claims.uid.clone();
    let event_actor_uid = claims.uid;
    let user_uid = request.user_uid;
    let event_user_uid = user_uid.clone();
    let channel_for_event = channel_uid.clone();
    let role_value = request.role;
    let result = tokio::task::spawn_blocking(move || -> Result<(), SqliteDatabaseError> { with_sql_connection(|connection| {
        ensure_collaboration_schema(connection)?;
        let actor_role = member_role(connection, &channel_uid, &actor_uid)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        if !matches!(actor_role.as_str(), "owner" | "admin") { return Err(rusqlite::Error::InvalidParameterName("channel administrator access is required".to_string())); }
        let kind: String = connection.query_row("SELECT kind FROM mx_channels WHERE uid = ?1", params![channel_uid], |row| row.get(0))?;
        if kind == "direct" { return Err(rusqlite::Error::InvalidParameterName("direct conversations cannot add members".to_string())); }
        connection.execute("INSERT INTO mx_channel_members(channel_uid, user_uid, role, joined_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(channel_uid, user_uid) DO UPDATE SET role = excluded.role", params![channel_uid, user_uid, role_value, chrono::Utc::now().timestamp_millis()])?;
        Ok(())
    })}).await;
    match result {
        Ok(Ok(())) => {
            publish_user_event(
                &event_user_uid,
                "channel.member_added",
                Some(&event_actor_uid),
                json!({"channel_uid":channel_for_event}),
            );
            let _ = notify_user(NewNotification {
                recipient_uid: event_user_uid,
                kind: "channel.member_added".to_string(),
                title: "Added to a collaboration channel".to_string(),
                body: "Open MX Collaboration to view the conversation.".to_string(),
                actor_uid: Some(event_actor_uid),
                target_type: Some("channel".to_string()),
                target_uid: Some(channel_for_event),
                module_uid: None,
                data: json!({}),
                dedup_key: None,
                mandatory: false,
            })
            .await;
            api_json(StatusCode::OK, json!({"response":"channel member saved"}))
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message)))) => {
            api_json(StatusCode::FORBIDDEN, json!({"response":message}))
        }
        _ => api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"failed to add channel member"}),
        ),
    }
}

pub async fn upload_message_file(
    claims: Claims,
    Path(message_uid): Path<String>,
    mut multipart: Multipart,
) -> Response {
    let actor_uid = claims.uid.clone();
    let metadata = tokio::task::spawn_blocking({
        let message_uid = message_uid.clone();
        let actor_uid = actor_uid.clone();
        move || -> Result<(String, Vec<(String, String)>), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_collaboration_schema(connection)?;
                let channel_uid: String = connection.query_row(
                    "SELECT channel_uid FROM mx_messages WHERE uid = ?1 AND deleted_at IS NULL",
                    params![message_uid],
                    |row| row.get(0),
                )?;
                if member_role(connection, &channel_uid, &actor_uid)?.is_none() {
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                }
                Ok((
                    channel_uid.clone(),
                    channel_members(connection, &channel_uid)?,
                ))
            })
        }
    })
    .await;
    let (channel_uid, members) = match metadata {
        Ok(Ok(value)) => value,
        _ => {
            return api_json(
                StatusCode::FORBIDDEN,
                json!({"response":"message or channel access was not found"}),
            );
        }
    };
    let field = match multipart.next_field().await {
        Ok(Some(field)) => field,
        _ => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"select a file to share"}),
            );
        }
    };
    let file_name = safe_component(
        field.file_name().unwrap_or("shared-file.bin"),
        "shared-file.bin",
    );
    let mime_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = match field.bytes().await {
        Ok(bytes) => bytes,
        Err(_) => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"could not read shared file"}),
            );
        }
    };
    let maximum = CONFIG
        .n1
        .attachment_max_size_mb
        .max(1)
        .saturating_mul(1024 * 1024);
    if bytes.len() > maximum {
        return api_json(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({"response":format!("shared files may not exceed {} MiB", CONFIG.n1.attachment_max_size_mb)}),
        );
    }
    let file_uid = Uuid::new_v4().to_string();
    let file_size = bytes.len() as i64;
    let object_key = format!(
        "__mx/collaboration/{}/{}/{}__{}",
        safe_component(&channel_uid, "channel"),
        safe_component(&message_uid, "message"),
        file_uid,
        file_name
    );
    let token = match n1_access_token().await {
        Ok(token) => token,
        Err(_) => {
            return api_json(
                StatusCode::BAD_GATEWAY,
                json!({"response":"N1 storage is unavailable"}),
            );
        }
    };
    for directory in [
        "__mx".to_string(),
        "__mx/collaboration".to_string(),
        format!(
            "__mx/collaboration/{}",
            safe_component(&channel_uid, "channel")
        ),
        format!(
            "__mx/collaboration/{}/{}",
            safe_component(&channel_uid, "channel"),
            safe_component(&message_uid, "message")
        ),
    ] {
        if n1_ensure_directory(&directory, &token).await.is_err() {
            return api_json(
                StatusCode::BAD_GATEWAY,
                json!({"response":"N1 could not prepare collaboration storage"}),
            );
        }
    }
    if n1_upload_one_shot(&object_key, &mime_type, bytes.clone().to_vec(), &token)
        .await
        .is_err()
    {
        return api_json(
            StatusCode::BAD_GATEWAY,
            json!({"response":"N1 could not store the shared file"}),
        );
    }
    let db_file_uid = file_uid.clone();
    let db_file_name = file_name.clone();
    let db_mime = mime_type.clone();
    let db_object_key = object_key.clone();
    let db_message_uid = message_uid.clone();
    let inserted = tokio::task::spawn_blocking(move || with_sql_connection(|connection| {
        ensure_collaboration_schema(connection)?;
        let now = chrono::Utc::now().timestamp_millis();
        connection.execute("INSERT INTO mx_message_files(uid, message_uid, file_name, mime_type, size, object_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![db_file_uid, db_message_uid, db_file_name, db_mime, file_size, db_object_key, now])?;
        Ok(now)
    })).await;
    let created_at = match inserted {
        Ok(Ok(value)) => value,
        _ => {
            let _ = n1_soft_delete(&object_key).await;
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to save shared file metadata"}),
            );
        }
    };
    let file = MessageFile {
        uid: file_uid.clone(),
        file_name: file_name.clone(),
        mime_type: mime_type.clone(),
        size: file_size,
        created_at,
    };
    let payload = serde_json::to_value(&file).unwrap_or_default();
    for (member_uid, _) in &members {
        publish_user_event(
            member_uid,
            "file.shared",
            Some(&actor_uid),
            json!({"message_uid":message_uid,"channel_uid":channel_uid,"file":payload}),
        );
    }
    let deliveries = members
        .into_iter()
        .filter(|(uid, level)| uid != &actor_uid && level != "muted")
        .map(|(uid, _)| {
            notify_user(NewNotification {
                recipient_uid: uid,
                kind: "file.shared".to_string(),
                title: "A file was shared".to_string(),
                body: file_name.clone(),
                actor_uid: Some(actor_uid.clone()),
                target_type: Some("channel".to_string()),
                target_uid: Some(channel_uid.clone()),
                module_uid: None,
                data: json!({"message_uid":message_uid,"file_uid":file_uid}),
                dedup_key: Some(format!("file:{file_uid}")),
                mandatory: false,
            })
        });
    let _ = join_all(deliveries).await;
    api_json(StatusCode::CREATED, json!({"file":file}))
}

pub async fn download_message_file(claims: Claims, Path(file_uid): Path<String>) -> Response {
    let user_uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || -> Result<(String, String, String), SqliteDatabaseError> { with_sql_connection(|connection| {
        ensure_collaboration_schema(connection)?;
        let value = connection.query_row(
            "SELECT f.object_key, f.file_name, f.mime_type, m.channel_uid FROM mx_message_files f JOIN mx_messages m ON m.uid = f.message_uid WHERE f.uid = ?1 AND f.deleted_at IS NULL",
            params![file_uid], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
        )?;
        if member_role(connection, &value.3, &user_uid)?.is_none() { return Err(rusqlite::Error::QueryReturnedNoRows); }
        Ok((value.0, value.1, value.2))
    })}).await;
    let (object_key, file_name, mime_type) = match result {
        Ok(Ok(value)) => value,
        _ => {
            return api_json(
                StatusCode::NOT_FOUND,
                json!({"response":"shared file was not found"}),
            );
        }
    };
    let bytes = match n1_download(&object_key, &file_name).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return api_json(
                StatusCode::BAD_GATEWAY,
                json!({"response":"N1 could not return the shared file"}),
            );
        }
    };
    let safe_name = file_name.replace('"', "_");
    let mut response = Response::new(axum::body::Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{safe_name}\"")) {
        response.headers_mut().insert(CONTENT_DISPOSITION, value);
    }
    response
}

pub async fn preview_message_file(claims: Claims, Path(file_uid): Path<String>) -> Response {
    let user_uid = claims.uid;
    let query_file_uid = file_uid.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<(String, String, String), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            let value = connection.query_row(
                "SELECT f.object_key, f.file_name, f.mime_type, m.channel_uid FROM mx_message_files f JOIN mx_messages m ON m.uid = f.message_uid WHERE f.uid = ?1 AND f.deleted_at IS NULL",
                params![query_file_uid],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
            )?;
            if member_role(connection, &value.3, &user_uid)?.is_none() {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            Ok((value.0, value.1, value.2))
        })
    })
    .await;
    let (object_key, file_name, mime_type) = match result {
        Ok(Ok(value)) => value,
        _ => {
            return api_json(
                StatusCode::NOT_FOUND,
                json!({"response":"shared file was not found"}),
            );
        }
    };
    let bytes = match n1_download(&object_key, &file_name).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return api_json(
                StatusCode::BAD_GATEWAY,
                json!({"response":"N1 could not return the shared file"}),
            );
        }
    };

    if office_preview_supported(&file_name) {
        let attachment = FileAttachment {
            uid: file_uid,
            file_name: file_name.clone(),
            mime_type: mime_type.clone(),
            size: bytes.len() as u64,
            object_key,
            version_id: None,
            attachment_field_uid: None,
            attachment_field_label: None,
            attachment_field_storage_name: None,
        };
        let preview =
            tokio::task::spawn_blocking(move || generate_office_pdf_preview(attachment, bytes))
                .await;
        let pdf_bytes = match preview {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(error)) => {
                crate::report_error!(error, "preview", "preview_message_file()");
                return api_json(
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({"response":"MX could not generate the Office preview. Verify that LibreOffice Writer, Calc, and Impress are installed on the MX server."}),
                );
            }
            Err(error) => {
                crate::report_error!(
                    format!("Collaboration preview blocking task failed: {error}"),
                    "preview",
                    "preview_message_file()"
                );
                return api_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"response":"MX Office preview task failed."}),
                );
            }
        };
        let preview_name = file_name
            .rsplit_once('.')
            .map(|(stem, _)| format!("{stem}.pdf"))
            .unwrap_or_else(|| format!("{file_name}.pdf"));
        return inline_attachment_response(pdf_bytes, "application/pdf", &preview_name);
    }

    inline_attachment_response(bytes, &mime_type, &file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_cleanup_preserves_line_breaks_but_rejects_other_controls() {
        let cleaned = clean_message_text(
            "  Email: admin@example.test\r\nPassword:\tsecret\u{0000}\rNext line  ",
            20_000,
        );

        assert_eq!(
            cleaned,
            "Email: admin@example.test\nPassword:\tsecret\nNext line"
        );
        assert!(!cleaned.contains('\0'));
    }

    #[test]
    fn read_markers_migrate_and_distinguish_same_millisecond_messages() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE users (uid TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
                CREATE TABLE mx_channel_members (
                    channel_uid TEXT NOT NULL,
                    user_uid TEXT NOT NULL,
                    role TEXT NOT NULL DEFAULT 'member',
                    notification_level TEXT NOT NULL DEFAULT 'all',
                    joined_at INTEGER NOT NULL,
                    last_read_at INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY(channel_uid, user_uid)
                );
                "#,
            )
            .unwrap();

        ensure_collaboration_schema(&connection).unwrap();
        let exact_marker_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('mx_channel_members') WHERE name = 'last_read_message_id')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exact_marker_exists);

        connection
            .execute_batch(
                r#"
                INSERT INTO users(uid,name) VALUES ('sender','Sender'),('reader','Reader');
                INSERT INTO mx_channels(uid,kind,name,description,created_by,created_at)
                VALUES ('channel','direct','Direct message','','sender',1000);
                INSERT INTO mx_channel_members(channel_uid,user_uid,role,notification_level,joined_at,last_read_at)
                VALUES ('channel','sender','member','all',1000,0),('channel','reader','member','all',1000,0);
                INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,created_at)
                VALUES ('message-1','channel','sender','First',2000),('message-2','channel','sender','Second',2000);
                "#,
            )
            .unwrap();

        let (state, members, changed) =
            advance_channel_read(&connection, "channel", "reader").unwrap();
        assert!(changed);
        assert_eq!(members.len(), 2);
        assert_eq!(state.last_read_message_id, 2);
        assert_eq!(state.last_read_at, 2000);

        let (_, members, changed) = advance_channel_read(&connection, "channel", "reader").unwrap();
        assert!(!changed);
        assert!(members.is_empty());

        connection
            .execute(
                "INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,created_at) VALUES ('message-3','channel','sender','Third',2000)",
                [],
            )
            .unwrap();
        let unread: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM mx_messages WHERE channel_uid='channel' AND rowid > ?1 AND sender_uid <> 'reader'",
                params![state.last_read_message_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unread, 1);
    }

    #[test]
    fn deleting_a_message_retains_its_content_but_exposes_only_a_tombstone() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE users (uid TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
                INSERT INTO users(uid,name) VALUES ('sender','Sender');
                "#,
            )
            .unwrap();
        ensure_collaboration_schema(&connection).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO mx_channels(uid,kind,name,description,created_by,created_at)
                VALUES ('channel','direct','Direct message','','sender',1000);
                INSERT INTO mx_channel_members(channel_uid,user_uid,role,notification_level,joined_at)
                VALUES ('channel','sender','member','all',1000);
                INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,created_at)
                VALUES ('message','channel','sender','Retained for audit',2000);
                "#,
            )
            .unwrap();

        assert_eq!(tombstone_message(&connection, "message", 3000).unwrap(), 1);
        assert_eq!(tombstone_message(&connection, "message", 4000).unwrap(), 0);
        let stored_body: String = connection
            .query_row(
                "SELECT body FROM mx_messages WHERE uid='message'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_body, "Retained for audit");

        let exposed = load_message(&connection, "message", 0).unwrap();
        assert!(exposed.body.is_empty());
        assert_eq!(exposed.deleted_at, Some(3000));
        assert!(exposed.files.is_empty());
        assert!(exposed.record_links.is_empty());
    }
}
