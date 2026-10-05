use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

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
    api::{collaboration::ensure_collaboration_schema, live::publish_user_event},
    config::load_config::CONFIG,
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

const CALL_PARTICIPANT_TTL: Duration = Duration::from_secs(90);
const MAX_SIGNAL_BYTES: usize = 96 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CallParticipant {
    pub user_uid: String,
    /// Identifies this specific browser join. A user may leave and rejoin the
    /// same room while delayed HTTP/WebSocket traffic from the previous join
    /// is still in flight, so user_uid alone is not enough to protect the new
    /// media session.
    pub session_uid: String,
    pub user_name: String,
    pub profile_photo_updated_at: Option<i64>,
    pub audio_enabled: bool,
    pub video_enabled: bool,
    pub screen_sharing: bool,
    pub joined_at: i64,
}

#[derive(Debug, Clone)]
struct ActiveParticipant {
    participant: CallParticipant,
    last_seen: Instant,
}

#[derive(Debug)]
struct CallRoom {
    started_at: i64,
    mode: String,
    participants: HashMap<String, ActiveParticipant>,
}

#[derive(Debug, Default)]
struct CallRegistry {
    rooms: HashMap<String, CallRoom>,
}

#[derive(Debug, Default)]
struct PrunedCall {
    participants: Vec<CallParticipant>,
    ended: bool,
}

#[derive(Debug)]
struct CallAccess {
    actor_name: String,
    profile_photo_updated_at: Option<i64>,
    channel_name: String,
    channel_kind: String,
    recipients: Vec<String>,
}

