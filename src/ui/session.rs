use crate::detector::DetectionReason;
use crate::models::Pane;
use crate::transcript::SessionStatus;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use std::path::PathBuf;
use std::time::SystemTime;
use unicode_width::UnicodeWidthChar;

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

/// Wrap text into lines with a given display width.
/// Uses unicode display width so CJK characters (2 cells) are measured correctly.
pub fn wrap_text_lines(
    text: &str,
    width: usize,
    max_lines: usize,
    color: Color,
) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            lines.push(Line::from(""));
        } else {
            let mut current = String::new();
            let mut current_width: usize = 0;
            for ch in line.chars() {
                let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
                if current_width + ch_w > width && !current.is_empty() {
                    lines.push(Line::from(Span::styled(
                        current.clone(),
                        Style::default().fg(color),
                    )));
                    if lines.len() >= max_lines {
                        return lines;
                    }
                    current.clear();
                    current_width = 0;
                }
                current.push(ch);
                current_width += ch_w;
            }
            if !current.is_empty() {
                lines.push(Line::from(Span::styled(
                    current,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn test_wrap_ascii_fits_in_width() {
        let lines = wrap_text_lines("hello", 10, usize::MAX, Color::White);
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "hello");
    }

    #[test]
    fn test_wrap_ascii_exceeds_width() {
        let lines = wrap_text_lines("abcdefghij", 5, usize::MAX, Color::White);
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "abcde");
        assert_eq!(line_text(&lines[1]), "fghij");
    }

    #[test]
    fn test_wrap_cjk_double_width() {
        // Each CJK char is 2 cells wide. Width=6 fits 3 CJK chars.
        let lines = wrap_text_lines("あいうえお", 6, usize::MAX, Color::White);
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "あいう");
        assert_eq!(line_text(&lines[1]), "えお");
    }

    #[test]
    fn test_wrap_mixed_ascii_and_cjk() {
        // "A" = 1 cell, "あ" = 2 cells. Width=5: "Aあ"=3, +next "い"=5 fits, +next "B"=6 overflows
        let lines = wrap_text_lines("AあいBう", 5, usize::MAX, Color::White);
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "Aあい");
        assert_eq!(line_text(&lines[1]), "Bう");
    }

    #[test]
    fn test_wrap_empty_lines_preserved() {
        let lines = wrap_text_lines("a\n\nb", 10, usize::MAX, Color::White);
        assert_eq!(lines.len(), 3);
        assert_eq!(line_text(&lines[0]), "a");
        assert_eq!(line_text(&lines[1]), "");
        assert_eq!(line_text(&lines[2]), "b");
    }

    #[test]
    fn test_wrap_max_lines_limit() {
        let lines = wrap_text_lines("a\nb\nc\nd", 10, 2, Color::White);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_wrap_markdown_table_with_cjk() {
        // Simulates a markdown table row with CJK content
        let row = "| スカラー | @required |";
        let lines = wrap_text_lines(row, 20, usize::MAX, Color::White);
        // "| スカラー | @required |" display width:
        // | (1) + space(1) + ス(2)+カ(2)+ラ(2)+ー(2) + space(1) + |(1) + space(1) + @required(9) + space(1) + |(1) = 24
        // Should wrap into 2 lines at width 20
        assert!(lines.len() >= 2, "Expected wrapping for CJK table row, got {} lines", lines.len());
    }
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
