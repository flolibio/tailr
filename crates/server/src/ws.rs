use axum::extract::ws::{Message, WebSocket};
use axum::extract::{WebSocketUpgrade, Extension};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tailr_protocol::{LogEntry, WSMessage};

use crate::AppState;
use tailr_tail_engine::LineIndex;

const RING_BUFFER_SIZE: usize = 2000;
const CLIENT_CHANNEL_SIZE: usize = 512;
const SLOW_THRESHOLD: f64 = 0.8;
const WATCHER_POLL_MS: u64 = 100;

pub struct FileSubscribers {
    ring_buffer: VecDeque<LogEntry>,
    next_seq: u64,
    subscribers: HashMap<String, mpsc::Sender<WSMessage>>,
}

impl FileSubscribers {
    fn new() -> Self {
        Self {
            ring_buffer: VecDeque::with_capacity(RING_BUFFER_SIZE),
            next_seq: 1,
            subscribers: HashMap::new(),
        }
    }

    fn push_entry(&mut self, entry: LogEntry) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;

        if self.ring_buffer.len() >= RING_BUFFER_SIZE {
            self.ring_buffer.pop_front();
        }
        self.ring_buffer.push_back(entry);

        seq
    }

    fn catchup(&self, after_seq: u64) -> Vec<LogEntry> {
        let buffer_start_seq = self.next_seq.saturating_sub(self.ring_buffer.len() as u64);
        if after_seq < buffer_start_seq {
            return self.ring_buffer.iter().cloned().collect();
        }
        let offset = (after_seq - buffer_start_seq) as usize;
        self.ring_buffer
            .iter()
            .skip(offset)
            .cloned()
            .collect()
    }

    fn subscribe(&mut self, client_id: String, tx: mpsc::Sender<WSMessage>) {
        self.subscribers.insert(client_id, tx);
    }

    fn unsubscribe(&mut self, client_id: &str) {
        self.subscribers.remove(client_id);
    }

    fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

