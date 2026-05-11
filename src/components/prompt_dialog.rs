/// Interactive prompt dialog — a floating modal for gathering structured input.
///
/// The [`PromptSchema`] type is fully serializable so the same definition can be
/// embedded in AI tool call schemas, sent as a tool response, or triggered by a
/// command.  The UI widget (`PromptDialogController`) is self-contained and renders
/// as a centered overlay on top of whatever is already drawn.
///
/// # Supported field types
/// - [`FieldSchema::Text`]         — free-form single-line text input
/// - [`FieldSchema::SingleChoice`] — radio-button style, pick exactly one option
/// - [`FieldSchema::MultiChoice`]  — checkbox style, pick zero or more options
///
/// # Key bindings (while the dialog has focus)
/// | Key          | Action                                    |
/// |--------------|-------------------------------------------|
/// | `Tab`        | Move to next field                        |
/// | `Shift+Tab`  | Move to previous field                    |
/// | `↑` / `↓`   | Navigate options (choice fields)          |
/// | `j` / `k`   | Navigate options (choice fields)          |
/// | `Space`      | Toggle checkbox (multi-choice fields)     |
/// | `Enter`      | Confirm current field and advance         |
/// | `Ctrl+P`     | Submit all fields                         |
/// | `Esc`        | Cancel the dialog                         |
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};

use crate::components::text_area::TextArea;

// ── Schema (shared with AI / commands) ────────────────────────────────────────

/// Definition of a single input field within a [`PromptSchema`].
///
/// Serialized with a `"type"` discriminant so it can be included verbatim in
/// AI tool definitions or emitted by the model as part of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldSchema {
    /// Free-form single-line text input.
    Text {
        id: String,
        label: String,
        /// Shown dimmed when the field is empty and unfocused.
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    /// Pick exactly one of the provided options.
    SingleChoice {
        id: String,
        label: String,
        options: Vec<String>,
        /// Zero-based index of the pre-selected option, defaults to 0.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<usize>,
    },
    /// Pick zero or more of the provided options.
    MultiChoice {
        id: String,
        label: String,
        options: Vec<String>,
        /// Zero-based indices of the pre-checked options.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<Vec<usize>>,
    },
}

impl FieldSchema {
    pub fn id(&self) -> &str {
        match self {
            Self::Text { id, .. }
            | Self::SingleChoice { id, .. }
            | Self::MultiChoice { id, .. } => id,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Text { label, .. }
            | Self::SingleChoice { label, .. }
            | Self::MultiChoice { label, .. } => label,
        }
    }
}

/// The full description of a prompt dialog.
///
/// Serialize this and send it to the AI model so it knows what questions it
/// may request the user to answer (and what format the responses will have).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSchema {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub fields: Vec<FieldSchema>,
}

// ── Response types ─────────────────────────────────────────────────────────────

/// The submitted value for one field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldResponse {
    Text {
        id: String,
        value: String,
    },
    SingleChoice {
        id: String,
        /// Zero-based index into `options`.
        selected: usize,
        /// Convenience copy of `options[selected]`.
        value: String,
    },
    MultiChoice {
        id: String,
        /// Zero-based indices of all checked options.
        selected: Vec<usize>,
        /// Convenience copies of the selected option strings.
        values: Vec<String>,
    },
}

/// Collected responses for all fields of a [`PromptSchema`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptDialogResponse {
    pub fields: Vec<FieldResponse>,
}

impl PromptDialogResponse {
    /// Look up the response for a specific field by its `id`.
    pub fn get(&self, id: &str) -> Option<&FieldResponse> {
        self.fields.iter().find(|f| match f {
            FieldResponse::Text { id: fid, .. }
            | FieldResponse::SingleChoice { id: fid, .. }
            | FieldResponse::MultiChoice { id: fid, .. } => fid == id,
        })
    }
}

// ── Internal mutable field state ───────────────────────────────────────────────

#[derive(Debug, Clone)]
enum FieldState {
    Text {
        value: String,
        /// Byte offset of the cursor within `value`.
        cursor: usize,
    },
    SingleChoice {
        /// Index of the currently highlighted (and committed) option.
        selected: usize,
    },
    MultiChoice {
        checked: Vec<bool>,
        /// Index of the option the cursor is resting on.
        cursor: usize,
    },
}

