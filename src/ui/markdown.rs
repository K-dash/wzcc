use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::sync::OnceLock;
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
};

// ---------------------------------------------------------------------------
// Syntect resource cache (loaded once, reused across renders)
// ---------------------------------------------------------------------------

struct HighlightResources {
    syntax_set: SyntaxSet,
    theme: Theme,
}

/// Returns the shared highlight resources, or `None` if loading failed.
fn highlight_resources() -> Option<&'static HighlightResources> {
    static RESOURCES: OnceLock<Option<HighlightResources>> = OnceLock::new();
    RESOURCES
        .get_or_init(|| {
            let syntax_set = SyntaxSet::load_defaults_newlines();
            let theme_set = ThemeSet::load_defaults();
            let theme = theme_set.themes.get("base16-ocean.dark")?.clone();
            Some(HighlightResources { syntax_set, theme })
        })
        .as_ref()
}

// ---------------------------------------------------------------------------
// Code block highlighting
// ---------------------------------------------------------------------------

/// Syntax-highlight a code block. Falls back to plain gray text on any error.
fn highlight_code_block(code: &str, lang: Option<&str>) -> Vec<Line<'static>> {
    if let Some(res) = highlight_resources() {
        let syntax = lang
            .and_then(|l| res.syntax_set.find_syntax_by_token(l))
            .unwrap_or_else(|| res.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &res.theme);
        let mut lines = Vec::new();

        for line_str in code.lines() {
            match highlighter.highlight_line(line_str, &res.syntax_set) {
                Ok(highlighted) => {
                    let spans: Vec<Span<'static>> = highlighted
                        .into_iter()
                        .map(|seg| {
                            let text_owned = seg.1.to_string();
                            match syntect_tui::into_span(seg) {
                                Ok(s) => Span::styled(text_owned, s.style),
                                Err(_) => {
                                    Span::styled(text_owned, Style::default().fg(Color::Gray))
                                }
                            }
                        })
                        .collect();
                    lines.push(Line::from(spans));
                }
                Err(_) => {
                    lines.push(Line::from(Span::styled(
                        line_str.to_string(),
                        Style::default().fg(Color::Gray),
                    )));
                }
            }
        }
        lines
    } else {
        // Syntect unavailable — plain text fallback
        code.lines()
            .map(|l| {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(Color::Gray),
                ))
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Plain-text fallback
// ---------------------------------------------------------------------------

/// Simple newline-split fallback used when markdown parsing encounters issues.
fn plain_text_lines(text: &str) -> Vec<Line<'static>> {
    text.lines()
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(Color::Gray),
            ))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Markdown → ratatui conversion
// ---------------------------------------------------------------------------

/// Convert markdown text to styled ratatui lines, pre-wrapped to fit within
/// `width` display columns. Each returned `Line` represents exactly one
/// visual row, so `lines.len()` gives an accurate scroll height.
///
/// This function never fails — on any internal error it falls back to
/// plain gray text split by newlines.
pub fn markdown_to_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    // Guard: catch panics from pulldown-cmark (shouldn't happen, but be safe)
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let logical = markdown_to_lines_inner(text, width);
        wrap_lines(logical, width)
    })) {
        Ok(lines) => lines,
        Err(_) => plain_text_lines(text),
    }
}

/// Convert markdown text to styled ratatui lines, pre-wrapped and truncated
/// to `max_lines` visual rows.
pub fn markdown_to_lines_truncated(
    text: &str,
    width: usize,
    max_lines: usize,
) -> Vec<Line<'static>> {
    if max_lines == 0 {
        return Vec::new();
    }
    let all_lines = markdown_to_lines(text, width);
    if all_lines.len() <= max_lines {
        all_lines
    } else {
        let mut truncated: Vec<Line<'static>> = all_lines.into_iter().take(max_lines).collect();
        truncated.push(Line::from(Span::styled(
            "...",
            Style::default().fg(Color::DarkGray),
        )));
        truncated
    }
}