#[derive(Debug)]
struct CallChannel {
    uid: String,
    name: String,
    kind: String,
    recipients: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct JoinCallRequest {
    pub mode: String,
    pub session_uid: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCallRequest {
    pub session_uid: String,
    pub audio_enabled: Option<bool>,
    pub video_enabled: Option<bool>,
    pub screen_sharing: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CallSignalRequest {
    pub recipient_uid: String,
    pub sender_session_uid: String,
    pub recipient_session_uid: String,
    pub kind: String,
    pub data: Value,
}

#[derive(Debug, Deserialize)]
pub struct LeaveCallRequest {
    pub session_uid: String,
}

static CALL_REGISTRY: OnceLock<Mutex<CallRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<CallRegistry> {
    CALL_REGISTRY.get_or_init(|| Mutex::new(CallRegistry::default()))
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn sorted_participants(room: Option<&CallRoom>) -> Vec<CallParticipant> {
    let mut participants = room
        .into_iter()
        .flat_map(|room| room.participants.values())
        .map(|entry| entry.participant.clone())
        .collect::<Vec<_>>();
    participants.sort_by(|left, right| {
        left.joined_at
            .cmp(&right.joined_at)
            .then_with(|| left.user_name.cmp(&right.user_name))
            .then_with(|| left.user_uid.cmp(&right.user_uid))
    });
    participants
}

fn remove_expired(room: &mut CallRoom, now: Instant) -> Vec<CallParticipant> {
    let expired = room
        .participants
        .iter()
        .filter(|(_, entry)| now.duration_since(entry.last_seen) > CALL_PARTICIPANT_TTL)
        .map(|(uid, _)| uid.clone())
        .collect::<Vec<_>>();
    expired
        .into_iter()
        .filter_map(|uid| {
            room.participants
                .remove(&uid)
                .map(|entry| entry.participant)
        })
        .collect()
}

fn prune_channel(registry: &mut CallRegistry, channel_uid: &str) -> PrunedCall {
    let participants = registry
        .rooms
        .get_mut(channel_uid)
        .map(|room| remove_expired(room, Instant::now()))
        .unwrap_or_default();
    let ended = registry
        .rooms
        .get(channel_uid)
        .is_some_and(|room| room.participants.is_empty());
    if ended {
        registry.rooms.remove(channel_uid);
    }
    PrunedCall {
        participants,
        ended,
    }
}

async fn call_access(channel_uid: String, user_uid: String) -> Result<Option<CallAccess>, String> {
    tokio::task::spawn_blocking(move || -> Result<Option<CallAccess>, SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            channel_call_access(connection, &channel_uid, &user_uid)
        })
    })
    .await
    .map_err(|error| format!("call membership task failed: {error}"))?
    .map_err(|error| format!("call membership lookup failed: {error}"))
}

fn channel_call_access(
    connection: &rusqlite::Connection,
    channel_uid: &str,
    user_uid: &str,
) -> rusqlite::Result<Option<CallAccess>> {
    let actor = connection
        .query_row(
            r#"
            SELECT user.name, photo.updated_at, channel.name, channel.kind
            FROM mx_channel_members member
            JOIN users user ON user.uid = member.user_uid
            JOIN mx_channels channel ON channel.uid = member.channel_uid
            LEFT JOIN mx_user_profile_photos photo ON photo.user_uid = user.uid
            WHERE member.channel_uid = ?1 AND member.user_uid = ?2 AND channel.archived_at IS NULL
            "#,
            params![channel_uid, user_uid],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((actor_name, profile_photo_updated_at, channel_name, channel_kind)) = actor else {
        return Ok(None);
    };
    let mut statement = connection.prepare(
        "SELECT user_uid FROM mx_channel_members WHERE channel_uid = ?1 ORDER BY user_uid",
    )?;
    let recipients = statement
        .query_map(params![channel_uid], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(CallAccess {
        actor_name,
        profile_photo_updated_at,
        channel_name,
        channel_kind,
        recipients,
    }))
}

async fn member_call_channels(user_uid: String) -> Result<Vec<CallChannel>, String> {
    tokio::task::spawn_blocking(move || -> Result<Vec<CallChannel>, SqliteDatabaseError> {
        with_sql_connection(|connection| {
            ensure_collaboration_schema(connection)?;
            member_call_channels_for_connection(connection, &user_uid)
        })
    })
    .await
    .map_err(|error| format!("active call channel task failed: {error}"))?
    .map_err(|error| format!("active call channel lookup failed: {error}"))
}

fn member_call_channels_for_connection(
    connection: &rusqlite::Connection,
    user_uid: &str,
) -> rusqlite::Result<Vec<CallChannel>> {
    let mut statement = connection.prepare(
        r#"
        SELECT channel.uid, channel.name, channel.kind,
               GROUP_CONCAT(recipient.user_uid, ',')
        FROM mx_channels channel
        JOIN mx_channel_members viewer ON viewer.channel_uid = channel.uid
        JOIN mx_channel_members recipient ON recipient.channel_uid = channel.uid
        WHERE viewer.user_uid = ?1 AND channel.archived_at IS NULL
        GROUP BY channel.uid, channel.name, channel.kind
        ORDER BY channel.name COLLATE NOCASE, channel.uid
        "#,
    )?;
    statement
        .query_map(params![user_uid], |row| {
            Ok(CallChannel {
                uid: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                recipients: row
                    .get::<_, String>(3)?
                    .split(',')
                    .map(str::to_string)
                    .collect(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()
}

fn valid_signal_kind(kind: &str) -> bool {
    matches!(kind, "offer" | "answer" | "ice")
}

fn valid_session_uid(session_uid: &str) -> bool {
    Uuid::parse_str(session_uid).is_ok()
}

fn valid_signal_data(data: &Value) -> bool {
    data.is_object() && serde_json::to_vec(data).is_ok_and(|bytes| bytes.len() <= MAX_SIGNAL_BYTES)
}

fn signal_sessions_match(
    room: &CallRoom,
    sender_uid: &str,
    sender_session_uid: &str,
    recipient_uid: &str,
    recipient_session_uid: &str,
) -> bool {
    room.participants
        .get(sender_uid)
        .is_some_and(|entry| entry.participant.session_uid == sender_session_uid)
        && room
            .participants
            .get(recipient_uid)
            .is_some_and(|entry| entry.participant.session_uid == recipient_session_uid)
}

fn publish_to_channel(
    recipients: &[String],
    event_type: &str,
    actor_uid: Option<&str>,
    payload: &Value,
) {
    for recipient_uid in recipients {
        publish_user_event(recipient_uid, event_type, actor_uid, payload.clone());
    }
}

fn publish_pruned(channel_uid: &str, recipients: &[String], pruned: PrunedCall) {
    for participant in pruned.participants {
        publish_to_channel(
            recipients,
            "call.participant.left",
            Some(&participant.user_uid),
            &json!({
                "channel_uid": channel_uid,
                "user_uid": participant.user_uid,
                "session_uid": participant.session_uid,
                "reason": "timeout"
            }),
        );
    }
    if pruned.ended {
        publish_to_channel(
            recipients,
            "call.ended",
            None,
            &json!({"channel_uid":channel_uid,"reason":"empty"}),
        );
    }
}

fn ice_servers() -> Vec<Value> {
    let mut servers = Vec::new();
    if !CONFIG.webrtc.stun_urls.is_empty() {
        servers.push(json!({"urls":CONFIG.webrtc.stun_urls}));
    }
    if !CONFIG.webrtc.turn_urls.is_empty() {
        let mut server = json!({"urls":CONFIG.webrtc.turn_urls});
        if let Some(object) = server.as_object_mut() {
            if !CONFIG.webrtc.turn_username.is_empty() {
                object.insert(
                    "username".to_string(),
                    Value::String(CONFIG.webrtc.turn_username.clone()),
                );
            }
            if !CONFIG.webrtc.turn_credential.is_empty() {
                object.insert(
                    "credential".to_string(),
                    Value::String(CONFIG.webrtc.turn_credential.clone()),
                );
            }
        }
        servers.push(server);
    }
    servers
}

fn call_state_json(channel_uid: &str, registry: &CallRegistry) -> Value {
    let room = registry.rooms.get(channel_uid);
    json!({
        "channel_uid": channel_uid,
        "active": room.is_some_and(|room| !room.participants.is_empty()),
        "started_at": room.map(|room| room.started_at),
        "mode": room.map(|room| room.mode.as_str()),
        "participants": sorted_participants(room),
        "ice_servers": ice_servers(),
        "max_participants": CONFIG.webrtc.max_participants,
        "heartbeat_seconds": 15
    })
}

pub async fn list_active_calls(claims: Claims) -> Response {
    let channels = match member_call_channels(claims.uid).await {
        Ok(channels) => channels,
        Err(error) => {
            crate::report_error!(error, "calls", "list_active_calls()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to load active calls"}),
            );
        }
    };
    let (calls, pruned_calls) = {
        let mut registry = registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut pruned_calls = Vec::new();
        for channel in &channels {
            let pruned = prune_channel(&mut registry, &channel.uid);
            if !pruned.participants.is_empty() || pruned.ended {
                pruned_calls.push((channel.uid.clone(), channel.recipients.clone(), pruned));
            }
        }
        let active = channels
            .into_iter()
            .filter_map(|channel| {
                let room = registry.rooms.get(&channel.uid)?;
                Some(json!({
                    "channel_uid": channel.uid,
                    "channel_name": channel.name,
                    "channel_kind": channel.kind,
                    "mode": room.mode,
                    "started_at": room.started_at,
                    "participants": sorted_participants(Some(room))
                }))
            })
            .collect::<Vec<_>>();
        (active, pruned_calls)
    };
    for (channel_uid, recipients, pruned) in pruned_calls {
        publish_pruned(&channel_uid, &recipients, pruned);
    }
    api_json(StatusCode::OK, json!({"calls":calls}))
}

pub async fn get_call_state(claims: Claims, Path(channel_uid): Path<String>) -> Response {
    let user_uid = claims.uid;
    let access = match call_access(channel_uid.clone(), user_uid).await {
        Ok(Some(access)) => access,
        Ok(None) => {
            return api_json(
                StatusCode::FORBIDDEN,
                json!({"response":"you are not a member of this conversation"}),
            );
        }
        Err(error) => {
            crate::report_error!(error, "calls", "get_call_state()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to load call state"}),
            );
        }
    };
    let (state, pruned) = {
        let mut calls = registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let pruned = prune_channel(&mut calls, &channel_uid);
        (call_state_json(&channel_uid, &calls), pruned)
    };
    publish_pruned(&channel_uid, &access.recipients, pruned);
    api_json(StatusCode::OK, state)
}

pub async fn join_call(
    claims: Claims,
    Path(channel_uid): Path<String>,
    Json(request): Json<JoinCallRequest>,
) -> Response {
    if !matches!(request.mode.as_str(), "voice" | "video") {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"call mode must be voice or video"}),
        );
    }
    if !valid_session_uid(&request.session_uid) {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"call session identifier is invalid"}),
        );
    }
    let user_uid = claims.uid;
    let access = match call_access(channel_uid.clone(), user_uid.clone()).await {
        Ok(Some(access)) => access,
        Ok(None) => {
            return api_json(
                StatusCode::FORBIDDEN,
                json!({"response":"you are not a member of this conversation"}),
            );
        }
        Err(error) => {
            crate::report_error!(error, "calls", "join_call()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to join the call"}),
            );
        }
    };

    let now = now_millis();
    let (state, participant, joined, started, pruned) = {
        let mut calls = registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let pruned = prune_channel(&mut calls, &channel_uid);
        let started = !calls.rooms.contains_key(&channel_uid);
        let room = calls
            .rooms
            .entry(channel_uid.clone())
            .or_insert_with(|| CallRoom {
                started_at: now,
                mode: request.mode.clone(),
                participants: HashMap::new(),
            });
        let previous_session_uid = room
            .participants
            .get(&user_uid)
            .map(|entry| entry.participant.session_uid.as_str());
        let joined = previous_session_uid != Some(request.session_uid.as_str());
        let new_account = previous_session_uid.is_none();
        if new_account && room.participants.len() >= CONFIG.webrtc.max_participants {
            return api_json(
                StatusCode::CONFLICT,
                json!({"response":"this call has reached its participant limit"}),
            );
        }
        let joined_at = room
            .participants
            .get(&user_uid)
            .filter(|entry| entry.participant.session_uid == request.session_uid)
            .map(|entry| entry.participant.joined_at)
            .unwrap_or(now);
        let participant = CallParticipant {
            user_uid: user_uid.clone(),
            session_uid: request.session_uid.clone(),
            user_name: access.actor_name.clone(),
            profile_photo_updated_at: access.profile_photo_updated_at,
            audio_enabled: true,
            video_enabled: request.mode == "video",
            screen_sharing: false,
            joined_at,
        };
        room.participants.insert(
            user_uid.clone(),
            ActiveParticipant {
                participant: participant.clone(),
                last_seen: Instant::now(),
            },
        );
        (
            call_state_json(&channel_uid, &calls),
            participant,
            joined,
            started,
            pruned,
        )
    };
    publish_pruned(&channel_uid, &access.recipients, pruned);
    if started {
        publish_to_channel(
            &access.recipients,
            "call.started",
            Some(&user_uid),
            &json!({
                "channel_uid": channel_uid,
                "channel_name": access.channel_name,
                "channel_kind": access.channel_kind,
                "mode": request.mode,
                "started_at": now,
                "participant": participant
            }),
        );
    }
    publish_to_channel(
        &access.recipients,
        if joined {
            "call.participant.joined"
        } else {
            "call.participant.updated"
        },
        Some(&user_uid),
        &json!({"channel_uid":channel_uid,"participant":participant}),
    );
    api_json(StatusCode::OK, state)
}

