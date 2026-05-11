/// Per-conversation-turn widget.
///
/// Each [`MessageBlock`] represents one logical message in the response area:
/// a user input, an assistant reply, a thinking trace, a tool call/response,
/// or command output.  Collapsible kinds (thinking, tool calls) start collapsed
/// and can be toggled via [`MessageBlock::toggle_collapse`].
///
/// The block does **not** implement [`ratatui::widgets::Widget`] directly because
/// partial rendering (when the block is clipped at the viewport top) requires
/// passing a `skip_lines` offset.  Instead, callers use [`MessageBlock::build_lines`]
/// and render via `Paragraph::new(lines).scroll((skip_lines, 0))`.
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::components::collapsible::{CollapsibleText, compute_height};
use crate::components::markdown::{parse_markdown, parse_markdown_dimmed};
use crate::models::display::MessageKind;

pub struct MessageBlock {
    /// Role / category — kept public so the controller can do same-kind streaming merge.
    pub kind: MessageKind,
    /// Raw text content (always updated even for collapsible blocks so we can
    /// re-render after expand/collapse without re-fetching).
    content: String,
    /// Present only for collapsible kinds (thinking, tool calls).
    collapsible: Option<CollapsibleText>,
    /// Whether this block is the keyboard-focused block.
    selected: bool,
}

impl MessageBlock {
    /// Construct a new block for the given [`MessageKind`] and initial content.
    ///
    /// Collapsible kinds start collapsed.
    pub fn new(kind: MessageKind, content: String) -> Self {
        let collapsible = match &kind {
            MessageKind::AssistantThinking => Some(CollapsibleText::new(
                "thinking".to_string(),
                content.clone(),
                true, // collapsed by default
                true, // dim
            )),
            MessageKind::AssistantToolCall => Some(CollapsibleText::new(
                "tool call / response".to_string(),
                content.clone(),
                true, // collapsed by default
                true, // dim
            )),
            _ => None,
        };
        Self {
            kind,
            content,
            collapsible,
            selected: false,
        }
    }

    /// Append streamed text.  Called during streaming so consecutive same-kind
    /// chunks are merged into one block instead of creating a new entry per token.
    pub fn append(&mut self, text: &str) {
        self.content.push_str(text);
        if let Some(c) = &mut self.collapsible {
            c.append(text);
        }
    }

    /// Set or clear the keyboard-selection highlight on this block's header.
    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
        if let Some(c) = &mut self.collapsible {
            c.selected = selected;
        }
    }

    /// Flip collapsed/expanded.  No-op for non-collapsible kinds.
    pub fn toggle_collapse(&mut self) {
        if let Some(c) = &mut self.collapsible {
            c.toggle();
        }
    }

    /// Returns `true` if this block can be collapsed/expanded.
    pub fn is_collapsible(&self) -> bool {
        self.collapsible.is_some()
    }

    /// Build the full set of styled lines for this block, including the trailing
    /// blank separator.  The caller is responsible for scrolling / clipping.
    pub fn build_lines(&self) -> Vec<Line<'static>> {
        // Collapsible kinds delegate to CollapsibleText.
        if let Some(c) = &self.collapsible {
            return c.build_lines();
        }

        let mut lines: Vec<Line<'static>> = Vec::new();

        match self.kind {
            MessageKind::User => {
                let mut style =
                    Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD);
                if self.selected {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                lines.push(Line::from(Span::styled("user:", style)));
                lines.extend(parse_markdown(&self.content).lines);
            }
            MessageKind::AssistantContent => {
                let mut style =
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD);
                if self.selected {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                lines.push(Line::from(Span::styled("assistant:", style)));
                lines.extend(parse_markdown(&self.content).lines);
            }
            MessageKind::InfoCommandOutput => {
                let mut style = Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD | Modifier::ITALIC);
                if self.selected {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                lines.push(Line::from(Span::styled("info:", style)));
                lines.extend(parse_markdown_dimmed(&self.content).lines);
            }
            // AssistantThinking / AssistantToolCall handled by collapsible branch above.
            _ => {}
        }

        // Blank separator between blocks.
        lines.push(Line::default());
        lines
    }

    /// Estimated height in terminal rows at the given inner width.
    ///
    /// This is the same wrapping logic as `Paragraph` with `Wrap { trim: false }`.
    pub fn height(&self, inner_width: u16) -> u16 {
        compute_height(&self.build_lines(), inner_width)
    }
}
