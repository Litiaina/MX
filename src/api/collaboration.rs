use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path, Query},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{
            ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
            CONTENT_TYPE, ETAG, LAST_MODIFIED, RANGE,
        },
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
        account::handler::ensure_profile_photo_schema,
        deployment::collaboration_message_page_size,
        lifecycle::ensure_record_lifecycle_schema,
        live::publish_user_event,
        modules::module_can_for_user,
        mx::handler::{
            generate_office_pdf_preview, inline_attachment_response, n1_access_token, n1_download,
            n1_ensure_directory, n1_soft_delete, n1_stream, n1_upload, office_preview_supported,
        },
        mx::model::FileAttachment,
        notifications::{NewNotification, ensure_notification_schema, notify_user},
    },
    config::load_config::CONFIG,
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

type MessageInsertResult =
    Result<(ChatMessage, Vec<(String, String)>, Vec<String>), SqliteDatabaseError>;
type ChannelReadAdvance = (ChannelReadState, Vec<(String, String)>, bool, usize);
type MessagePage = (
    Vec<ChatMessage>,
    Vec<ChatMessage>,
    Vec<ChannelReadState>,
    bool,
    usize,
);
type MemberSave = (
    ChannelMember,
    bool,
    Vec<(String, String)>,
    Option<ChatMessage>,
);

const MEDIA_TICKET_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone)]
struct MediaTicket {
    file_uid: String,
    user_uid: String,
    expires_at: Instant,
}

static MEDIA_TICKETS: OnceLock<Mutex<HashMap<String, MediaTicket>>> = OnceLock::new();

