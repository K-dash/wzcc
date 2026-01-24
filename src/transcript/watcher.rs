//! Transcript file watcher using notify crate.

use super::path::{get_latest_transcript, get_transcript_dir};
use super::state::{detect_session_status, DetectionConfig, SessionStatus};
use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, RwLock};

/// Event sent when a session status changes.
#[derive(Debug, Clone)]
pub struct StatusChangeEvent {
    /// The cwd of the session
    pub cwd: String,
    /// The new status
    pub status: SessionStatus,
    /// The transcript file path
    pub transcript_path: PathBuf,
}

/// Watches transcript directories for changes and detects session status.
#[allow(dead_code)]
pub struct TranscriptWatcher {
    /// The internal watcher
    _watcher: RecommendedWatcher,
    /// Receiver for status change events
    pub rx: Receiver<StatusChangeEvent>,
    /// Current status cache
    status_cache: Arc<RwLock<HashMap<String, SessionStatus>>>,
    /// Detection configuration
    config: DetectionConfig,
}

impl TranscriptWatcher {
    /// Create a new transcript watcher.
    pub fn new() -> Result<Self> {
        Self::with_config(DetectionConfig::default())
    }

    /// Create a new transcript watcher with custom configuration.
    pub fn with_config(config: DetectionConfig) -> Result<Self> {
        let (tx, rx) = channel::<StatusChangeEvent>();
        let status_cache = Arc::new(RwLock::new(HashMap::new()));
        let cache_clone = status_cache.clone();
        let config_clone = config.clone();

        let watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    Self::handle_event(&event, &tx, &cache_clone, &config_clone);
                }
            },
            Config::default(),
        )?;

        Ok(Self {
            _watcher: watcher,
            rx,
            status_cache,
            config,
        })
    }

    /// Start watching a transcript directory for a given cwd.
    pub fn watch(&mut self, cwd: &str) -> Result<()> {
        if let Some(dir) = get_transcript_dir(cwd) {
            if dir.exists() {
                // Use the internal watcher
                // Note: We need to get mutable access to _watcher
                // This is a bit tricky with the current design
                // For now, we'll just do initial detection
                self.initial_detect(cwd)?;
            }
        }
        Ok(())
    }

    /// Perform initial status detection for a cwd.
    fn initial_detect(&self, cwd: &str) -> Result<()> {
        if let Some(dir) = get_transcript_dir(cwd) {
            if let Ok(Some(transcript_path)) = get_latest_transcript(&dir) {
                let status = detect_session_status(&transcript_path)?;
                let mut cache = self.status_cache.write().unwrap();
                cache.insert(cwd.to_string(), status);
            }
        }
        Ok(())
    }

    /// Get the current cached status for a cwd.
    pub fn get_status(&self, cwd: &str) -> Option<SessionStatus> {
        let cache = self.status_cache.read().unwrap();
        cache.get(cwd).cloned()
    }

    /// Manually update the status for a cwd by reading the transcript.
    pub fn update_status(&self, cwd: &str) -> Result<Option<SessionStatus>> {
        if let Some(dir) = get_transcript_dir(cwd) {
            if let Ok(Some(transcript_path)) = get_latest_transcript(&dir) {
                let status = detect_session_status(&transcript_path)?;
                let mut cache = self.status_cache.write().unwrap();
                cache.insert(cwd.to_string(), status.clone());
                return Ok(Some(status));
            }
        }
        Ok(None)
    }

    fn handle_event(
        event: &Event,
        tx: &Sender<StatusChangeEvent>,
        cache: &Arc<RwLock<HashMap<String, SessionStatus>>>,
        config: &DetectionConfig,
    ) {
        // Only handle modify events
        if !event.kind.is_modify() {
            return;
        }

        for path in &event.paths {
            // Only handle .jsonl files
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }

            // Extract cwd from path
            if let Some(cwd) = Self::extract_cwd_from_path(path) {
                // Detect new status
                if let Ok(status) =
                    super::state::detect_session_status_with_config(path, config)
                {
                    // Check if status changed
                    let status_changed = {
                        let cache_read = cache.read().unwrap();
                        cache_read.get(&cwd) != Some(&status)
                    };

                    if status_changed {
                        // Update cache
                        {
                            let mut cache_write = cache.write().unwrap();
                            cache_write.insert(cwd.clone(), status.clone());
                        }

                        // Send event
                        let _ = tx.send(StatusChangeEvent {
                            cwd,
                            status,
                            transcript_path: path.clone(),
                        });
                    }
                }
            }
        }
    }

    /// Extract the original cwd from an encoded transcript path.
    fn extract_cwd_from_path(path: &PathBuf) -> Option<String> {
        // Path format: ~/.claude/projects/{encoded-cwd}/{session_id}.jsonl
        let parent = path.parent()?;
        let encoded_cwd = parent.file_name()?.to_str()?;

        // Decode: replace leading - with /, then remaining - with /
        // This is a heuristic and may not be perfect for all cases
        if encoded_cwd.starts_with('-') {
            Some(encoded_cwd.replacen('-', "/", 1).replace('-', "/"))
        } else {
            None
        }
    }
}

/// A simpler polling-based status checker (alternative to notify watcher).
/// This is useful when notify doesn't work well or for simpler use cases.
#[allow(dead_code)]
pub struct StatusPoller {
    config: DetectionConfig,
    cache: HashMap<String, SessionStatus>,
}

#[allow(dead_code)]
impl StatusPoller {
    /// Create a new status poller.
    pub fn new() -> Self {
        Self::with_config(DetectionConfig::default())
    }

    /// Create a new status poller with custom configuration.
    pub fn with_config(config: DetectionConfig) -> Self {
        Self {
            config,
            cache: HashMap::new(),
        }
    }

    /// Poll and update the status for a cwd.
    /// Returns the new status and whether it changed.
    pub fn poll(&mut self, cwd: &str) -> Result<(SessionStatus, bool)> {
        let status = self.detect_status(cwd)?;
        let changed = self.cache.get(cwd) != Some(&status);

        if changed {
            self.cache.insert(cwd.to_string(), status.clone());
        }

        Ok((status, changed))
    }

    /// Get the cached status for a cwd without polling.
    pub fn get_cached(&self, cwd: &str) -> Option<&SessionStatus> {
        self.cache.get(cwd)
    }

    fn detect_status(&self, cwd: &str) -> Result<SessionStatus> {
        if let Some(dir) = get_transcript_dir(cwd) {
            if let Ok(Some(transcript_path)) = get_latest_transcript(&dir) {
                return super::state::detect_session_status_with_config(
                    &transcript_path,
                    &self.config,
                );
            }
        }
        Ok(SessionStatus::Unknown)
    }
}

impl Default for StatusPoller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_cwd_from_path() {
        let path = PathBuf::from(
            "/Users/test/.claude/projects/-Users-test-hobby-wzcc/abc123.jsonl",
        );
        let cwd = TranscriptWatcher::extract_cwd_from_path(&path);
        assert_eq!(cwd, Some("/Users/test/hobby/wzcc".to_string()));
    }

    #[test]
    fn test_status_poller_new() {
        let poller = StatusPoller::new();
        assert!(poller.cache.is_empty());
    }
}
