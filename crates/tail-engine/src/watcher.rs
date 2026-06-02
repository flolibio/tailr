use crate::session::TailSession;
use tailr_protocol::LogEntry;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

pub struct FileWatcher {
    watched: HashMap<PathBuf, TailSession>,
    poll_interval: Duration,
    notify_rx: mpsc::Receiver<notify::Result<Event>>,
    _watcher: RecommendedWatcher,
    dirty: HashSet<PathBuf>,
}

impl FileWatcher {
    pub fn new(poll_interval: Duration) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Err(e) = tx.send(res) {
                    tracing::error!("failed to send inotify event: {}", e);
                }
            },
            notify::Config::default(),
        )
            .map_err(std::io::Error::other)?;

        Ok(Self {
            watched: HashMap::new(),
            poll_interval,
            notify_rx: rx,
            _watcher: watcher,
            dirty: HashSet::new(),
        })
    }

    pub async fn watch(&mut self, path: PathBuf, initial_lines: u64) -> std::io::Result<()> {
        if self.watched.contains_key(&path) {
            debug!(path = %path.display(), "already watching");
            return Ok(());
        }

        let session = TailSession::new(path.clone(), initial_lines).await?;
        self.watched.insert(path.clone(), session);

        self._watcher
            .watch(&path, RecursiveMode::NonRecursive)
        .map_err(std::io::Error::other)?;

        info!(path = %path.display(), "started watching");
        Ok(())
    }

    pub fn unwatch(&mut self, path: &PathBuf) {
        if self.watched.remove(path).is_some() {
            let _ = self._watcher.unwatch(path);
            self.dirty.remove(path);
            info!(path = %path.display(), "stopped watching");
        }
    }

    pub async fn check(&mut self) -> HashMap<PathBuf, Vec<LogEntry>> {
        self.drain_inotify_events();

        let mut results = HashMap::new();

        let dirty_paths: Vec<PathBuf> = self.dirty.drain().collect();
        for path in dirty_paths {
            if let Some(session) = self.watched.get_mut(&path) {
                match session.check().await {
                    Ok(entries) => {
                        if !entries.is_empty() {
                            results.insert(path, entries);
                        }
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "check failed");
                    }
                }
            }
        }

        self.poll_all(&mut results).await;

        results
    }

    fn drain_inotify_events(&mut self) {
        loop {
            match self.notify_rx.try_recv() {
                Ok(Ok(event)) => {
                    self.handle_inotify_event(event);
                }
                Ok(Err(e)) => {
                    warn!(error = %e, "inotify watch error");
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    error!("inotify channel disconnected");
                    break;
                }
            }
        }
    }

    fn handle_inotify_event(&mut self, event: Event) {
        match event.kind {
            EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_) => {}
            _ => return,
        }

        for path in event.paths {
            if self.watched.contains_key(&path) {
                debug!(path = %path.display(), kind = ?event.kind, "inotify event");
                self.dirty.insert(path);
            }
        }
    }

    async fn poll_all(&mut self, results: &mut HashMap<PathBuf, Vec<LogEntry>>) {
        let paths: Vec<PathBuf> = self.watched.keys().cloned().collect();
        for path in paths {
            if results.contains_key(&path) {
                continue;
            }
            if let Some(session) = self.watched.get_mut(&path) {
                match session.check().await {
                    Ok(entries) => {
                        if !entries.is_empty() {
                            results.insert(path, entries);
                        }
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "poll check failed");
                    }
                }
            }
        }
    }

    pub fn watched_paths(&self) -> Vec<&PathBuf> {
        self.watched.keys().collect()
    }

    pub fn get_session(&self, path: &PathBuf) -> Option<&TailSession> {
        self.watched.get(path)
    }

    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }
}
