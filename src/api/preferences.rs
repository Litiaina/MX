use std::{collections::HashMap, sync::OnceLock};

use axum::{
    Json,
    body::Body,
    extract::Multipart,
    http::HeaderValue,
    http::StatusCode,
    http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::RwLock;

use crate::{
    api::{
        live::publish_user_event,
        mx::handler::{
            n1_access_token, n1_download, n1_ensure_directory, n1_soft_delete, n1_upload_one_shot,
        },
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct NotificationPreferences {
    pub browser_enabled: bool,
    pub sound_enabled: bool,
    pub sound_source: Option<String>,
    pub sound_volume: u8,
    pub messages: Option<bool>,
    pub mentions: Option<bool>,
    pub channel_activity: Option<bool>,
    pub record_created: Option<bool>,
    pub record_updated: Option<bool>,
    pub record_assigned: Option<bool>,
    pub attachment_received: Option<bool>,
    pub workflow_changes: Option<bool>,
    pub task_activity: Option<bool>,
    pub digest: Option<String>,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            browser_enabled: false,
            sound_enabled: true,
            sound_source: Some("default".to_string()),
            sound_volume: 70,
            messages: None,
            mentions: None,
            channel_activity: None,
            record_created: None,
            record_updated: None,
            record_assigned: None,
            attachment_received: None,
            workflow_changes: None,
            task_activity: None,
            digest: None,
            quiet_hours_start: None,
            quiet_hours_end: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct UserPreferences {
    pub theme: Option<String>,
    pub accent_color: Option<String>,
    pub auto_scale: Option<bool>,
    pub ui_scale_percent: Option<u16>,
    pub font_scale_percent: Option<u16>,
    pub density: Option<String>,
    pub content_width: Option<String>,
    pub reduced_motion: Option<bool>,
    pub auto_refresh_seconds: Option<u32>,
    pub default_workspace: Option<String>,
    pub notifications: NotificationPreferences,
}

#[derive(Debug, Deserialize)]
pub struct SavePreferencesRequest {
    pub preferences: UserPreferences,
}

#[derive(Debug, Clone)]
struct CachedNotificationSound {
    bytes: Vec<u8>,
    file_name: String,
    mime_type: String,
    updated_at: i64,
}

static NOTIFICATION_SOUND_CACHE: OnceLock<RwLock<HashMap<String, CachedNotificationSound>>> =
    OnceLock::new();

fn notification_sound_cache() -> &'static RwLock<HashMap<String, CachedNotificationSound>> {
    NOTIFICATION_SOUND_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

pub(crate) fn ensure_preferences_schema(connection: &rusqlite::Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_user_preferences (
            user_uid         TEXT PRIMARY KEY NOT NULL,
            revision         INTEGER NOT NULL DEFAULT 1 CHECK(revision >= 1),
            preferences_json TEXT NOT NULL DEFAULT '{}',
            updated_at       INTEGER NOT NULL,
            FOREIGN KEY(user_uid) REFERENCES users(uid) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS mx_user_notification_sounds (
            user_uid   TEXT PRIMARY KEY NOT NULL,
            object_key TEXT NOT NULL UNIQUE,
            file_name  TEXT NOT NULL,
            mime_type  TEXT NOT NULL,
            size       INTEGER NOT NULL CHECK(size > 0),
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(user_uid) REFERENCES users(uid) ON DELETE CASCADE
        );
        "#,
    )
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn valid_clock(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || !bytes[..2].iter().all(u8::is_ascii_digit)
        || !bytes[3..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    value[..2].parse::<u8>().is_ok_and(|hour| hour < 24)
        && value[3..].parse::<u8>().is_ok_and(|minute| minute < 60)
}

fn validate(preferences: &UserPreferences) -> Result<(), &'static str> {
    if preferences
        .theme
        .as_deref()
        .is_some_and(|value| !matches!(value, "light" | "dark" | "system"))
    {
        return Err("theme must be light, dark, system, or unset");
    }
    if preferences.accent_color.as_deref().is_some_and(|value| {
        value.len() != 7
            || !value.starts_with('#')
            || !value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        return Err("accent color must be a six-digit hexadecimal color or unset");
    }
    if preferences
        .ui_scale_percent
        .is_some_and(|value| !(85..=160).contains(&value))
    {
        return Err("interface scale must be between 85 and 160 percent");
    }
    if preferences
        .font_scale_percent
        .is_some_and(|value| !(85..=150).contains(&value))
    {
        return Err("font scale must be between 85 and 150 percent");
    }
    if preferences
        .density
        .as_deref()
        .is_some_and(|value| !matches!(value, "compact" | "normal" | "comfortable"))
    {
        return Err("density must be compact, normal, comfortable, or unset");
    }
    if preferences
        .content_width
        .as_deref()
        .is_some_and(|value| !matches!(value, "standard" | "wide" | "full"))
    {
        return Err("content width must be standard, wide, full, or unset");
    }
    if preferences
        .auto_refresh_seconds
        .is_some_and(|value| value != 0 && !(10..=86_400).contains(&value))
    {
        return Err("automatic refresh must be off or between 10 seconds and 24 hours");
    }
    if preferences
        .notifications
        .digest
        .as_deref()
        .is_some_and(|value| !matches!(value, "immediate" | "daily" | "weekly" | "off"))
    {
        return Err("notification digest must be immediate, daily, weekly, off, or unset");
    }
    if preferences
        .notifications
        .sound_source
        .as_deref()
        .is_some_and(|value| !matches!(value, "default" | "custom"))
    {
        return Err("notification sound source must be default, custom, or unset");
    }
    if preferences.notifications.sound_volume > 100 {
        return Err("notification sound volume must be between 0 and 100");
    }
    for value in [
        preferences.notifications.quiet_hours_start.as_deref(),
        preferences.notifications.quiet_hours_end.as_deref(),
    ] {
        if value.is_some_and(|value| !valid_clock(value)) {
            return Err("quiet hours must use HH:MM in 24-hour time");
        }
    }
    Ok(())
}

fn notification_sound_type(
    mime_type: &str,
    file_name: &str,
) -> Option<(&'static str, &'static str)> {
    let mime = mime_type.trim().to_ascii_lowercase();
    let name = file_name.to_ascii_lowercase();
    match mime.as_str() {
        "audio/mpeg" | "audio/mp3" => Some(("mp3", "audio/mpeg")),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some(("wav", "audio/wav")),
        "audio/ogg" => Some(("ogg", "audio/ogg")),
        "audio/webm" => Some(("webm", "audio/webm")),
        "audio/mp4" | "audio/x-m4a" => Some(("m4a", "audio/mp4")),
        "audio/aac" => Some(("aac", "audio/aac")),
        _ if name.ends_with(".mp3") => Some(("mp3", "audio/mpeg")),
        _ if name.ends_with(".wav") => Some(("wav", "audio/wav")),
        _ if name.ends_with(".ogg") => Some(("ogg", "audio/ogg")),
        _ if name.ends_with(".webm") => Some(("webm", "audio/webm")),
        _ if name.ends_with(".m4a") => Some(("m4a", "audio/mp4")),
        _ if name.ends_with(".aac") => Some(("aac", "audio/aac")),
        _ => None,
    }
}

fn notification_sound_json(file_name: &str, mime_type: &str, size: i64, updated_at: i64) -> Value {
    json!({
        "exists": true,
        "file_name": file_name,
        "mime_type": mime_type,
        "size": size,
        "updated_at": updated_at,
        "url": format!("/mx/v1/account/notification-sound?v={updated_at}")
    })
}

pub async fn get_notification_sound_info(claims: Claims) -> Response {
    let user_uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_preferences_schema(connection)?;
            connection
                .query_row(
                    "SELECT file_name,mime_type,size,updated_at FROM mx_user_notification_sounds WHERE user_uid=?1",
                    params![user_uid],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?)),
                )
                .optional()
        })
    })
    .await;
    match result {
        Ok(Ok(Some((file_name, mime_type, size, updated_at)))) => api_json(
            StatusCode::OK,
            notification_sound_json(&file_name, &mime_type, size, updated_at),
        ),
        Ok(Ok(None)) => api_json(StatusCode::OK, json!({"exists":false})),
        _ => api_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"response":"failed to load notification sound metadata"}),
        ),
    }
}