impl FieldState {
    fn init(schema: &FieldSchema) -> Self {
        match schema {
            FieldSchema::Text { .. } => FieldState::Text {
                value: String::new(),
                cursor: 0,
            },
            FieldSchema::SingleChoice { default, options, .. } => FieldState::SingleChoice {
                selected: default
                    .unwrap_or(0)
                    .min(options.len().saturating_sub(1)),
            },
            FieldSchema::MultiChoice { default, options, .. } => {
                let mut checked = vec![false; options.len()];
                if let Some(defaults) = default {
                    for &i in defaults {
                        if i < checked.len() {
                            checked[i] = true;
                        }
                    }
                }
                FieldState::MultiChoice { checked, cursor: 0 }
            }
        }
    }

    fn to_response(&self, schema: &FieldSchema) -> FieldResponse {
        match (self, schema) {
            (FieldState::Text { value, .. }, FieldSchema::Text { id, .. }) => {
                FieldResponse::Text {
                    id: id.clone(),
                    value: value.clone(),
                }
            }
            (
                FieldState::SingleChoice { selected },
                FieldSchema::SingleChoice { id, options, .. },
            ) => FieldResponse::SingleChoice {
                id: id.clone(),
                selected: *selected,
                value: options.get(*selected).cloned().unwrap_or_default(),
            },
            (
                FieldState::MultiChoice { checked, .. },
                FieldSchema::MultiChoice { id, options, .. },
            ) => {
                let selected: Vec<usize> = checked
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &c)| if c { Some(i) } else { None })
                    .collect();
                let values = selected
                    .iter()
                    .filter_map(|&i| options.get(i).cloned())
                    .collect();
                FieldResponse::MultiChoice {
                    id: id.clone(),
                    selected,
                    values,
                }
            }
            _ => unreachable!("schema/state mismatch in FieldState::to_response"),
        }
    }
}

// ── Controller events ──────────────────────────────────────────────────────────

/// Events emitted by [`PromptDialogController::handle_key`].
pub enum PromptDialogEvent {
    /// The user pressed `Ctrl+P`; all field values are collected.
    Submitted(PromptDialogResponse),
    /// The user pressed `Esc`; no response is produced.
    Cancelled,
}

// ── Controller ─────────────────────────────────────────────────────────────────

/// Owns all mutable state for the dialog and handles rendering.
///
/// Typical usage inside `App`:
/// ```rust,ignore
/// // Open a dialog
/// self.dialog = Some(PromptDialogController::new(my_schema));
///
/// // In the key-event handler
/// if let Some(dialog) = &mut self.dialog {
///     match dialog.handle_key(key_event) {
///         Some(PromptDialogEvent::Submitted(resp)) => { /* use resp */ self.dialog = None; }
///         Some(PromptDialogEvent::Cancelled) => { self.dialog = None; }
///         None => {}
///     }
///     return; // dialog consumed the event
/// }
///
/// // In ui()
/// if let Some(dialog) = &self.dialog {
///     dialog.render(f);
/// }
/// ```
pub struct PromptDialogController {
    pub schema: PromptSchema,
    states: Vec<FieldState>,
    active_field: usize,
}

impl PromptDialogController {
    pub fn new(schema: PromptSchema) -> Self {
        let states = schema.fields.iter().map(FieldState::init).collect();
        Self {
            schema,
            states,
            active_field: 0,
        }
    }

    // ── Event handling ─────────────────────────────────────────────────────────

    /// Process a key event. Returns `Some(event)` when the dialog terminates.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<PromptDialogEvent> {
        match (key.code, key.modifiers) {
            // Global: cancel
            (KeyCode::Esc, _) => return Some(PromptDialogEvent::Cancelled),

            // Global: submit all fields
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                return Some(PromptDialogEvent::Submitted(self.collect()));
            }

            // Tab: advance to next field
            (KeyCode::Tab, _) => {
                let n = self.states.len().max(1);
                self.active_field = (self.active_field + 1) % n;
            }

