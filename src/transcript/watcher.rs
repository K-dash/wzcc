use anyhow::Result;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

use super::get_transcript_dir;

/// Watches transcript directories for file changes.
pub struct TranscriptWatcher {
    /// The underlying notify watcher
    _watcher: RecommendedWatcher,
    /// Receiver for file change events
    rx: Receiver<PathBuf>,
    /// Currently watched directories
    watched_dirs: Vec<PathBuf>,
}

impl TranscriptWatcher {
    /// Create a new TranscriptWatcher.
    pub fn new() -> Result<Self> {
        let (tx, rx) = channel::<PathBuf>();

        let watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    // Only handle modify events for .jsonl files
                    if event.kind.is_modify() {
                        for path in event.paths {
                            if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                                let _ = tx.send(path);
                            }
                        }
                    }
                }
            },
            Config::default(),
        )?;

        Ok(Self {
            _watcher: watcher,
            rx,
            watched_dirs: Vec::new(),
        })
    }

    /// Update watched directories based on a list of CWD paths.
    ///
    /// Unwatches directories that are no longer needed and watches new ones.
    pub fn update_dirs(&mut self, cwds: &[String]) -> Result<()> {
        let mut new_dirs: Vec<PathBuf> = Vec::new();

        for cwd in cwds {
            if let Some(dir) = get_transcript_dir(cwd) {
                if dir.exists() && !new_dirs.contains(&dir) {
                    new_dirs.push(dir);
                }
            }
        }

        // Unwatch old dirs
        for dir in &self.watched_dirs {
            if !new_dirs.contains(dir) {
                let _ = self._watcher.unwatch(dir);
            }
        }

        // Watch new dirs
        for dir in &new_dirs {
            if !self.watched_dirs.contains(dir) {
                let _ = self._watcher.watch(dir, RecursiveMode::NonRecursive);
            }
        }

        self.watched_dirs = new_dirs;

        Ok(())
    }

    /// Drain file change events and return true if any were received.
    pub fn drain_changes(&self) -> bool {
        let mut had_changes = false;
        while self.rx.try_recv().is_ok() {
            had_changes = true;
        }
        had_changes
    }
}