pub async fn upload_notification_sound(claims: Claims, mut multipart: Multipart) -> Response {
    let field = match multipart.next_field().await {
        Ok(Some(field)) => field,
        _ => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"select an audio file to upload"}),
            );
        }
    };
    let file_name = field
        .file_name()
        .unwrap_or("notification-sound")
        .to_string();
    let supplied_type = field.content_type().unwrap_or("").to_string();
    let Some((extension, mime_type)) = notification_sound_type(&supplied_type, &file_name) else {
        return api_json(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            json!({"response":"notification sound must be MP3, WAV, OGG, WebM audio, M4A, or AAC"}),
        );
    };
    let bytes = match field.bytes().await {
        Ok(bytes) if !bytes.is_empty() && bytes.len() <= 5 * 1024 * 1024 => bytes.to_vec(),
        Ok(_) => {
            return api_json(
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({"response":"notification sound must be between 1 byte and 5 MiB"}),
            );
        }
        Err(_) => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"notification sound upload could not be read"}),
            );
        }
    };
    let user_uid = claims.uid;
    let object_key = format!("__mx/notification-sounds/{user_uid}/sound.{extension}");
    let token = match n1_access_token().await {
        Ok(token) => token,
        Err(_) => {
            return api_json(
                StatusCode::BAD_GATEWAY,
                json!({"response":"N1 is unavailable; the sound was not changed"}),
            );
        }
    };
    for directory in [
        "__mx".to_string(),
        "__mx/notification-sounds".to_string(),
        format!("__mx/notification-sounds/{user_uid}"),
    ] {
        if n1_ensure_directory(&directory, &token).await.is_err() {
            return api_json(
                StatusCode::BAD_GATEWAY,
                json!({"response":"N1 could not prepare notification sound storage"}),
            );
        }
    }
    if n1_upload_one_shot(&object_key, mime_type, bytes.clone(), &token)
        .await
        .is_err()
    {
        return api_json(
            StatusCode::BAD_GATEWAY,
            json!({"response":"N1 could not store the notification sound"}),
        );
    }
    let now = chrono::Utc::now().timestamp_millis();
    let stored_uid = user_uid.clone();
    let stored_key = object_key.clone();
    let stored_name = file_name.clone();
    let stored_type = mime_type.to_string();
    let size = bytes.len() as i64;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_preferences_schema(connection)?;
            let old_key = connection
                .query_row(
                    "SELECT object_key FROM mx_user_notification_sounds WHERE user_uid=?1",
                    params![stored_uid],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            connection.execute(
                "INSERT INTO mx_user_notification_sounds(user_uid,object_key,file_name,mime_type,size,updated_at) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(user_uid) DO UPDATE SET object_key=excluded.object_key,file_name=excluded.file_name,mime_type=excluded.mime_type,size=excluded.size,updated_at=excluded.updated_at",
                params![stored_uid,stored_key,stored_name,stored_type,size,now],
            )?;
            Ok(old_key)
        })
    })
    .await;
    let old_key = match result {
        Ok(Ok(value)) => value,
        _ => {
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"sound was stored but its metadata could not be saved"}),
            );
        }
    };
    if let Some(old_key) = old_key.filter(|key| key != &object_key) {
        let _ = n1_soft_delete(&old_key).await;
    }
    notification_sound_cache().write().await.insert(
        user_uid.clone(),
        CachedNotificationSound {
            bytes,
            file_name: file_name.clone(),
            mime_type: mime_type.to_string(),
            updated_at: now,
        },
    );
    publish_user_event(
        &user_uid,
        "preferences.updated",
        Some(&user_uid),
        json!({"notification_sound":true,"updated_at":now}),
    );
    api_json(
        StatusCode::OK,
        notification_sound_json(&file_name, mime_type, size, now),
    )
}

