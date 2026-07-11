use crate::components::response_block::ResponseBlock;
use crate::components::text_area::{TextArea, panel_block};
use crate::prompt::{ContentManager, PromptEvent};
use crate::service::clipboard;
use copypasta::ClipboardContext;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Borders, Paragraph, Wrap};
use ratatui::{Frame, layout::Rect};

// ---------------------------------------------------------------------------
// PromptController
// ---------------------------------------------------------------------------

/// Minimum content lines shown in the prompt (always visible, even when empty).
const PROMPT_MIN_LINES: usize = 2;
/// Maximum content lines shown in the prompt before internal scroll kicks in.
const PROMPT_MAX_LINES: usize = 5;
/// Top + bottom border rows.
const PROMPT_BORDER_ROWS: u16 = 2;

pub struct PromptController {
    content: ContentManager,
    is_focused: bool,
    clipboard: Option<ClipboardContext>,
}

impl PromptController {
    pub fn new() -> Self {
        Self {
            content: ContentManager::new(),
            is_focused: true,
            clipboard: None,
        }
    }

    pub fn set_focus(&mut self, focused: bool) {
        self.is_focused = focused;
    }

    /// Terminal rows needed to render the prompt (borders + up to 5 content lines).
    pub fn desired_height(&self) -> u16 {
        let content = self
            .content
            .lines()
            .len()
            .clamp(PROMPT_MIN_LINES, PROMPT_MAX_LINES) as u16;
        content + PROMPT_BORDER_ROWS
    }

    /// Handle a crossterm event. Returns `Some(PromptEvent)` when the user submits.
    pub fn handle_event(&mut self, event: &Event) -> Option<PromptEvent> {
        match event {
            Event::Paste(text) => {
                self.content.insert_text(text);
                None
            }
            Event::Key(KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            }) => return Some(PromptEvent::Cancel),
            Event::Key(key) => self.handle_key(key),
            _ => None,
        }
    }

    fn handle_key(&mut self, key: &KeyEvent) -> Option<PromptEvent> {
        // Ignore release events — only process Press and Repeat.
        if key.kind == KeyEventKind::Release {
            return None;
        }
        match (key.code, key.modifiers) {
            // Submit: Enter; Shift+Enter inserts a newline
            (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => self.content.new_line(),
            (KeyCode::Enter, _) => return self.content.submit(),
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => return self.content.submit(),
            // Paste from system clipboard
            (KeyCode::Char('v'), m)
                if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) =>
            {
                if let Some(text) = clipboard::read(&mut self.clipboard) {
                    self.content.insert_text(&text);
                }
            }
            // Backspace
            (KeyCode::Backspace, _) => self.content.pop(),
            // Clear buffer
            (KeyCode::Esc, _) => self.content.clear(),
            // Cursor / history navigation
            (KeyCode::Left, _) => self.content.cursor_pre(),
            (KeyCode::Right, _) => self.content.cursor_next(),
            (KeyCode::Up, _) => self.content.key_up(),
            (KeyCode::Down, _) => self.content.key_down(),
            // Regular character input
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => self.content.push(c),
            _ => {}
        }

        None
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let block = panel_block(self.is_focused, Borders::TOP | Borders::BOTTOM);
        let token_len = self.content.command_token_len();
        let mut text_area = TextArea::new(
            self.content.lines(),
            self.content.cursor_pos(),
            self.is_focused,
        )
        .with_block(block)
        .with_max_visible_lines(PROMPT_MAX_LINES);
        if token_len > 0 {
            text_area = text_area.with_command_prefix(token_len);
        }
        f.render_widget(text_area, area);
    }
}

// ---------------------------------------------------------------------------
// StatusLineController
// ---------------------------------------------------------------------------

pub struct StatusLineController {
    model_name: String,
    gpu_info: String,
    thread_id: String,
    usage_info: Option<crate::models::model::UsageInfo>,
}

