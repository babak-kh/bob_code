use crate::components::message_block::MessageBlock;
use crate::components::text_area::{TextArea, default_block};
use crate::models::display::ResponseAreaInput;
use crate::prompt::{ContentManager, PromptEvent};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Borders, Paragraph, Wrap};
use ratatui::{Frame, layout::Rect};


// ---------------------------------------------------------------------------
// PromptController
// ---------------------------------------------------------------------------

pub struct PromptController {
    content: ContentManager,
    is_focused: bool,
}

impl PromptController {
    pub fn new() -> Self {
        Self {
            content: ContentManager::new(),
            is_focused: true,
        }
    }

    pub fn set_focus(&mut self, focused: bool) {
        self.is_focused = focused;
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Handle a crossterm event. Returns `Some(PromptEvent)` when the user submits.
    pub fn handle_event(&mut self, event: &Event) -> Option<PromptEvent> {
        let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event
        else {
            return None;
        };

        match (code, *modifiers) {
            // Submit: Ctrl+P
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                return self.content.submit();
            }
            // New line: plain Enter
            (KeyCode::Enter, _) => self.content.new_line(),
            // Backspace
            (KeyCode::Backspace, _) => self.content.pop(),
            // Clear: Ctrl+C or Esc
            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => {
                self.content.clear()
            }
            // Cursor / history navigation
            (KeyCode::Left, _) => self.content.cursor_pre(),
            (KeyCode::Right, _) => self.content.cursor_next(),
            (KeyCode::Up, _) => self.content.key_up(),
            (KeyCode::Down, _) => self.content.key_down(),
            // Regular character input
            (KeyCode::Char(c), _) => self.content.push(*c),
            _ => {}
        }

        None
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let block = default_block(Some("Prompt"), self.is_focused, Borders::ALL);
        let token_len = self.content.command_token_len();
        let text_area = TextArea::new(
            self.content.lines(),
            self.content.cursor_pos(),
            self.is_focused,
        )
        .with_block(block);
        let text_area = if token_len > 0 {
            text_area.with_command_prefix(token_len)
        } else {
            text_area
        };
        f.render_widget(text_area, area);
    }
}

// ---------------------------------------------------------------------------
// StatusLineController
// ---------------------------------------------------------------------------

pub struct StatusLineController {
    gpu_info: String,
}

impl StatusLineController {
    pub fn new() -> Self {
        Self {
            gpu_info: "GPU: waiting…".to_string(),
        }
    }

    pub fn set_gpu_info(&mut self, raw: String) {
        self.gpu_info = parse_nvidia_smi(&raw);
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let line = ratatui::text::Line::from(vec![
            Span::styled(" ⬡ ", Style::default().fg(Color::Green)),
            Span::styled(self.gpu_info.clone(), Style::default().fg(Color::DarkGray)),
        ]);
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

// ---------------------------------------------------------------------------
// ResponseAreaController
// ---------------------------------------------------------------------------

/// Manages the scrollable list of conversation message blocks.
///
/// Each [`MessageBlock`] is a separate, independently rendered widget so that
/// collapsible blocks (thinking traces, tool calls) can change height without
/// affecting the rest of the layout.
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
    blocks: Vec<MessageBlock>,
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

    /// Append a new input chunk.
    ///
    /// Consecutive chunks with the **same** [`MessageKind`] are merged into the
    /// existing last block (streaming merge) so we don't create one entry per token.
    /// A different kind always starts a new block.
    pub async fn add_to_payload(&mut self, input: ResponseAreaInput) {
        // Streaming merge: same kind as the last block → append in place.
        if let Some(last) = self.blocks.last_mut() {
            if last.kind == input.kind {
                last.append(&input.content);
                return;
            }
        }

        // New block.
        let new_idx = self.blocks.len();
        let mut block = MessageBlock::new(input.kind, input.content);
        // Highlight if this block lands on the currently selected index.
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
                self.scroll_offset =
                    self.scroll_offset.saturating_add(self.last_area_height);
                self.auto_scroll = false;
                true
            }
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => {
                self.scroll_offset =
                    self.scroll_offset.saturating_sub(self.last_area_height);
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
            // `Space` or `Enter` — toggle collapse on the selected block.
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
        // Draw the outer border and get the usable inner rect.
        let outer_block = default_block(
            Some("Response"),
            self.is_focused,
            Borders::TOP | Borders::LEFT | Borders::RIGHT,
        );
        let inner = outer_block.inner(area);
        f.render_widget(outer_block, area);

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
            let show_h = (block_h - skip_lines)
                .min(inner.height.saturating_sub(screen_y - inner.y));

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
