use std::fmt::Display;

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
use crate::models::tool::{DiffLine, DiffViewData, ToolStructuredOutput};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Apply a background colour to every span in every line.
fn apply_bg(lines: &mut [Line<'static>], bg: Color) {
    if let Some(line) = lines.first_mut() {
        for span in &mut line.spans {
            span.style = span.style.bg(bg);
        }
    }
}

/// Background tint for each [`MessageKind`], used to visually delimit blocks
/// in the response area (same approach as the system-prompt red background).
fn bg_for_kind(kind: &MessageKind) -> Color {
    match kind {
        MessageKind::System => Color::Rgb(100, 35, 35),
        MessageKind::User => Color::Rgb(30, 30, 85),
        MessageKind::AssistantContent => Color::Rgb(35, 60, 35),
        MessageKind::AssistantThinking => Color::Rgb(55, 55, 55),
        MessageKind::AssistantToolCall => Color::Rgb(65, 55, 25),
        MessageKind::InfoCommandOutput => Color::Rgb(35, 55, 55),
        MessageKind::Error => Color::Rgb(90, 25, 25),
    }
}

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
    fn new(kind: MessageKind, label: &'static str, dim_body: bool, content: String) -> Self {
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

        apply_bg(&mut lines, bg_for_kind(&self.kind));
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
    fn new(kind: MessageKind, title: String, content: String, collapsed: bool) -> Self {
        Self {
            kind,
            inner: CollapsibleText::new(
                title, content, collapsed,
                true, /* dim */
            ),
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
        let mut lines = self.inner.build_lines();
        apply_bg(&mut lines, bg_for_kind(&self.kind));
        lines
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
// Tool block — collapsible with optional diff view
// ---------------------------------------------------------------------------

struct ToolBlock {
    inner: CollapsibleText,
    /// When `Some`, a diff view is rendered between the header and the raw text.
    diff: Option<DiffViewData>,
}

impl ToolBlock {
    fn new(tool_name: String, content: String, diff: Option<DiffViewData>) -> Self {
        Self {
            inner: CollapsibleText::new(
                tool_name, content,
                true, /* collapsed */
                true, /* dim */
            ),
            diff,
        }
    }

    fn build_diff_lines(&self) -> Vec<Line<'static>> {
        let header_style = Style::default().fg(Color::DarkGray);
        let added_style = Style::default().fg(Color::Green);
        let removed_style = Style::default().fg(Color::Red);
        let context_style = Style::default().fg(Color::DarkGray);

        let mut lines: Vec<Line<'static>> = Vec::new();

        if let Some(diff) = &self.diff {
            // Diff header line
            lines.push(Line::from(Span::styled(
                format!("── diff ── {}", diff.file_path),
                header_style.add_modifier(Modifier::BOLD),
            )));

            for hunk in &diff.hunks {
                // Hunk header
                lines.push(Line::from(Span::styled(
                    format!(
                        "@@ -{},{} +{},{} @@",
                        hunk.old_start,
                        hunk
                            .lines
                            .iter()
                            .filter(|l| !matches!(l, DiffLine::Added(_)))
                            .count(),
                        hunk.new_start,
                        hunk
                            .lines
                            .iter()
                            .filter(|l| !matches!(l, DiffLine::Removed(_)))
                            .count(),
                    ),
                    header_style,
                )));

                for line in &hunk.lines {
                    match line {
                        DiffLine::Context(text) => {
                            lines.push(Line::from(Span::styled(
                                format!(" {}", text),
                                context_style,
                            )));
                        }
                        DiffLine::Added(text) => {
                            lines.push(Line::from(Span::styled(
                                format!("+{}", text),
                                added_style,
                            )));
                        }
                        DiffLine::Removed(text) => {
                            lines.push(Line::from(Span::styled(
                                format!("-{}", text),
                                removed_style,
                            )));
                        }
                    }
                }
            }

            // Separator after diff
            lines.push(Line::default());
        }

        lines
    }
}

impl ResponseBlock for ToolBlock {
    fn block_kind(&self) -> MessageKind {
        MessageKind::AssistantToolCall
    }
    fn text(&self) -> &str {
        &self.inner.content
    }
    fn append_text(&mut self, text: &str) {
        self.inner.append(text);
    }

    fn build_lines(&self) -> Vec<Line<'static>> {
        if self.inner.collapsed {
            let mut lines = self.inner.build_lines();
            apply_bg(&mut lines, bg_for_kind(&MessageKind::AssistantToolCall));
            return lines;
        }

        let mut all: Vec<Line<'static>> = Vec::new();

        // Header line
        let header_lines = self.inner.build_lines();
        if let Some(header) = header_lines.first() {
            all.push(header.clone());
        }

        // Diff view
        all.extend(self.build_diff_lines());

        // Add remaining body lines from header_lines (skip header) including the trailing blank
        for line in header_lines.iter().skip(1) {
            all.push(line.clone());
        }

        apply_bg(&mut all, bg_for_kind(&MessageKind::AssistantToolCall));
        all
    }
    fn height(&self, inner_width: u16) -> u16 {
        compute_height(&self.build_lines(), inner_width)
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
// System prompt block — collapsible with light-red background
// ---------------------------------------------------------------------------

struct SystemBlock {
    inner: CollapsibleText,
}

impl SystemBlock {
    fn new(content: String) -> Self {
        Self {
            inner: CollapsibleText::new(
                "system prompt".to_string(),
                content,
                true, /* collapsed */
                false,
            ),
        }
    }
}

impl ResponseBlock for SystemBlock {
    fn block_kind(&self) -> MessageKind {
        MessageKind::System
    }
    fn text(&self) -> &str {
        &self.inner.content
    }
    fn append_text(&mut self, text: &str) {
        self.inner.append(text);
    }

    fn build_lines(&self) -> Vec<Line<'static>> {
        let mut lines = self.inner.build_lines();
        apply_bg(&mut lines, bg_for_kind(&MessageKind::System));
        lines
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

pub fn system_block(content: String) -> Box<dyn ResponseBlock> {
    Box::new(SystemBlock::new(content))
}

pub fn user_block(content: String) -> Box<dyn ResponseBlock> {
    Box::new(TextBlock::new(MessageKind::User, "user:", false, content))
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
        false, /* expanded */
    ))
}

pub fn tool_block(tool_name: String, content: String, structured: Option<ToolStructuredOutput>) -> Box<dyn ResponseBlock> {
    let diff = structured.map(|s| match s {
        ToolStructuredOutput::DiffView(d) => d,
    });
    Box::new(ToolBlock::new(tool_name, content, diff))
}

pub fn command_block(content: String) -> Box<dyn ResponseBlock> {
    Box::new(TextBlock::new(
        MessageKind::InfoCommandOutput,
        "info:",
        true,
        content,
    ))
}

pub fn error_block(content: impl Display) -> Box<dyn ResponseBlock> {
    Box::new(TextBlock {
        kind: MessageKind::Error,
        label: "error:",
        header_style: Style::default().fg(Color::Red),
        dim_body: false,
        content: content.to_string(),
        selected: false,
    })
}
