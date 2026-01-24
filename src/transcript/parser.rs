//! JSONL transcript file parser.

use anyhow::Result;
use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

/// A content block within a message.
#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub type_: String,
    pub name: Option<String>,
    pub text: Option<String>,
}

/// The message structure within an assistant entry.
#[derive(Debug, Clone, Deserialize)]
pub struct AssistantMessage {
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
}

/// A transcript entry (one line from the JSONL file).
#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptEntry {
    #[serde(rename = "type")]
    pub type_: String,
    pub subtype: Option<String>,
    pub timestamp: Option<String>,
    pub message: Option<AssistantMessage>,
}

impl TranscriptEntry {
    /// Check if this entry is an assistant message with tool_use.
    /// Checks both stop_reason == "tool_use" and content containing tool_use blocks.
    pub fn is_tool_use(&self) -> bool {
        if self.type_ != "assistant" {
            return false;
        }

        let Some(msg) = &self.message else {
            return false;
        };

        // Check stop_reason
        if msg.stop_reason.as_deref() == Some("tool_use") {
            return true;
        }

        // Also check if content has tool_use blocks (even with stop_reason: null)
        // This happens when Claude is waiting for user approval
        msg.content.iter().any(|c| c.type_ == "tool_use")
    }

    /// Check if this entry is an assistant message with an end_turn stop_reason.
    pub fn is_end_turn(&self) -> bool {
        self.type_ == "assistant"
            && self
                .message
                .as_ref()
                .and_then(|m| m.stop_reason.as_ref())
                .map(|s| s == "end_turn")
                .unwrap_or(false)
    }

    /// Check if this is a progress entry (indicates processing).
    pub fn is_progress(&self) -> bool {
        self.type_ == "progress"
    }

    /// Check if this is a system stop_hook_summary (indicates idle).
    pub fn is_stop_hook_summary(&self) -> bool {
        self.type_ == "system" && self.subtype.as_deref() == Some("stop_hook_summary")
    }

    /// Check if this is a user entry with a tool_result.
    pub fn is_tool_result(&self) -> bool {
        if self.type_ != "user" {
            return false;
        }
        self.message
            .as_ref()
            .map(|m| m.content.iter().any(|c| c.type_ == "tool_result"))
            .unwrap_or(false)
    }

    /// Get the tool names from a tool_use message.
    pub fn get_tool_names(&self) -> Vec<String> {
        self.message
            .as_ref()
            .map(|m| {
                m.content
                    .iter()
                    .filter(|c| c.type_ == "tool_use")
                    .filter_map(|c| c.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Read the last N entries from a transcript file.
/// Uses reverse file reading for efficiency with large files.
pub fn read_last_entries(path: &Path, count: usize) -> Result<Vec<TranscriptEntry>> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();

    if file_size == 0 {
        return Ok(Vec::new());
    }

    // Read from the end of the file
    let mut reader = BufReader::new(file);
    let mut entries = Vec::new();
    let mut lines = Vec::new();

    // For small files, just read all lines
    if file_size < 1024 * 1024 {
        // < 1MB
        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
    } else {
        // For large files, seek to near the end and read
        // We estimate ~10KB per entry and read extra to be safe
        let seek_pos = file_size.saturating_sub((count as u64 + 10) * 10 * 1024);
        reader.seek(SeekFrom::Start(seek_pos))?;

        // Skip partial line if we seeked to middle
        if seek_pos > 0 {
            let mut _skip = String::new();
            reader.read_line(&mut _skip)?;
        }

        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
    }

    // Parse the last N lines
    for line in lines.iter().rev().take(count) {
        match serde_json::from_str::<TranscriptEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(_) => continue, // Skip invalid JSON lines
        }
    }

    // Reverse to get chronological order
    entries.reverse();
    Ok(entries)
}

/// Read the last entry from a transcript file.
#[allow(dead_code)]
pub fn read_last_entry(path: &Path) -> Result<Option<TranscriptEntry>> {
    let entries = read_last_entries(path, 1)?;
    Ok(entries.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_assistant_entry() {
        let json = r#"{"type":"assistant","timestamp":"2026-01-23T16:29:06.719Z","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","name":"Bash"}]}}"#;
        let entry: TranscriptEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.type_, "assistant");
        assert!(entry.is_tool_use());
        assert_eq!(entry.get_tool_names(), vec!["Bash"]);
    }

    #[test]
    fn test_parse_assistant_entry_with_null_stop_reason() {
        // This is the case when Claude is waiting for user approval
        // stop_reason is null but content has tool_use
        let json = r#"{"type":"assistant","timestamp":"2026-01-23T16:29:06.719Z","message":{"stop_reason":null,"content":[{"type":"tool_use","name":"AskUserQuestion"}]}}"#;
        let entry: TranscriptEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.type_, "assistant");
        assert!(entry.is_tool_use());
        assert_eq!(entry.get_tool_names(), vec!["AskUserQuestion"]);
    }

    #[test]
    fn test_parse_progress_entry() {
        let json = r#"{"type":"progress","timestamp":"2026-01-23T16:29:06.719Z"}"#;
        let entry: TranscriptEntry = serde_json::from_str(json).unwrap();
        assert!(entry.is_progress());
    }

    #[test]
    fn test_parse_system_stop_hook() {
        let json = r#"{"type":"system","subtype":"stop_hook_summary","timestamp":"2026-01-23T16:29:06.719Z"}"#;
        let entry: TranscriptEntry = serde_json::from_str(json).unwrap();
        assert!(entry.is_stop_hook_summary());
    }
}
