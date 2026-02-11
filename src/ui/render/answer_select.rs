use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

use crate::ui::app::AnswerSelectState;

/// Render the answer selection popup overlay.
pub(super) fn render_answer_select(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AnswerSelectState,
    list_state: &mut ListState,
) {
    // Calculate popup dimensions: 50% of terminal width (wider for question text)
    let popup_width = (area.width * 50 / 100)
        .max(30)
        .min(area.width.saturating_sub(4));
    // +3 for top border (with title) + bottom border + title wrapping room
    let popup_height = ((state.options.len() as u16) + 2)
        .max(4)
        .min(area.height.saturating_sub(4));

    // Center the popup
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Clear the area behind the popup
    f.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = state
        .options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let mut spans = vec![
                Span::styled(
                    format!("[{}] ", i + 1),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(&opt.label),
            ];
            if let Some(ref desc) = opt.description {
                spans.push(Span::styled(
                    format!(" - {}", desc),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    // Truncate title to fit in border (leave room for " " padding and borders).
    // Use char boundary to avoid panic on multi-byte (CJK, emoji) titles.
    let max_title_len = popup_width.saturating_sub(4) as usize;
    let title = if state.title.chars().count() > max_title_len {
        let truncated: String = state
            .title
            .chars()
            .take(max_title_len.saturating_sub(3))
            .collect();
        format!(" {}... ", truncated)
    } else {
        format!(" {} ", state.title)
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, popup_area, list_state);
}
