//! Session status detection from transcript entries.

use super::parser::read_last_entries;
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

/// The detected status of a Claude Code session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
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
            SessionStatus::Processing => "Processing",
            SessionStatus::Idle => "Idle",
            SessionStatus::WaitingForUser { .. } => "Waiting",
            SessionStatus::Unknown => "Unknown",
        }
    }

    /// Get an icon for the status.
    pub fn icon(&self) -> &'static str {
        match self {
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

    // Check for progress type - always Processing
    if last.is_progress() {
        return Ok(SessionStatus::Processing);
    }

    // Check for system stop_hook_summary - indicates Idle
    if last.is_stop_hook_summary() {
        return Ok(SessionStatus::Idle);
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

    // Check for user entry with tool_result - Processing (waiting for next response)
    if last.is_tool_result() {
        return Ok(SessionStatus::Processing);
    }

    // Check for assistant with stop_reason: null - still streaming
    if last.type_ == "assistant" {
        if let Some(msg) = &last.message {
            if msg.stop_reason.is_none() {
                return Ok(SessionStatus::Processing);
            }
        }
    }

    // Look back for recent activity to determine state
    for entry in entries.iter().rev().skip(1) {
        if entry.is_progress() || entry.is_tool_result() {
            return Ok(SessionStatus::Processing);
        }
        if entry.is_stop_hook_summary() || entry.is_end_turn() {
            return Ok(SessionStatus::Idle);
        }
    }

    Ok(SessionStatus::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_status_display() {
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
        assert_eq!(SessionStatus::Processing.icon(), "●");
        assert_eq!(SessionStatus::Idle.icon(), "○");
        assert_eq!(
            SessionStatus::WaitingForUser { tools: vec![] }.icon(),
            "◐"
        );
    }
}