            // Shift+Tab: go back to previous field
            (KeyCode::BackTab, _) => {
                let n = self.states.len().max(1);
                self.active_field = (self.active_field + n - 1) % n;
            }

            // Everything else is field-specific
            _ => self.dispatch_field_key(key),
        }
        None
    }

    /// Route a key to the active field's handler.
    fn dispatch_field_key(&mut self, key: KeyEvent) {
        let idx = self.active_field;
        let n_fields = self.states.len().max(1);

        // Retrieve option count *before* mutably borrowing states (disjoint fields).
        let n_options = match &self.schema.fields[idx] {
            FieldSchema::SingleChoice { options, .. }
            | FieldSchema::MultiChoice { options, .. } => options.len(),
            FieldSchema::Text { .. } => 0,
        };

        match &mut self.states[idx] {
            FieldState::Text { value, cursor } => {
                Self::handle_text_key(key, value, cursor, &mut self.active_field, n_fields);
            }
            FieldState::SingleChoice { selected } => {
                Self::handle_single_choice_key(
                    key,
                    selected,
                    n_options,
                    &mut self.active_field,
                    n_fields,
                );
            }
            FieldState::MultiChoice { checked, cursor } => {
                Self::handle_multi_choice_key(
                    key,
                    checked,
                    cursor,
                    n_options,
                    &mut self.active_field,
                    n_fields,
                );
            }
        }
    }

    fn handle_text_key(
        key: KeyEvent,
        value: &mut String,
        cursor: &mut usize,
        active_field: &mut usize,
        n_fields: usize,
    ) {
        match (key.code, key.modifiers) {
            // Printable characters (plain or with Shift for uppercase)
            (KeyCode::Char(c), m)
                if m == KeyModifiers::NONE || m == KeyModifiers::SHIFT =>
            {
                value.insert(*cursor, c);
                *cursor += c.len_utf8();
            }
            (KeyCode::Backspace, _) => {
                if *cursor > 0 {
                    let prev = value[..*cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    value.remove(prev);
                    *cursor = prev;
                }
            }
            (KeyCode::Delete, _) => {
                if *cursor < value.len() {
                    value.remove(*cursor);
                }
            }
            (KeyCode::Left, _) => {
                if *cursor > 0 {
                    let prev = value[..*cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    *cursor = prev;
                }
            }
            (KeyCode::Right, _) => {
                if let Some(ch) = value[*cursor..].chars().next() {
                    *cursor += ch.len_utf8();
                }
            }
            (KeyCode::Home, _) => *cursor = 0,
            (KeyCode::End, _) => *cursor = value.len(),
            // Enter: advance to next field
            (KeyCode::Enter, _) => {
                *active_field = (*active_field + 1) % n_fields;
            }
            _ => {}
        }
    }

    fn handle_single_choice_key(
        key: KeyEvent,
        selected: &mut usize,
        n_options: usize,
        active_field: &mut usize,
        n_fields: usize,
    ) {
        if n_options == 0 {
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if *selected > 0 {
                    *selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if *selected + 1 < n_options {
                    *selected += 1;
                }
            }
            // Enter: confirm selection and move to next field
            KeyCode::Enter => {
                *active_field = (*active_field + 1) % n_fields;
            }
            _ => {}
        }
    }

    fn handle_multi_choice_key(
        key: KeyEvent,
        checked: &mut Vec<bool>,
        cursor: &mut usize,
        n_options: usize,
        active_field: &mut usize,
        n_fields: usize,
    ) {
        if n_options == 0 {
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if *cursor > 0 {
                    *cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if *cursor + 1 < n_options {
                    *cursor += 1;
                }
            }
            // Space: toggle the item under the cursor
            KeyCode::Char(' ') => {
                let c = *cursor;
                if c < checked.len() {
                    checked[c] = !checked[c];
                }
            }
            // Enter: move to next field
            KeyCode::Enter => {
                *active_field = (*active_field + 1) % n_fields;
            }
            _ => {}
        }
    }

    fn collect(&self) -> PromptDialogResponse {
        let fields = self
            .states
            .iter()
            .zip(self.schema.fields.iter())
            .map(|(state, schema)| state.to_response(schema))
            .collect();
        PromptDialogResponse { fields }
    }

    // ── Rendering ──────────────────────────────────────────────────────────────

    /// Render the dialog as a centered floating panel.
    ///
    /// Call this **after** rendering all other widgets so the `Clear` erases the
    /// correct background content.
    pub fn render(&self, f: &mut Frame) {
        let dialog_area = self.compute_dialog_rect(f.area());

        // Erase whatever was drawn behind the dialog
        f.render_widget(Clear, dialog_area);

        // Outer border
        let block = Block::default()
            .title(Span::styled(
                format!(" {} ", self.schema.title),
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
        self.render_inner(f, inner);
    }

    /// Compute a centered Rect for the dialog based on content height.
    fn compute_dialog_rect(&self, area: Rect) -> Rect {
        let content_height = self.total_content_height();
        // border top + border bottom + optional description + fields + hint
        let dialog_h = (content_height + 2).min(area.height.saturating_sub(2));
        let dialog_w = ((area.width * 6) / 10).max(40).min(area.width - 4);
        let x = (area.width.saturating_sub(dialog_w)) / 2;
        let y = (area.height.saturating_sub(dialog_h)) / 2;
        Rect::new(x, y, dialog_w, dialog_h)
    }

    /// Total inner height required by the dialog content (excluding outer border).
    fn total_content_height(&self) -> u16 {
        let desc_h = self
            .schema
            .description
            .as_deref()
            .map(|d| d.lines().count() as u16 + 1)
            .unwrap_or(0);
        let fields_h: u16 = self
            .schema
            .fields
            .iter()
            .map(field_display_height)
            .sum();
        desc_h + fields_h + 1 // +1 for the hint line
    }

    fn render_inner(&self, f: &mut Frame, area: Rect) {
        let mut constraints: Vec<Constraint> = Vec::new();

        // Optional description
        if let Some(desc) = &self.schema.description {
            let h = desc.lines().count() as u16 + 1;
            constraints.push(Constraint::Length(h));
        }

        // One slot per field
        for schema in &self.schema.fields {
            constraints.push(Constraint::Length(field_display_height(schema)));
        }

        // Hint line at the bottom
        constraints.push(Constraint::Length(1));

        // Absorb any leftover space so layout doesn't panic on short terminals
        constraints.push(Constraint::Min(0));

        let chunks = Layout::vertical(constraints).split(area);
        let mut chunk_idx = 0;

        // Description
        if let Some(desc) = &self.schema.description {
            f.render_widget(
                Paragraph::new(desc.as_str())
                    .style(Style::default().fg(Color::Gray))
                    .wrap(Wrap { trim: false }),
                chunks[chunk_idx],
            );
            chunk_idx += 1;
        }

        // Fields
        for (field_idx, (schema, state)) in
            self.schema.fields.iter().zip(self.states.iter()).enumerate()
        {
            let is_active = field_idx == self.active_field;
            self.render_field(f, chunks[chunk_idx], schema, state, is_active);
            chunk_idx += 1;
        }

        // Hint line
        f.render_widget(render_hint_line(), chunks[chunk_idx]);
    }

    fn render_field(
        &self,
        f: &mut Frame,
        area: Rect,
        schema: &FieldSchema,
        state: &FieldState,
        is_active: bool,
    ) {
        match (schema, state) {
            (FieldSchema::Text { label, placeholder, .. }, FieldState::Text { value, cursor }) => {
                self.render_text_field(f, area, label, placeholder.as_deref(), value, *cursor, is_active);
            }
            (FieldSchema::SingleChoice { label, options, .. }, FieldState::SingleChoice { selected }) => {
                self.render_single_choice(f, area, label, options, *selected, is_active);
            }
            (FieldSchema::MultiChoice { label, options, .. }, FieldState::MultiChoice { checked, cursor }) => {
                self.render_multi_choice(f, area, label, options, checked, *cursor, is_active);
            }
            _ => {} // schema/state mismatch — should never happen
        }
    }

    fn render_text_field(
        &self,
        f: &mut Frame,
        area: Rect,
        label: &str,
        placeholder: Option<&str>,
        value: &str,
        cursor: usize,
        is_active: bool,
    ) {
        let label_style = if is_active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Split area: 1 row for label, rest for the bordered text box
        let parts = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

        // Label
        f.render_widget(
            Paragraph::new(Span::styled(label, label_style)),
            parts[0],
        );

        // Input box
        if is_active || !value.is_empty() {
            // Use the existing TextArea widget; it draws its own block cursor
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(if is_active {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                });
            let lines = vec![value.to_string()];
            // cursor_pos in TextArea is (col, row), value is single-line so row=0
            let text_area =
                TextArea::new(&lines, (cursor, 0), is_active).with_block(block);
            f.render_widget(text_area, parts[1]);
        } else if let Some(ph) = placeholder {
            // Show placeholder when empty and unfocused
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray));
            f.render_widget(
                Paragraph::new(Span::styled(ph, Style::default().fg(Color::DarkGray)))
                    .block(block),
                parts[1],
            );
        } else {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray));
            f.render_widget(block, parts[1]);
        }
    }

    fn render_single_choice(
        &self,
        f: &mut Frame,
        area: Rect,
        label: &str,
        options: &[String],
        selected: usize,
        is_active: bool,
    ) {
        let label_style = if is_active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let parts = Layout::vertical([
            Constraint::Length(1),              // label
            Constraint::Length(options.len() as u16), // options
        ])
        .split(area);

        f.render_widget(
            Paragraph::new(Span::styled(label, label_style)),
            parts[0],
        );

        let lines: Vec<Line> = options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let is_selected = i == selected;
                let indicator = if is_selected { "●" } else { "○" };
                let row_style = if is_active && is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if is_selected {
                    Style::default().fg(Color::White)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Line::from(vec![
                    Span::styled(format!("  {indicator} "), row_style),
                    Span::styled(opt.clone(), row_style),
                ])
            })
            .collect();

        f.render_widget(Paragraph::new(lines), parts[1]);
    }

    fn render_multi_choice(
        &self,
        f: &mut Frame,
        area: Rect,
        label: &str,
        options: &[String],
        checked: &[bool],
        cursor: usize,
        is_active: bool,
    ) {
        let label_style = if is_active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let parts = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(options.len() as u16),
        ])
        .split(area);

        f.render_widget(
            Paragraph::new(Span::styled(label, label_style)),
            parts[0],
        );

        let lines: Vec<Line> = options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let is_checked = checked.get(i).copied().unwrap_or(false);
                let is_cursor = is_active && i == cursor;
                let indicator = if is_checked { "[✓]" } else { "[ ]" };
                let row_style = if is_cursor {
                    // Highlighted cursor row: reversed colours
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if is_checked {
                    Style::default().fg(Color::Green)
                } else if is_active {
                    Style::default().fg(Color::White)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Line::from(vec![
                    Span::styled(format!("  {indicator} "), row_style),
                    Span::styled(opt.clone(), row_style),
                ])
            })
            .collect();

        f.render_widget(Paragraph::new(lines), parts[1]);
    }
}

