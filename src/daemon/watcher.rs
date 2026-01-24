//! Daemon watcher for monitoring Claude Code sessions.

use crate::cli::WeztermCli;
use crate::datasource::{PaneDataSource, SystemProcessDataSource, WeztermDataSource};
use crate::detector::ClaudeCodeDetector;
use crate::models::Pane;
use crate::transcript::{
    detect_session_status, get_latest_transcript, get_transcript_dir, SessionStatus,
};
use anyhow::Result;
use std::collections::HashMap;
use tokio::time::{interval, Duration};

/// Session info with cached status
struct SessionInfo {
    #[allow(dead_code)]
    pane: Pane,
    status: SessionStatus,
    original_title: String,
}

/// Run the daemon
pub async fn run() -> Result<()> {
    let pane_ds = WeztermDataSource::new();
    let process_ds = SystemProcessDataSource::new();
    let detector = ClaudeCodeDetector::new();

    let mut sessions: HashMap<u32, SessionInfo> = HashMap::new();

    // Poll every 3 seconds
    let mut ticker = interval(Duration::from_secs(3));

    println!("Daemon started. Monitoring Claude Code sessions...");
    println!("Press Ctrl+C to stop.");

    loop {
        ticker.tick().await;

        // Get current workspace
        let current_workspace = match pane_ds.get_current_workspace() {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("Failed to get current workspace: {}", e);
                continue;
            }
        };

        // Get pane list
        let panes = match pane_ds.list_panes() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to list panes: {}", e);
                continue;
            }
        };

        // Track current sessions
        let mut current_pane_ids: Vec<u32> = Vec::new();

        for pane in panes {
            // Only target current workspace
            if pane.workspace != current_workspace {
                continue;
            }

            // Detect Claude Code
            let is_claude = detector
                .detect_by_tty(&pane, &process_ds)
                .ok()
                .flatten()
                .is_some();

            if !is_claude {
                // If no longer Claude Code, restore original title
                if let Some(info) = sessions.remove(&pane.pane_id) {
                    let _ = WeztermCli::set_tab_title(pane.pane_id, &info.original_title);
                    println!(
                        "Pane {} is no longer Claude Code, restored title",
                        pane.pane_id
                    );
                }
                continue;
            }

            current_pane_ids.push(pane.pane_id);

            // Get session status
            let status = detect_status_for_pane(&pane);

            // Check if existing session
            if let Some(info) = sessions.get_mut(&pane.pane_id) {
                // Update only when status changes
                if info.status != status {
                    let old_status = info.status.clone();
                    info.status = status.clone();

                    // Update tab title
                    let new_title = format_title(&info.original_title, &status);
                    if let Err(e) = WeztermCli::set_tab_title(pane.pane_id, &new_title) {
                        eprintln!("Failed to set tab title: {}", e);
                    } else {
                        println!(
                            "Pane {} status changed: {:?} -> {:?}",
                            pane.pane_id, old_status, status
                        );
                    }
                }
            } else {
                // New session
                let original_title = pane.title.clone();
                let new_title = format_title(&original_title, &status);

                if let Err(e) = WeztermCli::set_tab_title(pane.pane_id, &new_title) {
                    eprintln!("Failed to set tab title: {}", e);
                } else {
                    println!(
                        "New Claude Code session detected: Pane {} ({:?})",
                        pane.pane_id, status
                    );
                }

                sessions.insert(
                    pane.pane_id,
                    SessionInfo {
                        pane,
                        status,
                        original_title,
                    },
                );
            }
        }

        // Remove closed sessions (restore title)
        let gone_pane_ids: Vec<u32> = sessions
            .keys()
            .filter(|id| !current_pane_ids.contains(id))
            .copied()
            .collect();

        for pane_id in gone_pane_ids {
            if let Some(info) = sessions.remove(&pane_id) {
                // Don't try to restore title when pane is gone (will error)
                println!("Pane {} closed", pane_id);
                let _ = info; // suppress unused warning
            }
        }
    }
}

/// Detect status by reading transcript from pane's cwd
fn detect_status_for_pane(pane: &Pane) -> SessionStatus {
    let cwd = match pane.cwd_path() {
        Some(cwd) => cwd,
        None => return SessionStatus::Unknown,
    };

    let dir = match get_transcript_dir(&cwd) {
        Some(dir) => dir,
        None => return SessionStatus::Unknown,
    };

    let transcript_path = match get_latest_transcript(&dir) {
        Ok(Some(path)) => path,
        _ => return SessionStatus::Unknown,
    };

    detect_session_status(&transcript_path).unwrap_or(SessionStatus::Unknown)
}

/// Add status icon to title
fn format_title(original_title: &str, status: &SessionStatus) -> String {
    let icon = status.icon();
    format!("{} {}", icon, original_title)
}