fn media_tickets() -> &'static Mutex<HashMap<String, MediaTicket>> {
    MEDIA_TICKETS.get_or_init(|| Mutex::new(HashMap::new()))
}

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
    invite_policy: String,
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
struct ChannelMember {
    user_uid: String,
    user_name: String,
    profile_photo_updated_at: Option<i64>,
    role: String,
    joined_at: i64,
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
struct ChannelFile {
    uid: String,
    message_uid: String,
    file_name: String,
    mime_type: String,
    size: i64,
    created_at: i64,
    sender_uid: String,
    sender_name: String,
    message_created_at: i64,
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
struct MessageReplyPreview {
    uid: String,
    sender_uid: String,
    sender_name: String,
    body: String,
    deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct ReactionUser {
    uid: String,
    name: String,
}

#[derive(Debug, Clone, Serialize)]
struct MessageReaction {
    emoji: String,
    users: Vec<ReactionUser>,
}

#[derive(Debug, Clone, Serialize)]
struct MessageMention {
    uid: String,
    name: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChatMessage {
    uid: String,
    channel_uid: String,
    sender_uid: String,
    sender_name: String,
    sender_profile_photo_updated_at: Option<i64>,
    body: String,
    event_kind: Option<String>,
    reply_to_uid: Option<String>,
    reply_preview: Option<MessageReplyPreview>,
    created_at: i64,
    edited_at: Option<i64>,
    deleted_at: Option<i64>,
    sequence: i64,
    files: Vec<MessageFile>,
    record_links: Vec<MessageRecordLink>,
    reactions: Vec<MessageReaction>,
    mentions: Vec<MessageMention>,
    pinned_at: Option<i64>,
    pinned_by_uid: Option<String>,
    pinned_by_name: Option<String>,
}

fn live_message_payload(message: &ChatMessage) -> Value {
    let mut safe = message.clone();
    // Record links are resolved per recipient when the conversation refreshes.
    // A shared WebSocket payload must never carry a link authorized only for
    // the sender or moderator who performed the action.
    safe.record_links.clear();
    serde_json::to_value(safe).unwrap_or_default()
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
pub struct ToggleReactionRequest {
    pub emoji: String,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_uid: String,
    #[serde(default = "default_member_role")]
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub invite_policy: Option<String>,
}

fn default_member_role() -> String {
    "member".to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct MessageQuery {
    pub before: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ChannelSearchQuery {
    #[serde(default)]
    pub q: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ChannelFileQuery {
    #[serde(default)]
    pub q: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub(crate) fn ensure_collaboration_schema(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    ensure_record_lifecycle_schema(connection)?;
    ensure_profile_photo_schema(connection)?;
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
            invite_policy TEXT NOT NULL DEFAULT 'admins' CHECK(invite_policy IN ('owner','admins','members')),
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
            event_kind    TEXT,
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

        CREATE TABLE IF NOT EXISTS mx_message_pins (
            message_uid TEXT PRIMARY KEY NOT NULL,
            channel_uid TEXT NOT NULL,
            pinned_by   TEXT NOT NULL,
            pinned_at   INTEGER NOT NULL,
            FOREIGN KEY(message_uid) REFERENCES mx_messages(uid) ON DELETE CASCADE,
            FOREIGN KEY(channel_uid) REFERENCES mx_channels(uid) ON DELETE CASCADE,
            FOREIGN KEY(pinned_by) REFERENCES users(uid) ON DELETE RESTRICT
        );

        CREATE TABLE IF NOT EXISTS mx_message_record_links (
            message_uid TEXT NOT NULL,
            record_uid  TEXT NOT NULL,
            PRIMARY KEY(message_uid, record_uid),
            FOREIGN KEY(message_uid) REFERENCES mx_messages(uid) ON DELETE CASCADE,
            FOREIGN KEY(record_uid) REFERENCES mx_records(uid) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS mx_message_reactions (
            message_uid TEXT NOT NULL,
            user_uid    TEXT NOT NULL,
            emoji       TEXT NOT NULL,
            created_at  INTEGER NOT NULL,
            PRIMARY KEY(message_uid, user_uid, emoji),
            FOREIGN KEY(message_uid) REFERENCES mx_messages(uid) ON DELETE CASCADE,
            FOREIGN KEY(user_uid) REFERENCES users(uid) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_mx_channel_members_user ON mx_channel_members(user_uid, channel_uid);
        CREATE INDEX IF NOT EXISTS idx_mx_messages_channel_created ON mx_messages(channel_uid, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_mx_message_files_message ON mx_message_files(message_uid, created_at);
        CREATE INDEX IF NOT EXISTS idx_mx_message_reactions_message ON mx_message_reactions(message_uid, created_at);
        CREATE INDEX IF NOT EXISTS idx_mx_message_pins_channel ON mx_message_pins(channel_uid, pinned_at DESC);
        "#,
    )?;

    let has_invite_policy: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('mx_channels') WHERE name = 'invite_policy')",
        [],
        |row| row.get(0),
    )?;
    if !has_invite_policy {
        connection.execute(
            "ALTER TABLE mx_channels ADD COLUMN invite_policy TEXT NOT NULL DEFAULT 'admins' CHECK(invite_policy IN ('owner','admins','members'))",
            [],
        )?;
    }

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
    let has_event_kind: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('mx_messages') WHERE name = 'event_kind')",
        [],
        |row| row.get(0),
    )?;
    if !has_event_kind {
        connection.execute("ALTER TABLE mx_messages ADD COLUMN event_kind TEXT", [])?;
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
                "SELECT user.uid,user.name,photo.updated_at FROM users user LEFT JOIN mx_user_profile_photos photo ON photo.user_uid=user.uid WHERE user.uid<>?1 ORDER BY user.name COLLATE NOCASE LIMIT 1000",
            )?;
            statement
                .query_map(params![current_uid], |row| {
                    Ok(json!({"uid":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"profile_photo_updated_at":row.get::<_,Option<i64>>(2)?}))
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

fn like_pattern(value: &str) -> String {
    format!(
        "%{}%",
        value
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
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

fn validate_member_save_permission(
    actor_role: &str,
    requested_role: &str,
    existing_role: Option<&str>,
) -> Result<(), &'static str> {
    if !matches!(actor_role, "owner" | "admin" | "member") {
        return Err("space membership is required");
    }
    if existing_role == Some("owner") {
        return Err("the space owner role cannot be changed");
    }
    if existing_role.is_some() && actor_role != "owner" {
        return Err("only the space owner can change existing member access");
    }
    if requested_role == "owner" {
        if actor_role != "owner" {
            return Err("only the current owner can transfer space ownership");
        }
        if existing_role.is_none() {
            return Err("add the person as a member before transferring ownership");
        }
    }
    if requested_role == "admin" && actor_role != "owner" {
        return Err("only the owner can assign space administrators");
    }
    if actor_role == "member" && requested_role != "member" {
        return Err("space members can only add standard members");
    }
    Ok(())
}

fn validate_member_remove_permission(
    actor_uid: &str,
    target_uid: &str,
    actor_role: &str,
    target_role: &str,
) -> Result<(), &'static str> {
    if actor_uid == target_uid {
        return if target_role == "owner" {
            Err("the space owner must transfer ownership before leaving")
        } else {
            Ok(())
        };
    }
    if !matches!(actor_role, "owner" | "admin") {
        return Err("space administrator access is required");
    }
    if target_role == "owner" {
        return Err("the space owner cannot be removed");
    }
    if actor_role != "owner" && target_role == "admin" {
        return Err("only the owner can remove a space administrator");
    }
    Ok(())
}

fn can_invite_member(actor_role: &str, invite_policy: &str) -> bool {
    match invite_policy {
        "owner" => actor_role == "owner",
        "admins" => matches!(actor_role, "owner" | "admin"),
        "members" => matches!(actor_role, "owner" | "admin" | "member"),
        _ => false,
    }
}

fn can_pin_message(channel_kind: &str, actor_role: &str) -> bool {
    channel_kind == "direct" || matches!(actor_role, "owner" | "admin")
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

fn channel_member_details(
    connection: &rusqlite::Connection,
    channel_uid: &str,
) -> rusqlite::Result<Vec<ChannelMember>> {
    let mut statement = connection.prepare(
        r#"
        SELECT member.user_uid, user.name, photo.updated_at, member.role, member.joined_at
        FROM mx_channel_members member
        JOIN users user ON user.uid = member.user_uid
        LEFT JOIN mx_user_profile_photos photo ON photo.user_uid = member.user_uid
        WHERE member.channel_uid = ?1
        ORDER BY
            CASE member.role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END,
            user.name COLLATE NOCASE,
            member.user_uid
        "#,
    )?;
    statement
        .query_map(params![channel_uid], |row| {
            Ok(ChannelMember {
                user_uid: row.get(0)?,
                user_name: row.get(1)?,
                profile_photo_updated_at: row.get(2)?,
                role: row.get(3)?,
                joined_at: row.get(4)?,
            })
        })?
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
    ensure_notification_schema(connection)?;
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
    let notifications_read = transaction.execute(
        r#"
        UPDATE mx_notifications
        SET read_at = COALESCE(read_at, ?1)
        WHERE recipient_uid = ?2
          AND target_type = 'channel'
          AND target_uid = ?3
          AND read_at IS NULL
          AND (kind LIKE 'message.%' OR kind LIKE 'mention.%' OR kind LIKE 'file.%')
        "#,
        params![chrono::Utc::now().timestamp_millis(), user_uid, channel_uid],
    )?;
    transaction.commit()?;
    Ok((state, members, changed, notifications_read))
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
    user_uid: &str,
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
                    WHEN 'formula' THEN CAST(value.value_real AS TEXT)
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
            match module_can_for_user(
                connection,
                &record.module_uid,
                user_uid,
                access_level,
                "read",
            ) {
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
    user_uid: &str,
    access_level: i64,
) -> rusqlite::Result<ChatMessage> {
    let mut message = connection.query_row(
        r#"
        SELECT m.uid, m.channel_uid, m.sender_uid, u.name, photo.updated_at,
               m.body, m.reply_to_uid, m.created_at, m.edited_at, m.deleted_at, m.rowid,
               pin.pinned_at, pin.pinned_by, pin_user.name, m.event_kind
        FROM mx_messages m
        JOIN users u ON u.uid = m.sender_uid
        LEFT JOIN mx_user_profile_photos photo ON photo.user_uid = m.sender_uid
        LEFT JOIN mx_message_pins pin ON pin.message_uid = m.uid
        LEFT JOIN users pin_user ON pin_user.uid = pin.pinned_by
        WHERE m.uid = ?1
        "#,
        params![uid],
        |row| {
            Ok(ChatMessage {
                uid: row.get(0)?,
                channel_uid: row.get(1)?,
                sender_uid: row.get(2)?,
                sender_name: row.get(3)?,
                sender_profile_photo_updated_at: row.get(4)?,
                body: row.get(5)?,
                event_kind: row.get(14)?,
                reply_to_uid: row.get(6)?,
                reply_preview: None,
                created_at: row.get(7)?,
                edited_at: row.get(8)?,
                deleted_at: row.get(9)?,
                sequence: row.get(10)?,
                files: Vec::new(),
                record_links: Vec::new(),
                reactions: Vec::new(),
                mentions: Vec::new(),
                pinned_at: row.get(11)?,
                pinned_by_uid: row.get(12)?,
                pinned_by_name: row.get(13)?,
            })
        },
    )?;
    if message.deleted_at.is_some() {
        message.body.clear();
    } else {
        if let Some(reply_uid) = message.reply_to_uid.as_deref() {
            message.reply_preview = connection
                .query_row(
                    r#"
                    SELECT parent.uid, parent.sender_uid, sender.name, parent.body, parent.deleted_at
                    FROM mx_messages parent
                    JOIN users sender ON sender.uid = parent.sender_uid
                    WHERE parent.uid = ?1 AND parent.channel_uid = ?2
                    "#,
                    params![reply_uid, message.channel_uid],
                    |row| {
                        Ok(MessageReplyPreview {
                            uid: row.get(0)?,
                            sender_uid: row.get(1)?,
                            sender_name: row.get(2)?,
                            body: row.get(3)?,
                            deleted_at: row.get(4)?,
                        })
                    },
                )
                .optional()?;
            if let Some(preview) = message.reply_preview.as_mut()
                && preview.deleted_at.is_some()
            {
                preview.body.clear();
            }
        }
        message.files = load_files(connection, uid)?;
        message.record_links = load_record_links(connection, uid, user_uid, access_level)?;
        message.reactions = load_reactions(connection, uid)?;
        message.mentions = load_mentions(connection, uid)?;
    }
    Ok(message)
}

fn insert_membership_event(
    connection: &rusqlite::Connection,
    channel_uid: &str,
    actor_uid: &str,
    target_uid: &str,
    event_kind: &str,
    viewer_uid: &str,
    access_level: i64,
) -> rusqlite::Result<ChatMessage> {
    let actor_name: String = connection.query_row(
        "SELECT name FROM users WHERE uid=?1",
        params![actor_uid],
        |row| row.get(0),
    )?;
    let target_name: String = connection.query_row(
        "SELECT name FROM users WHERE uid=?1",
        params![target_uid],
        |row| row.get(0),
    )?;
    let body = match event_kind {
        "member_added" => format!("{target_name} was added by {actor_name}."),
        "member_left" => format!("{target_name} left the space."),
        _ => format!("{target_name} was removed by {actor_name}."),
    };
    let uid = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,event_kind,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![uid, channel_uid, actor_uid, body, event_kind, chrono::Utc::now().timestamp_millis()],
    )?;
    load_message(connection, &uid, viewer_uid, access_level)
}

fn load_pinned_messages(
    connection: &rusqlite::Connection,
    channel_uid: &str,
    user_uid: &str,
    access_level: i64,
) -> rusqlite::Result<Vec<ChatMessage>> {
    let mut statement = connection.prepare(
        "SELECT message_uid FROM mx_message_pins WHERE channel_uid = ?1 ORDER BY pinned_at DESC LIMIT 100",
    )?;
    let ids = statement
        .query_map(params![channel_uid], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter()
        .map(|uid| load_message(connection, uid, user_uid, access_level))
        .collect()
}

fn load_mentions(
    connection: &rusqlite::Connection,
    message_uid: &str,
) -> rusqlite::Result<Vec<MessageMention>> {
    let mut statement = connection.prepare(
        r#"
        SELECT mention.user_uid, user.name
        FROM mx_message_mentions mention
        JOIN users user ON user.uid = mention.user_uid
        WHERE mention.message_uid = ?1
        ORDER BY user.name COLLATE NOCASE, mention.rowid
        "#,
    )?;
    statement
        .query_map(params![message_uid], |row| {
            Ok(MessageMention {
                uid: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .collect()
}

fn load_reactions(
    connection: &rusqlite::Connection,
    message_uid: &str,
) -> rusqlite::Result<Vec<MessageReaction>> {
    let mut statement = connection.prepare(
        r#"
        SELECT reaction.emoji, reaction.user_uid, user.name
        FROM mx_message_reactions reaction
        JOIN users user ON user.uid = reaction.user_uid
        WHERE reaction.message_uid = ?1
        ORDER BY reaction.created_at, reaction.rowid
        "#,
    )?;
    let rows = statement
        .query_map(params![message_uid], |row| {
            Ok((
                row.get::<_, String>(0)?,
                ReactionUser {
                    uid: row.get(1)?,
                    name: row.get(2)?,
                },
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut reactions: Vec<MessageReaction> = Vec::new();
    for (emoji, user) in rows {
        if let Some(reaction) = reactions
            .iter_mut()
            .find(|reaction| reaction.emoji == emoji)
        {
            reaction.users.push(user);
        } else {
            reactions.push(MessageReaction {
                emoji,
                users: vec![user],
            });
        }
    }
    Ok(reactions)
}

fn search_channel_messages_db(
    connection: &rusqlite::Connection,
    channel_uid: &str,
    user_uid: &str,
    access_level: i64,
    needle: &str,
    limit: i64,
) -> rusqlite::Result<Vec<ChatMessage>> {
    let pattern = like_pattern(needle);
    let mut statement = connection.prepare(
        r#"
        SELECT DISTINCT message.uid, message.rowid
        FROM mx_messages message
        LEFT JOIN mx_message_files file
          ON file.message_uid = message.uid AND file.deleted_at IS NULL
        WHERE message.channel_uid = ?1
          AND message.deleted_at IS NULL
          AND (
            LOWER(message.body) LIKE LOWER(?2) ESCAPE '\'
            OR LOWER(COALESCE(file.file_name, '')) LIKE LOWER(?2) ESCAPE '\'
          )
        ORDER BY message.rowid DESC
        LIMIT ?3
        "#,
    )?;
    let ids = statement
        .query_map(params![channel_uid, pattern, limit], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter()
        .map(|uid| load_message(connection, uid, user_uid, access_level))
        .collect()
}

fn channel_files_db(
    connection: &rusqlite::Connection,
    channel_uid: &str,
    needle: &str,
    limit: i64,
    offset: i64,
) -> rusqlite::Result<(Vec<ChannelFile>, i64)> {
    let pattern = like_pattern(needle);
    let total = connection.query_row(
        r#"
        SELECT COUNT(*)
        FROM mx_message_files file
        JOIN mx_messages message ON message.uid = file.message_uid
        WHERE message.channel_uid = ?1
          AND message.deleted_at IS NULL
          AND file.deleted_at IS NULL
          AND (?2 = '' OR LOWER(file.file_name) LIKE LOWER(?3) ESCAPE '\')
        "#,
        params![channel_uid, needle, pattern],
        |row| row.get::<_, i64>(0),
    )?;
    let mut statement = connection.prepare(
        r#"
        SELECT file.uid, file.message_uid, file.file_name, file.mime_type,
               file.size, file.created_at, message.sender_uid, user.name,
               message.created_at
        FROM mx_message_files file
        JOIN mx_messages message ON message.uid = file.message_uid
        JOIN users user ON user.uid = message.sender_uid
        WHERE message.channel_uid = ?1
          AND message.deleted_at IS NULL
          AND file.deleted_at IS NULL
          AND (?2 = '' OR LOWER(file.file_name) LIKE LOWER(?3) ESCAPE '\')
        ORDER BY file.created_at DESC, file.rowid DESC
        LIMIT ?4 OFFSET ?5
        "#,
    )?;
    let files = statement
        .query_map(
            params![channel_uid, needle, pattern, limit, offset],
            |row| {
                Ok(ChannelFile {
                    uid: row.get(0)?,
                    message_uid: row.get(1)?,
                    file_name: row.get(2)?,
                    mime_type: row.get(3)?,
                    size: row.get(4)?,
                    created_at: row.get(5)?,
                    sender_uid: row.get(6)?,
                    sender_name: row.get(7)?,
                    message_created_at: row.get(8)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((files, total))
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

fn list_channels_db(
    connection: &rusqlite::Connection,
    user_uid: &str,
) -> rusqlite::Result<Vec<ChannelSummary>> {
    let mut statement = connection.prepare(
        r#"
        SELECT c.uid, c.kind,
               CASE WHEN c.kind = 'direct' THEN COALESCE(other.name, c.name) ELSE c.name END,
               c.description, c.created_by, c.created_at, mine.role, mine.notification_level,
               c.invite_policy,
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
    statement
        .query_map(params![user_uid], |row| {
            Ok(ChannelSummary {
                uid: row.get(0)?,
                kind: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                created_by: row.get(4)?,
                created_at: row.get(5)?,
                role: row.get(6)?,
                notification_level: row.get(7)?,
                invite_policy: row.get(8)?,
                member_count: row.get(9)?,
                unread_count: row.get(10)?,
                last_message_at: row.get(11)?,
                direct_user_uid: row.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
}

pub async fn list_channels(claims: Claims) -> Response {
    let uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            list_channels_db(connection, &uid)
        })
    })
    .await;
    match result {
        Ok(Ok(channels)) => api_json(
            StatusCode::OK,
            json!({
                "channels":channels,
                "file_max_size_bytes":CONFIG.n1.collaboration_file_max_size_mb.max(1).saturating_mul(1024 * 1024)
            }),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to load collaboration channels"}),
        ),
    }
}

pub async fn list_channel_members(claims: Claims, Path(channel_uid): Path<String>) -> Response {
    let user_uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            member_role(connection, &channel_uid, &user_uid)?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            channel_member_details(connection, &channel_uid)
        })
    })
    .await;
    match result {
        Ok(Ok(members)) => api_json(StatusCode::OK, json!({"members":members})),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"you are not a member of this channel"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to load channel members"}),
        ),
    }
}

pub async fn update_channel(
    claims: Claims,
    Path(channel_uid): Path<String>,
    Json(request): Json<UpdateChannelRequest>,
) -> Response {
    let name = clean_text(&request.name, 80);
    let description = clean_text(&request.description, 500);
    let invite_policy = request
        .invite_policy
        .map(|policy| policy.trim().to_string());
    if invite_policy
        .as_deref()
        .is_some_and(|policy| !matches!(policy, "owner" | "admins" | "members"))
    {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"invite policy must be owner, admins, or members"}),
        );
    }
    if name.is_empty() {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"space name is required"}),
        );
    }
    let actor_uid = claims.uid;
    let db_actor_uid = actor_uid.clone();
    let event_channel_uid = channel_uid.clone();
    let result = tokio::task::spawn_blocking(
        move || -> Result<Vec<(String, String)>, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_collaboration_schema(connection)?;
                let role = member_role(connection, &channel_uid, &db_actor_uid)?
                    .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
                if !matches!(role.as_str(), "owner" | "admin") {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "space administrator access is required".to_string(),
                    ));
                }
                let (kind, current_invite_policy): (String, String) = connection.query_row(
                    "SELECT kind, invite_policy FROM mx_channels WHERE uid = ?1 AND archived_at IS NULL",
                    params![channel_uid],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                if kind == "direct" {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "direct conversations cannot be edited".to_string(),
                    ));
                }
                if invite_policy.as_deref().is_some_and(|policy| policy != current_invite_policy)
                    && role != "owner"
                {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "only the space owner can change who may add people".to_string(),
                    ));
                }
                connection.execute(
                    "UPDATE mx_channels SET name = ?1, description = ?2, invite_policy = COALESCE(?3, invite_policy) WHERE uid = ?4",
                    params![name, description, invite_policy, channel_uid],
                )?;
                channel_members(connection, &channel_uid)
            })
        },
    )
    .await;
    match result {
        Ok(Ok(members)) => {
            for (member_uid, _) in members {
                publish_user_event(
                    &member_uid,
                    "channel.updated",
                    Some(&actor_uid),
                    json!({"channel_uid":event_channel_uid}),
                );
            }
            api_json(StatusCode::OK, json!({"response":"space updated"}))
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message)))) => {
            api_json(StatusCode::FORBIDDEN, json!({"response":message}))
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"space was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to update space"}),
        ),
    }
}

pub async fn remove_channel_member(
    claims: Claims,
    Path((channel_uid, target_uid)): Path<(String, String)>,
) -> Response {
    let access_level = claims.access_level;
    let actor_uid = claims.uid;
    let db_actor_uid = actor_uid.clone();
    let event_channel_uid = channel_uid.clone();
    let event_target_uid = target_uid.clone();
    let result = tokio::task::spawn_blocking(
        move || -> Result<(ChatMessage, Vec<(String, String)>), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_collaboration_schema(connection)?;
                let actor_role = member_role(connection, &channel_uid, &db_actor_uid)?
                    .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
                let kind: String = connection.query_row(
                    "SELECT kind FROM mx_channels WHERE uid = ?1 AND archived_at IS NULL",
                    params![channel_uid],
                    |row| row.get(0),
                )?;
                if kind == "direct" {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "direct conversation membership cannot be changed".to_string(),
                    ));
                }
                let target_role = member_role(connection, &channel_uid, &target_uid)?
                    .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
                validate_member_remove_permission(
                    &db_actor_uid,
                    &target_uid,
                    &actor_role,
                    &target_role,
                )
                .map_err(|message| rusqlite::Error::InvalidParameterName(message.to_string()))?;
                connection.execute(
                    "DELETE FROM mx_channel_members WHERE channel_uid = ?1 AND user_uid = ?2",
                    params![channel_uid, target_uid],
                )?;
                let event_kind = if db_actor_uid == target_uid {
                    "member_left"
                } else {
                    "member_removed"
                };
                let message = insert_membership_event(
                    connection,
                    &channel_uid,
                    &db_actor_uid,
                    &target_uid,
                    event_kind,
                    &db_actor_uid,
                    access_level,
                )?;
                Ok((message, channel_members(connection, &channel_uid)?))
            })
        },
    )
    .await;
    match result {
        Ok(Ok((message, members))) => {
            let message_payload = live_message_payload(&message);
            let call_recipients = members
                .iter()
                .map(|(member_uid, _)| member_uid.clone())
                .collect::<Vec<_>>();
            crate::api::calls::evict_call_participant(
                &event_channel_uid,
                &event_target_uid,
                &call_recipients,
                Some(&actor_uid),
                "membership_removed",
                None,
            );
            publish_user_event(
                &event_target_uid,
                "channel.member_removed",
                Some(&actor_uid),
                json!({"channel_uid":event_channel_uid}),
            );
            for (member_uid, _) in members {
                publish_user_event(
                    &member_uid,
                    "message.created",
                    Some(&actor_uid),
                    message_payload.clone(),
                );
                publish_user_event(
                    &member_uid,
                    "channel.member_removed",
                    Some(&actor_uid),
                    json!({"channel_uid":event_channel_uid,"user_uid":event_target_uid}),
                );
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(message)))) => {
            api_json(StatusCode::FORBIDDEN, json!({"response":message}))
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"space or member was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to remove channel member"}),
        ),
    }
}