// ── Helper functions ───────────────────────────────────────────────────────────

/// Compute the number of terminal rows a field occupies (label + widget + spacer).
fn field_display_height(schema: &FieldSchema) -> u16 {
    match schema {
        // 1 label row + 3 rows for bordered single-line input (top + content + bottom) + 1 spacer
        FieldSchema::Text { .. } => 5,
        // 1 label row + one row per option + 1 spacer
        FieldSchema::SingleChoice { options, .. } => 1 + options.len() as u16 + 1,
        FieldSchema::MultiChoice { options, .. } => 1 + options.len() as u16 + 1,
    }
}

/// Build the hint/key-binding line shown at the bottom of the dialog.
fn render_hint_line<'a>() -> Paragraph<'a> {
    let line = Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(" next  "),
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
        Span::raw(" navigate  "),
        Span::styled("Space", Style::default().fg(Color::Yellow)),
        Span::raw(" toggle  "),
        Span::styled("Ctrl+P", Style::default().fg(Color::Yellow)),
        Span::raw(" submit  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(" cancel"),
    ]);
    Paragraph::new(line)
}

// ── Builder helpers ────────────────────────────────────────────────────────────

/// Convenience builders on [`PromptSchema`] for quick schema construction.
impl PromptSchema {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            fields: Vec::new(),
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_field(mut self, field: FieldSchema) -> Self {
        self.fields.push(field);
        self
    }
}