pub async fn get_notification_sound(claims: Claims) -> Response {
    let user_uid = claims.uid;
    if let Some(sound) = notification_sound_cache()
        .read()
        .await
        .get(&user_uid)
        .cloned()
    {
        return notification_sound_response(sound);
    }
    let lookup_uid = user_uid.clone();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_preferences_schema(connection)?;
            connection
                .query_row(
                    "SELECT object_key,file_name,mime_type,updated_at FROM mx_user_notification_sounds WHERE user_uid=?1",
                    params![lookup_uid],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?)),
                )
                .optional()
        })
    })
    .await;
    let Ok(Ok(Some((object_key, file_name, mime_type, updated_at)))) = result else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let bytes = match n1_download(&object_key, &file_name).await {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };
    let sound = CachedNotificationSound {
        bytes,
        file_name,
        mime_type,
        updated_at,
    };
    notification_sound_cache()
        .write()
        .await
        .insert(user_uid, sound.clone());
    notification_sound_response(sound)
}

fn notification_sound_response(sound: CachedNotificationSound) -> Response {
    let safe_name = sound.file_name.replace('"', "_");
    let mut response = Response::new(Body::from(sound.bytes));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&sound.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    if let Ok(value) = HeaderValue::from_str(&format!("inline; filename=\"{safe_name}\"")) {
        response.headers_mut().insert(CONTENT_DISPOSITION, value);
    }
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    if let Ok(value) = HeaderValue::from_str(&format!("\"{}\"", sound.updated_at)) {
        response.headers_mut().insert("etag", value);
    }
    response
}

