use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

use crate::ui::slash_commands::SlashCommand;

/// Render the slash command autocomplete popup overlay.
///
/// Anchored to the details panel area (right column), positioned above the input box.
pub(super) fn render_slash_complete(
    f: &mut ratatui::Frame,
    details_area: Rect,
    commands: &[SlashCommand],
    filtered: &[usize],
    list_state: &mut ListState,
) {
    if filtered.is_empty() {
        return;
    }

    // Popup dimensions: full width of details area minus 2 for padding
    let popup_width = details_area.width.saturating_sub(2).max(20);
    // Height: min(filtered_count + 2 for borders, 12), clamped to available space
    let popup_height = ((filtered.len() as u16) + 2)
        .min(12)
        .min(details_area.height.saturating_sub(4));

    if popup_height < 3 || popup_width < 10 {
        return;
    }

    // Position: anchored to bottom of details area (just above where input box would be)
    let x = details_area.x + 1;
    let y = details_area
        .y
        .saturating_add(details_area.height)
        .saturating_sub(popup_height)
        .saturating_sub(3); // above input area (input box is ~3 lines from bottom)
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Clear the area behind the popup
    f.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = filtered
        .iter()
        .filter_map(|&idx| commands.get(idx))
        .map(|cmd| {
            let mut spans = vec![Span::styled(
                format!("/{}", cmd.name),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )];

            if let Some(hint) = &cmd.argument_hint {
                spans.push(Span::styled(
                    format!(" {}", hint),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            if !cmd.description.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", cmd.description),
                    Style::default().fg(Color::Gray),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" / Commands ")
                .border_style(Style::default().fg(Color::Green)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, popup_area, list_state);
}
