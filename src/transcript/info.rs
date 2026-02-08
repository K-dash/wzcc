//! Combined transcript reading: reads a transcript file once and extracts
//! status, last user prompt, and last assistant text in a single pass.
//!
//! This module acts as an orchestration layer between `parser` (file reading
//! and data extraction) and `state` (status detection logic), avoiding a
//! direct dependency from parser to state.

use super::parser::{extract_last_assistant_text, extract_last_user_prompt, TranscriptSnapshot};
use super::state::{detect_session_status_from_entries, SessionStatus};
use anyhow::Result;
use std::path::Path;

/// Result of reading all transcript information in a single file read.
pub struct TranscriptInfo {
    pub status: SessionStatus,
    pub last_prompt: Option<String>,
    pub last_output: Option<String>,
}

/// Read a transcript file once and extract status, last user prompt, and
/// last assistant text. This replaces three separate file reads with one.
pub fn read_transcript_info(path: &Path) -> Result<TranscriptInfo> {
    let snapshot = TranscriptSnapshot::from_path(path)?;
    let entries = snapshot.last_entries(10);
    let status = detect_session_status_from_entries(&entries);
    let last_prompt = extract_last_user_prompt(&snapshot, 200);
    let last_output = extract_last_assistant_text(&snapshot, 1000);
    Ok(TranscriptInfo {
        status,
        last_prompt,
        last_output,
    })
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
    fn test_read_transcript_info_empty_file() {
        let file = NamedTempFile::new().unwrap();
        let info = read_transcript_info(file.path()).unwrap();
        assert_eq!(info.status, SessionStatus::Unknown);
        assert!(info.last_prompt.is_none());
        assert!(info.last_output.is_none());
    }

    #[test]
    fn test_read_transcript_info_idle_with_prompt_and_output() {
        let file = create_transcript(&[
            r#"{"type":"user","timestamp":"2026-01-23T16:29:00.000Z","message":{"content":"Hello Claude"}}"#,
            r#"{"type":"assistant","timestamp":"2026-01-23T16:29:01.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Hi! How can I help?"}]}}"#,
            r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-01-23T16:29:02.000Z"}"#,
        ]);
        let info = read_transcript_info(file.path()).unwrap();
        assert_eq!(info.status, SessionStatus::Idle);
        assert_eq!(info.last_prompt.as_deref(), Some("Hello Claude"));
        assert_eq!(info.last_output.as_deref(), Some("Hi! How can I help?"));
    }

    #[test]
    fn test_read_transcript_info_processing() {
        let file = create_transcript(&[
            r#"{"type":"user","timestamp":"2026-01-23T16:29:00.000Z","message":{"content":"Do something"}}"#,
            r#"{"type":"progress","timestamp":"2026-01-23T16:29:01.000Z"}"#,
        ]);
        let info = read_transcript_info(file.path()).unwrap();
        assert_eq!(info.status, SessionStatus::Processing);
        assert_eq!(info.last_prompt.as_deref(), Some("Do something"));
        assert!(info.last_output.is_none());
    }

    #[test]
    fn test_read_transcript_info_matches_individual_functions() {
        // Verify that read_transcript_info produces the same results as
        // calling the three functions individually.
        use crate::transcript::{
            detect_session_status, get_last_assistant_text, get_last_user_prompt,
        };

        let file = create_transcript(&[
            r#"{"type":"user","timestamp":"2026-01-23T16:29:00.000Z","message":{"content":"Explain closures"}}"#,
            r#"{"type":"assistant","timestamp":"2026-01-23T16:29:01.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"A closure captures variables from its environment."}]}}"#,
            r#"{"type":"system","subtype":"stop_hook_summary","timestamp":"2026-01-23T16:29:02.000Z"}"#,
        ]);

        let info = read_transcript_info(file.path()).unwrap();
        let individual_status = detect_session_status(file.path()).unwrap();
        let individual_prompt = get_last_user_prompt(file.path(), 200).unwrap();
        let individual_output = get_last_assistant_text(file.path(), 1000).unwrap();

        assert_eq!(info.status, individual_status);
        assert_eq!(info.last_prompt, individual_prompt);
        assert_eq!(info.last_output, individual_output);
    }
}
