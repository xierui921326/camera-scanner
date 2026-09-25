use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tokio::process::Child;

use crate::model::Device;

pub enum StreamKind {
    Ffmpeg { child: Child },
    Retina { cancel: Arc<AtomicBool> },
}

/// A running preview stream.
pub struct StreamSession {
    pub kind: StreamKind,
    pub dir: PathBuf,
    pub mode: String,
    pub label: String,
    pub url: String,
    pub manifest_url: Option<String>,
}

/// Shared application state managed by Tauri.
#[derive(Default)]
pub struct ScanState {
    /// Monotonic id generator (used for both scan tasks and stream sessions).
    pub counter: AtomicU64,
    /// task_id -> cancellation flag.
    pub cancel_flags: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    /// task_id -> discovered devices (live, in-memory).
    pub devices: Mutex<HashMap<u64, Vec<Device>>>,
    /// Confirmed, authorized non-private targets (CIDR strings), mirrored in DB.
    pub whitelist: Mutex<Vec<String>>,
    /// SQLite connection, initialized in `setup`.
    pub db: Mutex<Option<Connection>>,
    /// Active preview sessions keyed by session id.
    pub streams: Mutex<HashMap<String, StreamSession>>,
    /// Loopback media server port, started lazily on first `stream_open`.
    pub media_port: Mutex<Option<u16>>,
    /// Root directory served by the media server.
    pub streams_root: Mutex<Option<PathBuf>>,
}

impl ScanState {
    pub fn next_id(&self) -> u64 {
        self.counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1
    }
}
