use crate::detector::DetectionReason;
use crate::models::Pane;
use crate::transcript::{
    detect_session_status, get_last_assistant_text, get_last_user_prompt, get_latest_transcript,
    get_transcript_dir, SessionStatus,
};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

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
pub fn wrap_text_lines(text: &str, width: usize, max_lines: usize, color: Color) -> Vec<Line<'static>> {
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

/// Claude Code セッション情報
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
}

impl ClaudeSession {
    /// Pane の cwd からトランスクリプトを読んで状態、最終プロンプト、最終出力を検出
    pub fn detect_status_and_output(pane: &Pane) -> (SessionStatus, Option<String>, Option<String>) {
        let cwd = match pane.cwd_path() {
            Some(cwd) => cwd,
            None => return (SessionStatus::Unknown, None, None),
        };

        let dir = match get_transcript_dir(&cwd) {
            Some(dir) => dir,
            // No transcript directory = Claude Code is running but no session yet
            None => return (SessionStatus::Ready, None, None),
        };

        let transcript_path = match get_latest_transcript(&dir) {
            Ok(Some(path)) => path,
            // No transcript file = Claude Code is running but no session yet
            _ => return (SessionStatus::Ready, None, None),
        };

        let status = detect_session_status(&transcript_path).unwrap_or(SessionStatus::Unknown);

        // Get last user prompt (max 200 chars)
        let last_prompt = get_last_user_prompt(&transcript_path, 200).ok().flatten();

        // Get last assistant text (max 1000 chars)
        let last_output = get_last_assistant_text(&transcript_path, 1000)
            .ok()
            .flatten();

        (status, last_prompt, last_output)
    }

    /// cwd から git branch を取得
    pub fn get_git_branch(cwd: &str) -> Option<String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["-C", cwd, "rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()?;

        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !branch.is_empty() {
                return Some(branch);
            }
        }
        None
    }
}