impl StatusLineController {
    pub fn new() -> Self {
        Self {
            model_name: String::new(),
            thread_id: String::new(),
            gpu_info: "GPU: waiting…".to_string(),
            usage_info: None,
        }
    }

    pub fn set_thread_id(&mut self, t_id: String) {
        self.thread_id = t_id;
    }

    pub fn set_model_name(&mut self, name: String) {
        self.model_name = name;
    }

    pub fn set_gpu_info(&mut self, raw: String) {
        self.gpu_info = parse_nvidia_smi(&raw);
    }

    pub fn set_usage_info(&mut self, usage: crate::models::model::UsageInfo) {
        self.usage_info = Some(usage);
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let mut spans: Vec<Span> = Vec::new();

        spans.push(Span::styled(
            " F1 help ",
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));

        if !self.model_name.is_empty() {
            spans.push(Span::styled(
                format!(" {} ", self.model_name),
                Style::default().fg(Color::Cyan),
            ));
            spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
        }

        spans.push(Span::styled(" ⬡ ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            self.gpu_info.clone(),
            Style::default().fg(Color::DarkGray),
        ));

        // Token / cost context info — only show if the backend reported it.
        if let Some(ref u) = self.usage_info {
            spans.push(Span::styled("  │", Style::default().fg(Color::DarkGray)));

            let mut parts: Vec<String> = Vec::new();
            if let Some(p) = u.prompt_tokens {
                parts.push(format!("↑{}", format_tokens(p)));
            }
            if let Some(c) = u.completion_tokens {
                parts.push(format!("↓{}", format_tokens(c)));
            }
            if let Some(t) = u.total_tokens
                && (u.prompt_tokens.is_none() || u.completion_tokens.is_none())
            {
                parts.push(format!("Σ{}", format_tokens(t)));
            }

            if let Some(cost) = u.cost {
                parts.push(format!("${:.4}", cost));
            }
            if !parts.is_empty() {
                spans.push(Span::styled(
                    format!(" {} ", parts.join("  ")),
                    Style::default().fg(Color::Green),
                ));
            }
        }

        spans.push(Span::styled(
            format!(" | id: {}", self.thread_id),
            Style::default().fg(Color::DarkGray),
        ));

        let line = ratatui::text::Line::from(spans);
        f.render_widget(Paragraph::new(line), area);
    }
}

/// Parse nvidia-smi CSV output (two lines: header + data) into a compact
/// single-line string.  Falls back to the raw trimmed string on any error.
fn parse_nvidia_smi(raw: &str) -> String {
    let mut lines = raw.lines();
    lines.next(); // skip header
    if let Some(data) = lines.next() {
        let parts: Vec<&str> = data.split(',').collect();
        if parts.len() >= 3 {
            let name = parts[0].trim();
            let util = parts[1].trim();
            let mem = parts[2].trim();
            return format!("{name}  │  GPU: {util}  │  Mem: {mem}");
        }
        return data.trim().to_string();
    }
    raw.trim().to_string()
}

/// Format a token count with human-friendly suffixes (K, M).
fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

// ---------------------------------------------------------------------------
// ResponseAreaController
// ---------------------------------------------------------------------------

