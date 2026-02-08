//! Session status detection from transcript entries.

use super::parser::{read_last_entries, TranscriptEntry};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

/// Check if an entry is an internal/system entry that doesn't indicate real activity.
fn is_internal_entry(entry: &TranscriptEntry) -> bool {
    entry.type_ == "file-history-snapshot"
        || entry.type_ == "queue-operation"
        || entry.is_hook_progress()
}

/// Check if a tool_use entry has timed out and should be considered as WaitingForUser.
/// Returns Some(SessionStatus) if status can be determined, None otherwise.
fn check_tool_use_status(entry: &TranscriptEntry, config: &DetectionConfig) -> SessionStatus {
    let tools = entry.get_tool_names();

    if let Some(timestamp) = &entry.timestamp {
        if let Ok(entry_time) = DateTime::parse_from_rfc3339(timestamp) {
            let elapsed = Utc::now().signed_duration_since(entry_time.with_timezone(&Utc));
            if elapsed.num_seconds() > config.waiting_timeout_secs as i64 {
                return SessionStatus::WaitingForUser { tools };
            }
        }
    }

    SessionStatus::Processing
}

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
    let entries = read_last_entries(path, 10)?;
    Ok(detect_status_from_entries_with_config(&entries, config))
}

/// Detect session status from pre-parsed entries with default configuration.
pub fn detect_session_status_from_entries(entries: &[TranscriptEntry]) -> SessionStatus {
    detect_status_from_entries_with_config(entries, &DetectionConfig::default())
}

