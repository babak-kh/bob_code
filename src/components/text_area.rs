use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::*,
};

pub fn cursor_like_span<'a>(c: char) -> Span<'a> {
    Span::raw(c.to_string()).style(
        Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(Color::White),
    )
}

pub fn panel_block(is_focused: bool, borders: Borders) -> Block<'static> {
    Block::default()
        .borders(borders)
        .border_style(if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .border_type(BorderType::Plain)
}

/// Kept for callers that need a titled block (e.g. dialogs).
pub fn default_block(name: Option<&str>, is_focused: bool, borders: Borders) -> Block<'_> {
    let b = panel_block(is_focused, borders);
    if let Some(name) = name {
        b.title(Span::styled(name, Style::default().fg(Color::White)))
    } else {
        b
    }
}

/// A pure renderer for multi-line text with a visible cursor.
/// All state is owned by `ContentManager`; this struct is built per frame.
pub struct TextArea<'a> {
    lines: Vec<String>,
    cursor_pos: (usize, usize),
    is_focused: bool,
    block: Option<Block<'a>>,
    /// When `Some(n)`, the first `n` characters of line 0 are rendered as a
    /// command token (Cyan + Bold) instead of the default foreground.
    command_prefix_len: Option<usize>,
    /// Cap visible content lines; scrolls to keep the cursor in view.
    max_visible_lines: Option<usize>,
}

impl<'a> TextArea<'a> {
    pub fn new(lines: &[String], cursor_pos: (usize, usize), is_focused: bool) -> Self {
        TextArea {
            lines: lines.to_vec(),
            cursor_pos,
            is_focused,
            block: None,
            command_prefix_len: None,
            max_visible_lines: None,
        }
    }

    pub fn with_block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn with_command_prefix(mut self, len: usize) -> Self {
        self.command_prefix_len = Some(len);
        self
    }

    pub fn with_max_visible_lines(mut self, max: usize) -> Self {
        self.max_visible_lines = Some(max);
        self
    }

    /// Slice lines to the viewport and adjust cursor row when content exceeds
    /// `max_visible_lines`.
    fn viewport(&self) -> (Vec<String>, (usize, usize), usize) {
        let max = self.max_visible_lines.unwrap_or(self.lines.len());
        if self.lines.len() <= max {
            return (self.lines.clone(), self.cursor_pos, 0);
        }
        let scroll_start = self
            .cursor_pos
            .1
            .saturating_sub(max - 1)
            .min(self.lines.len().saturating_sub(max));
        let visible = self.lines[scroll_start..scroll_start + max].to_vec();
        let cursor = (self.cursor_pos.0, self.cursor_pos.1 - scroll_start);
        (visible, cursor, scroll_start)
    }

    fn style_cursor(
        lines: &[String],
        cursor_pos: (usize, usize),
        command_prefix_len: Option<usize>,
    ) -> Vec<Line<'static>> {
        let cmd_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);

        lines
            .iter()
            .enumerate()
            .map(|(row, line)| {
                let col = if row == cursor_pos.1 {
                    cursor_pos.0.min(line.len())
                } else {
                    // No cursor on this row — just style the command prefix if needed.
                    usize::MAX
                };

                // On line 0 with a command prefix we need fine-grained span splits.
                if row == 0 {
                    if let Some(pfx) = command_prefix_len {
                        return build_line_with_prefix(line, col, pfx, cmd_style);
                    }
                }

                // Plain line — only cursor styling.
                if col == usize::MAX {
                    return Line::raw(line.clone());
                }
                let before = line[..col].to_string();
                let cursor_char = line[col..].chars().next().unwrap_or(' ');
                let after = if col < line.len() {
                    line[col + cursor_char.len_utf8()..].to_string()
                } else {
                    String::new()
                };
                let mut ll = Line::default();
                if !before.is_empty() {
                    ll.push_span(Span::raw(before));
                }
                ll.push_span(cursor_like_span(cursor_char));
                if !after.is_empty() {
                    ll.push_span(Span::raw(after));
                }
                ll
            })
            .collect()
    }
}

impl<'a> Widget for TextArea<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let (display_lines, display_cursor, scroll_start) = self.viewport();
        let command_prefix = if scroll_start == 0 {
            self.command_prefix_len
        } else {
            None
        };

        let inner_area = match self.block {
            Some(block) => {
                let inner = block.inner(area);
                block.render(area, buf);
                inner
            }
            None => area,
        };

        let content = if self.is_focused {
            Self::style_cursor(&display_lines, display_cursor, command_prefix)
        } else {
            // Unfocused: still apply command prefix colouring if present.
            if let Some(pfx) = command_prefix {
                let cmd_style = Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
                let mut styled: Vec<Line<'static>> = display_lines
                    .iter()
                    .map(|l| Line::raw(l.clone()))
                    .collect();
                if !display_lines.is_empty() {
                    styled[0] =
                        build_line_with_prefix(&display_lines[0], usize::MAX, pfx, cmd_style);
                }
                styled
            } else {
                display_lines
                    .iter()
                    .map(|l| Line::raw(l.clone()))
                    .collect()
            }
        };

        Paragraph::new(content)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false })
            .render(inner_area, buf);
    }
}

/// Build a single styled [`Line`] for line 0 that respects both the command
/// prefix colour and the cursor position.
///
/// - Characters `0..pfx` are rendered with `cmd_style` (the command token).
/// - The cursor character at `col` is rendered with `REVERSED`.
/// - `col == usize::MAX` means no cursor on this line (unfocused).
fn build_line_with_prefix(
    line: &str,
    col: usize,       // byte index of cursor; usize::MAX = no cursor
    pfx: usize,       // byte length of the command prefix
    cmd_style: Style,
) -> Line<'static> {
    // Clamp pfx to actual line length.
    let pfx = pfx.min(line.len());
    let cursor_col = col.min(line.len()); // byte index where cursor sits

    // We need to emit spans that cover [0, pfx) with cmd_style and
    // [pfx, end) with default, while inserting a REVERSED cursor span
    // at cursor_col.
    //
    // Strategy: collect "segments" as (byte_start, byte_end, style), then
    // split/insert the cursor into them.

    // Segment boundaries to consider (sorted, deduped):
    let mut cuts: Vec<usize> = vec![0, pfx, line.len()];
    if col != usize::MAX {
        let cursor_end = cursor_col
            + line[cursor_col..]
                .chars()
                .next()
                .map_or(0, |c| c.len_utf8());
        cuts.push(cursor_col);
        cuts.push(cursor_end.min(line.len()));
    }
    cuts.sort_unstable();
    cuts.dedup();

    let mut ll = Line::default();
    for window in cuts.windows(2) {
        let (start, end) = (window[0], window[1]);
        if start >= end {
            continue;
        }
        let text = line[start..end].to_string();
        let is_cursor_char =
            col != usize::MAX && start == cursor_col && start < line.len();
        if is_cursor_char {
            // The cursor overlays whatever style this segment would have.
            let c = line[start..].chars().next().unwrap_or(' ');
            ll.push_span(cursor_like_span(c));
        } else {
            let style = if start < pfx { cmd_style } else { Style::default() };
            ll.push_span(Span::styled(text, style));
        }
    }

    // If the cursor is past the end of the line (col >= line.len()), append a
    // synthetic space with cursor styling.
    if col != usize::MAX && cursor_col >= line.len() {
        ll.push_span(cursor_like_span(' '));
    }

    ll
}
