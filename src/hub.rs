use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::Body,
    extract::{Query, Request, State},
    http::{header, Method, StatusCode},
    middleware::Next,
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Notify;

use crate::web;

const MAX_MESSAGES: usize = 500;
const DEFAULT_ROOM: &str = "near";

fn default_room() -> String {
    DEFAULT_ROOM.to_string()
}

fn default_timeout() -> u64 {
    20_000
}

fn default_name() -> String {
    "device".to_string()
}

#[derive(Clone)]
pub struct HubState {
    inner: Arc<Mutex<Inner>>,
    changed: Arc<Notify>,
}

#[derive(Clone)]
pub struct AppState {
    pub hub: HubState,
    pub web_root: Option<PathBuf>,
}

impl HubState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                rooms: HashMap::new(),
                sequence: 0,
            })),
            changed: Arc::new(Notify::new()),
        }
    }
}

struct Inner {
    rooms: HashMap<String, Room>,
    sequence: u64,
}

struct Room {
    peers: HashMap<String, String>,
    messages: Vec<Message>,
}

impl Room {
    fn new() -> Self {
        Self { peers: HashMap::new(), messages: Vec::new() }
    }
}

#[derive(Clone, Serialize)]
struct Message {
    seq: u64,
    from: String,
    to: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    payload: Value,
}

fn with_room<T>(inner: &mut Inner, name: &str, action: impl FnOnce(&mut Room) -> T) -> T {
    action(inner.rooms.entry(name.to_string()).or_insert_with(Room::new))
}

fn peers(room: &Room) -> Value {
    let mut list: Vec<Value> = room
        .peers
        .iter()
        .map(|(id, name)| json!({ "id": id, "name": name }))
        .collect();
    list.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Value::Array(list)
}

#[derive(Deserialize)]
struct RoomQuery {
    #[serde(default = "default_room")]
    room: String,
}

#[derive(Deserialize)]
struct JoinBody {
    #[serde(default = "default_room")]
    room: String,
    id: String,
    #[serde(default = "default_name")]
    name: String,
}

#[derive(Deserialize)]
struct LeaveBody {
    #[serde(default = "default_room")]
    room: String,
    id: String,
}

#[derive(Deserialize)]
struct SignalBody {
    #[serde(default = "default_room")]
    room: String,
    from: String,
    #[serde(default)]
    to: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Deserialize)]