pub async fn search_channel(
    claims: Claims,
    Path(channel_uid): Path<String>,
    Query(query): Query<ChannelSearchQuery>,
) -> Response {
    let user_uid = claims.uid;
    let access_level = claims.access_level;
    let needle = clean_text(&query.q, 200);
    if needle.is_empty() {
        return api_json(StatusCode::OK, json!({"messages":[]}));
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 100) as i64;
    let result =
        tokio::task::spawn_blocking(move || -> Result<Vec<ChatMessage>, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_collaboration_schema(connection)?;
                member_role(connection, &channel_uid, &user_uid)?
                    .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
                search_channel_messages_db(
                    connection,
                    &channel_uid,
                    &user_uid,
                    access_level,
                    &needle,
                    limit,
                )
            })
        })
        .await;
    match result {
        Ok(Ok(messages)) => api_json(StatusCode::OK, json!({"messages":messages})),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"you are not a member of this channel"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"conversation search failed"}),
        ),
    }
}

pub async fn list_channel_files(
    claims: Claims,
    Path(channel_uid): Path<String>,
    Query(query): Query<ChannelFileQuery>,
) -> Response {
    let user_uid = claims.uid;
    let needle = clean_text(&query.q, 200);
    let limit = query.limit.unwrap_or(50).clamp(1, 100) as i64;
    let offset = query.offset.unwrap_or(0).min(100_000) as i64;
    let result = tokio::task::spawn_blocking(
        move || -> Result<(Vec<ChannelFile>, i64), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_collaboration_schema(connection)?;
                member_role(connection, &channel_uid, &user_uid)?
                    .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
                channel_files_db(connection, &channel_uid, &needle, limit, offset)
            })
        },
    )
    .await;
    match result {
        Ok(Ok((files, total))) => api_json(StatusCode::OK, json!({"files":files,"total":total})),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"you are not a member of this channel"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to load shared files"}),
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
    let result = tokio::task::spawn_blocking(move || -> Result<MessagePage, SqliteDatabaseError> {
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
            let mut messages = ids.iter().map(|uid| load_message(connection, uid, &user_uid, access_level)).collect::<Result<Vec<_>, _>>()?;
            messages.reverse();
            let pinned_messages = load_pinned_messages(connection, &channel_uid, &user_uid, access_level)?;
            let read_states = channel_read_states(connection, &channel_uid)?;
            Ok((messages, pinned_messages, read_states, has_more, page_size))
        })
    }).await;
    match result {
        Ok(Ok((messages, pinned_messages, read_states, has_more, page_size))) => api_json(
            StatusCode::OK,
            json!({"messages":messages,"pinned_messages":pinned_messages,"read_states":read_states,"has_more":has_more,"page_size":page_size}),
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
                let valid: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM mx_messages WHERE uid = ?1 AND channel_uid = ?2 AND deleted_at IS NULL)", params![reply_uid, channel_uid], |row| row.get(0))?;
                if !valid { return Err(rusqlite::Error::InvalidParameterName("the message being replied to is unavailable".to_string())); }
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
                if !module_can_for_user(&transaction, &module_uid, &sender_uid, access_level, "read")? {
                    return Err(rusqlite::Error::InvalidParameterName("you cannot share a record you cannot access".to_string()));
                }
                transaction.execute("INSERT OR IGNORE INTO mx_message_record_links(message_uid, record_uid) VALUES (?1, ?2)", params![message_uid, record_uid])?;
            }
            transaction.commit()?;
            let message = load_message(connection, &message_uid, &sender_uid, access_level)?;
            Ok((message, members, mentions))
        })
    }).await;
    match result {
        Ok(Ok((message, members, mentions))) => {
            let message_json = live_message_payload(&message);
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
            Ok((load_message(connection, &message_uid, &actor_uid, access_level)?, channel_members(connection, &channel_uid)?))
        })
    }).await;
    match result {
        Ok(Ok((message, members))) => {
            let payload = live_message_payload(&message);
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

pub async fn toggle_message_reaction(
    claims: Claims,
    Path(message_uid): Path<String>,
    Json(request): Json<ToggleReactionRequest>,
) -> Response {
    const ALLOWED_REACTIONS: [&str; 6] = ["👍", "❤️", "😂", "🎉", "😮", "😢"];
    let emoji = request.emoji.trim().to_string();
    if !ALLOWED_REACTIONS.contains(&emoji.as_str()) {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"unsupported message reaction"}),
        );
    }
    let actor_uid = claims.uid.clone();
    let event_actor_uid = claims.uid;
    let access_level = claims.access_level;
    let event_message_uid = message_uid.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<(ChatMessage, Vec<(String, String)>), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            let channel_uid: String = connection.query_row(
                "SELECT channel_uid FROM mx_messages WHERE uid=?1 AND deleted_at IS NULL",
                params![message_uid],
                |row| row.get(0),
            )?;
            member_role(connection, &channel_uid, &actor_uid)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM mx_message_reactions WHERE message_uid=?1 AND user_uid=?2 AND emoji=?3)",
                params![message_uid, actor_uid, emoji],
                |row| row.get(0),
            )?;
            if exists {
                connection.execute(
                    "DELETE FROM mx_message_reactions WHERE message_uid=?1 AND user_uid=?2 AND emoji=?3",
                    params![message_uid, actor_uid, emoji],
                )?;
            } else {
                connection.execute(
                    "INSERT INTO mx_message_reactions(message_uid,user_uid,emoji,created_at) VALUES (?1,?2,?3,?4)",
                    params![message_uid, actor_uid, emoji, chrono::Utc::now().timestamp_millis()],
                )?;
            }
            Ok((load_message(connection, &message_uid, &actor_uid, access_level)?, channel_members(connection, &channel_uid)?))
        })
    }).await;
    match result {
        Ok(Ok((message, members))) => {
            let payload = live_message_payload(&message);
            for (uid, _) in members {
                publish_user_event(
                    &uid,
                    "message.updated",
                    Some(&event_actor_uid),
                    payload.clone(),
                );
            }
            api_json(
                StatusCode::OK,
                json!({"message":message,"uid":event_message_uid}),
            )
        }
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"message was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"message reaction could not be updated"}),
        ),
    }
}

