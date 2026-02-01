use crate::transcript::SessionStatus;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::time::SystemTime;

use super::session::{status_display, wrap_text_lines, ClaudeSession};

/// Format relative time (e.g., "5s ago", "2m ago", "1h ago")
fn format_relative_time(time: &SystemTime) -> String {
    let now = SystemTime::now();
    let duration = match now.duration_since(*time) {
        Ok(d) => d,
        Err(_) => return "now".to_string(),
    };

    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// Get color for elapsed time display
/// - < 5 minutes: Green (fresh/active)
/// - 5-30 minutes: Yellow (slightly stale)
/// - > 30 minutes: Red (inactive/stale)
fn elapsed_time_color(time: &SystemTime) -> Color {
    let now = SystemTime::now();
    let duration = match now.duration_since(*time) {
        Ok(d) => d,
        Err(_) => return Color::Green,
    };

    let secs = duration.as_secs();
    if secs < 300 {
        // < 5 minutes
        Color::Green
    } else if secs < 1800 {
        // 5-30 minutes
        Color::Yellow
    } else {
        // > 30 minutes
        Color::Red
    }
}

/// Animation frames for Processing status (rotating dots)
const PROCESSING_FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];

/// Render the session list.
pub fn render_list(
    f: &mut ratatui::Frame,
    area: Rect,
    sessions: &[ClaudeSession],
    list_state: &mut ListState,
    refreshing: bool,
    animation_frame: u8,
    current_workspace: &str,
) -> Option<Rect> {
    // Count sessions per (workspace, cwd)
    let mut cwd_info: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for session in sessions {
        let ws = session.pane.workspace.clone();
        if let Some(cwd) = session.pane.cwd_path() {
            *cwd_info.entry((ws, cwd)).or_insert(0) += 1;
        }
    }

    // Build list items (workspace header + cwd header + sessions)
    let mut items: Vec<ListItem> = Vec::new();
    let mut session_indices: Vec<usize> = Vec::new(); // ListItem index -> session index mapping
    let mut current_ws: Option<String> = None;
    let mut current_cwd: Option<String> = None;

    for (session_idx, session) in sessions.iter().enumerate() {
        let pane = &session.pane;
        let ws = &pane.workspace;
        let cwd = pane.cwd_path().unwrap_or_default();

        // Add workspace header for new workspace
        if current_ws.as_ref() != Some(ws) {
            current_ws = Some(ws.clone());
            current_cwd = None; // Reset cwd tracking for new workspace

            // Visual distinction for current vs other workspace (subtle colors)
            let (ws_icon, ws_style) = if ws == current_workspace {
                (
                    "🏠",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("📍", Style::default().fg(Color::DarkGray))
            };

            let ws_header = Line::from(vec![Span::styled(
                format!("{} Workspace: {}", ws_icon, ws),
                ws_style,
            )]);
            items.push(ListItem::new(ws_header));
            session_indices.push(usize::MAX); // Header is not a session
        }

        // Get group info
        let count = cwd_info
            .get(&(ws.clone(), cwd.clone()))
            .copied()
            .unwrap_or(1);

        // Add header for new CWD (within the same workspace)
        if current_cwd.as_ref() != Some(&cwd) {
            current_cwd = Some(cwd.clone());

            // Get directory name from cwd
            let dir_name = std::path::Path::new(&cwd)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&cwd)
                .to_string();

            // Show session count if multiple sessions
            let header_text = if count > 1 {
                format!("  📂 {} ({} sessions)", dir_name, count)
            } else {
                format!("  📂 {}", dir_name)
            };

            let header_line = Line::from(vec![Span::raw(header_text)]);
            items.push(ListItem::new(header_line));
            session_indices.push(usize::MAX); // Header is not a session
        }

        // Status icon and color (Processing uses animated spinner)
        let (status_icon, status_color) = match &session.status {
            SessionStatus::Ready => ("◇", Color::Cyan),
            SessionStatus::Processing => (
                PROCESSING_FRAMES[animation_frame as usize % 4],
                Color::Yellow,
            ),
            SessionStatus::Idle => ("○", Color::Green),
            SessionStatus::WaitingForUser { .. } => ("◐", Color::Magenta),
            SessionStatus::Unknown => ("?", Color::DarkGray),
        };

        // Title (max 35 chars)
        let title = if pane.title.chars().count() > 35 {
            let truncated: String = pane.title.chars().take(32).collect();
            format!("{}...", truncated)
        } else {
            pane.title.clone()
        };

        // Quick select number (1-9, or space if > 9)
        let quick_num = if session_idx < 9 {
            format!("[{}]", session_idx + 1)
        } else {
            "   ".to_string()
        };

        // Relative time display with color based on elapsed time
        let (time_display, time_color) = session
            .updated_at
            .as_ref()
            .map(|t| {
                (
                    format!(" {}", format_relative_time(t)),
                    elapsed_time_color(t),
                )
            })
            .unwrap_or((String::new(), Color::DarkGray));

        // Indent (all sessions are indented under workspace + cwd headers)
        let line = Line::from(vec![
            Span::raw("    "), // Extra indent for hierarchy
            Span::styled(format!("{} ", quick_num), Style::default().fg(Color::White)),
            Span::styled(
                format!("{} ", status_icon),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("Pane {}: ", pane.pane_id),
                Style::default().fg(Color::White),
            ),
            Span::raw(title),
            Span::styled(
                format!(" [{}]", session.status.as_str()),
                Style::default().fg(status_color),
            ),
            Span::styled(time_display, Style::default().fg(time_color)),
        ]);

        items.push(ListItem::new(line));
        session_indices.push(session_idx);
    }

    // Convert list_state index to ListItem index
    let list_index = list_state
        .selected()
        .and_then(|session_idx| session_indices.iter().position(|&idx| idx == session_idx));

    let mut render_state = ListState::default();
    render_state.select(list_index);

    // Title (show indicator while refreshing)
    let title = if refreshing {
        " ⌛ Claude Code Sessions - Refreshing... ".to_string()
    } else {
        format!(" Claude Code Sessions ({}) ", sessions.len())
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut render_state);

    Some(area)
}