/// Manages the scrollable list of conversation message blocks.
///
/// Each block implements [`ResponseBlock`] so that collapsible blocks
/// (thinking traces, tool calls) can change height without affecting the
/// rest of the layout.
///
/// # Scroll model
/// `scroll_offset` is a **line** count (same as the old `Paragraph::scroll`).
/// The render loop walks the block list, accumulates virtual line positions, and
/// renders only the blocks that overlap `[scroll_offset, scroll_offset + height)`.
/// A block partially scrolled off the top is rendered via `Paragraph::scroll` so
/// only its visible portion appears on screen.
///
/// # Key bindings (when focused)
/// | Key            | Action                                        |
/// |----------------|-----------------------------------------------|
/// | `j` / `↓`     | Scroll one line down                          |
/// | `k` / `↑`     | Scroll one line up                            |
/// | `Ctrl+D`       | Half-page down                                |
/// | `Ctrl+U`       | Half-page up                                  |
/// | `Ctrl+F`       | Full-page down                                |
/// | `Ctrl+B`       | Full-page up                                  |
/// | `g`            | Jump to top                                   |
/// | `G`            | Jump to bottom (re-enables auto-scroll)       |
/// | `[`            | Move block selection to previous block        |
/// | `]`            | Move block selection to next block            |
/// | `Space`/`Enter`| Toggle collapse on the selected block         |
pub struct ResponseAreaController {
    /// All conversation blocks in chronological order.
    blocks: Vec<Box<dyn ResponseBlock>>,
    /// Line-level scroll offset (0 = top of all content).
    scroll_offset: u16,
    /// Index of the currently keyboard-selected block.
    selected_block: usize,
    is_focused: bool,
    /// Inner height remembered from the last render (used by scroll helpers).
    last_area_height: u16,
    /// Inner width remembered from the last render (used for height estimation).
    last_inner_width: u16,
    /// Sum of all block heights at `last_inner_width` (updated each render).
    total_content_lines: u16,
    /// When `true`, every new chunk auto-scrolls to the bottom.
    auto_scroll: bool,
}