async fn set_message_pin(claims: Claims, message_uid: String, pinned: bool) -> Response {
    let actor_uid = claims.uid.clone();
    let event_actor_uid = claims.uid;
    let access_level = claims.access_level;
    let result = tokio::task::spawn_blocking(
        move || -> Result<(ChatMessage, Vec<(String, String)>), SqliteDatabaseError> {
            with_sql_connection(|connection| {
                ensure_collaboration_schema(connection)?;
                let (channel_uid, channel_kind, deleted_at): (String, String, Option<i64>) =
                    connection.query_row(
                        r#"
                SELECT message.channel_uid, channel.kind, message.deleted_at
                FROM mx_messages message
                JOIN mx_channels channel ON channel.uid = message.channel_uid
                WHERE message.uid = ?1 AND channel.archived_at IS NULL
                "#,
                        params![message_uid],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )?;
                let actor_role = member_role(connection, &channel_uid, &actor_uid)?
                    .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
                if deleted_at.is_some() {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "deleted messages cannot be pinned".to_string(),
                    ));
                }
                if !can_pin_message(&channel_kind, &actor_role) {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "only the space owner or an administrator can manage pinned messages"
                            .to_string(),
                    ));
                }
                if pinned {
                    connection.execute(
                        r#"
                    INSERT INTO mx_message_pins(message_uid,channel_uid,pinned_by,pinned_at)
                    VALUES (?1,?2,?3,?4)
                    ON CONFLICT(message_uid) DO UPDATE SET
                        pinned_by=excluded.pinned_by,
                        pinned_at=excluded.pinned_at
                    "#,
                        params![
                            message_uid,
                            channel_uid,
                            actor_uid,
                            chrono::Utc::now().timestamp_millis()
                        ],
                    )?;
                } else {
                    connection.execute(
                        "DELETE FROM mx_message_pins WHERE message_uid = ?1",
                        params![message_uid],
                    )?;
                }
                Ok((
                    load_message(connection, &message_uid, &actor_uid, access_level)?,
                    channel_members(connection, &channel_uid)?,
                ))
            })
        },
    )
    .await;
    match result {
        Ok(Ok((message, members))) => {
            let payload = live_message_payload(&message);
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
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::QueryReturnedNoRows))) => api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"message was not found"}),
        ),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"pinned message could not be updated"}),
        ),
    }
}

