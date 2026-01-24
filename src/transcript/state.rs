//! Session status detection from transcript entries.

use super::parser::read_last_entries;
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

/// The detected status of a Claude Code session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    /// Claude Code is running but no session started yet (waiting for first input)
    Ready,
    /// Claude is actively processing (streaming, executing tools, etc.)
    Processing,
    /// Claude is idle, waiting for user input
    Idle,
    /// Claude is waiting for user action (permission approval, question response, etc.)
    /// Contains the tool name(s) that are waiting for user action
    WaitingForUser { tools: Vec<String> },
    /// Status cannot be determined
    Unknown,
}

impl SessionStatus {
    /// Get a short display string for the status.
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Ready => "Ready",
            SessionStatus::Processing => "Processing",
            SessionStatus::Idle => "Idle",
            SessionStatus::WaitingForUser { .. } => "Waiting",
            SessionStatus::Unknown => "Unknown",
        }
    }

    /// Get an icon for the status.
    pub fn icon(&self) -> &'static str {
        match self {
            SessionStatus::Ready => "◇",
            SessionStatus::Processing => "●",
            SessionStatus::Idle => "○",
            SessionStatus::WaitingForUser { .. } => "◐",
            SessionStatus::Unknown => "?",
        }
    }
}

/// Configuration for status detection.
#[derive(Debug, Clone)]
pub struct DetectionConfig {
    /// Seconds after tool_use before considering it as WaitingForUser
    pub waiting_timeout_secs: u64,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            waiting_timeout_secs: 10,
        }
    }
}

/// Detect the session status from a transcript file.
pub fn detect_session_status(path: &Path) -> Result<SessionStatus> {
    detect_session_status_with_config(path, &DetectionConfig::default())
}

/// Detect the session status with custom configuration.
pub fn detect_session_status_with_config(
    path: &Path,
    config: &DetectionConfig,
) -> Result<SessionStatus> {
    // Read the last few entries to determine state
    let entries = read_last_entries(path, 10)?;

    if entries.is_empty() {
        return Ok(SessionStatus::Unknown);
    }

    // Find the last meaningful entry
    let last = entries.last().unwrap();

    // Check for progress type - Processing (but not hook_progress which is just session hooks)
    if last.is_progress() && !last.is_hook_progress() {
        return Ok(SessionStatus::Processing);
    }

    // Check for system stop_hook_summary or turn_duration - indicates Idle
    if last.is_stop_hook_summary() || last.is_turn_duration() {
        return Ok(SessionStatus::Idle);
    }

    // For system entries (other than stop_hook_summary/turn_duration), internal entries
    // like file-history-snapshot, queue-operation, or hook_progress, look at previous entries
    if last.type_ == "system"
        || last.type_ == "file-history-snapshot"
        || last.type_ == "queue-operation"
        || last.is_hook_progress()
    {
        // Find the last meaningful entry (assistant or user)
        for entry in entries.iter().rev().skip(1) {
            // Skip internal entries that don't indicate real activity
            if entry.type_ == "file-history-snapshot"
                || entry.type_ == "queue-operation"
                || entry.is_hook_progress()
            {
                continue;
            }

            if entry.is_stop_hook_summary() || entry.is_turn_duration() || entry.is_end_turn() {
                return Ok(SessionStatus::Idle);
            }
            if entry.type_ == "assistant" && !entry.is_tool_use() {
                return Ok(SessionStatus::Idle);
            }
            if entry.is_tool_use() {
                // Check time for WaitingForUser
                if let Some(timestamp) = &entry.timestamp {
                    if let Ok(entry_time) = DateTime::parse_from_rfc3339(timestamp) {
                        let now = Utc::now();
                        let elapsed = now.signed_duration_since(entry_time.with_timezone(&Utc));
                        if elapsed.num_seconds() > config.waiting_timeout_secs as i64 {
                            return Ok(SessionStatus::WaitingForUser {
                                tools: entry.get_tool_names(),
                            });
                        }
                    }
                }
                return Ok(SessionStatus::Processing);
            }
            if entry.type_ == "user" {
                return Ok(SessionStatus::Processing);
            }
            if entry.is_progress() {
                return Ok(SessionStatus::Processing);
            }
        }

        // No meaningful entries found - this is a fresh session (e.g., after /clear)
        return Ok(SessionStatus::Ready);
    }

    // Check for assistant with end_turn - Idle
    if last.is_end_turn() {
        return Ok(SessionStatus::Idle);
    }

    // Check for assistant with tool_use
    if last.is_tool_use() {
        let tools = last.get_tool_names();

        // Check if enough time has passed to consider it as waiting for user
        if let Some(timestamp) = &last.timestamp {
            if let Ok(entry_time) = DateTime::parse_from_rfc3339(timestamp) {
                let now = Utc::now();
                let elapsed = now.signed_duration_since(entry_time.with_timezone(&Utc));

                if elapsed.num_seconds() > config.waiting_timeout_secs as i64 {
                    return Ok(SessionStatus::WaitingForUser { tools });
                }
            }
        }

        // Recent tool_use - still Processing (executing)
        return Ok(SessionStatus::Processing);
    }

    // Check for assistant with text only (no tool_use) - this means Claude finished responding
    // and is waiting for user input, so it's Idle
    if last.type_ == "assistant" && !last.is_tool_use() {
        return Ok(SessionStatus::Idle);
    }

    // Check for user entry with tool_result - Processing (waiting for next response)
    if last.is_tool_result() {
        return Ok(SessionStatus::Processing);
    }

    // Check for user entry (not tool_result) - Processing (Claude is about to respond)
    if last.type_ == "user" {
        return Ok(SessionStatus::Processing);
    }

    // Look back to find the most recent user message, then check what happened after
    // Find the index of the last user entry
    let last_user_idx = entries.iter().rposition(|e| e.type_ == "user");

    if let Some(idx) = last_user_idx {
        // Check entries after the last user message
        let after_user = &entries[idx + 1..];

        // If turn_duration or stop_hook_summary exists after user message, it's Idle
        for entry in after_user.iter() {
            if entry.is_stop_hook_summary() || entry.is_turn_duration() || entry.is_end_turn() {
                return Ok(SessionStatus::Idle);
            }
        }

        // If progress exists after user message (and no turn_duration), it's Processing
        for entry in after_user.iter() {
            if entry.is_progress() {
                return Ok(SessionStatus::Processing);
            }
        }
    }

    // Check for assistant with stop_reason: null - still streaming
    if last.type_ == "assistant" {
        if let Some(msg) = &last.message {
            if msg.stop_reason.is_none() {
                return Ok(SessionStatus::Processing);
            }
        }
    }

    Ok(SessionStatus::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_status_display() {
        assert_eq!(SessionStatus::Ready.as_str(), "Ready");
        assert_eq!(SessionStatus::Processing.as_str(), "Processing");
        assert_eq!(SessionStatus::Idle.as_str(), "Idle");
        assert_eq!(
            SessionStatus::WaitingForUser {
                tools: vec!["Bash".to_string()]
            }
            .as_str(),
            "Waiting"
        );
    }

    #[test]
    fn test_session_status_icon() {
        assert_eq!(SessionStatus::Ready.icon(), "◇");
        assert_eq!(SessionStatus::Processing.icon(), "●");
        assert_eq!(SessionStatus::Idle.icon(), "○");
        assert_eq!(SessionStatus::WaitingForUser { tools: vec![] }.icon(), "◐");
    }
}