impl ResponseAreaController {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            scroll_offset: 0,
            selected_block: 0,
            is_focused: false,
            last_area_height: 20,
            last_inner_width: 80,
            total_content_lines: 0,
            auto_scroll: true,
        }
    }

    pub fn set_focus(&mut self, focused: bool) {
        self.is_focused = focused;
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Append a new block.
    ///
    /// Consecutive blocks with the **same** [`MessageKind`] are merged into
    /// the existing last block (streaming merge) so we don't create one entry
    /// per token.  A different kind always starts a new block.
    pub fn add_block(&mut self, mut block: Box<dyn ResponseBlock>) {
        // Streaming merge: same kind as the last block → append in place.
        let kind = block.block_kind();
        if let Some(last) = self.blocks.last_mut()
            && last.block_kind() == kind
        {
            let text = block.text().to_string();
            last.append_text(&text);
            return;
        }

        // New block.
        let new_idx = self.blocks.len();
        if new_idx == self.selected_block {
            block.set_selected(true);
        }
        self.blocks.push(block);
    }

    /// Handle a key event when the response area is focused.
    /// Returns `true` if the key was consumed.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> bool {
        let half = (self.last_area_height / 2).max(1);
        match (key.code, key.modifiers) {
            // ── Scroll ──────────────────────────────────────────────────────
            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
                self.auto_scroll = false;
                true
            }
            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                self.auto_scroll = false;
                true
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                self.scroll_offset = self.scroll_offset.saturating_add(half);
                self.auto_scroll = false;
                true
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(half);
                self.auto_scroll = false;
                true
            }
            (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                self.scroll_offset = self.scroll_offset.saturating_add(self.last_area_height);
                self.auto_scroll = false;
                true
            }
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(self.last_area_height);
                self.auto_scroll = false;
                true
            }
            (KeyCode::Char('g'), _) => {
                self.scroll_offset = 0;
                self.auto_scroll = false;
                true
            }
            (KeyCode::Char('G'), _) => {
                self.auto_scroll = true;
                true
            }

            // ── Block selection ─────────────────────────────────────────────
            // `[` — move selection to the previous block.
            (KeyCode::Char('['), _) => {
                if self.selected_block > 0 {
                    self.blocks[self.selected_block].set_selected(false);
                    self.selected_block -= 1;
                    self.blocks[self.selected_block].set_selected(true);
                    self.scroll_to_show_selected();
                }
                true
            }
            // `]` — move selection to the next block.
            (KeyCode::Char(']'), _) => {
                if self.selected_block + 1 < self.blocks.len() {
                    self.blocks[self.selected_block].set_selected(false);
                    self.selected_block += 1;
                    self.blocks[self.selected_block].set_selected(true);
                    self.scroll_to_show_selected();
                }
                true
            }
            // `Space` or `Enter` — toggle collapse on the selected block
            // (only if the block is collapsible).
            (KeyCode::Char(' '), _) | (KeyCode::Enter, _) => {
                if let Some(block) = self.blocks.get_mut(self.selected_block) {
                    block.toggle_collapse();
                }
                true
            }

            _ => false,
        }
    }

    // ── Scroll helpers ───────────────────────────────────────────────────────

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self
            .total_content_lines
            .saturating_sub(self.last_area_height);
    }

    /// Scroll just enough to keep the selected block fully within the viewport.
    fn scroll_to_show_selected(&mut self) {
        if self.blocks.is_empty() {
            return;
        }
        let w = self.last_inner_width;
        let virtual_top: u16 = self.blocks[..self.selected_block]
            .iter()
            .map(|b| b.height(w))
            .sum();
        let block_h = self.blocks[self.selected_block].height(w);

        if virtual_top < self.scroll_offset {
            // Block is above the viewport — scroll up to its top.
            self.scroll_offset = virtual_top;
        } else if virtual_top + block_h > self.scroll_offset + self.last_area_height {
            // Block is below the viewport — scroll down so its last line is visible.
            self.scroll_offset = virtual_top + block_h - self.last_area_height;
        }
        self.auto_scroll = false;
    }

    // ── Render ───────────────────────────────────────────────────────────────

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let inner = area;

        self.last_area_height = inner.height.max(1);
        self.last_inner_width = inner.width;

        // Recompute total height (block heights change when collapse is toggled).
        let w = self.last_inner_width;
        self.total_content_lines = self.blocks.iter().map(|b| b.height(w)).sum();

        // Auto-scroll: always track the bottom while enabled.
        if self.auto_scroll {
            self.scroll_to_bottom();
        }

        // Clamp scroll so it never goes past the last line.
        let max_scroll = self
            .total_content_lines
            .saturating_sub(self.last_area_height);
        self.scroll_offset = self.scroll_offset.min(max_scroll);

        // ── Per-block render loop ────────────────────────────────────────────
        // Walk blocks in order, accumulating a virtual y position (in lines).
        // For each block that intersects the viewport [scroll_offset, scroll_offset + height):
        //   • Compute how many lines of the block are above the viewport (skip_lines).
        //   • Compute the screen rect for the visible portion.
        //   • Render via Paragraph::scroll((skip_lines, 0)) into that rect.

        let viewport_top = self.scroll_offset;
        let viewport_bot = self.scroll_offset + self.last_area_height;
        let mut virtual_y: u16 = 0;

        for block in &self.blocks {
            let block_h = block.height(w);
            let block_top = virtual_y;
            let block_bot = virtual_y + block_h;

            // Entirely above the viewport — skip.
            if block_bot <= viewport_top {
                virtual_y = block_bot;
                continue;
            }
            // Entirely below the viewport — done.
            if block_top >= viewport_bot {
                break;
            }

            // Lines of this block that are above the viewport top (to skip).
            let skip_lines = viewport_top.saturating_sub(block_top);
            // Where on screen the visible portion of this block starts.
            let screen_y = inner.y + block_top.saturating_sub(viewport_top);
            // How many lines of this block to display.
            let show_h =
                (block_h - skip_lines).min(inner.height.saturating_sub(screen_y - inner.y));

            if show_h == 0 {
                virtual_y = block_bot;
                continue;
            }

            let rect = Rect {
                x: inner.x,
                y: screen_y,
                width: inner.width,
                height: show_h,
            };

            let lines = block.build_lines();
            let para = Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((skip_lines, 0));
            f.render_widget(para, rect);

            virtual_y = block_bot;
        }
    }
}