pub async fn warm_notification_sound_cache() {
    let result = tokio::task::spawn_blocking(|| {
        with_sql_connection(|connection| {
            ensure_preferences_schema(connection)?;
            let mut statement = connection.prepare(
                "SELECT user_uid,object_key,file_name,mime_type,updated_at FROM mx_user_notification_sounds",
            )?;
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()
        })
    })
    .await;
    let Ok(Ok(sounds)) = result else {
        return;
    };
    for (user_uid, object_key, file_name, mime_type, updated_at) in sounds {
        if let Ok(bytes) = n1_download(&object_key, &file_name).await {
            notification_sound_cache().write().await.insert(
                user_uid,
                CachedNotificationSound {
                    bytes,
                    file_name,
                    mime_type,
                    updated_at,
                },
            );
        }
    }
}

pub async fn delete_notification_sound(claims: Claims) -> Response {
    let user_uid = claims.uid;
    let delete_uid = user_uid.clone();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_preferences_schema(connection)?;
            let object_key = connection
                .query_row(
                    "SELECT object_key FROM mx_user_notification_sounds WHERE user_uid=?1",
                    params![delete_uid],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            connection.execute(
                "DELETE FROM mx_user_notification_sounds WHERE user_uid=?1",
                params![delete_uid],
            )?;
            Ok(object_key)
        })
    })
    .await;
    let object_key = match result {
        Ok(Ok(value)) => value,
        _ => {
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to remove notification sound"}),
            );
        }
    };
    notification_sound_cache().write().await.remove(&user_uid);
    if let Some(object_key) = object_key {
        let _ = n1_soft_delete(&object_key).await;
    }
    publish_user_event(
        &user_uid,
        "preferences.updated",
        Some(&user_uid),
        json!({"notification_sound":false}),
    );
    api_json(
        StatusCode::OK,
        json!({"response":"custom notification sound removed"}),
    )
}