pub async fn pin_message(claims: Claims, Path(message_uid): Path<String>) -> Response {
    set_message_pin(claims, message_uid, true).await
}

pub async fn unpin_message(claims: Claims, Path(message_uid): Path<String>) -> Response {
    set_message_pin(claims, message_uid, false).await
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
            connection.execute(
                "DELETE FROM mx_message_pins WHERE message_uid = ?1",
                params![db_message_uid],
            )?;
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
        Ok(Ok((state, members, changed, notifications_read))) => {
            if changed {
                let payload =
                    json!({"channel_uid":event_channel_uid.clone(),"read_state":state.clone()});
                for (member_uid, _) in members {
                    publish_user_event(
                        &member_uid,
                        "channel.read",
                        Some(&actor_uid),
                        payload.clone(),
                    );
                }
            }
            if notifications_read > 0 {
                publish_user_event(
                    &actor_uid,
                    "notification.context_read",
                    Some(&actor_uid),
                    json!({"target_type":"channel","target_uid":event_channel_uid,"updated":notifications_read}),
                );
            }
            api_json(
                StatusCode::OK,
                json!({"read_state":state,"notifications_read":notifications_read}),
            )
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
    if !matches!(request.role.as_str(), "owner" | "admin" | "member") {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"role must be owner, admin, or member"}),
        );
    }
    let access_level = claims.access_level;
    let actor_uid = claims.uid.clone();
    let event_actor_uid = claims.uid;
    let user_uid = request.user_uid;
    let event_user_uid = user_uid.clone();
    let channel_for_event = channel_uid.clone();
    let role_value = request.role;
    let result = tokio::task::spawn_blocking(move || -> Result<MemberSave, SqliteDatabaseError> { with_sql_connection(|connection| {
        ensure_collaboration_schema(connection)?;
        let actor_role = member_role(connection, &channel_uid, &actor_uid)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let (kind, invite_policy): (String, String) = connection.query_row("SELECT kind, invite_policy FROM mx_channels WHERE uid = ?1", params![channel_uid], |row| Ok((row.get(0)?, row.get(1)?)))?;
        if kind == "direct" { return Err(rusqlite::Error::InvalidParameterName("direct conversations cannot add members".to_string())); }
        let user_exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE uid = ?1)", params![user_uid], |row| row.get(0))?;
        if !user_exists { return Err(rusqlite::Error::InvalidParameterName("the selected account was not found".to_string())); }
        let existing_role = member_role(connection, &channel_uid, &user_uid)?;
        let added = existing_role.is_none();
        if added && !can_invite_member(&actor_role, &invite_policy) {
            return Err(rusqlite::Error::InvalidParameterName(
                "this space only allows its owner or administrators to add people".to_string(),
            ));
        }
        validate_member_save_permission(&actor_role, &role_value, existing_role.as_deref())
            .map_err(|message| rusqlite::Error::InvalidParameterName(message.to_string()))?;
        if role_value == "owner" {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute(
                "UPDATE mx_channel_members SET role = 'admin' WHERE channel_uid = ?1 AND user_uid = ?2 AND role = 'owner'",
                params![channel_uid, actor_uid],
            )?;
            transaction.execute(
                "UPDATE mx_channel_members SET role = 'owner' WHERE channel_uid = ?1 AND user_uid = ?2",
                params![channel_uid, user_uid],
            )?;
            transaction.commit()?;
        } else {
            connection.execute("INSERT INTO mx_channel_members(channel_uid, user_uid, role, joined_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(channel_uid, user_uid) DO UPDATE SET role = excluded.role", params![channel_uid, user_uid, role_value, chrono::Utc::now().timestamp_millis()])?;
        }
        let member = channel_member_details(connection, &channel_uid)?
            .into_iter()
            .find(|member| member.user_uid == user_uid)
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let message = if added {
            Some(insert_membership_event(connection, &channel_uid, &actor_uid, &user_uid, "member_added", &actor_uid, access_level)?)
        } else {
            None
        };
        Ok((member, added, channel_members(connection, &channel_uid)?, message))
    })}).await;
    match result {
        Ok(Ok((member, added, members, message))) => {
            let event_kind = if added {
                "channel.member_added"
            } else {
                "channel.member_updated"
            };
            for (member_uid, _) in members {
                if let Some(message) = message.as_ref() {
                    publish_user_event(
                        &member_uid,
                        "message.created",
                        Some(&event_actor_uid),
                        serde_json::to_value(message).unwrap_or_default(),
                    );
                }
                publish_user_event(
                    &member_uid,
                    event_kind,
                    Some(&event_actor_uid),
                    json!({"channel_uid":channel_for_event,"user_uid":event_user_uid}),
                );
            }
            if added {
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
            }
            api_json(
                StatusCode::OK,
                json!({"response":"channel member saved","member":member}),
            )
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
        .collaboration_file_max_size_mb
        .max(1)
        .saturating_mul(1024 * 1024);
    if bytes.len() > maximum {
        return api_json(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({"response":format!("shared files may not exceed {} MiB", CONFIG.n1.collaboration_file_max_size_mb)}),
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
    if n1_upload(&object_key, &mime_type, bytes.clone().to_vec(), &token)
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

fn accessible_message_file(
    connection: &rusqlite::Connection,
    file_uid: &str,
    user_uid: &str,
) -> rusqlite::Result<(String, String, String, String)> {
    let value = connection.query_row(
        "SELECT f.object_key, f.file_name, f.mime_type, m.channel_uid FROM mx_message_files f JOIN mx_messages m ON m.uid = f.message_uid WHERE f.uid = ?1 AND f.deleted_at IS NULL",
        params![file_uid],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
    )?;
    if member_role(connection, &value.3, user_uid)?.is_none() {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(value)
}

pub async fn issue_message_file_preview_ticket(
    claims: Claims,
    Path(file_uid): Path<String>,
) -> Response {
    let user_uid = claims.uid;
    let lookup_file_uid = file_uid.clone();
    let lookup_user_uid = user_uid.clone();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            accessible_message_file(connection, &lookup_file_uid, &lookup_user_uid)
        })
    })
    .await;

    let (_, file_name, mime_type, _) = match result {
        Ok(Ok(value)) => value,
        _ => {
            return api_json(
                StatusCode::NOT_FOUND,
                json!({"response":"shared file was not found"}),
            );
        }
    };

    let ticket = Uuid::new_v4().to_string();
    let expires_at = Instant::now() + MEDIA_TICKET_TTL;
    if let Ok(mut tickets) = media_tickets().lock() {
        let now = Instant::now();
        tickets.retain(|_, item| item.expires_at > now);
        tickets.insert(
            ticket.clone(),
            MediaTicket {
                file_uid,
                user_uid,
                expires_at,
            },
        );
    } else {
        return api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"preview ticket service is unavailable"}),
        );
    }

    api_json(
        StatusCode::CREATED,
        json!({
            "url": format!("/mx/v1/collaboration/media/{ticket}"),
            "file_name": file_name,
            "mime_type": mime_type,
            "expires_in_seconds": MEDIA_TICKET_TTL.as_secs(),
        }),
    )
}