/// Convenience builders on [`FieldSchema`].
impl FieldSchema {
    pub fn text(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::Text {
            id: id.into(),
            label: label.into(),
            placeholder: None,
        }
    }

    pub fn text_with_placeholder(
        id: impl Into<String>,
        label: impl Into<String>,
        placeholder: impl Into<String>,
    ) -> Self {
        Self::Text {
            id: id.into(),
            label: label.into(),
            placeholder: Some(placeholder.into()),
        }
    }

    pub fn single_choice(
        id: impl Into<String>,
        label: impl Into<String>,
        options: Vec<String>,
    ) -> Self {
        Self::SingleChoice {
            id: id.into(),
            label: label.into(),
            options,
            default: None,
        }
    }

    pub fn multi_choice(
        id: impl Into<String>,
        label: impl Into<String>,
        options: Vec<String>,
    ) -> Self {
        Self::MultiChoice {
            id: id.into(),
            label: label.into(),
            options,
            default: None,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_schema() -> PromptSchema {
        PromptSchema::new("Test Dialog")
            .with_description("Please fill in the details below.")
            .with_field(FieldSchema::text_with_placeholder("name", "Your name", "e.g. Alice"))
            .with_field(FieldSchema::single_choice(
                "lang",
                "Language",
                vec!["Rust".into(), "Go".into(), "Python".into()],
            ))
            .with_field(FieldSchema::multi_choice(
                "features",
                "Features",
                vec!["Tests".into(), "CI".into(), "Docker".into()],
            ))
    }

    #[test]
    fn schema_round_trips_through_json() {
        let schema = make_schema();
        let json = serde_json::to_string_pretty(&schema).expect("serialize");
        let back: PromptSchema = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.fields.len(), schema.fields.len());
        assert_eq!(back.fields[0].id(), "name");
        assert_eq!(back.fields[1].id(), "lang");
        assert_eq!(back.fields[2].id(), "features");
    }

    #[test]
    fn initial_single_choice_default_is_zero() {
        let schema = make_schema();
        let ctrl = PromptDialogController::new(schema);
        if let FieldState::SingleChoice { selected } = &ctrl.states[1] {
            assert_eq!(*selected, 0);
        } else {
            panic!("expected SingleChoice state");
        }
    }

    #[test]
    fn text_input_and_cursor_movement() {
        let schema = PromptSchema::new("T").with_field(FieldSchema::text("q", "Question"));
        let mut ctrl = PromptDialogController::new(schema);

        // Type "hi"
        ctrl.handle_key(key(KeyCode::Char('h')));
        ctrl.handle_key(key(KeyCode::Char('i')));

        if let FieldState::Text { value, cursor } = &ctrl.states[0] {
            assert_eq!(value, "hi");
            assert_eq!(*cursor, 2);
        } else {
            panic!("expected Text state");
        }

        // Move left, then type 'e'
        ctrl.handle_key(key(KeyCode::Left));
        ctrl.handle_key(key(KeyCode::Left));
        ctrl.handle_key(key(KeyCode::Char('e')));

        if let FieldState::Text { value, .. } = &ctrl.states[0] {
            assert_eq!(value, "ehi");
        }
    }

    #[test]
    fn backspace_removes_char() {
        let schema = PromptSchema::new("T").with_field(FieldSchema::text("q", "Q"));
        let mut ctrl = PromptDialogController::new(schema);
        ctrl.handle_key(key(KeyCode::Char('a')));
        ctrl.handle_key(key(KeyCode::Char('b')));
        ctrl.handle_key(key(KeyCode::Backspace));
        if let FieldState::Text { value, .. } = &ctrl.states[0] {
            assert_eq!(value, "a");
        }
    }

    #[test]
    fn single_choice_navigation() {
        let schema = PromptSchema::new("T").with_field(FieldSchema::single_choice(
            "c",
            "Choose",
            vec!["A".into(), "B".into(), "C".into()],
        ));
        let mut ctrl = PromptDialogController::new(schema);

        ctrl.handle_key(key(KeyCode::Down));
        ctrl.handle_key(key(KeyCode::Down));
        if let FieldState::SingleChoice { selected } = &ctrl.states[0] {
            assert_eq!(*selected, 2);
        }

        // Can't go past the end
        ctrl.handle_key(key(KeyCode::Down));
        if let FieldState::SingleChoice { selected } = &ctrl.states[0] {
            assert_eq!(*selected, 2);
        }

        ctrl.handle_key(key(KeyCode::Up));
        if let FieldState::SingleChoice { selected } = &ctrl.states[0] {
            assert_eq!(*selected, 1);
        }
    }

    #[test]
    fn multi_choice_toggle() {
        let schema = PromptSchema::new("T").with_field(FieldSchema::multi_choice(
            "f",
            "Features",
            vec!["A".into(), "B".into()],
        ));
        let mut ctrl = PromptDialogController::new(schema);

        // Toggle item 0
        ctrl.handle_key(key(KeyCode::Char(' ')));
        // Move down, toggle item 1
        ctrl.handle_key(key(KeyCode::Down));
        ctrl.handle_key(key(KeyCode::Char(' ')));
        // Toggle item 0 back off (move up first)
        ctrl.handle_key(key(KeyCode::Up));
        ctrl.handle_key(key(KeyCode::Char(' ')));

        if let FieldState::MultiChoice { checked, .. } = &ctrl.states[0] {
            assert!(!checked[0]);
            assert!(checked[1]);
        }
    }

    #[test]
    fn tab_advances_field() {
        let schema = make_schema();
        let mut ctrl = PromptDialogController::new(schema);
        assert_eq!(ctrl.active_field, 0);
        ctrl.handle_key(key(KeyCode::Tab));
        assert_eq!(ctrl.active_field, 1);
        ctrl.handle_key(key(KeyCode::BackTab));
        assert_eq!(ctrl.active_field, 0);
    }

    #[test]
    fn ctrl_p_collects_response() {
        let schema = PromptSchema::new("T")
            .with_field(FieldSchema::text("name", "Name"))
            .with_field(FieldSchema::single_choice(
                "lang",
                "Lang",
                vec!["Rust".into(), "Go".into()],
            ));
        let mut ctrl = PromptDialogController::new(schema);

        // Type a name
        for c in "Alice".chars() {
            ctrl.handle_key(key(KeyCode::Char(c)));
        }
        // Submit
        let event = ctrl.handle_key(ctrl_p());
        let resp = match event {
            Some(PromptDialogEvent::Submitted(r)) => r,
            _ => panic!("expected Submitted"),
        };

        assert_eq!(resp.fields.len(), 2);
        let name_resp = resp.get("name").expect("name field");
        if let FieldResponse::Text { value, .. } = name_resp {
            assert_eq!(value, "Alice");
        } else {
            panic!("expected Text response");
        }
    }

    #[test]
    fn esc_returns_cancelled() {
        let schema = PromptSchema::new("T").with_field(FieldSchema::text("q", "Q"));
        let mut ctrl = PromptDialogController::new(schema);
        let event = ctrl.handle_key(key(KeyCode::Esc));
        assert!(matches!(event, Some(PromptDialogEvent::Cancelled)));
    }

    #[test]
    fn response_serializes_to_json() {
        let schema = PromptSchema::new("T")
            .with_field(FieldSchema::multi_choice(
                "tags",
                "Tags",
                vec!["async".into(), "sync".into()],
            ));
        let mut ctrl = PromptDialogController::new(schema);
        ctrl.handle_key(key(KeyCode::Char(' '))); // toggle "async"
        let event = ctrl.handle_key(ctrl_p());
        if let Some(PromptDialogEvent::Submitted(resp)) = event {
            let json = serde_json::to_string(&resp).expect("serialize response");
            assert!(json.contains("async"));
        }
    }

    // ── Key helpers ────────────────────────────────────────────────────────────

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_p() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)
    }
}
