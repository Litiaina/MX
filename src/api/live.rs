use std::{
    collections::HashMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{
        Query,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures::{SinkExt, StreamExt};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::auth::Claims,
};

const LIVE_TICKET_TTL_MS: i64 = 60_000;
const PRESENCE_OFFLINE_GRACE_MS: u64 = 12_000;
const SOCKET_HEARTBEAT_MS: u64 = 20_000;
const SOCKET_STALE_MS: u64 = 65_000;
const EVENT_BUFFER_CAPACITY: usize = 2048;

#[derive(Debug, Clone)]
struct LiveTicket {
    uid: String,
    name: String,
    access_level: i64,
    auth_version: i64,
    expires_at: i64,
}

#[derive(Debug, Clone)]
struct PresenceState {
    name: String,
    access_level: i64,
    connections: u64,
    online: bool,
    generation: u64,
}

struct LiveHub {
    tickets: Mutex<HashMap<String, LiveTicket>>,
    presence: Mutex<HashMap<String, PresenceState>>,
    events: broadcast::Sender<String>,
    sequence: AtomicU64,
}

impl LiveHub {
    fn new() -> Self {
        let (events, _) = broadcast::channel(EVENT_BUFFER_CAPACITY);
        Self {
            tickets: Mutex::new(HashMap::new()),
            presence: Mutex::new(HashMap::new()),
            events,
            sequence: AtomicU64::new(0),
        }
    }
}

static LIVE_HUB: OnceLock<LiveHub> = OnceLock::new();

fn hub() -> &'static LiveHub {
    LIVE_HUB.get_or_init(LiveHub::new)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn access_name(access_level: i64) -> &'static str {
    match access_level {
        0 => "Administrator",
        1 => "Manager",
        2 => "Editor",
        3 => "Viewer",
        _ => "Unknown",
    }
}

fn api_json(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

/// Publish a small invalidation/change event after the authoritative database
/// transaction has committed. WebSocket messages are notifications, not the
/// source of truth; clients may always re-fetch through the normal MX API.
pub fn publish_live_event(event_type: &str, actor_uid: Option<&str>, payload: Value) {
    let sequence = hub().sequence.fetch_add(1, Ordering::Relaxed) + 1;
    let body = json!({
        "type": event_type,
        "sequence": sequence,
        "at": now_millis(),
        "actor_uid": actor_uid,
        "payload": payload,
    });

    if let Ok(text) = serde_json::to_string(&body) {
        // It is valid for there to be no connected receivers.
        let _ = hub().events.send(text);
    }
}

#[derive(Debug, Serialize)]
struct PresenceAccount {
    uid: String,
    name: String,
    access_level: i64,
    access_name: String,
}

fn online_presence_accounts() -> Vec<PresenceAccount> {
    let presence = hub()
        .presence
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    online_presence_accounts_from(&presence)
}

fn online_presence_accounts_from(
    presence: &HashMap<String, PresenceState>,
) -> Vec<PresenceAccount> {
    let mut accounts = presence
        .iter()
        .filter(|(_, state)| state.online)
        .map(|(uid, state)| PresenceAccount {
            uid: uid.clone(),
            name: state.name.clone(),
            access_level: state.access_level,
            access_name: access_name(state.access_level).to_string(),
        })
        .collect::<Vec<_>>();
    accounts.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.uid.cmp(&right.uid))
    });
    accounts
}

fn prune_expired_tickets(tickets: &mut HashMap<String, LiveTicket>, now: i64) {
    tickets.retain(|_, ticket| ticket.expires_at > now);
}

async fn load_ticket_identity(uid: String) -> Result<(String, i64, i64), String> {
    tokio::task::spawn_blocking(move || -> Result<(String, i64, i64), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            connection.query_row(
                "SELECT name, access_level, auth_version FROM users WHERE uid = ?1 LIMIT 1",
                params![uid],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
        })
    })
    .await
    .map_err(|error| format!("live ticket task failed: {error}"))?
    .map_err(|error| format!("live ticket account lookup failed: {error}"))
}

async fn live_credentials_current(uid: String, expected_auth_version: i64) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || -> Result<bool, SqliteDatabaseError> {
        with_sql_connection(|connection| {
            let auth_version = connection
                .query_row(
                    "SELECT auth_version FROM users WHERE uid = ?1 LIMIT 1",
                    params![uid],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;

            Ok(auth_version == Some(expected_auth_version))
        })
    })
    .await
    .map_err(|error| format!("live credential check task failed: {error}"))?
    .map_err(|error| format!("live credential check failed: {error}"))
}