pub fn routes() -> Router {
    Router::new().route("/ws", get(ws_handler))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let client_id = Uuid::new_v4().to_string();
    let (mut ws_tx, mut ws_rx) = socket.split();

    let (tx, mut rx) = mpsc::channel::<WSMessage>(CLIENT_CHANNEL_SIZE);

    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    error!("failed to serialize WSMessage: {}", e);
                    continue;
                }
            };
            if ws_tx.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    info!(client_id = %client_id, "WebSocket client connected");

    while let Some(result) = ws_rx.next().await {
        let msg = match result {
            Ok(m) => m,
            Err(e) => {
                warn!(client_id = %client_id, error = %e, "WebSocket read error");
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                let text_str: &str = &text;
                match serde_json::from_str::<WSMessage>(text_str) {
                    Ok(WSMessage::Subscribe { path, after_seq }) => {
                        handle_subscribe(
                            &state,
                            &client_id,
                            tx.clone(),
                            &path,
                            after_seq,
                        )
                        .await;
                    }
                    Ok(WSMessage::Unsubscribe { path }) => {
                        handle_unsubscribe(&state, &client_id, &path).await;
                    }
                    Ok(WSMessage::Ping) => {
                        let _ = tx.send(WSMessage::Pong).await;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        warn!(client_id = %client_id, error = %e, "invalid WS message");
                        let _ = tx
                            .send(WSMessage::Error {
                                code: "INVALID_MESSAGE".to_string(),
                                message: "invalid message format".to_string(),
                            })
                            .await;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    cleanup_client(&state, &client_id).await;
    write_task.abort();
    info!(client_id = %client_id, "WebSocket client disconnected");
}

async fn handle_subscribe(
    state: &AppState,
    client_id: &str,
    tx: mpsc::Sender<WSMessage>,
    path: &str,
    after_seq: Option<u64>,
) {
    let path_buf = match crate::api::validate_path(path, &state.allowed_dirs, &state.log_files) {
        Ok(p) => p,
        Err(_) => {
            let _ = tx
                .send(WSMessage::Error {
                    code: "ACCESS_DENIED".to_string(),
                    message: "path not allowed".to_string(),
                })
                .await;
            return;
        }
    };

    let initial_lines = {
        if let Some(entry) = state.line_indices.get(&path_buf) {
            entry.value().total_lines()
        } else {
            match LineIndex::build(&path_buf) {
                Ok(idx) => {
                    let lines = idx.total_lines();
                    state.line_indices.insert(path_buf.clone(), idx);
                    lines
                }
                Err(_) => 0,
            }
        }
    };

    {
        let mut watcher = state.watcher.lock().await;
        if let Err(e) = watcher.watch(path_buf.clone(), initial_lines).await {
            warn!(path = %path, error = %e, "failed to start watching");
            let _ = tx
                .send(WSMessage::Error {
                    code: "WATCH_FAILED".to_string(),
                    message: "failed to watch file".to_string(),
                })
                .await;
            return;
        }
    }

    let mut subs = state.file_subscribers.lock().await;
    let file_sub = subs
        .entry(path.to_string())
        .or_insert_with(FileSubscribers::new);

    if let Some(after) = after_seq {
        let catchup_entries = file_sub.catchup(after);
        if !catchup_entries.is_empty() {
            let last_seq = after + catchup_entries.len() as u64;
            let _ = tx
                .send(WSMessage::Catchup {
                    path: path.to_string(),
                    entries: catchup_entries,
                    last_seq,
                })
                .await;
        }
    }

    file_sub.subscribe(client_id.to_string(), tx.clone());

    let _ = tx
        .send(WSMessage::Subscribed {
            path: path.to_string(),
        })
        .await;

    debug!(
        client_id = %client_id,
        path = %path,
        subscribers = file_sub.subscriber_count(),
        "client subscribed"
    );
}

async fn handle_unsubscribe(state: &AppState, client_id: &str, path: &str) {
    let mut subs = state.file_subscribers.lock().await;
    if let Some(file_sub) = subs.get_mut(path) {
        file_sub.unsubscribe(client_id);
        debug!(
            client_id = %client_id,
            path = %path,
            subscribers = file_sub.subscriber_count(),
            "client unsubscribed"
        );
    }
}

async fn cleanup_client(state: &AppState, client_id: &str) {
    let mut subs = state.file_subscribers.lock().await;
    for (_, file_sub) in subs.iter_mut() {
        file_sub.unsubscribe(client_id);
    }
}

pub fn spawn_watcher_loop(state: Arc<AppState>) {
    tokio::spawn(async move {
        let poll_interval = {
            let watcher = state.watcher.lock().await;
            watcher.poll_interval()
        };
        let interval = if poll_interval.is_zero() {
            Duration::from_millis(WATCHER_POLL_MS)
        } else {
            poll_interval
        };

        info!(interval_ms = interval.as_millis(), "watcher loop started");

        loop {
            tokio::time::sleep(interval).await;

            let new_entries = {
                let mut watcher = state.watcher.lock().await;
                watcher.check().await
            };

            if new_entries.is_empty() {
                continue;
            }

            let mut subs = state.file_subscribers.lock().await;

            for (path, entries) in new_entries {
                let key = path.to_string_lossy().to_string();
                let file_sub = match subs.get_mut(&key) {
                    Some(fs) => fs,
                    None => {
                        debug!(path = %key, "no subscribers for path, skipping");
                        continue;
                    }
                };

                if file_sub.subscriber_count() == 0 {
                    debug!(path = %key, "zero subscribers for path");
                    continue;
                }

                debug!(
                    path = %key,
                    new_entries = entries.len(),
                    subscribers = file_sub.subscriber_count(),
                    "fan-out: sending entries to subscribers"
                );

                let mut batch: Vec<(u64, LogEntry)> = Vec::with_capacity(entries.len());
                for entry in entries {
                    let seq = file_sub.push_entry(entry.clone());
                    batch.push((seq, entry));
                }

                let entries_for_msg: Vec<LogEntry> = batch.iter().map(|(_, e)| e.clone()).collect();
                let last_seq = batch.last().map(|(s, _)| *s).unwrap_or(0);

                let msg = WSMessage::Append {
                    path: key.clone(),
                    seq: last_seq,
                    entries: entries_for_msg,
                };

                let mut dead_clients = Vec::new();

                for (cid, sender) in &file_sub.subscribers {
                    let remaining = sender.capacity();
                    let threshold = (CLIENT_CHANNEL_SIZE as f64 * (1.0 - SLOW_THRESHOLD)) as usize;
                    if remaining <= threshold {
                        let _ = sender
                            .send(WSMessage::Error {
                                code: "SLOW".to_string(),
                                message: "client is falling behind".to_string(),
                            })
                            .await;
                    }

                    if sender.try_send(msg.clone()).is_err() {
                        warn!(client_id = %cid, path = %key, "try_send failed, marking as dead client");
                        dead_clients.push(cid.clone());
                    }
                }

                for cid in dead_clients {
                    file_sub.unsubscribe(&cid);
                    debug!(client_id = %cid, path = %key, "removed dead client");
                }
            }
        }
    });
}
