pub enum PromptEvent {
    Submitted(String),
    /// A command with its name (first word after `/`) and space-separated arguments.
    Command {
        name: String,
        args: Vec<String>,
    },
    Cancel,
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
        self.cursor_pos.0 += c.len_utf8();
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
            let line = &mut self.lines[self.cursor_pos.1];
            let prev = line[..self.cursor_pos.0]
                .chars()
                .last()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
            self.cursor_pos.0 -= prev;
            line.remove(self.cursor_pos.0);
        }
        self.update_command_mode();
    }

    /// Insert `text` at the cursor. Newlines split into multiple buffer lines.
    pub fn insert_text(&mut self, text: &str) {
        self.cancel_history_browse();
        if text.is_empty() {
            return;
        }

        let segments: Vec<&str> = text.split('\n').collect();

        if let Some(line) = self.lines.get_mut(self.cursor_pos.1) {
            line.insert_str(self.cursor_pos.0, segments[0]);
            self.cursor_pos.0 += segments[0].len();
        }

        for segment in segments.iter().skip(1) {
            let tail = self.lines[self.cursor_pos.1].split_off(self.cursor_pos.0);
            let mut new_line = segment.to_string();
            new_line.push_str(&tail);
            self.lines.insert(self.cursor_pos.1 + 1, new_line);
            self.cursor_pos.1 += 1;
            self.cursor_pos.0 = segment.len();
        }

        self.update_command_mode();
    }

    fn cancel_history_browse(&mut self) {
        if self.history_idx.take().is_some() {
            let draft = self.draft.clone();
            self.set_content(&draft);
        }
    }

    fn update_command_mode(&mut self) {
        self.command_mode = self.lines.first().is_some_and(|line| line.starts_with('/'));
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
            let mut parts = self.lines[0].trim_start_matches('/').split_whitespace();
            let name = parts.next().unwrap_or("").to_string();
            let args: Vec<String> = parts.map(|s| s.to_string()).collect();
            PromptEvent::Command { name, args }
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
        self.lines[0].find(' ').unwrap_or(self.lines[0].len())
    }
}

#[cfg(test)]
mod tests {
    use super::ContentManager;

    #[test]
    fn insert_text_single_line_at_cursor() {
        let mut cm = ContentManager::new();
        cm.push('h');
        cm.push('i');
        cm.insert_text(" there");
        assert_eq!(cm.lines(), &["hi there"]);
        assert_eq!(cm.cursor_pos(), (8, 0));
    }

    #[test]
    fn insert_text_multiline() {
        let mut cm = ContentManager::new();
        cm.insert_text("line1\nline2\nline3");
        assert_eq!(cm.lines(), &["line1", "line2", "line3"]);
        assert_eq!(cm.cursor_pos(), (5, 2));
    }

    #[test]
    fn insert_text_enters_command_mode() {
        let mut cm = ContentManager::new();
        cm.insert_text("/model groq");
        assert!(cm.is_command_mode());
    }

    #[test]
    fn insert_text_cancels_history_browse() {
        let mut cm = ContentManager::new();
        cm.insert_text("old");
        let _ = cm.submit();
        cm.key_up(); // browse history ("old"); draft was empty
        cm.insert_text("new");
        assert_eq!(cm.lines(), &["new"]);
    }
}