struct PollQuery {
    #[serde(default = "default_room")]
    room: String,
    id: String,
    #[serde(default)]
    since: u64,
    #[serde(default = "default_timeout")]
    timeout: u64,
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn state(Query(query): Query<RoomQuery>, State(app): State<AppState>) -> Json<Value> {
    let guard = app.hub.inner.lock().unwrap();
    let empty = Room::new();
    let room = guard.rooms.get(&query.room).unwrap_or(&empty);
    let peer_count = guard.rooms.values().map(|r| r.peers.len()).sum::<usize>();
    let room_count = guard.rooms.values().filter(|r| !r.peers.is_empty()).count();
    Json(json!({
        "peers": peers(room),
        "peerCount": peer_count,
        "roomCount": room_count,
    }))
}

async fn join(State(app): State<AppState>, Json(body): Json<JoinBody>) -> Json<Value> {
    let id = body.id.chars().take(64).collect::<String>();
    let name = body.name.chars().take(64).collect::<String>();
    let list = {
        let mut guard = app.hub.inner.lock().unwrap();
        let seq = {
            let mut seq = guard.sequence;
            seq += 1;
            guard.sequence = seq;
            seq
        };
        let message = Message {
            seq,
            from: id.clone(),
            to: None,
            kind: "hello".to_string(),
            payload: Value::Null,
        };
        let list = with_room(&mut guard, &body.room, |room| {
            room.peers.insert(id.clone(), name);
            room.messages.push(message);
            if room.messages.len() > MAX_MESSAGES {
                let excess = room.messages.len() - MAX_MESSAGES;
                room.messages.drain(..excess);
            }
            peers(room)
        });
        list
    };
    app.hub.changed.notify_waiters();
    Json(json!({ "peers": list }))
}

async fn leave(State(app): State<AppState>, Json(body): Json<LeaveBody>) -> Json<Value> {
    {
        let mut guard = app.hub.inner.lock().unwrap();
        let seq = {
            let mut seq = guard.sequence;
            seq += 1;
            guard.sequence = seq;
            seq
        };
        let message = Message {
            seq,
            from: body.id.chars().take(64).collect(),
            to: None,
            kind: "hello".to_string(),
            payload: Value::Null,
        };
        with_room(&mut guard, &body.room, |room| {
            room.peers.remove(&body.id);
            room.messages.push(message);
            if room.messages.len() > MAX_MESSAGES {
                let excess = room.messages.len() - MAX_MESSAGES;
                room.messages.drain(..excess);
            }
        });
    }
    app.hub.changed.notify_waiters();
    Json(json!({ "ok": true }))
}

async fn signal(State(app): State<AppState>, Json(body): Json<SignalBody>) -> Json<Value> {
    let seq = {
        let mut guard = app.hub.inner.lock().unwrap();
        let mut seq = guard.sequence;
        seq += 1;
        guard.sequence = seq;
        let message = Message {
            seq,
            from: body.from.chars().take(64).collect(),
            to: body.to.map(|value| value.chars().take(64).collect()),
            kind: body.kind.chars().take(32).collect(),
            payload: body.payload,
        };
        with_room(&mut guard, &body.room, |room| {
            room.messages.push(message);
            if room.messages.len() > MAX_MESSAGES {
                let excess = room.messages.len() - MAX_MESSAGES;
                room.messages.drain(..excess);
            }
        });
        seq
    };
    app.hub.changed.notify_waiters();
    Json(json!({ "seq": seq }))
}

fn collect(app: &AppState, query: &PollQuery) -> Vec<Message> {
    let guard = app.hub.inner.lock().unwrap();
    match guard.rooms.get(&query.room) {
        Some(room) => room
            .messages
            .iter()
            .filter(|message| {
                message.seq > query.since
                    && message.from != query.id
                    && (message.to.is_none() || message.to.as_deref() == Some(query.id.as_str()))
            })
            .cloned()
            .collect(),
        None => Vec::new(),
    }
}

async fn poll(Query(query): Query<PollQuery>, State(app): State<AppState>) -> Json<Value> {
    let timeout = query.timeout.clamp(100, 30_000);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout);
    loop {
        let notified = app.hub.changed.notified();
        tokio::pin!(notified);

        let messages = collect(&app, &query);
        if !messages.is_empty() {
            return Json(json!({ "messages": messages }));
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Json(json!({ "messages": [] }));
        }

        tokio::select! {
            _ = &mut notified => {}
            _ = tokio::time::sleep(remaining) => {
                return Json(json!({ "messages": collect(&app, &query) }));
            }
        }
    }
}

async fn cors(request: Request, next: Next) -> Response {
    let is_api = request.uri().path().starts_with("/api/");
    if request.method() == Method::OPTIONS {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        add_headers(response.headers_mut(), is_api);
        return response;
    }
    let mut response = next.run(request).await;
    add_headers(response.headers_mut(), is_api);
    response
}

fn add_headers(headers: &mut axum::http::HeaderMap, is_api: bool) {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        header::HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        header::HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        header::HeaderValue::from_static("content-type"),
    );
    if is_api {
        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
    }
}

pub fn router(app: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/state", get(state))
        .route("/api/join", post(join))
        .route("/api/leave", post(leave))
        .route("/api/signal", post(signal))
        .route("/api/poll", get(poll))
        .fallback(web::serve)
        .layer(axum::middleware::from_fn(cors))
        .with_state(app)
}
