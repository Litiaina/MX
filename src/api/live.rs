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
use rusqlite::params;
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
    expires_at: i64,
}

#[derive(Debug, Clone)]
struct PresenceState {
    name: String,
    access_level: i64,
    connections: u64,
    online: bool,
    generation: u64,
    last_seen_at: i64,
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
    online: bool,
    last_seen_at: Option<i64>,
}

fn presence_snapshot() -> HashMap<String, PresenceState> {
    hub()
        .presence
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn prune_expired_tickets(tickets: &mut HashMap<String, LiveTicket>, now: i64) {
    tickets.retain(|_, ticket| ticket.expires_at > now);
}

async fn load_ticket_identity(uid: String) -> Result<(String, i64), String> {
    tokio::task::spawn_blocking(move || -> Result<(String, i64), SqliteDatabaseError> {
        with_sql_connection(|connection| {
            connection.query_row(
                "SELECT name, access_level FROM users WHERE uid = ?1 LIMIT 1",
                params![uid],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
        })
    })
    .await
    .map_err(|error| format!("live ticket task failed: {error}"))?
    .map_err(|error| format!("live ticket account lookup failed: {error}"))
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

    let (name, access_level) = match load_ticket_identity(claims.uid.clone()).await {
        Ok(identity) => identity,
        Err(error) => {
            crate::report_error!(error, "live", "issue_live_ticket()");
            return api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to prepare MX live connection"}),
            );
        }
    };

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

    ws.on_upgrade(move |socket| run_live_socket(socket, ticket))
}

fn mark_presence_connected(ticket: &LiveTicket) {
    let mut became_online = false;

    {
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
                last_seen_at: now_millis(),
            });

        entry.name = ticket.name.clone();
        entry.access_level = ticket.access_level;
        entry.connections = entry.connections.saturating_add(1);
        entry.generation = entry.generation.saturating_add(1);
        entry.last_seen_at = now_millis();

        if !entry.online {
            entry.online = true;
            became_online = true;
        }
    }

    if became_online {
        publish_live_event(
            "presence.user.online",
            Some(&ticket.uid),
            json!({
                "uid": ticket.uid,
                "name": ticket.name,
                "access_level": ticket.access_level,
                "access_name": access_name(ticket.access_level),
                "online": true,
            }),
        );
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
        entry.last_seen_at = now_millis();
        entry.generation
    };

    let uid = ticket.uid.clone();
    let name = ticket.name.clone();
    let access_level = ticket.access_level;

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(PRESENCE_OFFLINE_GRACE_MS)).await;

        let should_publish = {
            let mut presence = hub()
                .presence
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            let Some(entry) = presence.get_mut(&uid) else {
                return;
            };

            if entry.connections == 0 && entry.generation == generation && entry.online {
                entry.online = false;
                entry.last_seen_at = now_millis();
                true
            } else {
                false
            }
        };

        if should_publish {
            publish_live_event(
                "presence.user.offline",
                Some(&uid),
                json!({
                    "uid": uid,
                    "name": name,
                    "access_level": access_level,
                    "access_name": access_name(access_level),
                    "online": false,
                }),
            );
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

    let online = presence_snapshot();

    let result = tokio::task::spawn_blocking(
        move || -> Result<Vec<PresenceAccount>, SqliteDatabaseError> {
            with_sql_connection(|connection| {
                let mut statement = connection.prepare(
                    r#"
                    SELECT uid, name, access_level
                    FROM users
                    ORDER BY name COLLATE NOCASE ASC, uid ASC
                    "#,
                )?;

                let rows = statement.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })?;

                let mut accounts = Vec::new();
                for row in rows {
                    let (uid, name, access_level) = row?;
                    let presence = online.get(&uid);
                    accounts.push(PresenceAccount {
                        uid,
                        name,
                        access_level,
                        access_name: access_name(access_level).to_string(),
                        online: presence.is_some_and(|entry| entry.online),
                        last_seen_at: presence.map(|entry| entry.last_seen_at),
                    });
                }
                Ok(accounts)
            })
        },
    )
    .await;

    match result {
        Ok(Ok(accounts)) => {
            let online_count = accounts.iter().filter(|account| account.online).count();
            api_json(
                StatusCode::OK,
                json!({
                    "accounts": accounts,
                    "online": online_count,
                    "total": accounts.len(),
                    "at": now_millis(),
                }),
            )
        }
        Ok(Err(error)) => {
            crate::report_error!(format!("{error}"), "live", "get_presence()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"failed to load account presence"}),
            )
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "live", "get_presence()");
            api_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"response":"presence lookup task failed"}),
            )
        }
    }
}