pub async fn update_call_participant(
    claims: Claims,
    Path(channel_uid): Path<String>,
    Json(request): Json<UpdateCallRequest>,
) -> Response {
    if !valid_session_uid(&request.session_uid) {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"call session identifier is invalid"}),
        );
    }
    let user_uid = claims.uid;
    let access = match call_access(channel_uid.clone(), user_uid.clone()).await {
        Ok(Some(access)) => access,
        Ok(None) => {
            return api_json(
                StatusCode::FORBIDDEN,
                json!({"response":"you are not a member of this conversation"}),
            );
        }
        Err(error) => {
            crate::report_error!(error, "calls", "update_call_participant()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to update call state"}),
            );
        }
    };
    let (state, participant, pruned, changed) = {
        let mut calls = registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let pruned = prune_channel(&mut calls, &channel_uid);
        let Some(entry) = calls
            .rooms
            .get_mut(&channel_uid)
            .and_then(|room| room.participants.get_mut(&user_uid))
        else {
            return api_json(
                StatusCode::CONFLICT,
                json!({"response":"join the call before updating media state"}),
            );
        };
        if entry.participant.session_uid != request.session_uid {
            return api_json(
                StatusCode::CONFLICT,
                json!({"response":"this call session was replaced by a newer join"}),
            );
        }
        let mut changed = false;
        if let Some(value) = request.audio_enabled {
            changed |= entry.participant.audio_enabled != value;
            entry.participant.audio_enabled = value;
        }
        if let Some(value) = request.video_enabled {
            changed |= entry.participant.video_enabled != value;
            entry.participant.video_enabled = value;
        }
        if let Some(value) = request.screen_sharing {
            changed |= entry.participant.screen_sharing != value;
            entry.participant.screen_sharing = value;
            if value {
                changed |= !entry.participant.video_enabled;
                entry.participant.video_enabled = true;
            }
        }
        entry.last_seen = Instant::now();
        let participant = entry.participant.clone();
        (
            call_state_json(&channel_uid, &calls),
            participant,
            pruned,
            changed,
        )
    };
    publish_pruned(&channel_uid, &access.recipients, pruned);
    if changed {
        publish_to_channel(
            &access.recipients,
            "call.participant.updated",
            Some(&user_uid),
            &json!({"channel_uid":channel_uid,"participant":participant}),
        );
    }
    api_json(StatusCode::OK, state)
}