pub async fn get_preferences(claims: Claims) -> Response {
    let uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_preferences_schema(connection)?;
            let row = connection
                .query_row(
                    "SELECT revision, preferences_json, updated_at FROM mx_user_preferences WHERE user_uid = ?1",
                    params![uid],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)),
                )
                .optional()?;
            let (revision, text, updated_at) = row.unwrap_or((0, "{}".to_string(), 0));
            let preferences = serde_json::from_str::<UserPreferences>(&text).unwrap_or_default();
            Ok((revision, preferences, updated_at))
        })
    })
    .await;

    match result {
        Ok(Ok((revision, preferences, updated_at))) => api_json(
            StatusCode::OK,
            json!({"revision":revision,"preferences":preferences,"updated_at":updated_at}),
        ),
        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "preferences", "get_preferences()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to load account preferences"}),
            )
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "preferences", "get_preferences()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"account preference task failed"}),
            )
        }
    }
}

pub async fn save_preferences(
    claims: Claims,
    Json(request): Json<SavePreferencesRequest>,
) -> Response {
    if let Err(message) = validate(&request.preferences) {
        return api_json(StatusCode::BAD_REQUEST, json!({"response":message}));
    }
    let uid = claims.uid.clone();
    let event_uid = claims.uid;
    let text = match serde_json::to_string(&request.preferences) {
        Ok(text) => text,
        Err(_) => {
            return api_json(
                StatusCode::BAD_REQUEST,
                json!({"response":"invalid preferences"}),
            );
        }
    };
    let preferences = request.preferences;
    let result = tokio::task::spawn_blocking(move || -> Result<(i64, i64), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_preferences_schema(connection)?;
            let now = chrono::Utc::now().timestamp_millis();
            connection.execute(
                r#"
                INSERT INTO mx_user_preferences(user_uid, revision, preferences_json, updated_at)
                VALUES (?1, 1, ?2, ?3)
                ON CONFLICT(user_uid) DO UPDATE SET
                    revision = mx_user_preferences.revision + 1,
                    preferences_json = excluded.preferences_json,
                    updated_at = excluded.updated_at
                "#,
                params![uid, text, now],
            )?;
            let revision = connection.query_row(
                "SELECT revision FROM mx_user_preferences WHERE user_uid = ?1",
                params![uid],
                |row| row.get(0),
            )?;
            Ok((revision, now))
        })
    })
    .await;

    match result {
        Ok(Ok((revision, updated_at))) => {
            publish_user_event(
                &event_uid,
                "preferences.updated",
                Some(&event_uid),
                json!({"revision":revision}),
            );
            api_json(
                StatusCode::OK,
                json!({
                    "response":"preferences saved",
                    "revision":revision,
                    "preferences":preferences,
                    "updated_at":updated_at
                }),
            )
        }
        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "preferences", "save_preferences()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to save account preferences"}),
            )
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "preferences", "save_preferences()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"account preference task failed"}),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_supported_preference_ranges() {
        let mut preferences = UserPreferences::default();
        assert!(preferences.notifications.sound_enabled);
        assert_eq!(
            preferences.notifications.sound_source.as_deref(),
            Some("default")
        );
        assert_eq!(preferences.notifications.sound_volume, 70);
        preferences.theme = Some("dark".to_string());
        preferences.accent_color = Some("#2563eb".to_string());
        preferences.ui_scale_percent = Some(100);
        preferences.font_scale_percent = Some(110);
        assert!(validate(&preferences).is_ok());

        preferences.ui_scale_percent = Some(10);
        assert!(validate(&preferences).is_err());

        preferences.ui_scale_percent = Some(100);
        preferences.accent_color = Some("blue".to_string());
        assert!(validate(&preferences).is_err());
        preferences.accent_color = None;
        preferences.notifications.sound_source = Some("remote-url".to_string());
        assert!(validate(&preferences).is_err());
        preferences.notifications.sound_source = Some("custom".to_string());
        preferences.notifications.sound_volume = 101;
        assert!(validate(&preferences).is_err());
    }

    #[test]
    fn accepts_only_browser_audio_notification_formats() {
        assert_eq!(
            notification_sound_type("audio/mpeg", "sound.bin"),
            Some(("mp3", "audio/mpeg"))
        );
        assert_eq!(
            notification_sound_type("application/octet-stream", "sound.ogg"),
            Some(("ogg", "audio/ogg"))
        );
        assert_eq!(notification_sound_type("text/plain", "sound.txt"), None);
    }
}
