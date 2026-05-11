pub enum PromptEvent {
    Submitted(String),
    /// Finalized command name (the first word after `/`, without the slash).
    Command(String),
}

pub struct ContentManager {
    lines: Vec<String>,
    cursor_pos: (usize, usize),
    history: Vec<String>,
    /// `None` = editing current draft; `Some(i)` = browsing history[i]
    history_idx: Option<usize>,
    /// Draft saved when the user starts navigating history
    draft: String,
    // If true, the prompt is in command mode, where the first word is command
    command_mode: bool,
}

impl ContentManager {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_pos: (0, 0),
            history: Vec::new(),
            history_idx: None,
            draft: String::new(),
            command_mode: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| l.trim().is_empty())
    }

    // ── Text mutation ──────────────────────────────────────────────────────────

    pub fn push(&mut self, c: char) {
        if c == '\n' {
            self.new_line();
            return;
        }
        // Enter command mode when '/' is the very first character on an empty buffer.
        if c == '/' && self.is_empty() && self.cursor_pos == (0, 0) {
            self.command_mode = true;
        }
        match self.lines.get_mut(self.cursor_pos.1) {
            None => self.lines.push(c.to_string()),
            Some(line) => line.insert(self.cursor_pos.0, c),
        }
        self.cursor_pos.0 += 1;
    }

    pub fn pop(&mut self) {
        if self.cursor_pos.0 == 0 && self.cursor_pos.1 == 0 {
            return;
        }
        if self.cursor_pos.0 == 0 {
            // Merge current line into the previous one
            let current = self.lines.remove(self.cursor_pos.1);
            self.cursor_pos.1 -= 1;
            self.cursor_pos.0 = self.lines[self.cursor_pos.1].len();
            self.lines[self.cursor_pos.1].push_str(&current);
        } else {
            self.lines[self.cursor_pos.1].remove(self.cursor_pos.0 - 1);
            self.cursor_pos.0 -= 1;
        }
        // Exit command mode if the leading '/' has been removed.
        if self.command_mode && !self.lines[0].starts_with('/') {
            self.command_mode = false;
        }
    }

    pub fn new_line(&mut self) {
        let tail = self.lines[self.cursor_pos.1].split_off(self.cursor_pos.0);
        self.lines.insert(self.cursor_pos.1 + 1, tail);
        self.cursor_pos.1 += 1;
        self.cursor_pos.0 = 0;
    }

    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_pos = (0, 0);
        self.history_idx = None;
        self.draft = String::new();
        self.command_mode = false;
    }

    // ── Submission ─────────────────────────────────────────────────────────────

    /// Returns the prompt text and resets the buffer. Saves to history.
    /// When in command mode, returns [`PromptEvent::Command`] with the command
    /// name (the first word of line 0, without the leading `/`).
    /// Otherwise returns [`PromptEvent::Submitted`] with the full text.
    pub fn submit(&mut self) -> Option<PromptEvent> {
        let text = self.lines.join("\n");
        if text.trim().is_empty() {
            return None;
        }
        self.history.push(text.clone());
        self.history_idx = None;
        self.draft = String::new();

        let event = if self.command_mode {
            let name = self.lines[0]
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            PromptEvent::Command(name)
        } else {
            PromptEvent::Submitted(text)
        };

        self.lines = vec![String::new()];
        self.cursor_pos = (0, 0);
        self.command_mode = false;
        Some(event)
    }

    // ── Cursor movement ────────────────────────────────────────────────────────

    pub fn cursor_pre(&mut self) {
        if self.cursor_pos.0 > 0 {
            self.cursor_pos.0 -= 1;
        } else if self.cursor_pos.1 > 0 {
            self.cursor_pos.1 -= 1;
            self.cursor_pos.0 = self.lines[self.cursor_pos.1].len();
        }
    }

    pub fn cursor_next(&mut self) {
        if self.cursor_pos.0 < self.lines[self.cursor_pos.1].len() {
            self.cursor_pos.0 += 1;
        } else if self.cursor_pos.1 < self.lines.len() - 1 {
            self.cursor_pos.1 += 1;
            self.cursor_pos.0 = 0;
        }
    }

    /// ↑ key: cursor up within content, or navigate to previous history entry
    /// when already on the first line.
    pub fn key_up(&mut self) {
        if self.cursor_pos.1 > 0 {
            self.cursor_pos.1 -= 1;
            let line_len = self.lines[self.cursor_pos.1].len();
            self.cursor_pos.0 = self.cursor_pos.0.min(line_len);
        } else {
            self.history_prev();
        }
    }

    /// ↓ key: cursor down within content, or navigate to next history entry
    /// (or restore the draft) when already on the last line.
    pub fn key_down(&mut self) {
        if self.cursor_pos.1 < self.lines.len() - 1 {
            self.cursor_pos.1 += 1;
            let line_len = self.lines[self.cursor_pos.1].len();
            self.cursor_pos.0 = self.cursor_pos.0.min(line_len);
        } else {
            self.history_next();
        }
    }

    // ── History navigation ─────────────────────────────────────────────────────

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let new_idx = match self.history_idx {
            None => {
                // Save the current draft before entering history
                self.draft = self.lines.join("\n");
                self.history.len() - 1
            }
            Some(0) => return, // already at the oldest entry
            Some(i) => i - 1,
        };
        self.history_idx = Some(new_idx);
        self.load_history_entry(new_idx);
    }

    fn history_next(&mut self) {
        match self.history_idx {
            None => {}
            Some(i) if i + 1 >= self.history.len() => {
                // Return to the saved draft
                self.history_idx = None;
                let draft = self.draft.clone();
                self.set_content(&draft);
            }
            Some(i) => {
                let new_idx = i + 1;
                self.history_idx = Some(new_idx);
                self.load_history_entry(new_idx);
            }
        }
    }

    fn load_history_entry(&mut self, idx: usize) {
        let entry = self.history[idx].clone();
        self.set_content(&entry);
    }

    fn set_content(&mut self, text: &str) {
        self.lines = text.lines().map(|l| l.to_string()).collect::<Vec<_>>();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        // Place cursor at end of last line
        let last = self.lines.len() - 1;
        self.cursor_pos = (self.lines[last].len(), last);
    }

    // ── Read access for renderer ───────────────────────────────────────────────

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor_pos(&self) -> (usize, usize) {
        self.cursor_pos
    }

    pub fn is_command_mode(&self) -> bool {
        self.command_mode
    }

    /// Number of characters at the start of line 0 that form the command token
    /// (i.e. from the leading `/` up to but not including the first space, or
    /// end of the line).  Returns 0 when not in command mode.
    pub fn command_token_len(&self) -> usize {
        if !self.command_mode {
            return 0;
        }
        self.lines[0]
            .find(' ')
            .unwrap_or(self.lines[0].len())
    }
}
