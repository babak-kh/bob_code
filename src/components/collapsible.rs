/// Low-level collapsible text component.
///
/// Builds styled [`ratatui::text::Line`] slices for a titled content block that
/// can be toggled between collapsed (header only) and expanded (header + body).
/// Consumers render the lines however they see fit — typically via
/// `Paragraph::new(lines).scroll((skip, 0)).render(rect, buf)`.
///
/// # Visual format
/// ```text
/// Collapsed:  ▶ title  [space to expand]
/// Expanded:   ▼ title
///             <markdown body>
/// ```
/// When `selected` is `true` the header span gets `Modifier::REVERSED`.
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::components::markdown::{parse_markdown, parse_markdown_dimmed};

pub struct CollapsibleText {
    pub title: String,
    pub content: String,
    /// Whether the body is hidden.
    pub collapsed: bool,
    /// Highlighted header (keyboard focus on this block).
    pub selected: bool,
    /// Render body with dimmed (`DarkGray`) markdown colours.
    pub dim: bool,
}

impl CollapsibleText {
    pub fn new(title: String, content: String, collapsed: bool, dim: bool) -> Self {
        Self {
            title,
            content,
            collapsed,
            selected: false,
            dim,
        }
    }

    /// Flip the collapsed/expanded state.
    pub fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
    }

    /// Append streamed text (called while a response is streaming in).
    pub fn append(&mut self, text: &str) {
        self.content.push_str(text);
    }

    /// Build the full set of styled lines for this block, including a trailing
    /// blank separator line.
    pub fn build_lines(&self) -> Vec<Line<'static>> {
        let fg = if self.dim { Color::DarkGray } else { Color::Yellow };
        let mut header_style = Style::default()
            .fg(fg)
            .add_modifier(Modifier::BOLD | Modifier::ITALIC);
        if self.selected {
            header_style = header_style.add_modifier(Modifier::REVERSED);
        }

        let mut lines: Vec<Line<'static>> = Vec::new();

        if self.collapsed {
            let text = format!("▶ {}  [space to expand]", self.title);
            lines.push(Line::from(Span::styled(text, header_style)));
        } else {
            let text = format!("▼ {}", self.title);
            lines.push(Line::from(Span::styled(text, header_style)));
            let body = if self.dim {
                parse_markdown_dimmed(&self.content)
            } else {
                parse_markdown(&self.content)
            };
            lines.extend(body.lines);
        }

        // Blank separator between blocks.
        lines.push(Line::default());
        lines
    }

    /// Estimated height in terminal rows when wrapped to `inner_width` columns.
    pub fn height(&self, inner_width: u16) -> u16 {
        compute_height(&self.build_lines(), inner_width)
    }
}

/// Compute the number of terminal rows occupied by `lines` when word-wrapped
/// to `inner_width` columns (mirrors `Paragraph`'s `Wrap { trim: false }` logic).
pub fn compute_height(lines: &[Line<'_>], inner_width: u16) -> u16 {
    if inner_width == 0 {
        return lines.len() as u16;
    }
    lines
        .iter()
        .map(|line| {
            let w = line.width();
            if w == 0 {
                1u16
            } else {
                ((w + inner_width as usize - 1) / inner_width as usize) as u16
            }
        })
        .sum()
}