pub async fn leave_call(
    claims: Claims,
    Path(channel_uid): Path<String>,
    Query(request): Query<LeaveCallRequest>,
) -> Response {
    if !valid_session_uid(&request.session_uid) {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"call session identifier is invalid"}),
        );
    }
    let user_uid = claims.uid;
    let access = match call_access(channel_uid.clone(), user_uid.clone()).await {
        Ok(Some(access)) => access,
        Ok(None) => {
            evict_call_participant(
                &channel_uid,
                &user_uid,
                &[],
                Some(&user_uid),
                "access_revoked",
                Some(&request.session_uid),
            );
            return StatusCode::NO_CONTENT.into_response();
        }
        Err(error) => {
            crate::report_error!(error, "calls", "leave_call()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to leave the call"}),
            );
        }
    };
    evict_call_participant(
        &channel_uid,
        &user_uid,
        &access.recipients,
        Some(&user_uid),
        "left",
        Some(&request.session_uid),
    );
    StatusCode::NO_CONTENT.into_response()
}

pub async fn relay_call_signal(
    claims: Claims,
    Path(channel_uid): Path<String>,
    Json(request): Json<CallSignalRequest>,
) -> Response {
    if request.recipient_uid == claims.uid {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"call signals require another participant"}),
        );
    }
    if !valid_signal_kind(&request.kind) {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"unsupported call signal type"}),
        );
    }
    if !valid_session_uid(&request.sender_session_uid)
        || !valid_session_uid(&request.recipient_session_uid)
    {
        return api_json(
            StatusCode::BAD_REQUEST,
            json!({"response":"call signal session identifier is invalid"}),
        );
    }
    if !valid_signal_data(&request.data) {
        return api_json(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({"response":"call signal is invalid or too large"}),
        );
    }
    let user_uid = claims.uid;
    let access = match call_access(channel_uid.clone(), user_uid.clone()).await {
        Ok(Some(access)) => access,
        Ok(None) => {
            return api_json(
                StatusCode::FORBIDDEN,
                json!({"response":"you are not a member of this conversation"}),
            );
        }
        Err(error) => {
            crate::report_error!(error, "calls", "relay_call_signal()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to relay call signal"}),
            );
        }
    };
    if !access.recipients.contains(&request.recipient_uid) {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"the signal recipient is not in this conversation"}),
        );
    }
    let (room_active, allowed, pruned) = {
        let mut calls = registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let pruned = prune_channel(&mut calls, &channel_uid);
        let allowed = calls.rooms.get_mut(&channel_uid).is_some_and(|room| {
            let matches = signal_sessions_match(
                room,
                &user_uid,
                &request.sender_session_uid,
                &request.recipient_uid,
                &request.recipient_session_uid,
            );
            if matches && let Some(sender) = room.participants.get_mut(&user_uid) {
                sender.last_seen = Instant::now();
            }
            matches
        });
        (calls.rooms.contains_key(&channel_uid), allowed, pruned)
    };
    publish_pruned(&channel_uid, &access.recipients, pruned);
    if !room_active {
        return api_json(
            StatusCode::CONFLICT,
            json!({"response":"this call is no longer active"}),
        );
    }
    if !allowed {
        return api_json(
            StatusCode::CONFLICT,
            json!({"response":"both accounts must join before signaling"}),
        );
    }
    publish_user_event(
        &request.recipient_uid,
        "call.signal",
        Some(&user_uid),
        json!({
            "channel_uid": channel_uid,
            "sender_uid": user_uid,
            "sender_session_uid": request.sender_session_uid,
            "recipient_session_uid": request.recipient_session_uid,
            "kind": request.kind,
            "data": request.data
        }),
    );
    StatusCode::ACCEPTED.into_response()
}