/// Pre-wrap logical lines to fit within the given viewport width.
/// Each returned `Line` represents exactly one visual row.
/// Spans are split at character boundaries to preserve styling across wraps.
fn wrap_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut result = Vec::new();

    for line in lines {
        if line.spans.is_empty() || (line.spans.len() == 1 && line.spans[0].content.is_empty()) {
            result.push(line);
            continue;
        }

        let mut row_spans: Vec<Span<'static>> = Vec::new();
        let mut buf = String::new();
        let mut buf_style = Style::default();
        let mut col: usize = 0;
        let mut first = true;

        for span in &line.spans {
            // When the style changes, flush the buffer as a Span
            if first {
                buf_style = span.style;
                first = false;
            } else if span.style != buf_style {
                if !buf.is_empty() {
                    row_spans.push(Span::styled(std::mem::take(&mut buf), buf_style));
                }
                buf_style = span.style;
            }

            for ch in span.content.chars() {
                let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if col + ch_w > width && col > 0 {
                    // Flush current buffer, then emit the row
                    if !buf.is_empty() {
                        row_spans.push(Span::styled(std::mem::take(&mut buf), buf_style));
                    }
                    if !row_spans.is_empty() {
                        result.push(Line::from(std::mem::take(&mut row_spans)));
                    }
                    col = 0;
                }
                buf.push(ch);
                col += ch_w;
            }
        }

        // Flush remaining content
        if !buf.is_empty() {
            row_spans.push(Span::styled(buf, buf_style));
        }
        if !row_spans.is_empty() {
            result.push(Line::from(row_spans));
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Internal implementation
// ---------------------------------------------------------------------------

fn markdown_to_lines_inner(text: &str, width: usize) -> Vec<Line<'static>> {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(text, options);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default().fg(Color::Gray)];

    let mut in_code_block = false;
    let mut code_block_lang: Option<String> = None;
    let mut code_block_content = String::new();

    let mut list_depth: usize = 0;
    let mut list_index_stack: Vec<Option<u64>> = Vec::new();
    let mut link_url_stack: Vec<String> = Vec::new();

    // Table state
    let mut in_table = false;
    let mut in_table_header = false;
    let mut table_header: Vec<Vec<Span<'static>>> = Vec::new();
    let mut table_rows: Vec<Vec<Vec<Span<'static>>>> = Vec::new();
    let mut current_row: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current_cell_spans: Vec<Span<'static>> = Vec::new();

    for event in parser {
        match event {
            // -- Block-level events --
            Event::Start(Tag::Heading { level, .. }) => {
                flush_line(&mut current_spans, &mut lines);
                style_stack.push(heading_style(level));
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_line(&mut current_spans, &mut lines);
                style_stack.pop();
                lines.push(Line::from(""));
            }

            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush_line(&mut current_spans, &mut lines);
                lines.push(Line::from(""));
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                flush_line(&mut current_spans, &mut lines);
                in_code_block = true;
                code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        let s = lang.to_string();
                        if s.is_empty() {
                            None
                        } else {
                            Some(s)
                        }
                    }
                    CodeBlockKind::Indented => None,
                };
                code_block_content.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                let highlighted =
                    highlight_code_block(&code_block_content, code_block_lang.as_deref());
                lines.push(code_block_separator());
                lines.extend(highlighted);
                lines.push(code_block_separator());
                lines.push(Line::from(""));
                in_code_block = false;
                code_block_lang = None;
                code_block_content.clear();
            }

            Event::Start(Tag::List(first_item)) => {
                flush_line(&mut current_spans, &mut lines);
                list_depth += 1;
                list_index_stack.push(first_item);
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                list_index_stack.pop();
                lines.push(Line::from(""));
            }

            Event::Start(Tag::Item) => {
                let indent = "  ".repeat(list_depth.saturating_sub(1));
                let bullet = if let Some(Some(idx)) = list_index_stack.last_mut() {
                    let s = format!("{}{}. ", indent, idx);
                    *idx += 1;
                    s
                } else {
                    format!("{}- ", indent)
                };
                current_spans.push(Span::styled(bullet, Style::default().fg(Color::Cyan)));
            }
            Event::End(TagEnd::Item) => {
                flush_line(&mut current_spans, &mut lines);
            }

            Event::Start(Tag::BlockQuote(_)) => {
                flush_line(&mut current_spans, &mut lines);
                current_spans.push(Span::styled("│ ", Style::default().fg(Color::DarkGray)));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush_line(&mut current_spans, &mut lines);
                lines.push(Line::from(""));
            }

            Event::Rule => {
                flush_line(&mut current_spans, &mut lines);
                lines.push(Line::from(Span::styled(
                    "────────────────────────────────",
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::from(""));
            }

            // -- Inline events --
            Event::Start(Tag::Emphasis) => {
                let current = current_style(&style_stack);
                style_stack.push(current.add_modifier(Modifier::ITALIC));
            }
            Event::End(TagEnd::Emphasis) => {
                style_stack.pop();
            }

            Event::Start(Tag::Strong) => {
                let current = current_style(&style_stack);
                style_stack.push(current.add_modifier(Modifier::BOLD));
            }
            Event::End(TagEnd::Strong) => {
                style_stack.pop();
            }

            Event::Start(Tag::Strikethrough) => {
                let current = current_style(&style_stack);
                style_stack.push(current.add_modifier(Modifier::CROSSED_OUT));
            }
            Event::End(TagEnd::Strikethrough) => {
                style_stack.pop();
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                // Push underline style for link text; store URL for End handler
                let current = current_style(&style_stack);
                style_stack.push(current.add_modifier(Modifier::UNDERLINED));
                // Stash the URL on the stack as a marker (we use a dedicated vec)
                link_url_stack.push(dest_url.to_string());
            }
            Event::End(TagEnd::Link) => {
                style_stack.pop();
                if let Some(url) = link_url_stack.pop() {
                    if !url.is_empty() {
                        if in_table {
                            current_cell_spans.push(Span::styled(
                                format!(" ({})", url),
                                Style::default().fg(Color::DarkGray),
                            ));
                        } else {
                            current_spans.push(Span::styled(
                                format!(" ({})", url),
                                Style::default().fg(Color::DarkGray),
                            ));
                        }
                    }
                }
            }

            Event::Code(code_text) => {
                if in_table {
                    current_cell_spans.push(Span::styled(
                        format!(" {} ", code_text),
                        Style::default().fg(Color::Yellow).bg(Color::DarkGray),
                    ));
                } else {
                    current_spans.push(Span::styled(
                        format!(" {} ", code_text),
                        Style::default().fg(Color::Yellow).bg(Color::DarkGray),
                    ));
                }
            }

            Event::Text(text_content) => {
                if in_code_block {
                    code_block_content.push_str(&text_content);
                } else if in_table {
                    current_cell_spans.push(Span::styled(
                        text_content.to_string(),
                        current_style(&style_stack),
                    ));
                } else {
                    let parts: Vec<&str> = text_content.split('\n').collect();
                    for (i, part) in parts.iter().enumerate() {
                        if i > 0 {
                            flush_line(&mut current_spans, &mut lines);
                        }
                        if !part.is_empty() {
                            current_spans
                                .push(Span::styled(part.to_string(), current_style(&style_stack)));
                        }
                    }
                }
            }

            Event::SoftBreak => {
                if in_table {
                    current_cell_spans.push(Span::raw(" "));
                } else {
                    current_spans.push(Span::raw(" "));
                }
            }
            Event::HardBreak => {
                if in_table {
                    current_cell_spans.push(Span::raw(" "));
                } else {
                    flush_line(&mut current_spans, &mut lines);
                }
            }

            // -- Table events --
            Event::Start(Tag::Table(_)) => {
                flush_line(&mut current_spans, &mut lines);
                in_table = true;
                table_header.clear();
                table_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                render_table(&table_header, &table_rows, width, &mut lines);
                in_table = false;
                lines.push(Line::from(""));
            }
            Event::Start(Tag::TableHead) => {
                in_table_header = true;
                current_row.clear();
                // Header cells inherit Cyan+Bold as their base style
                style_stack.push(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            }
            Event::End(TagEnd::TableHead) => {
                // If TableHead acts as the row itself (no nested TableRow),
                // flush collected cells as the header.
                if !current_row.is_empty() && table_header.is_empty() {
                    table_header = std::mem::take(&mut current_row);
                }
                in_table_header = false;
                style_stack.pop();
            }
            Event::Start(Tag::TableRow) => {
                current_row.clear();
            }
            Event::End(TagEnd::TableRow) => {
                if in_table_header {
                    table_header = std::mem::take(&mut current_row);
                } else {
                    table_rows.push(std::mem::take(&mut current_row));
                }
            }
            Event::Start(Tag::TableCell) => {
                current_cell_spans.clear();
            }
            Event::End(TagEnd::TableCell) => {
                current_row.push(std::mem::take(&mut current_cell_spans));
            }

            // Skip unsupported events (footnotes, etc.)
            _ => {}
        }
    }

    // Flush remaining content
    flush_line(&mut current_spans, &mut lines);

    // Trim trailing empty lines
    while lines.last().is_some_and(|l| {
        l.spans.is_empty() || (l.spans.len() == 1 && l.spans[0].content.is_empty())
    }) {
        lines.pop();
    }

    lines
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Move accumulated spans into a new Line and push it to `lines`.
fn flush_line(current_spans: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>) {
    if !current_spans.is_empty() {
        lines.push(Line::from(std::mem::take(current_spans)));
    }
}

/// Get the current style from the top of the stack.
fn current_style(stack: &[Style]) -> Style {
    stack.last().copied().unwrap_or_default()
}

/// Style for heading levels.
fn heading_style(level: HeadingLevel) -> Style {
    match level {
        HeadingLevel::H1 => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        HeadingLevel::H2 => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        HeadingLevel::H3 => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        _ => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    }
}

/// Visual separator line for code blocks.
fn code_block_separator() -> Line<'static> {
    Line::from(Span::styled("───", Style::default().fg(Color::DarkGray)))
}

// ---------------------------------------------------------------------------
// Table rendering
// ---------------------------------------------------------------------------

/// Render a collected table (header + body rows) into styled `Line`s with
/// box-drawing borders.  Column widths are capped so the table fits within
/// `max_width` display columns.
fn render_table(
    header: &[Vec<Span<'static>>],
    rows: &[Vec<Vec<Span<'static>>>],
    max_width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let num_cols = header
        .len()
        .max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if num_cols == 0 {
        return;
    }

    // Determine the natural display width of each column
    let mut col_widths: Vec<usize> = vec![0; num_cols];
    for (i, cell) in header.iter().enumerate() {
        col_widths[i] = col_widths[i].max(cell_display_width(cell));
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                col_widths[i] = col_widths[i].max(cell_display_width(cell));
            }
        }
    }

    // Constrain column widths so the table fits within max_width.
    // Overhead per column: "│ " (2) + " " (1) = 3, plus a trailing "│" (1).
    let overhead = 3 * num_cols + 1;
    let total_content: usize = col_widths.iter().sum();
    if total_content + overhead > max_width {
        if max_width > overhead {
            let available = max_width - overhead;
            if total_content > 0 {
                for w in &mut col_widths {
                    *w = (*w * available / total_content).max(1);
                }
            }
            // max(1) can cause the sum to exceed available; trim the widest
            // columns until the constraint is satisfied.
            let mut sum: usize = col_widths.iter().sum();
            while sum > available {
                if let Some(max_w) = col_widths.iter_mut().max() {
                    if *max_w <= 1 {
                        break;
                    }
                    *max_w -= 1;
                    sum -= 1;
                } else {
                    break;
                }
            }
        } else {
            // Viewport too narrow for any bordered table — give each
            // column 1 char so content is at least partially visible.
            col_widths.fill(1);
        }
    }

    let border = Style::default().fg(Color::DarkGray);

    // ┌───┬───┐
    lines.push(table_border_line(&col_widths, "┌", "┬", "┐", border));

    // Header row
    if !header.is_empty() {
        lines.push(table_data_line(header, &col_widths, border));
        // ├───┼───┤
        lines.push(table_border_line(&col_widths, "├", "┼", "┤", border));
    }

    // Body rows
    for row in rows {
        lines.push(table_data_line(row, &col_widths, border));
    }

    // └───┴───┘
    lines.push(table_border_line(&col_widths, "└", "┴", "┘", border));
}

/// Build a horizontal border line: e.g. `┌───┬───┐`
fn table_border_line(
    col_widths: &[usize],
    left: &str,
    mid: &str,
    right: &str,
    style: Style,
) -> Line<'static> {
    let mut s = left.to_string();
    for (i, &w) in col_widths.iter().enumerate() {
        // +2 accounts for one space of padding on each side of the cell content
        for _ in 0..w + 2 {
            s.push('─');
        }
        if i + 1 < col_widths.len() {
            s.push_str(mid);
        }
    }
    s.push_str(right);
    Line::from(Span::styled(s, style))
}

