use crate::detector::DetectionReason;
use crate::models::Pane;
use crate::transcript::SessionStatus;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use std::path::PathBuf;
use std::time::SystemTime;

/// Get display color and text for a SessionStatus.
pub fn status_display(status: &SessionStatus) -> (Color, String) {
    match status {
        SessionStatus::Ready => (Color::Cyan, "Ready".to_string()),
        SessionStatus::Processing => (Color::Yellow, "Processing".to_string()),
        SessionStatus::Idle => (Color::Green, "Idle".to_string()),
        SessionStatus::WaitingForUser { tools } => {
            let text = if tools.is_empty() {
                "Approval".to_string()
            } else {
                format!("Approval ({})", tools.join(", "))
            };
            (Color::Magenta, text)
        }
        SessionStatus::Unknown => (Color::DarkGray, "Unknown".to_string()),
    }
}

/// Wrap text into lines with a given width.
pub fn wrap_text_lines(
    text: &str,
    width: usize,
    max_lines: usize,
    color: Color,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            lines.push(Line::from(""));
        } else {
            let chars: Vec<char> = line.chars().collect();
            for chunk in chars.chunks(width.max(1)) {
                lines.push(Line::from(Span::styled(
                    chunk.iter().collect::<String>(),
                    Style::default().fg(color),
                )));
                if lines.len() >= max_lines {
                    return lines;
                }
            }
        }
        if lines.len() >= max_lines {
            break;
        }
    }
    lines
}

/// Claude Code session information
#[derive(Debug, Clone)]
pub struct ClaudeSession {
    pub pane: Pane,
    pub detected: bool,
    #[allow(dead_code)]
    pub reason: DetectionReason,
    /// Session status (Processing/Idle/WaitingForUser/Unknown)
    pub status: SessionStatus,
    /// Git branch name
    pub git_branch: Option<String>,
    /// Last user prompt (from transcript)
    pub last_prompt: Option<String>,
    /// Last assistant output text (from transcript)
    pub last_output: Option<String>,
    /// Session ID from statusLine bridge (if available)
    pub session_id: Option<String>,
    /// Transcript path from statusLine bridge (if available)
    pub transcript_path: Option<PathBuf>,
    /// Last updated time (from transcript file modification time)
    pub updated_at: Option<SystemTime>,
    /// Warning message to display in details
    pub warning: Option<String>,
}
