/// Trait and concrete block types for response area content.
///
/// Each [`ResponseBlock`] controls its own rendering, collapse state, and
/// keyboard-selection styling. The [`crate::ui::ResponseAreaController`]
/// treats all blocks uniformly through this trait — it never inspects
/// concrete types.
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::collapsible::{CollapsibleText, compute_height};
use super::markdown::{parse_markdown, parse_markdown_dimmed};
use crate::models::display::MessageKind;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait ResponseBlock: Send + Sync {
    /// Discriminant for streaming merge: consecutive blocks with the same
    /// kind are merged instead of creating a new entry per token.
    fn block_kind(&self) -> MessageKind;

    /// The current raw text (used during streaming merge to extract new
    /// token text and append it to the existing block).
    fn text(&self) -> &str;

    /// Append streamed token text.
    fn append_text(&mut self, text: &str);

    /// Build the full set of styled lines (including trailing separator).
    fn build_lines(&self) -> Vec<Line<'static>>;

    /// Estimated height in terminal rows when wrapped to `inner_width` cols.
    fn height(&self, inner_width: u16) -> u16;

    /// Mark / un-mark this block as the keyboard-selected block.
    fn set_selected(&mut self, selected: bool);

    /// Toggle collapsed / expanded.  No-op for non-collapsible blocks.
    fn toggle_collapse(&mut self);

    /// Whether [`toggle_collapse`](Self::toggle_collapse) has any effect.
    fn is_collapsible(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Helper: plain text block with a styled header
// ---------------------------------------------------------------------------

/// A plain text block with a styled "role" header (e.g. `user:`, `assistant:`,
/// `info:`) followed by markdown-parsed body lines.
struct TextBlock {
    kind: MessageKind,
    label: &'static str,
    header_style: Style,
    dim_body: bool,
    content: String,
    selected: bool,
}

impl TextBlock {
    fn new(
        kind: MessageKind,
        label: &'static str,
        dim_body: bool,
        content: String,
    ) -> Self {
        Self {
            kind,
            label,
            header_style: Style::default().fg(Color::DarkGray),
            dim_body,
            content,
            selected: false,
        }
    }
}

impl ResponseBlock for TextBlock {
    fn block_kind(&self) -> MessageKind {
        self.kind.clone()
    }
    fn text(&self) -> &str {
        &self.content
    }
    fn append_text(&mut self, text: &str) {
        self.content.push_str(text);
    }

    fn build_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();

        let mut style = self.header_style;
        if self.selected {
            style = style.add_modifier(Modifier::REVERSED);
        }
        lines.push(Line::from(Span::styled(self.label, style)));

        let body = if self.dim_body {
            parse_markdown_dimmed(&self.content)
        } else {
            parse_markdown(&self.content)
        };
        lines.extend(body.lines);
        lines.push(Line::default());
        lines
    }

    fn height(&self, inner_width: u16) -> u16 {
        compute_height(&self.build_lines(), inner_width)
    }

    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
    fn toggle_collapse(&mut self) {}
}

// ---------------------------------------------------------------------------
// Helper: collapsible block backed by CollapsibleText
// ---------------------------------------------------------------------------

struct CollapsibleBlock {
    kind: MessageKind,
    inner: CollapsibleText,
}

impl CollapsibleBlock {
    fn new(kind: MessageKind, title: String, content: String) -> Self {
        Self {
            kind,
            inner: CollapsibleText::new(title, content, true /* collapsed */, true /* dim */),
        }
    }
}

impl ResponseBlock for CollapsibleBlock {
    fn block_kind(&self) -> MessageKind {
        self.kind.clone()
    }
    fn text(&self) -> &str {
        &self.inner.content
    }
    fn append_text(&mut self, text: &str) {
        self.inner.append(text);
    }

    fn build_lines(&self) -> Vec<Line<'static>> {
        self.inner.build_lines()
    }
    fn height(&self, inner_width: u16) -> u16 {
        self.inner.height(inner_width)
    }

    fn set_selected(&mut self, selected: bool) {
        self.inner.selected = selected;
    }
    fn toggle_collapse(&mut self) {
        self.inner.toggle();
    }
    fn is_collapsible(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Public constructors — one per MessageKind
// ---------------------------------------------------------------------------

pub fn user_block(content: String) -> Box<dyn ResponseBlock> {
    Box::new(TextBlock::new(
        MessageKind::User,
        "user:",
        false,
        content,
    ))
}

pub fn assistant_block(content: String) -> Box<dyn ResponseBlock> {
    Box::new(TextBlock::new(
        MessageKind::AssistantContent,
        "assistant:",
        false,
        content,
    ))
}

pub fn thinking_block(content: String) -> Box<dyn ResponseBlock> {
    Box::new(CollapsibleBlock::new(
        MessageKind::AssistantThinking,
        "thinking".to_string(),
        content,
    ))
}

pub fn tool_block(content: String) -> Box<dyn ResponseBlock> {
    Box::new(CollapsibleBlock::new(
        MessageKind::AssistantToolCall,
        "tool call / response".to_string(),
        content,
    ))
}

pub fn command_block(content: String) -> Box<dyn ResponseBlock> {
    Box::new(TextBlock::new(
        MessageKind::InfoCommandOutput,
        "info:",
        true,
        content,
    ))
}

pub fn error_block(content: String) -> Box<dyn ResponseBlock> {
    Box::new(TextBlock {
        kind: MessageKind::Error,
        label: "error:",
        header_style: Style::default().fg(Color::Red),
        dim_body: false,
        content,
        selected: false,
    })
}
