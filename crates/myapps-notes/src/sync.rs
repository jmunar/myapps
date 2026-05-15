//! WebSocket-based Yjs sync for the Notes editor.
//!
//! Each note has an in-memory `Room` holding a `yrs::Doc` and a broadcast
//! channel. WebSocket sessions speak the y-sync protocol: SyncStep1 →
//! SyncStep2 → Update. Every received update is applied to the in-memory
//! doc, persisted to `notes_note_updates`, and broadcast to other peers.
//!
//! The room is created lazily on first connection with an empty Doc.
//! Pre-CRDT notes still have their markdown in `notes_notes.body` (used by
//! the list-view preview); a future client-side migration will pipe that
//! through Tiptap's markdown parser into the CRDT on first open.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use axum::Router;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use sqlx::SqlitePool;
use tokio::sync::{broadcast, mpsc};
use yrs::sync::{Message as SyncWrapper, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

use myapps_core::auth::UserId;
use myapps_core::routes::AppState;

const BROADCAST_CAPACITY: usize = 64;
const IDLE_EVICTION_THRESHOLD_SECS: i64 = 60;
const IDLE_EVICTION_INTERVAL_SECS: u64 = 30;

pub type Rooms = Arc<tokio::sync::RwLock<HashMap<String, Arc<Room>>>>;

pub fn new_rooms() -> Rooms {
    Arc::new(tokio::sync::RwLock::new(HashMap::new()))
}

pub struct Room {
    pub note_id: i64,
    pub doc: Arc<Doc>,
    pub tx: broadcast::Sender<Vec<u8>>,
    pub last_active: AtomicI64,
    pub subscribers: AtomicUsize,
}

impl Room {
    fn touch(&self) {
        self.last_active.store(now_secs(), Ordering::Relaxed);
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/{uuid}/ws", get(ws_handler))
}

pub async fn acquire_room(
    rooms: &Rooms,
    pool: &SqlitePool,
    uuid: &str,
    note_id: i64,
) -> anyhow::Result<Arc<Room>> {
    if let Some(room) = rooms.read().await.get(uuid) {
        return Ok(room.clone());
    }

    let updates: Vec<(Vec<u8>,)> =
        sqlx::query_as("SELECT update_blob FROM notes_note_updates WHERE note_id = ? ORDER BY id")
            .bind(note_id)
            .fetch_all(pool)
            .await?;

    // Replay each persisted update in isolation: a single corrupted row must
    // not brick the entire note. Log the offender and continue — the room
    // ends up with whatever state the surviving updates encode, and the next
    // idle eviction will snapshot that state into one clean row.
    let doc = Doc::new();
    let mut skipped: usize = 0;
    {
        let mut txn = doc.transact_mut();
        for (blob,) in &updates {
            match Update::decode_v1(blob) {
                Ok(update) => {
                    if let Err(e) = txn.apply_update(update) {
                        tracing::warn!(
                            "notes: skipping unapplyable update for note_id={note_id}: {e}"
                        );
                        skipped += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!("notes: skipping undecodable update for note_id={note_id}: {e}");
                    skipped += 1;
                }
            }
        }
    }
    if skipped > 0 {
        tracing::warn!(
            "notes: note_id={note_id} replayed {applied} updates, skipped {skipped} corrupted rows",
            applied = updates.len() - skipped,
        );
    }

    let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
    let room = Arc::new(Room {
        note_id,
        doc: Arc::new(doc),
        tx,
        last_active: AtomicI64::new(now_secs()),
        subscribers: AtomicUsize::new(0),
    });

    let mut w = rooms.write().await;
    Ok(w.entry(uuid.to_string()).or_insert(room).clone())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(user_id): Extension<UserId>,
    Extension(rooms): Extension<Rooms>,
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> Response {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM notes_notes WHERE client_uuid = ? AND user_id = ?")
            .bind(&uuid)
            .bind(user_id.0)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let Some((note_id,)) = row else {
        return StatusCode::FORBIDDEN.into_response();
    };

    let room = match acquire_room(&rooms, &state.pool, &uuid, note_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("notes ws acquire_room failed: {e:#}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let pool = state.pool.clone();
    ws.on_upgrade(move |socket| run_session(socket, room, pool))
}

async fn run_session(socket: WebSocket, room: Arc<Room>, pool: SqlitePool) {
    let (mut sender, mut receiver) = socket.split();
    room.subscribers.fetch_add(1, Ordering::Relaxed);
    room.touch();

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let mut bcast = room.tx.subscribe();

    let forward = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(bytes) = out_rx.recv() => {
                    if sender.send(WsMessage::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                res = bcast.recv() => match res {
                    Ok(bytes) => {
                        if sender.send(WsMessage::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                else => break,
            }
        }
    });

    let initial = {
        let sv = room.doc.transact().state_vector();
        SyncWrapper::Sync(SyncMessage::SyncStep1(sv)).encode_v1()
    };
    let _ = out_tx.send(initial);

    while let Some(Ok(msg)) = receiver.next().await {
        let bytes: Vec<u8> = match msg {
            WsMessage::Binary(b) => b.into(),
            WsMessage::Close(_) => break,
            _ => continue,
        };
        room.touch();
        if let Err(e) = handle_inbound(&room, &pool, &bytes, &out_tx).await {
            tracing::warn!("notes ws inbound failed: {e:#}");
        }
    }

    drop(out_tx);
    forward.abort();
    room.subscribers.fetch_sub(1, Ordering::Relaxed);
}

async fn handle_inbound(
    room: &Room,
    pool: &SqlitePool,
    bytes: &[u8],
    out: &mpsc::UnboundedSender<Vec<u8>>,
) -> anyhow::Result<()> {
    let msg = SyncWrapper::decode_v1(bytes)?;
    match msg {
        SyncWrapper::Sync(SyncMessage::SyncStep1(sv)) => {
            let diff = room.doc.transact().encode_state_as_update_v1(&sv);
            let reply = SyncWrapper::Sync(SyncMessage::SyncStep2(diff)).encode_v1();
            let _ = out.send(reply);
        }
        SyncWrapper::Sync(SyncMessage::SyncStep2(update))
        | SyncWrapper::Sync(SyncMessage::Update(update)) => {
            apply_and_broadcast(room, pool, update).await?;
        }
        _ => {}
    }
    Ok(())
}

async fn apply_and_broadcast(
    room: &Room,
    pool: &SqlitePool,
    update: Vec<u8>,
) -> anyhow::Result<()> {
    {
        let mut txn = room.doc.transact_mut();
        txn.apply_update(Update::decode_v1(&update)?)?;
    }
    sqlx::query("INSERT INTO notes_note_updates (note_id, update_blob) VALUES (?, ?)")
        .bind(room.note_id)
        .bind(&update)
        .execute(pool)
        .await?;
    let wire = SyncWrapper::Sync(SyncMessage::Update(update)).encode_v1();
    let _ = room.tx.send(wire);
    Ok(())
}

/// Background task: every `IDLE_EVICTION_INTERVAL_SECS`, snapshot-and-evict
/// any room with no subscribers that's been idle for at least
/// `IDLE_EVICTION_THRESHOLD_SECS`. Spawned from `NotesApp::on_serve`.
pub fn spawn_eviction_task(rooms: Rooms, pool: SqlitePool) {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(IDLE_EVICTION_INTERVAL_SECS));
        interval.tick().await; // skip the immediate first tick
        loop {
            interval.tick().await;
            if let Err(e) = evict_idle_rooms(&rooms, &pool, IDLE_EVICTION_THRESHOLD_SECS).await {
                tracing::warn!("notes eviction task error: {e:#}");
            }
        }
    });
}

/// For each room with zero subscribers and `last_active` older than
/// `idle_threshold_secs`: encode the current doc as a single snapshot update,
/// replace all rows in `notes_note_updates` for that note with the snapshot,
/// and remove the room from the registry. Holds the registry write-lock for
/// the duration, which keeps `acquire_room` from racing with the SQL replace.
pub async fn evict_idle_rooms(
    rooms: &Rooms,
    pool: &SqlitePool,
    idle_threshold_secs: i64,
) -> anyhow::Result<()> {
    let now = now_secs();
    let mut w = rooms.write().await;
    let to_evict: Vec<(String, Arc<Room>)> = w
        .iter()
        .filter(|(_, room)| {
            room.subscribers.load(Ordering::Relaxed) == 0
                && now - room.last_active.load(Ordering::Relaxed) >= idle_threshold_secs
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    for (uuid, room) in to_evict {
        let snapshot = room
            .doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM notes_note_updates WHERE note_id = ?")
            .bind(room.note_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO notes_note_updates (note_id, update_blob) VALUES (?, ?)")
            .bind(room.note_id)
            .bind(&snapshot)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        w.remove(&uuid);
    }
    Ok(())
}
