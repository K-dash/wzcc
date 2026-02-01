use crate::detector::DetectionReason;
use crate::models::Pane;
use crate::session_mapping::{MappingResult, SessionMapping};
use crate::transcript::{
    detect_session_status, get_last_assistant_text, get_last_user_prompt, get_latest_transcript,
    get_transcript_dir, SessionStatus,
};
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

/// Result of session info detection
pub struct SessionInfo {
    pub status: SessionStatus,
    pub last_prompt: Option<String>,
    pub last_output: Option<String>,
    pub session_id: Option<String>,
    pub transcript_path: Option<PathBuf>,
    /// Whether this session was identified via statusLine bridge
    pub has_mapping: bool,
    /// Last updated time (from transcript file modification time)
    pub updated_at: Option<SystemTime>,
    /// Warning message to display (e.g., stale mapping)
    pub warning: Option<String>,
}

impl ClaudeSession {
    /// Get file modification time
    fn get_file_mtime(path: &PathBuf) -> Option<SystemTime> {
        std::fs::metadata(path).ok()?.modified().ok()
    }

    /// Detect session info from statusLine bridge mapping (TTY-based).
    ///
    /// This method tries to find session information using the TTY as the key.
    /// If a valid mapping exists, it uses the transcript_path from the mapping
    /// instead of guessing based on CWD.
    pub fn detect_session_info(pane: &Pane) -> SessionInfo {
        // Try to get session mapping from TTY
        if let Some(tty) = pane.tty_short() {
            match SessionMapping::from_tty_with_status(&tty) {
                MappingResult::Valid(mapping) => {
                    // We have a valid mapping - use the transcript path from it
                    let transcript_path = mapping.transcript_path.clone();

                    let (status, updated_at) = if transcript_path.exists() {
                        let status = detect_session_status(&transcript_path)
                            .unwrap_or(SessionStatus::Unknown);
                        let mtime = Self::get_file_mtime(&transcript_path);
                        (status, mtime)
                    } else {
                        (SessionStatus::Ready, None)
                    };

                    let last_prompt = if transcript_path.exists() {
                        get_last_user_prompt(&transcript_path, 200).ok().flatten()
                    } else {
                        None
                    };

                    let last_output = if transcript_path.exists() {
                        get_last_assistant_text(&transcript_path, 1000)
                            .ok()
                            .flatten()
                    } else {
                        None
                    };

                    return SessionInfo {
                        status,
                        last_prompt,
                        last_output,
                        session_id: Some(mapping.session_id),
                        transcript_path: Some(transcript_path),
                        has_mapping: true,
                        updated_at,
                        warning: None,
                    };
                }
                MappingResult::Stale => {
                    // Mapping exists but is stale - don't fallback to CWD
                    // This prevents showing wrong status from another session with same CWD
                    return SessionInfo {
                        status: SessionStatus::Unknown,
                        last_prompt: None,
                        last_output: None,
                        session_id: None,
                        transcript_path: None,
                        has_mapping: false,
                        updated_at: None,
                        warning: Some(
                            "Session info stale (statusLine not updating). Try interacting with the session.".to_string(),
                        ),
                    };
                }
                MappingResult::NotFound => {
                    // No mapping - fall through to CWD-based detection
                }
            }
        }

        // Fallback to CWD-based detection
        let (status, last_prompt, last_output, updated_at) =
            Self::detect_status_and_output_by_cwd(pane);

        SessionInfo {
            status,
            last_prompt,
            last_output,
            session_id: None,
            transcript_path: None,
            has_mapping: false,
            updated_at,
            warning: None,
        }
    }

    /// Internal: Detect session info by CWD (legacy method)
    fn detect_status_and_output_by_cwd(
        pane: &Pane,
    ) -> (
        SessionStatus,
        Option<String>,
        Option<String>,
        Option<SystemTime>,
    ) {
        let cwd = match pane.cwd_path() {
            Some(cwd) => cwd,
            None => return (SessionStatus::Unknown, None, None, None),
        };

        let dir = match get_transcript_dir(&cwd) {
            Some(dir) => dir,
            // No transcript directory = Claude Code is running but no session yet
            None => return (SessionStatus::Ready, None, None, None),
        };

        let transcript_path = match get_latest_transcript(&dir) {
            Ok(Some(path)) => path,
            // No transcript file = Claude Code is running but no session yet
            _ => return (SessionStatus::Ready, None, None, None),
        };

        let status = detect_session_status(&transcript_path).unwrap_or(SessionStatus::Unknown);
        let updated_at = Self::get_file_mtime(&transcript_path);

        // Get last user prompt (max 200 chars)
        let last_prompt = get_last_user_prompt(&transcript_path, 200).ok().flatten();

        // Get last assistant text (max 1000 chars)
        let last_output = get_last_assistant_text(&transcript_path, 1000)
            .ok()
            .flatten();

        (status, last_prompt, last_output, updated_at)
    }

    /// Get git branch from cwd
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
