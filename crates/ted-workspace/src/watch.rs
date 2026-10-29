//! Watching the expanded folders of the explorer. Each watch is non-recursive, and events are
//! batched so a `git checkout` or a build that touches thousands of files sends one message.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// How long events are collected after the first one before a message is sent.
const BATCH_WINDOW: Duration = Duration::from_millis(50);

/// Why the watcher could not start.
#[derive(thiserror::Error, Debug)]
pub enum WatchError {
    #[error("cannot start the file watcher: {0}")]
    Notify(#[from] notify::Error),
    #[error("cannot start the watcher thread: {0}")]
    Thread(#[source] io::Error),
}

type Watched = Arc<Mutex<HashSet<PathBuf>>>;

/// Watches folders (not their subfolders) and reports which ones had direct children created,
/// removed, renamed or modified.
pub struct FolderWatcher {
    watcher: RecommendedWatcher,
    watched: Watched,
}

impl FolderWatcher {
    /// Each message of the receiver is a batch: the folders whose direct children changed,
    /// deduplicated and sorted. The batching thread blocks while nothing changes (no timer, no
    /// polling) and ends when the watcher is dropped or the receiver is.
    pub fn new() -> Result<(Self, async_channel::Receiver<Vec<PathBuf>>), WatchError> {
        let (raw_tx, raw_rx) = mpsc::channel::<notify::Result<Event>>();
        let watcher = notify::recommended_watcher(move |event| {
            // The receiver is gone only when the thread has ended.
            let _ = raw_tx.send(event);
        })?;
        let watched: Watched = Arc::new(Mutex::new(HashSet::new()));
        let (tx, rx) = async_channel::unbounded();

        let thread_watched = Arc::clone(&watched);
        std::thread::Builder::new()
            .name("ted-folder-watcher".into())
            .spawn(move || batch_loop(raw_rx, tx, thread_watched))
            .map_err(WatchError::Thread)?;

        Ok((Self { watcher, watched }, rx))
    }

    /// Starts watching `dir`. Failures (the folder vanished, no inotify watches left) are logged
    /// and the folder is simply not live.
    pub fn watch(&mut self, dir: &Path) {
        if !lock(&self.watched).insert(dir.to_path_buf()) {
            return;
        }
        if let Err(err) = self.watcher.watch(dir, RecursiveMode::NonRecursive) {
            log::warn!("cannot watch {}: {err}", dir.display());
            lock(&self.watched).remove(dir);
        }
    }

    /// Stops watching `dir`.
    pub fn unwatch(&mut self, dir: &Path) {
        if lock(&self.watched).remove(dir)
            && let Err(err) = self.watcher.unwatch(dir)
        {
            // Normal when the folder was deleted: the watch is already gone.
            log::debug!("cannot unwatch {}: {err}", dir.display());
        }
    }
}

fn lock(watched: &Watched) -> std::sync::MutexGuard<'_, HashSet<PathBuf>> {
    watched
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn batch_loop(
    raw: mpsc::Receiver<notify::Result<Event>>,
    tx: async_channel::Sender<Vec<PathBuf>>,
    watched: Watched,
) {
    let mut changed: HashSet<PathBuf> = HashSet::new();
    // Blocks while idle; ends when the watcher (and its sender) is dropped.
    while let Ok(first) = raw.recv() {
        let deadline = Instant::now() + BATCH_WINDOW;
        let mut next = Some(first);
        while let Some(event) = next.take() {
            collect(event, &watched, &mut changed);
            let left = deadline.saturating_duration_since(Instant::now());
            match raw.recv_timeout(left) {
                Ok(event) => next = Some(event),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
            }
        }
        if changed.is_empty() {
            continue;
        }
        let mut batch: Vec<PathBuf> = changed.drain().collect();
        batch.sort();
        if tx.send_blocking(batch).is_err() {
            return;
        }
    }
}

fn collect(event: notify::Result<Event>, watched: &Watched, changed: &mut HashSet<PathBuf>) {
    let event = match event {
        Ok(event) => event,
        Err(err) => {
            log::warn!("file watcher error: {err}");
            return;
        }
    };
    if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
        return;
    }
    let watched = lock(watched);
    for path in &event.paths {
        // A child changed: its folder. A watched folder itself changed (removed, renamed): the
        // folder, so the view can list it again and show the error.
        match path.parent() {
            Some(parent) if watched.contains(parent) => {
                changed.insert(parent.to_path_buf());
            }
            _ if watched.contains(path) => {
                changed.insert(path.clone());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    const WAIT: Duration = Duration::from_secs(1);

    fn recv(rx: &async_channel::Receiver<Vec<PathBuf>>, wait: Duration) -> Option<Vec<PathBuf>> {
        let deadline = Instant::now() + wait;
        loop {
            match rx.try_recv() {
                Ok(batch) => return Some(batch),
                Err(_) if Instant::now() >= deadline => return None,
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }

    #[test]
    fn reports_created_file_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let (mut w, rx) = FolderWatcher::new().unwrap();
        w.watch(&dir);
        fs::write(dir.join("new.txt"), "x").unwrap();
        assert_eq!(recv(&rx, WAIT), Some(vec![dir]));
    }

    #[test]
    fn reports_delete_and_rename() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        fs::write(dir.join("a"), "x").unwrap();
        fs::write(dir.join("b"), "x").unwrap();
        let (mut w, rx) = FolderWatcher::new().unwrap();
        w.watch(&dir);
        fs::rename(dir.join("a"), dir.join("a2")).unwrap();
        assert_eq!(recv(&rx, WAIT), Some(vec![dir.clone()]));
        fs::remove_file(dir.join("b")).unwrap();
        assert_eq!(recv(&rx, WAIT), Some(vec![dir]));
    }

    #[test]
    fn batches_many_creations() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let (mut w, rx) = FolderWatcher::new().unwrap();
        w.watch(&dir);
        for i in 0..500 {
            fs::write(dir.join(format!("f{i}")), "x").unwrap();
        }
        let mut messages = Vec::new();
        while let Some(batch) = recv(&rx, Duration::from_millis(300)) {
            messages.push(batch);
        }
        assert!(!messages.is_empty());
        assert!(messages.len() <= 5, "{} messages", messages.len());
        assert!(messages.iter().all(|m| m == &vec![dir.clone()]));
    }

    #[test]
    fn only_watched_folders_and_not_recursive() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        fs::create_dir(dir.join("sub")).unwrap();
        fs::create_dir(dir.join("other")).unwrap();
        let (mut w, rx) = FolderWatcher::new().unwrap();
        w.watch(&dir);
        w.watch(&dir.join("other"));
        w.unwatch(&dir.join("other"));
        fs::write(dir.join("sub").join("deep.txt"), "x").unwrap();
        fs::write(dir.join("other").join("x.txt"), "x").unwrap();
        assert_eq!(recv(&rx, Duration::from_millis(300)), None);
    }

    #[test]
    fn thread_ends_when_dropped() {
        let (w, rx) = FolderWatcher::new().unwrap();
        drop(w);
        // The batching thread drops its sender, which closes the channel.
        let deadline = Instant::now() + WAIT;
        while !rx.is_closed() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(rx.is_closed());
    }
}