pub fn evict_call_participant(
    channel_uid: &str,
    user_uid: &str,
    recipients: &[String],
    actor_uid: Option<&str>,
    reason: &str,
    expected_session_uid: Option<&str>,
) {
    let (removed, call_ended) = {
        let mut calls = registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        remove_registry_participant(&mut calls, channel_uid, user_uid, expected_session_uid)
    };
    if let Some(participant) = removed {
        let payload = json!({"channel_uid":channel_uid,"user_uid":user_uid,"session_uid":participant.session_uid,"reason":reason});
        publish_to_channel(recipients, "call.participant.left", actor_uid, &payload);
        if !recipients.iter().any(|recipient| recipient == user_uid) {
            publish_user_event(user_uid, "call.participant.left", actor_uid, payload);
        }
        if call_ended {
            let payload = json!({"channel_uid":channel_uid,"reason":"empty"});
            publish_to_channel(recipients, "call.ended", actor_uid, &payload);
            if !recipients.iter().any(|recipient| recipient == user_uid) {
                publish_user_event(user_uid, "call.ended", actor_uid, payload);
            }
        }
    }
}

fn remove_registry_participant(
    registry: &mut CallRegistry,
    channel_uid: &str,
    user_uid: &str,
    expected_session_uid: Option<&str>,
) -> (Option<CallParticipant>, bool) {
    let removed = registry
        .rooms
        .get_mut(channel_uid)
        .and_then(|room| remove_participant_if_session(room, user_uid, expected_session_uid))
        .map(|entry| entry.participant);
    let ended = removed.is_some()
        && registry
            .rooms
            .get(channel_uid)
            .is_some_and(|room| room.participants.is_empty());
    if ended {
        registry.rooms.remove(channel_uid);
    }
    (removed, ended)
}