/// Build a data row: e.g. `│ foo │ bar │`
///
/// Cell spans carry their own styles (applied during markdown parsing), so
/// no extra `text_style` parameter is needed.
fn table_data_line(
    cells: &[Vec<Span<'static>>],
    col_widths: &[usize],
    border_style: Style,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let empty_cell: Vec<Span<'static>> = Vec::new();
    for (i, &w) in col_widths.iter().enumerate() {
        spans.push(Span::styled("│ ", border_style));
        let cell = cells.get(i).unwrap_or(&empty_cell);
        let clipped = clip_cell_spans(cell, w);
        let clipped_w = cell_display_width(&clipped);
        spans.extend(clipped);
        let padding = w.saturating_sub(clipped_w);
        spans.push(Span::raw(" ".repeat(padding + 1)));
    }
    spans.push(Span::styled("│", border_style));
    Line::from(spans)
}

/// Total display width of a cell (sum of all span widths).
fn cell_display_width(cell: &[Span]) -> usize {
    cell.iter().map(|s| unicode_display_width(&s.content)).sum()
}

/// Clip a cell's spans so their combined width does not exceed `max_width`.
fn clip_cell_spans(spans: &[Span<'static>], max_width: usize) -> Vec<Span<'static>> {
    let mut result = Vec::new();
    let mut remaining = max_width;
    for span in spans {
        if remaining == 0 {
            break;
        }
        let w = unicode_display_width(&span.content);
        if w <= remaining {
            result.push(span.clone());
            remaining -= w;
        } else {
            let mut clipped = String::new();
            let mut col = 0;
            for ch in span.content.chars() {
                let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if col + ch_w > remaining {
                    break;
                }
                clipped.push(ch);
                col += ch_w;
            }
            if !clipped.is_empty() {
                result.push(Span::styled(clipped, span.style));
            }
            break;
        }
    }
    result
}

/// Compute the display width of a string, accounting for CJK and other
/// wide characters.
fn unicode_display_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 200; // generous width so wrapping doesn't interfere

    #[test]
    fn test_empty_input() {
        let lines = markdown_to_lines("", W);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_plain_text() {
        let lines = markdown_to_lines("hello world", W);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content.as_ref(), "hello world");
    }

    #[test]
    fn test_heading() {
        let lines = markdown_to_lines("# Title", W);
        assert!(!lines.is_empty());
        let first = &lines[0];
        assert_eq!(first.spans[0].content.as_ref(), "Title");
        assert!(first.spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_bold_text() {
        let lines = markdown_to_lines("**bold**", W);
        assert!(!lines.is_empty());
        let span = &lines[0].spans[0];
        assert_eq!(span.content.as_ref(), "bold");
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_italic_text() {
        let lines = markdown_to_lines("*italic*", W);
        assert!(!lines.is_empty());
        let span = &lines[0].spans[0];
        assert_eq!(span.content.as_ref(), "italic");
        assert!(span.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_nested_bold_italic() {
        let lines = markdown_to_lines("***bold italic***", W);
        assert!(!lines.is_empty());
        let span = &lines[0].spans[0];
        assert_eq!(span.content.as_ref(), "bold italic");
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
        assert!(span.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_inline_code() {
        let lines = markdown_to_lines("`code`", W);
        assert!(!lines.is_empty());
        let span = &lines[0].spans[0];
        assert_eq!(span.content.as_ref(), " code ");
        assert_eq!(span.style.fg, Some(Color::Yellow));
        assert_eq!(span.style.bg, Some(Color::DarkGray));
    }

    #[test]
    fn test_code_block_no_lang() {
        let input = "```\nfn main() {}\n```";
        let lines = markdown_to_lines(input, W);
        // Should have separator + code + separator
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_code_block_unknown_lang() {
        let input = "```xyzunknown\nsome code\n```";
        let lines = markdown_to_lines(input, W);
        // Should not panic, should produce lines
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_bullet_list() {
        let input = "- item1\n- item2";
        let lines = markdown_to_lines(input, W);
        assert!(lines.len() >= 2);
        // First non-empty line should contain a bullet
        let first_content: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first_content.contains("- "));
        assert!(first_content.contains("item1"));
    }

    #[test]
    fn test_truncation() {
        let input = "line1\n\nline2\n\nline3\n\nline4\n\nline5";
        let lines = markdown_to_lines_truncated(input, W, 3);
        assert_eq!(lines.len(), 4); // 3 lines + "..."
        assert_eq!(lines.last().unwrap().spans[0].content.as_ref(), "...");
    }

    #[test]
    fn test_truncation_no_truncation_needed() {
        let input = "short";
        let lines = markdown_to_lines_truncated(input, W, 100);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_wrap_long_line() {
        let long = "a".repeat(160);
        let lines = markdown_to_lines(&long, 80);
        // 160 chars at width 80 => 2 visual rows
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_wrap_preserves_styles() {
        // Bold text wider than viewport should wrap while keeping bold style
        let input = format!("**{}**", "b".repeat(100));
        let lines = markdown_to_lines(&input, 50);
        assert!(lines.len() >= 2);
        for line in &lines {
            for span in &line.spans {
                assert!(span.style.add_modifier.contains(Modifier::BOLD));
            }
        }
    }

    #[test]
    fn test_cjk_in_code_block() {
        let input = "```\nlet x = \"日本語\";\n```";
        let lines = markdown_to_lines(input, W);
        // Should not panic and should produce content
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_horizontal_rule() {
        let input = "above\n\n---\n\nbelow";
        let lines = markdown_to_lines(input, W);
        let has_rule = lines
            .iter()
            .any(|l| l.spans.iter().any(|s| s.content.contains('─')));
        assert!(has_rule);
    }

    #[test]
    fn test_nested_ordered_list() {
        let input = "1. outer1\n   1. inner1\n   2. inner2\n2. outer2";
        let lines = markdown_to_lines(input, W);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        // After the nested list ends, the outer list should continue with "2."
        assert!(
            all_text.contains("2. outer2"),
            "outer numbering broken: {all_text}"
        );
    }

    #[test]
    fn test_link_shows_url() {
        let input = "[click here](https://example.com)";
        let lines = markdown_to_lines(input, W);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(
            all_text.contains("click here"),
            "link text missing: {all_text}"
        );
        assert!(
            all_text.contains("https://example.com"),
            "link URL missing: {all_text}"
        );
    }

    #[test]
    fn test_truncation_zero_max_lines() {
        let input = "some text";
        let lines = markdown_to_lines_truncated(input, W, 0);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_table_rendering() {
        let input = "| H1 | H2 |\n|---|---|\n| a | b |\n| c | d |";
        let lines = markdown_to_lines(input, W);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        // Should contain box-drawing borders
        assert!(all_text.contains('┌'), "missing top border: {all_text}");
        assert!(all_text.contains('┘'), "missing bottom border: {all_text}");
        // Should contain header text
        assert!(all_text.contains("H1"), "missing header: {all_text}");
        assert!(all_text.contains("H2"), "missing header: {all_text}");
        // Should contain cell text
        assert!(all_text.contains('a'), "missing cell: {all_text}");
        assert!(all_text.contains('d'), "missing cell: {all_text}");
        // Rows should be on separate lines (not collapsed into one)
        assert!(
            lines.len() >= 6,
            "table should have at least 6 lines (borders + header + sep + 2 rows), got {}",
            lines.len()
        );
    }

    #[test]
    fn test_table_with_cjk() {
        let input = "| 名前 | 値 |\n|---|---|\n| テスト | 123 |";
        let lines = markdown_to_lines(input, W);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(all_text.contains("名前"), "missing CJK header: {all_text}");
        assert!(all_text.contains("テスト"), "missing CJK cell: {all_text}");
    }

    #[test]
    fn test_table_with_inline_code() {
        let input = "| Col |\n|---|\n| `code` |";
        let lines = markdown_to_lines(input, W);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(
            all_text.contains("code"),
            "missing inline code in table: {all_text}"
        );
    }

    #[test]
    fn test_table_preserves_inline_styles() {
        let input = "| Col |\n|---|\n| **bold** and `code` |";
        let lines = markdown_to_lines(input, W);
        let has_bold = lines.iter().any(|l| {
            l.spans.iter().any(|s| {
                s.content.contains("bold") && s.style.add_modifier.contains(Modifier::BOLD)
            })
        });
        let has_code_style = lines.iter().any(|l| {
            l.spans
                .iter()
                .any(|s| s.content.contains("code") && s.style.fg == Some(Color::Yellow))
        });
        assert!(has_bold, "bold style should be preserved in table cells");
        assert!(
            has_code_style,
            "code style should be preserved in table cells"
        );
    }

    #[test]
    fn test_table_header_style() {
        let input = "| Header |\n|---|\n| body |";
        let lines = markdown_to_lines(input, W);
        let has_cyan_bold = lines.iter().any(|l| {
            l.spans.iter().any(|s| {
                s.content.contains("Header")
                    && s.style.fg == Some(Color::Cyan)
                    && s.style.add_modifier.contains(Modifier::BOLD)
            })
        });
        assert!(has_cyan_bold, "header should be Cyan+Bold");
    }

    #[test]
    fn test_table_narrow_width() {
        let input = "| Header1 | Header2 |\n|---|---|\n| cell1 | cell2 |";
        // Very narrow width — columns must shrink to fit
        let lines = markdown_to_lines(input, 20);
        assert!(
            !lines.is_empty(),
            "should produce lines even at narrow width"
        );
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(
            all_text.contains('┌'),
            "should still have borders: {all_text}"
        );
        // No line should exceed the requested width
        for (i, line) in lines.iter().enumerate() {
            let line_w: usize = line
                .spans
                .iter()
                .map(|s| unicode_display_width(&s.content))
                .sum();
            assert!(
                line_w <= 20,
                "line {i} exceeds width 20: {line_w} cols — {line:?}"
            );
        }
    }

    #[test]
    fn test_table_extremely_narrow_width() {
        // 2 columns: overhead = 3*2+1 = 7.  width=8 → only 1 char available.
        let input = "| AB | CD |\n|---|---|\n| ef | gh |";
        let lines = markdown_to_lines(input, 8);
        assert!(!lines.is_empty(), "should not be empty at width 8");
        // width=4 → narrower than overhead; should still not panic.
        let lines2 = markdown_to_lines(input, 4);
        assert!(!lines2.is_empty(), "should not be empty at width 4");
        // width=1 → extreme edge; must not panic.
        let lines3 = markdown_to_lines(input, 1);
        assert!(!lines3.is_empty(), "should not be empty at width 1");
    }

    #[test]
    fn test_table_many_columns_narrow() {
        // 5 columns: overhead = 3*5+1 = 16.  width=15 → less than overhead.
        let input = "| a | b | c | d | e |\n|---|---|---|---|---|\n| 1 | 2 | 3 | 4 | 5 |";
        let lines = markdown_to_lines(input, 15);
        assert!(
            !lines.is_empty(),
            "many-column table at narrow width should not panic"
        );
    }
}