/// Render the details panel.
pub fn render_details(
    f: &mut ratatui::Frame,
    area: Rect,
    sessions: &[ClaudeSession],
    selected: Option<usize>,
) {
    let text = if let Some(i) = selected {
        if let Some(session) = sessions.get(i) {
            let pane = &session.pane;

            // Quick select number display (1-9 or none)
            let quick_num_display = if i < 9 {
                format!(" [{}]", i + 1)
            } else {
                String::new()
            };

            let mut lines = vec![Line::from(vec![
                Span::styled("Pane: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(pane.pane_id.to_string()),
                Span::styled(quick_num_display, Style::default().fg(Color::DarkGray)),
            ])];

            // Display workspace
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Workspace: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(&pane.workspace, Style::default().fg(Color::Yellow)),
            ]));

            if let Some(cwd) = pane.cwd_path() {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "CWD:",
                    Style::default().add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(cwd));
            }

            if let Some(tty) = &pane.tty_name {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("TTY: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(tty),
                ]));
            }

            // Display session status
            lines.push(Line::from(""));
            let (status_color, status_text) = status_display(&session.status);
            lines.push(Line::from(vec![
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(status_text, Style::default().fg(status_color)),
            ]));

            // Display warning message if present
            if let Some(warning) = &session.warning {
                lines.push(Line::from(vec![Span::styled(
                    format!("⚠️  {}", warning),
                    Style::default().fg(Color::Red),
                )]));
            }

            // Display git branch
            if let Some(branch) = &session.git_branch {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Branch: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(branch, Style::default().fg(Color::Cyan)),
                ]));
            }

            // Display last prompt and last output preview
            // Fixed lines: Pane(2) + Workspace(2) + CWD(3) + TTY(2) + Status(2) + Branch(2) + border(2) = ~15 lines
            let fixed_lines: u16 = 15;
            let available_for_preview = area.height.saturating_sub(fixed_lines) as usize;
            let inner_width = (area.width.saturating_sub(2)) as usize;

            // Display if at least 1 line available (previously 3 lines was too strict)
            if available_for_preview >= 1 {
                // Separator line
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "─".repeat(inner_width),
                    Style::default().fg(Color::DarkGray),
                )]));

                // Display last prompt (1-2 lines)
                if let Some(prompt) = &session.last_prompt {
                    lines.push(Line::from(vec![Span::styled(
                        "💬 Last prompt:",
                        Style::default().add_modifier(Modifier::BOLD),
                    )]));
                    // Truncate prompt to 1-2 lines
                    let prompt_chars: Vec<char> = prompt.chars().collect();
                    let max_prompt_len = inner_width * 2;
                    let truncated: String = if prompt_chars.len() > max_prompt_len {
                        prompt_chars[..max_prompt_len].iter().collect::<String>() + "..."
                    } else {
                        prompt_chars.iter().collect()
                    };
                    for line in truncated.lines().take(2) {
                        lines.push(Line::from(Span::styled(
                            line.to_string(),
                            Style::default().fg(Color::Cyan),
                        )));
                    }
                }

                // Display last output
                if let Some(output) = &session.last_output {
                    // Separator between prompt and output
                    if session.last_prompt.is_some() {
                        lines.push(Line::from(""));
                        lines.push(Line::from(vec![Span::styled(
                            "─".repeat(inner_width),
                            Style::default().fg(Color::DarkGray),
                        )]));
                    }

                    lines.push(Line::from(vec![Span::styled(
                        "🤖 Last output:",
                        Style::default().add_modifier(Modifier::BOLD),
                    )]));

                    // Separator + prompt + output label uses ~8 lines
                    let preview_lines = available_for_preview.saturating_sub(8);
                    let output_lines =
                        wrap_text_lines(output, inner_width, preview_lines, Color::Gray);
                    lines.extend(output_lines);
                }
            }

            lines
        } else {
            vec![Line::from("No selection")]
        }
    } else {
        vec![Line::from("No sessions")]
    };

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Details "))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Render the footer with keybindings help.
pub fn render_footer(f: &mut ratatui::Frame, area: Rect) {
    let help_text = Line::from(vec![
        Span::styled("[↑↓/jk]", Style::default().fg(Color::Cyan)),
        Span::raw("Select "),
        Span::styled("[Enter]", Style::default().fg(Color::Cyan)),
        Span::raw("Focus "),
        Span::styled("[1-9]", Style::default().fg(Color::Cyan)),
        Span::raw("Quick "),
        Span::styled("[r]", Style::default().fg(Color::Cyan)),
        Span::raw("Refresh "),
        Span::styled("[q]", Style::default().fg(Color::Cyan)),
        Span::raw("Quit"),
    ]);

    let paragraph = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));

    f.render_widget(paragraph, area);
}