fn remove_participant_if_session(
    room: &mut CallRoom,
    user_uid: &str,
    expected_session_uid: Option<&str>,
) -> Option<ActiveParticipant> {
    let matches = expected_session_uid.is_none_or(|expected| {
        room.participants
            .get(user_uid)
            .is_some_and(|entry| entry.participant.session_uid == expected)
    });
    matches
        .then(|| room.participants.remove(user_uid))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn participant(uid: &str, joined_at: i64) -> ActiveParticipant {
        participant_with_session(uid, joined_at, &Uuid::new_v4().to_string())
    }

    fn participant_with_session(uid: &str, joined_at: i64, session_uid: &str) -> ActiveParticipant {
        ActiveParticipant {
            participant: CallParticipant {
                user_uid: uid.to_string(),
                session_uid: session_uid.to_string(),
                user_name: format!("User {uid}"),
                profile_photo_updated_at: None,
                audio_enabled: true,
                video_enabled: false,
                screen_sharing: false,
                joined_at,
            },
            last_seen: Instant::now(),
        }
    }

    #[test]
    fn call_participants_are_stable_and_sorted() {
        let mut room = CallRoom {
            started_at: 1,
            mode: "voice".to_string(),
            participants: HashMap::new(),
        };
        room.participants
            .insert("later".to_string(), participant("later", 20));
        room.participants
            .insert("first".to_string(), participant("first", 10));
        let sorted = sorted_participants(Some(&room));
        assert_eq!(
            sorted
                .iter()
                .map(|item| item.user_uid.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "later"]
        );
    }

    #[test]
    fn delayed_leave_cannot_remove_a_rejoined_session() {
        let old_session = Uuid::new_v4().to_string();
        let new_session = Uuid::new_v4().to_string();
        let mut room = CallRoom {
            started_at: 1,
            mode: "video".to_string(),
            participants: HashMap::from([(
                "member".to_string(),
                participant_with_session("member", 20, &new_session),
            )]),
        };

        assert!(remove_participant_if_session(&mut room, "member", Some(&old_session)).is_none());
        assert_eq!(
            room.participants
                .get("member")
                .map(|entry| entry.participant.session_uid.as_str()),
            Some(new_session.as_str())
        );
        assert!(remove_participant_if_session(&mut room, "member", Some(&new_session)).is_some());
        assert!(room.participants.is_empty());
    }

    #[test]
    fn call_session_identifiers_must_be_real_uuids() {
        assert!(valid_session_uid(&Uuid::new_v4().to_string()));
        assert!(!valid_session_uid("old-browser-session"));
        assert!(!valid_session_uid(""));
    }

    #[test]
    fn stale_signaling_cannot_attach_to_rejoined_participants() {
        let sender_session = Uuid::new_v4().to_string();
        let recipient_session = Uuid::new_v4().to_string();
        let stale_session = Uuid::new_v4().to_string();
        let room = CallRoom {
            started_at: 1,
            mode: "video".to_string(),
            participants: HashMap::from([
                (
                    "sender".to_string(),
                    participant_with_session("sender", 10, &sender_session),
                ),
                (
                    "recipient".to_string(),
                    participant_with_session("recipient", 20, &recipient_session),
                ),
            ]),
        };

        assert!(signal_sessions_match(
            &room,
            "sender",
            &sender_session,
            "recipient",
            &recipient_session
        ));
        assert!(!signal_sessions_match(
            &room,
            "sender",
            &stale_session,
            "recipient",
            &recipient_session
        ));
        assert!(!signal_sessions_match(
            &room,
            "sender",
            &sender_session,
            "recipient",
            &stale_session
        ));
    }

    #[test]
    fn stale_participants_are_removed_without_touching_active_people() {
        let mut stale = participant("stale", 10);
        stale.last_seen = Instant::now() - CALL_PARTICIPANT_TTL - Duration::from_secs(1);
        let mut room = CallRoom {
            started_at: 1,
            mode: "voice".to_string(),
            participants: HashMap::from([
                ("stale".to_string(), stale),
                ("active".to_string(), participant("active", 20)),
            ]),
        };
        let removed = remove_expired(&mut room, Instant::now());
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].user_uid, "stale");
        assert!(room.participants.contains_key("active"));
    }

    #[test]
    fn pruning_the_final_expired_participant_ends_and_removes_the_room() {
        let mut stale = participant("stale", 10);
        stale.last_seen = Instant::now() - CALL_PARTICIPANT_TTL - Duration::from_secs(1);
        let mut calls = CallRegistry {
            rooms: HashMap::from([(
                "room".to_string(),
                CallRoom {
                    started_at: 1,
                    mode: "voice".to_string(),
                    participants: HashMap::from([("stale".to_string(), stale)]),
                },
            )]),
        };

        let pruned = prune_channel(&mut calls, "room");
        assert!(pruned.ended);
        assert_eq!(pruned.participants.len(), 1);
        assert!(!calls.rooms.contains_key("room"));
    }

    #[test]
    fn final_explicit_leave_ends_room_but_stale_session_leave_does_not() {
        let session_uid = Uuid::new_v4().to_string();
        let mut calls = CallRegistry {
            rooms: HashMap::from([(
                "room".to_string(),
                CallRoom {
                    started_at: 1,
                    mode: "video".to_string(),
                    participants: HashMap::from([(
                        "member".to_string(),
                        participant_with_session("member", 10, &session_uid),
                    )]),
                },
            )]),
        };

        let stale_session = Uuid::new_v4().to_string();
        let (removed, ended) =
            remove_registry_participant(&mut calls, "room", "member", Some(&stale_session));
        assert!(removed.is_none());
        assert!(!ended);
        assert!(calls.rooms.contains_key("room"));

        let (removed, ended) =
            remove_registry_participant(&mut calls, "room", "member", Some(&session_uid));
        assert_eq!(
            removed.map(|item| item.user_uid),
            Some("member".to_string())
        );
        assert!(ended);
        assert!(!calls.rooms.contains_key("room"));
    }

    #[test]
    fn signal_contract_accepts_only_expected_types_and_bounded_objects() {
        for kind in ["offer", "answer", "ice"] {
            assert!(valid_signal_kind(kind));
        }
        for kind in ["hangup", "join", "candidate", ""] {
            assert!(!valid_signal_kind(kind));
        }
        let small = json!({"candidate":"candidate:1 1 UDP 1 127.0.0.1 1234 typ host"});
        assert!(valid_signal_data(&small));
        assert!(!valid_signal_data(&json!("not-an-object")));
        let oversized = json!({"sdp":"x".repeat(MAX_SIGNAL_BYTES + 1)});
        assert!(!valid_signal_data(&oversized));
    }

    #[test]
    fn call_access_is_strictly_scoped_to_channel_members() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
            CREATE TABLE users (uid TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL);
            INSERT INTO users(uid,name) VALUES ('member','Member User'),('other','Other User');
            "#,
            )
            .unwrap();
        ensure_collaboration_schema(&connection).unwrap();
        connection
            .execute_batch(
                r#"
            INSERT INTO mx_channels(uid,kind,name,description,created_by,created_at)
            VALUES ('room','group','Private room','','member',1),
                   ('other-room','group','Other room','','other',1);
            INSERT INTO mx_channel_members(channel_uid,user_uid,role,joined_at)
            VALUES ('room','member','owner',1),
                   ('other-room','other','owner',1);
            "#,
            )
            .unwrap();
        let member = channel_call_access(&connection, "room", "member")
            .unwrap()
            .expect("member access");
        assert_eq!(member.actor_name, "Member User");
        assert_eq!(member.channel_name, "Private room");
        assert_eq!(member.channel_kind, "group");
        assert_eq!(member.recipients, vec!["member"]);
        let visible_channels = member_call_channels_for_connection(&connection, "member").unwrap();
        assert_eq!(visible_channels.len(), 1);
        assert_eq!(visible_channels[0].uid, "room");
        assert_eq!(visible_channels[0].name, "Private room");
        assert_eq!(visible_channels[0].recipients, vec!["member"]);
        assert!(
            channel_call_access(&connection, "room", "other")
                .unwrap()
                .is_none()
        );
    }
}
