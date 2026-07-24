//! `notify`-backed adapter for `FileWatcherPort`. Watches a project tree
//! recursively and forwards only structural changes (create / remove / rename)
//! through a channel, so the explorer can refresh the affected directories.
//! Content-only writes are filtered out to avoid churn during builds.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use notify::event::ModifyKind;
use notify::{EventKind, RecursiveMode, Watcher};

use crate::application::ports::{FileWatchSession, FileWatcherPort};
use crate::domain::file_system::FileSystemError;

pub struct NotifyFileWatcher;

impl NotifyFileWatcher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NotifyFileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Owns the live `notify` watcher (dropping it stops the background thread) and
/// the receiving end of the change channel.
struct NotifySession {
    // Kept alive for its `Drop`; the concrete type is hidden behind the trait.
    _watcher: notify::RecommendedWatcher,
    changes: Receiver<PathBuf>,
}

impl FileWatchSession for NotifySession {
    fn drain(&self) -> Vec<PathBuf> {
        self.changes.try_iter().collect()
    }
}

/// Only structural changes alter what the tree displays; skip content writes.
fn is_structural(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
    )
}

impl FileWatcherPort for NotifyFileWatcher {
    fn watch(&self, root: &Path) -> Result<Box<dyn FileWatchSession>, FileSystemError> {
        let (tx, rx) = channel::<PathBuf>();

        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result
                    && is_structural(&event.kind)
                {
                    for path in event.paths {
                        // A closed receiver just means the editor went away.
                        let _ = tx.send(path);
                    }
                }
            })
            .map_err(|e| FileSystemError::Io(e.to_string()))?;

        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| FileSystemError::Io(e.to_string()))?;

        Ok(Box::new(NotifySession {
            _watcher: watcher,
            changes: rx,
        }))
    }
}