/// Authenticated REST endpoint used to mint a one-time, short-lived ticket.
/// Native browser WebSocket does not allow setting an Authorization header, so
/// the long-lived JWT is never placed in the WebSocket URL.
pub async fn issue_live_ticket(claims: Claims) -> Response {
    if !claims.can_read_records() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"this account does not have MX live access"}),
        );
    }

    let (name, access_level, auth_version) = match load_ticket_identity(claims.uid.clone()).await {
        Ok(identity) => identity,
        Err(error) => {
            crate::report_error!(error, "live", "issue_live_ticket()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to prepare MX live connection"}),
            );
        }
    };

    // The normal auth middleware already validates this value. Check it again
    // here to close the small race between middleware validation and ticket
    // creation when credentials are changed concurrently.
    if auth_version != claims.auth_version {
        return api_json(
            StatusCode::UNAUTHORIZED,
            json!({"response":"session credentials have changed; sign in again"}),
        );
    }

    let now = now_millis();
    let expires_at = now.saturating_add(LIVE_TICKET_TTL_MS);
    let ticket = Uuid::new_v4().to_string();

    {
        let mut tickets = hub()
            .tickets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        prune_expired_tickets(&mut tickets, now);
        tickets.insert(
            ticket.clone(),
            LiveTicket {
                uid: claims.uid,
                name,
                access_level,
                auth_version,
                expires_at,
            },
        );
    }

    api_json(
        StatusCode::CREATED,
        json!({
            "ticket": ticket,
            "expires_at": expires_at,
            "heartbeat_ms": SOCKET_HEARTBEAT_MS,
            "stale_after_ms": SOCKET_STALE_MS,
        }),
    )
}

#[derive(Debug, Deserialize)]
pub struct LiveSocketQuery {
    ticket: String,
}

fn consume_ticket(ticket: &str) -> Option<LiveTicket> {
    let now = now_millis();
    let mut tickets = hub()
        .tickets
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune_expired_tickets(&mut tickets, now);
    tickets.remove(ticket).filter(|item| item.expires_at > now)
}

pub async fn live_socket(ws: WebSocketUpgrade, Query(query): Query<LiveSocketQuery>) -> Response {
    let Some(ticket) = consume_ticket(query.ticket.trim()) else {
        return api_json(
            StatusCode::UNAUTHORIZED,
            json!({"response":"invalid or expired MX live ticket"}),
        );
    };

    match live_credentials_current(ticket.uid.clone(), ticket.auth_version).await {
        Ok(true) => {}
        Ok(false) => {
            return api_json(
                StatusCode::UNAUTHORIZED,
                json!({"response":"session credentials have changed; sign in again"}),
            );
        }
        Err(error) => {
            crate::report_error!(error, "live", "live_socket()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to validate MX live session"}),
            );
        }
    }

    ws.on_upgrade(move |socket| run_live_socket(socket, ticket))
}

fn mark_presence_connected(ticket: &LiveTicket) {
    let became_online = {
        let mut presence = hub()
            .presence
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = presence
            .entry(ticket.uid.clone())
            .or_insert_with(|| PresenceState {
                name: ticket.name.clone(),
                access_level: ticket.access_level,
                connections: 0,
                online: false,
                generation: 0,
            });

        entry.name = ticket.name.clone();
        entry.access_level = ticket.access_level;
        entry.connections = entry.connections.saturating_add(1);
        entry.generation = entry.generation.saturating_add(1);
        let became_online = !entry.online;
        entry.online = true;
        became_online
    };

    if became_online {
        // The API is authoritative for the current online list. The event only
        // tells clients to refresh and deliberately contains no account data.
        publish_live_event("presence.changed", None, json!({}));
    }
}

fn mark_presence_disconnected(ticket: LiveTicket) {
    let generation = {
        let mut presence = hub()
            .presence
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(entry) = presence.get_mut(&ticket.uid) else {
            return;
        };
        entry.connections = entry.connections.saturating_sub(1);
        entry.generation = entry.generation.saturating_add(1);
        entry.generation
    };

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(PRESENCE_OFFLINE_GRACE_MS)).await;
        let went_offline = {
            let mut presence = hub()
                .presence
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let should_remove = presence.get(&ticket.uid).is_some_and(|entry| {
                entry.connections == 0 && entry.generation == generation && entry.online
            });
            if should_remove {
                presence.remove(&ticket.uid);
            }
            should_remove
        };

        if went_offline {
            publish_live_event("presence.changed", None, json!({}));
        }
    });
}

