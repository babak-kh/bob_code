use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

// ── Keybinding data ────────────────────────────────────────────────────────────

/// Returns a formatted list of all keybindings as lines of text suitable for
/// display in the notification window.
pub fn keybinds_content() -> Vec<String> {
    vec![
        // Section: Global
        "── Global ──".to_string(),
        "  F1                Show / hide this help window".to_string(),
        "  Tab               Toggle focus between Prompt / Response".to_string(),
        "  Ctrl+D            Quit".to_string(),
        String::new(),
        // Section: Prompt
        "── Prompt (when focused) ──".to_string(),
        "  Enter             Submit prompt".to_string(),
        "  Shift+Enter       New line in prompt".to_string(),
        "  Ctrl+P            Submit prompt (alternate)".to_string(),
        "  Ctrl+Shift+V      Paste from system clipboard".to_string(),
        "  Shift+Insert      Paste via bracketed paste".to_string(),
        "  Esc               Clear prompt buffer".to_string(),
        "  ↑ / ↓             History navigation".to_string(),
        "  ← / →             Cursor movement".to_string(),
        String::new(),
        // Section: Response
        "── Response area (when focused) ──".to_string(),
        "  j / ↓             Scroll one line down".to_string(),
        "  k / ↑             Scroll one line up".to_string(),
        "  Ctrl+D            Half-page down".to_string(),
        "  Ctrl+U            Half-page up".to_string(),
        "  Ctrl+F            Full-page down".to_string(),
        "  Ctrl+B            Full-page up".to_string(),
        "  g                 Jump to top".to_string(),
        "  G                 Jump to bottom (re-enables auto-scroll)".to_string(),
        "  [                 Select previous block".to_string(),
        "  ]                 Select next block".to_string(),
        "  Space / Enter     Toggle collapse on selected block".to_string(),
        String::new(),
        // Section: Slash Commands
        "── Slash Commands (in prompt) ──".to_string(),
        "  /models           Open model selection dialog".to_string(),
        "  /tree             Show conversation tree".to_string(),
        "  /new              Start a new conversation thread".to_string(),
    ]
}

// ── Controller ─────────────────────────────────────────────────────────────────

/// A read-only floating notification overlay for displaying information like
/// keybindings. Not an input dialog — dismisses on Esc or the trigger key.
pub struct NotificationController {
    title: String,
    content: Vec<String>,
    scroll_offset: u16,
}

impl NotificationController {
    pub fn new(title: impl Into<String>, content: Vec<String>) -> Self {
        Self {
            title: title.into(),
            content,
            scroll_offset: 0,
        }
    }

    // ── Event handling ─────────────────────────────────────────────────────────

    /// Process a key event. Returns `Some(())` when the user dismisses.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<()> {
        match key.code {
            // Dismiss
            KeyCode::Esc | KeyCode::F(1) => return Some(()),

            // Scroll
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
            }
            KeyCode::Char('g') => {
                self.scroll_offset = 0;
            }
            KeyCode::Char('G') => {
                self.scroll_offset = u16::MAX; // clamped at render time
            }
            // Any other key also dismisses
            _ => return Some(()),
        }
        None
    }

    // ── Rendering ──────────────────────────────────────────────────────────────

    /// Render the notification as a centered floating panel.
    ///
    /// Call this **after** rendering all other widgets so the `Clear` erases the
    /// correct background content.
    pub fn render(&self, f: &mut Frame) {
        let area = f.area();
        let dialog_w = ((area.width * 7) / 10).max(46).min(area.width - 4);
        let dialog_h = ((area.height * 7) / 10).max(10).min(area.height - 2);
        let x = (area.width.saturating_sub(dialog_w)) / 2;
        let y = (area.height.saturating_sub(dialog_h)) / 2;
        let dialog_area = Rect::new(x, y, dialog_w, dialog_h);

        // Erase background
        f.render_widget(Clear, dialog_area);

        // Outer border
        let block = Block::default()
            .title(Span::styled(
                format!(" {} ", self.title),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));
        f.render_widget(block, dialog_area);

        // Inner area (inside the border)
        let inner = dialog_area.inner(Margin {
            horizontal: 1,
            vertical: 1,
        });

        // Split inner area: content + hint line at the bottom
        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);

        // Total content lines including section spacing
        let total_lines = self.content.len() as u16;
        let visible_lines = chunks[0].height.saturating_sub(0);
        let max_scroll = total_lines.saturating_sub(visible_lines);

        // Clamp scroll
        let scroll = self.scroll_offset.min(max_scroll);

        // Build styled lines from content
        let styled_lines: Vec<Line> = self
            .content
            .iter()
            .map(|line| {
                if line.starts_with("──") && line.ends_with("──") {
                    // Section header
                    Line::from(Span::styled(
                        line.clone(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ))
                } else if line.is_empty() {
                    Line::from("")
                } else {
                    Line::from(Span::styled(
                        line.clone(),
                        Style::default().fg(Color::Gray),
                    ))
                }
            })
            .collect();

        // Render with scroll
        f.render_widget(
            Paragraph::new(styled_lines)
                .scroll((scroll, 0))
                .wrap(Wrap { trim: false }),
            chunks[0],
        );

        // Hint line
        let hint = Line::from(Span::styled(
            " Esc / F1 to close  |  j/k to scroll  |  g/G top/bottom ",
            Style::default().fg(Color::DarkGray),
        ));
        let hint_para = Paragraph::new(hint).style(Style::default().fg(Color::DarkGray));
        f.render_widget(hint_para, chunks[1]);
    }
}