/// Detect session status from pre-parsed entries with custom configuration.
/// This is the core logic, extracted to avoid re-reading the file when
/// status detection is combined with other transcript queries.
pub fn detect_status_from_entries_with_config(
    entries: &[TranscriptEntry],
    config: &DetectionConfig,
) -> SessionStatus {
    if entries.is_empty() {
        return SessionStatus::Unknown;
    }

    // Find the last meaningful entry
    let last = entries.last().unwrap();

    // Check for progress type - Processing (but not hook_progress which is just session hooks)
    if last.is_progress() && !last.is_hook_progress() {
        return SessionStatus::Processing;
    }

    // Check for system stop_hook_summary or turn_duration - indicates Idle
    if last.is_stop_hook_summary() || last.is_turn_duration() {
        return SessionStatus::Idle;
    }

    // For system entries (other than stop_hook_summary/turn_duration), internal entries
    // like file-history-snapshot, queue-operation, or hook_progress, look at previous entries
    if last.type_ == "system" || is_internal_entry(last) {
        // Find the last meaningful entry (assistant or user)
        for entry in entries.iter().rev().skip(1) {
            if is_internal_entry(entry) {
                continue;
            }

            if entry.is_stop_hook_summary() || entry.is_turn_duration() || entry.is_end_turn() {
                return SessionStatus::Idle;
            }
            if entry.type_ == "assistant" && !entry.is_tool_use() {
                return SessionStatus::Idle;
            }
            if entry.is_tool_use() {
                return check_tool_use_status(entry, config);
            }
            if entry.type_ == "user" || entry.is_progress() {
                return SessionStatus::Processing;
            }
        }

        // No meaningful entries found - this is a fresh session (e.g., after /clear)
        return SessionStatus::Ready;
    }

    // Check for assistant with end_turn - Idle
    if last.is_end_turn() {
        return SessionStatus::Idle;
    }

    // Check for assistant with tool_use
    if last.is_tool_use() {
        return check_tool_use_status(last, config);
    }

    // Check for assistant with text only (no tool_use) - this means Claude finished responding
    // and is waiting for user input, so it's Idle
    if last.type_ == "assistant" && !last.is_tool_use() {
        return SessionStatus::Idle;
    }

    // Check for interrupted user entry - Idle (Claude hasn't started responding yet)
    if last.is_interrupted() {
        return SessionStatus::Idle;
    }

    // Check for user entry with tool_result - Processing (waiting for next response)
    // But first check if it's an interrupted result
    if last.is_tool_result() {
        // Look back to see if there's an interruption message after the tool_result
        for entry in entries.iter().rev().skip(1).take(3) {
            if entry.is_interrupted() {
                return SessionStatus::Idle;
            }
        }
        return SessionStatus::Processing;
    }

    // Check for user entry (not tool_result) - Processing (Claude is about to respond)
    if last.type_ == "user" {
        return SessionStatus::Processing;
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
                return SessionStatus::Idle;
            }
        }

        // If progress exists after user message (and no turn_duration), it's Processing
        for entry in after_user.iter() {
            if entry.is_progress() {
                return SessionStatus::Processing;
            }
        }
    }

    // Check for assistant with stop_reason: null - still streaming
    if last.type_ == "assistant" {
        if let Some(msg) = &last.message {
            if msg.stop_reason.is_none() {
                return SessionStatus::Processing;
            }
        }
    }

    SessionStatus::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_transcript(entries: &[&str]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        for entry in entries {
            writeln!(file, "{}", entry).unwrap();
        }
        file.flush().unwrap();
        file
    }

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
    fn test_detect_empty_file_returns_unknown() {
        let file = NamedTempFile::new().unwrap();
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Unknown);
    }

    #[test]
    fn test_detect_progress_returns_processing() {
        let file =
            create_transcript(&[r#"{"type":"progress","timestamp":"2026-01-23T16:29:06.719Z"}"#]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Processing);
    }

    #[test]
    fn test_detect_hook_progress_skipped_returns_ready() {
        // hook_progress is internal, should be skipped and result in Ready if no other entries
        let file = create_transcript(&[
            r#"{"type":"progress","timestamp":"2026-01-23T16:29:06.719Z","data":{"type":"hook_progress"}}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Ready);
    }

    #[test]
    fn test_detect_stop_hook_summary_returns_idle() {
        let file = create_transcript(&[
            r#"{"type":"system","subtype":"stop_hook_summary","timestamp":"2026-01-23T16:29:06.719Z"}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_detect_turn_duration_returns_idle() {
        let file = create_transcript(&[
            r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-01-23T16:29:06.719Z"}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_detect_end_turn_returns_idle() {
        let file = create_transcript(&[
            r#"{"type":"assistant","timestamp":"2026-01-23T16:29:06.719Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Done!"}]}}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_detect_user_entry_returns_processing() {
        // Use valid JSON format that parser can handle
        let file =
            create_transcript(&[r#"{"type":"user","timestamp":"2026-01-23T16:29:06.719Z"}"#]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Processing);
    }

    #[test]
    fn test_detect_tool_result_returns_processing() {
        let file = create_transcript(&[
            r#"{"type":"user","timestamp":"2026-01-23T16:29:06.719Z","message":{"content":[{"type":"tool_result","tool_use_id":"123"}]}}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Processing);
    }

    #[test]
    fn test_detect_recent_tool_use_returns_processing() {
        // Tool use with very recent timestamp should be Processing
        let now = Utc::now();
        let timestamp = now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let entry = format!(
            r#"{{"type":"assistant","timestamp":"{}","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"Bash"}}]}}}}"#,
            timestamp
        );
        let file = create_transcript(&[&entry]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Processing);
    }

    #[test]
    fn test_detect_old_tool_use_returns_waiting_for_user() {
        // Tool use with old timestamp (> 10 seconds) should be WaitingForUser
        let old_time = Utc::now() - chrono::Duration::seconds(15);
        let timestamp = old_time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let entry = format!(
            r#"{{"type":"assistant","timestamp":"{}","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"AskUserQuestion"}}]}}}}"#,
            timestamp
        );
        let file = create_transcript(&[&entry]);
        let status = detect_session_status(file.path()).unwrap();
        assert!(
            matches!(status, SessionStatus::WaitingForUser { tools } if tools == vec!["AskUserQuestion"])
        );
    }

    #[test]
    fn test_detect_assistant_text_only_returns_idle() {
        // Assistant with text only (no tool_use) means Claude finished responding
        let file = create_transcript(&[
            r#"{"type":"assistant","timestamp":"2026-01-23T16:29:06.719Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Here is the answer"}]}}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_detect_internal_entries_skipped() {
        // file-history-snapshot should be skipped, and we should look at previous entry
        let file = create_transcript(&[
            r#"{"type":"assistant","timestamp":"2026-01-23T16:29:06.719Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Done"}]}}"#,
            r#"{"type":"file-history-snapshot","timestamp":"2026-01-23T16:29:07.719Z"}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_detect_queue_operation_skipped() {
        let file = create_transcript(&[
            r#"{"type":"assistant","timestamp":"2026-01-23T16:29:06.719Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Done"}]}}"#,
            r#"{"type":"queue-operation","timestamp":"2026-01-23T16:29:07.719Z"}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_detect_only_internal_entries_returns_ready() {
        // If only internal entries exist, it's a fresh session
        let file = create_transcript(&[
            r#"{"type":"file-history-snapshot","timestamp":"2026-01-23T16:29:06.719Z"}"#,
            r#"{"type":"queue-operation","timestamp":"2026-01-23T16:29:07.719Z"}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Ready);
    }

    #[test]
    fn test_detect_assistant_without_tool_use_returns_idle() {
        // Assistant without tool_use is considered Idle (even with stop_reason: null)
        // This is because the current impl checks !is_tool_use() first
        let file = create_transcript(&[
            r#"{"type":"assistant","timestamp":"2026-01-23T16:29:06.719Z","message":{"stop_reason":null,"content":[]}}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_detect_with_custom_timeout() {
        // Tool use with 5 seconds old should be WaitingForUser with 3 second timeout
        let old_time = Utc::now() - chrono::Duration::seconds(5);
        let timestamp = old_time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let entry = format!(
            r#"{{"type":"assistant","timestamp":"{}","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"Bash"}}]}}}}"#,
            timestamp
        );
        let file = create_transcript(&[&entry]);

        let config = DetectionConfig {
            waiting_timeout_secs: 3,
        };
        let status = detect_session_status_with_config(file.path(), &config).unwrap();
        assert!(matches!(status, SessionStatus::WaitingForUser { tools } if tools == vec!["Bash"]));
    }

    #[test]
    fn test_detect_complex_sequence_user_then_idle() {
        // user -> progress -> turn_duration should be Idle
        let file = create_transcript(&[
            r#"{"type":"user","timestamp":"2026-01-23T16:29:00.000Z","message":{"content":"Hello"}}"#,
            r#"{"type":"progress","timestamp":"2026-01-23T16:29:01.000Z"}"#,
            r#"{"type":"assistant","timestamp":"2026-01-23T16:29:02.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Hi!"}]}}"#,
            r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-01-23T16:29:03.000Z"}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_detect_tool_use_with_null_stop_reason() {
        // tool_use in content but stop_reason is null (waiting for approval)
        let old_time = Utc::now() - chrono::Duration::seconds(15);
        let timestamp = old_time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let entry = format!(
            r#"{{"type":"assistant","timestamp":"{}","message":{{"stop_reason":null,"content":[{{"type":"tool_use","name":"Edit"}}]}}}}"#,
            timestamp
        );
        let file = create_transcript(&[&entry]);
        let status = detect_session_status(file.path()).unwrap();
        assert!(matches!(status, SessionStatus::WaitingForUser { tools } if tools == vec!["Edit"]));
    }

    #[test]
    fn test_detect_interrupted_text_returns_idle() {
        // Interrupted text message should return Idle
        let file = create_transcript(&[
            r#"{"type":"user","timestamp":"2026-01-23T16:29:06.719Z","message":{"content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]}}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_detect_interrupted_after_tool_result_returns_idle() {
        // Sequence: assistant tool_use -> user tool_result (error) -> user interrupted message
        let old_time = Utc::now() - chrono::Duration::seconds(5);
        let timestamp = old_time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let tool_use_entry = format!(
            r#"{{"type":"assistant","timestamp":"{}","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"AskUserQuestion"}}]}}}}"#,
            timestamp
        );
        let file = create_transcript(&[
            &tool_use_entry,
            r#"{"type":"user","timestamp":"2026-01-23T16:29:10.719Z","message":{"content":[{"type":"tool_result","content":"[Request interrupted by user for tool use]","is_error":true}]}}"#,
            r#"{"type":"user","timestamp":"2026-01-23T16:29:10.720Z","message":{"content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]}}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn test_detect_interrupted_simple_format_returns_idle() {
        // Simple interruption without "for tool use"
        let file = create_transcript(&[
            r#"{"type":"user","timestamp":"2026-01-23T16:29:06.719Z","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#,
        ]);
        let status = detect_session_status(file.path()).unwrap();
        assert_eq!(status, SessionStatus::Idle);
    }
}