async fn run_live_socket(socket: WebSocket, ticket: LiveTicket) {
    mark_presence_connected(&ticket);

    let (mut sender, mut receiver) = socket.split();
    let mut events = hub().events.subscribe();
    let last_activity = std::sync::Arc::new(AtomicU64::new(now_millis().max(0) as u64));

    let hello = json!({
        "type": "live.ready",
        "sequence": hub().sequence.load(Ordering::Relaxed),
        "at": now_millis(),
        "actor_uid": Value::Null,
        "payload": {
            "uid": ticket.uid.clone(),
            "name": ticket.name.clone(),
            "access_level": ticket.access_level,
            "access_name": access_name(ticket.access_level),
            "heartbeat_ms": SOCKET_HEARTBEAT_MS,
            "stale_after_ms": SOCKET_STALE_MS,
        }
    })
    .to_string();

    if sender.send(Message::Text(hello.into())).await.is_err() {
        mark_presence_disconnected(ticket);
        return;
    }

    let outbound_activity = last_activity.clone();
    let credential_uid = ticket.uid.clone();
    let credential_auth_version = ticket.auth_version;
    let mut outbound = tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval(Duration::from_millis(SOCKET_HEARTBEAT_MS));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                event = events.recv() => {
                    match event {
                        Ok(text) => {
                            if sender.send(Message::Text(text.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            let sync = json!({
                                "type":"sync.required",
                                "sequence": hub().sequence.load(Ordering::Relaxed),
                                "at": now_millis(),
                                "actor_uid": Value::Null,
                                "payload": {"reason":"live_event_lag", "skipped": skipped}
                            }).to_string();
                            if sender.send(Message::Text(sync.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = heartbeat.tick() => {
                    match live_credentials_current(
                        credential_uid.clone(),
                        credential_auth_version,
                    ).await {
                        Ok(true) => {}
                        Ok(false) => {
                            let _ = sender.send(Message::Close(None)).await;
                            break;
                        }
                        Err(error) => {
                            crate::report_error!(error, "live", "run_live_socket()");
                            let _ = sender.send(Message::Close(None)).await;
                            break;
                        }
                    }

                    let now = now_millis().max(0) as u64;
                    let last = outbound_activity.load(Ordering::Relaxed);
                    if now.saturating_sub(last) > SOCKET_STALE_MS {
                        let _ = sender.send(Message::Close(None)).await;
                        break;
                    }

                    let heartbeat_message = json!({
                        "type":"live.heartbeat",
                        "sequence": hub().sequence.load(Ordering::Relaxed),
                        "at": now_millis(),
                        "actor_uid": Value::Null,
                        "payload": {}
                    }).to_string();

                    if sender.send(Message::Text(heartbeat_message.into())).await.is_err() {
                        break;
                    }

                    if sender.send(Message::Ping(Vec::<u8>::new().into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let inbound_activity = last_activity.clone();
    let mut inbound = tokio::spawn(async move {
        while let Some(message) = receiver.next().await {
            match message {
                Ok(Message::Close(_)) => break,
                Ok(Message::Pong(_)) | Ok(Message::Ping(_)) => {
                    inbound_activity.store(now_millis().max(0) as u64, Ordering::Relaxed);
                }
                Ok(Message::Text(_)) | Ok(Message::Binary(_)) => {
                    // Any client traffic is sufficient to prove liveness. The
                    // protocol intentionally has no client-side mutation path;
                    // authoritative writes continue to use REST/PATCH.
                    inbound_activity.store(now_millis().max(0) as u64, Ordering::Relaxed);
                }
                Err(_) => break,
            }
        }
    });

    tokio::select! {
        _ = &mut outbound => inbound.abort(),
        _ = &mut inbound => outbound.abort(),
    }

    mark_presence_disconnected(ticket);
}

pub async fn get_presence(claims: Claims) -> Response {
    if !claims.can_read_records() {
        return api_json(
            StatusCode::FORBIDDEN,
            json!({"response":"this account does not have permission to view MX presence"}),
        );
    }

    let accounts = online_presence_accounts();
    api_json(
        StatusCode::OK,
        json!({
            "online": accounts.len(),
            "accounts": accounts,
            "at": now_millis(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_response_contains_only_online_accounts() {
        let presence = HashMap::from([
            (
                "offline-user".to_string(),
                PresenceState {
                    name: "Offline User".to_string(),
                    access_level: 2,
                    connections: 0,
                    online: false,
                    generation: 1,
                },
            ),
            (
                "online-user".to_string(),
                PresenceState {
                    name: "Online User".to_string(),
                    access_level: 1,
                    connections: 1,
                    online: true,
                    generation: 1,
                },
            ),
        ]);

        let accounts = online_presence_accounts_from(&presence);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].uid, "online-user");
        assert_eq!(accounts[0].name, "Online User");
    }
}