pub async fn stream_message_file(Path(ticket): Path<String>, headers: HeaderMap) -> Response {
    let ticket_data = if let Ok(mut tickets) = media_tickets().lock() {
        let now = Instant::now();
        tickets.retain(|_, item| item.expires_at > now);
        tickets.get_mut(&ticket).map(|item| {
            item.expires_at = now + MEDIA_TICKET_TTL;
            item.clone()
        })
    } else {
        None
    };
    let Some(ticket_data) = ticket_data else {
        return api_json(
            StatusCode::NOT_FOUND,
            json!({"response":"preview link expired; reopen the file"}),
        );
    };

    let file_uid = ticket_data.file_uid;
    let user_uid = ticket_data.user_uid;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            accessible_message_file(connection, &file_uid, &user_uid)
        })
    })
    .await;
    let (object_key, file_name, mime_type, _) = match result {
        Ok(Ok(value)) => value,
        _ => {
            return api_json(
                StatusCode::NOT_FOUND,
                json!({"response":"shared file was not found"}),
            );
        }
    };

    let requested_range = headers.get(RANGE).and_then(|value| value.to_str().ok());
    let upstream = match n1_stream(&object_key, requested_range).await {
        Ok(response) => response,
        Err(error) => {
            crate::report_error!(format!("{error:?}"), "n1", "stream_message_file()");
            return api_json(
                StatusCode::BAD_GATEWAY,
                json!({"response":"N1 could not stream the shared file"}),
            );
        }
    };

    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=300"),
    );
    for header in [
        ACCEPT_RANGES,
        CONTENT_RANGE,
        CONTENT_LENGTH,
        ETAG,
        LAST_MODIFIED,
    ] {
        if let Some(value) = upstream_headers.get(&header).cloned() {
            response.headers_mut().insert(header, value);
        }
    }
    let safe_name = file_name.replace('"', "_");
    if let Ok(value) = HeaderValue::from_str(&format!("inline; filename=\"{safe_name}\"")) {
        response.headers_mut().insert(CONTENT_DISPOSITION, value);
    }
    response
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
    fn regular_space_members_can_invite_standard_members_only() {
        assert_eq!(
            validate_member_save_permission("member", "member", None),
            Ok(())
        );
        assert_eq!(
            validate_member_save_permission("member", "admin", None),
            Err("only the owner can assign space administrators")
        );
        assert_eq!(
            validate_member_save_permission("member", "member", Some("member")),
            Err("only the space owner can change existing member access")
        );
    }

    #[test]
    fn space_owners_retain_role_management_authority() {
        assert_eq!(
            validate_member_save_permission("owner", "admin", None),
            Ok(())
        );
        assert_eq!(
            validate_member_save_permission("owner", "member", Some("owner")),
            Err("the space owner role cannot be changed")
        );
    }

    #[test]
    fn invitation_policy_is_enforced_for_each_space_role() {
        assert!(can_invite_member("owner", "owner"));
        assert!(!can_invite_member("admin", "owner"));
        assert!(can_invite_member("admin", "admins"));
        assert!(!can_invite_member("member", "admins"));
        assert!(can_invite_member("member", "members"));
        assert!(!can_invite_member("member", "unexpected"));
    }

    #[test]
    fn ownership_transfer_requires_the_current_owner_and_an_existing_member() {
        assert_eq!(
            validate_member_save_permission("owner", "owner", Some("member")),
            Ok(())
        );
        assert_eq!(
            validate_member_save_permission("admin", "owner", Some("member")),
            Err("only the space owner can change existing member access")
        );
        assert_eq!(
            validate_member_save_permission("owner", "owner", None),
            Err("add the person as a member before transferring ownership")
        );
    }

    #[test]
    fn leave_and_kick_permissions_preserve_the_owner_and_admin_boundary() {
        assert_eq!(
            validate_member_remove_permission("member-1", "member-1", "member", "member"),
            Ok(())
        );
        assert_eq!(
            validate_member_remove_permission("owner", "owner", "owner", "owner"),
            Err("the space owner must transfer ownership before leaving")
        );
        assert_eq!(
            validate_member_remove_permission("admin", "member", "admin", "member"),
            Ok(())
        );
        assert_eq!(
            validate_member_remove_permission("admin-1", "admin-2", "admin", "admin"),
            Err("only the owner can remove a space administrator")
        );
        assert_eq!(
            validate_member_remove_permission("member-1", "member-2", "member", "member"),
            Err("space administrator access is required")
        );
    }

    #[test]
    fn pin_permissions_match_direct_and_space_moderation_rules() {
        assert!(can_pin_message("direct", "member"));
        assert!(can_pin_message("group", "owner"));
        assert!(can_pin_message("channel", "admin"));
        assert!(!can_pin_message("group", "member"));
    }

    #[test]
    fn existing_channel_schema_receives_private_invitation_default() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE users (uid TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
                CREATE TABLE mx_channels (
                    uid TEXT PRIMARY KEY NOT NULL,
                    kind TEXT NOT NULL,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL DEFAULT '',
                    direct_key TEXT UNIQUE,
                    created_by TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    archived_at INTEGER
                );
                "#,
            )
            .unwrap();

        ensure_collaboration_schema(&connection).unwrap();
        let has_invite_policy: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('mx_channels') WHERE name='invite_policy')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(has_invite_policy);
    }

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
    fn message_reactions_are_grouped_with_the_accounts_that_reacted() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE users (uid TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
                INSERT INTO users(uid,name) VALUES ('sender','Sender'),('reader','Reader');
                "#,
            )
            .unwrap();
        ensure_collaboration_schema(&connection).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO mx_channels(uid,kind,name,description,created_by,created_at)
                VALUES ('channel','group','Operations','','sender',1000);
                INSERT INTO mx_channel_members(channel_uid,user_uid,role,notification_level,joined_at)
                VALUES ('channel','sender','owner','all',1000),('channel','reader','member','all',1000);
                INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,created_at)
                VALUES ('message','channel','sender','Ready for review',2000);
                INSERT INTO mx_message_reactions(message_uid,user_uid,emoji,created_at)
                VALUES ('message','sender','👍',2100),('message','reader','👍',2200),('message','reader','🎉',2300);
                "#,
            )
            .unwrap();

        let message = load_message(&connection, "message", "", 0).unwrap();

        assert_eq!(message.reactions.len(), 2);
        assert_eq!(message.reactions[0].emoji, "👍");
        assert_eq!(message.reactions[0].users.len(), 2);
        assert_eq!(message.reactions[1].emoji, "🎉");
        assert_eq!(message.reactions[1].users[0].name, "Reader");
    }

    #[test]
    fn message_mentions_are_returned_with_their_account_names() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE users (uid TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
                INSERT INTO users(uid,name) VALUES ('sender','Sender'),('reader','Glayde John Mendoza');
                "#,
            )
            .unwrap();
        ensure_collaboration_schema(&connection).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO mx_channels(uid,kind,name,description,created_by,created_at)
                VALUES ('channel','group','Operations','','sender',1000);
                INSERT INTO mx_channel_members(channel_uid,user_uid,role,notification_level,joined_at)
                VALUES ('channel','sender','owner','all',1000),('channel','reader','member','mentions',1000);
                INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,created_at)
                VALUES ('message','channel','sender','@Glayde John Mendoza please review this',2000);
                INSERT INTO mx_message_mentions(message_uid,user_uid)
                VALUES ('message','reader');
                "#,
            )
            .unwrap();

        let message = load_message(&connection, "message", "", 0).unwrap();

        assert_eq!(message.mentions.len(), 1);
        assert_eq!(message.mentions[0].uid, "reader");
        assert_eq!(message.mentions[0].name, "Glayde John Mendoza");
    }

    #[test]
    fn pinned_messages_return_pin_actor_and_channel_index() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE users (uid TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
                INSERT INTO users(uid,name) VALUES ('sender','Sender'),('owner','Space Owner');
                "#,
            )
            .unwrap();
        ensure_collaboration_schema(&connection).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO mx_channels(uid,kind,name,description,created_by,created_at)
                VALUES ('channel','group','Operations','','owner',1000);
                INSERT INTO mx_channel_members(channel_uid,user_uid,role,notification_level,joined_at)
                VALUES ('channel','owner','owner','all',1000),('channel','sender','member','all',1000);
                INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,created_at)
                VALUES ('message','channel','sender','Final release checklist',2000);
                INSERT INTO mx_message_pins(message_uid,channel_uid,pinned_by,pinned_at)
                VALUES ('message','channel','owner',2500);
                "#,
            )
            .unwrap();

        let message = load_message(&connection, "message", "", 0).unwrap();
        assert_eq!(message.pinned_at, Some(2500));
        assert_eq!(message.pinned_by_uid.as_deref(), Some("owner"));
        assert_eq!(message.pinned_by_name.as_deref(), Some("Space Owner"));

        let pinned = load_pinned_messages(&connection, "channel", "", 0).unwrap();
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned[0].uid, "message");
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
        ensure_notification_schema(&connection).unwrap();
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
                INSERT INTO mx_notifications(uid,recipient_uid,kind,title,target_type,target_uid,data_json,created_at)
                VALUES ('notification-1','reader','message.created','New message','channel','channel','{}',2000);
                "#,
            )
            .unwrap();

        let (state, members, changed, notifications_read) =
            advance_channel_read(&connection, "channel", "reader").unwrap();
        assert!(changed);
        assert_eq!(members.len(), 2);
        assert_eq!(state.last_read_message_id, 2);
        assert_eq!(state.last_read_at, 2000);
        assert_eq!(notifications_read, 1);
        let notification_read_at: Option<i64> = connection
            .query_row(
                "SELECT read_at FROM mx_notifications WHERE uid='notification-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(notification_read_at.is_some());

        let (_, members, changed, notifications_read) =
            advance_channel_read(&connection, "channel", "reader").unwrap();
        assert!(!changed);
        assert!(members.is_empty());
        assert_eq!(notifications_read, 0);

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

        let exposed = load_message(&connection, "message", "", 0).unwrap();
        assert!(exposed.body.is_empty());
        assert_eq!(exposed.deleted_at, Some(3000));
        assert!(exposed.files.is_empty());
        assert!(exposed.record_links.is_empty());
    }

    #[test]
    fn channel_search_and_file_index_cover_complete_active_history() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE users (uid TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
                INSERT INTO users(uid,name) VALUES ('owner','Space Owner'),('member','Space Member');
                "#,
            )
            .unwrap();
        ensure_collaboration_schema(&connection).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO mx_channels(uid,kind,name,description,created_by,created_at)
                VALUES ('space','group','Operations','Shared work','owner',1000);
                INSERT INTO mx_channel_members(channel_uid,user_uid,role,notification_level,joined_at)
                VALUES ('space','owner','owner','all',1000),('space','member','member','all',1001);
                INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,created_at)
                VALUES
                    ('message-body','space','owner','Quarterly operations review',2000),
                    ('message-file','space','member','Please review the attachment',3000),
                    ('message-deleted','space','owner','Quarterly deleted item',4000);
                UPDATE mx_messages SET deleted_at=5000 WHERE uid='message-deleted';
                INSERT INTO mx_message_files(uid,message_uid,file_name,mime_type,size,object_key,created_at)
                VALUES
                    ('file-plan','message-file','Quarterly 100% Plan.docx','application/vnd.openxmlformats-officedocument.wordprocessingml.document',1024,'object-plan',3100),
                    ('file-notes','message-body','Meeting notes.pdf','application/pdf',2048,'object-notes',2100),
                    ('file-deleted','message-deleted','Quarterly hidden.pdf','application/pdf',512,'object-hidden',4100);
                "#,
            )
            .unwrap();

        let matches =
            search_channel_messages_db(&connection, "space", "", 0, "quarterly", 50).unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().any(|message| message.uid == "message-body"));
        assert!(matches.iter().any(|message| message.uid == "message-file"));
        assert!(
            !matches
                .iter()
                .any(|message| message.uid == "message-deleted")
        );

        let literal_percent =
            search_channel_messages_db(&connection, "space", "", 0, "100%", 50).unwrap();
        assert_eq!(literal_percent.len(), 1);
        assert_eq!(literal_percent[0].uid, "message-file");

        let (first_page, total) = channel_files_db(&connection, "space", "", 1, 0).unwrap();
        assert_eq!(total, 2);
        assert_eq!(first_page.len(), 1);
        assert_eq!(first_page[0].file_name, "Quarterly 100% Plan.docx");

        let (filtered, filtered_total) =
            channel_files_db(&connection, "space", "notes", 50, 0).unwrap();
        assert_eq!(filtered_total, 1);
        assert_eq!(filtered[0].sender_name, "Space Owner");

        let members = channel_member_details(&connection, "space").unwrap();
        assert_eq!(members[0].role, "owner");
        assert_eq!(members[1].user_name, "Space Member");
    }

    #[test]
    fn conversation_directory_is_membership_scoped_for_every_access_level() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE users (uid TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
                INSERT INTO users(uid,name) VALUES
                    ('editor','Editor Account'),
                    ('viewer','Viewer Account'),
                    ('outsider','Unrelated Account');
                "#,
            )
            .unwrap();
        ensure_collaboration_schema(&connection).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO mx_channels(uid,kind,name,description,direct_key,created_by,created_at)
                VALUES
                    ('private-dm','direct','Direct message','','editor:viewer','editor',1000),
                    ('private-space','group','Private Operations','Members only',NULL,'editor',1100),
                    ('unrelated-space','group','Unrelated Space','Hidden from both',NULL,'outsider',1200);
                INSERT INTO mx_channel_members(channel_uid,user_uid,role,joined_at)
                VALUES
                    ('private-dm','editor','member',1000),
                    ('private-dm','viewer','member',1000),
                    ('private-space','editor','owner',1100),
                    ('unrelated-space','outsider','owner',1200);
                INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,created_at)
                VALUES
                    ('dm-secret','private-dm','editor','Private direct message',2000),
                    ('space-secret','private-space','editor','Private group message',2100),
                    ('unrelated-secret','unrelated-space','outsider','Unrelated message',2200);
                "#,
            )
            .unwrap();

        // Access level is intentionally absent from this lookup: an editor or
        // viewer receives the same membership-scoped collaboration behavior as
        // an administrator, never an administrator-wide view.
        let editor_channels = list_channels_db(&connection, "editor").unwrap();
        assert_eq!(editor_channels.len(), 2);
        assert!(editor_channels.iter().any(|item| item.uid == "private-dm"));
        assert!(
            editor_channels
                .iter()
                .any(|item| item.uid == "private-space")
        );
        assert!(
            !editor_channels
                .iter()
                .any(|item| item.uid == "unrelated-space")
        );

        let viewer_channels = list_channels_db(&connection, "viewer").unwrap();
        assert_eq!(viewer_channels.len(), 1);
        assert_eq!(viewer_channels[0].uid, "private-dm");
        assert_eq!(viewer_channels[0].name, "Editor Account");
        assert_eq!(
            viewer_channels[0].direct_user_uid.as_deref(),
            Some("editor")
        );

        let outsider_channels = list_channels_db(&connection, "outsider").unwrap();
        assert_eq!(outsider_channels.len(), 1);
        assert_eq!(outsider_channels[0].uid, "unrelated-space");

        assert!(
            member_role(&connection, "private-dm", "outsider")
                .unwrap()
                .is_none()
        );
        assert!(
            member_role(&connection, "private-space", "viewer")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn reply_preview_survives_pagination_and_respects_message_deletion() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE users (uid TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
                INSERT INTO users(uid,name) VALUES ('sender','Original Sender'),('replier','Reply Sender');
                "#,
            )
            .unwrap();
        ensure_collaboration_schema(&connection).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO mx_channels(uid,kind,name,description,created_by,created_at)
                VALUES ('conversation','direct','Direct message','','sender',1000);
                INSERT INTO mx_channel_members(channel_uid,user_uid,role,joined_at)
                VALUES ('conversation','sender','member',1000),('conversation','replier','member',1000);
                INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,created_at)
                VALUES ('original','conversation','sender','Original message outside the current page',2000);
                INSERT INTO mx_messages(uid,channel_uid,sender_uid,body,reply_to_uid,created_at)
                VALUES ('reply','conversation','replier','This is the reply','original',3000);
                "#,
            )
            .unwrap();

        let reply = load_message(&connection, "reply", "member", 2).unwrap();
        let preview = reply.reply_preview.expect("reply preview");
        assert_eq!(preview.uid, "original");
        assert_eq!(preview.sender_name, "Original Sender");
        assert_eq!(preview.body, "Original message outside the current page");
        assert!(preview.deleted_at.is_none());

        tombstone_message(&connection, "original", 4000).unwrap();
        let reply = load_message(&connection, "reply", "member", 2).unwrap();
        let preview = reply.reply_preview.expect("deleted reply preview");
        assert_eq!(preview.deleted_at, Some(4000));
        assert!(preview.body.is_empty());
    }
}
