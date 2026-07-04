use pulldown_cmark::{
    CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

/// Parse a markdown string into ratatui [`Text`] with basic styling.
pub fn parse_markdown(text: &str) -> Text<'static> {
    MarkdownRenderer::new(false).render(text)
}

/// Same as [`parse_markdown`] but renders all text in `DarkGray` — used for
/// thinking/internal-monologue sections to visually distinguish them.
pub fn parse_markdown_dimmed(text: &str) -> Text<'static> {
    MarkdownRenderer::new(true).render(text)
}

// ---------------------------------------------------------------------------
// Theme — mirrors the colour scheme of the previous hand-rolled parser
// ---------------------------------------------------------------------------

#[derive(Copy, Clone)]
struct Theme {
    dim: bool,
}

impl Theme {
    fn text_fg(self) -> Color {
        if self.dim {
            Color::DarkGray
        } else {
            Color::White
        }
    }

    fn code_fg(self) -> Color {
        if self.dim {
            Color::DarkGray
        } else {
            Color::Green
        }
    }

    fn code_block_fg(self) -> Color {
        if self.dim {
            Color::DarkGray
        } else {
            Color::Cyan
        }
    }

    fn heading_fg(self, level: HeadingLevel) -> Color {
        if self.dim {
            Color::DarkGray
        } else {
            match level {
                HeadingLevel::H1 => Color::Yellow,
                HeadingLevel::H2 => Color::Cyan,
                HeadingLevel::H3 => Color::Green,
                _ => Color::White,
            }
        }
    }

    fn link_fg(self) -> Color {
        if self.dim {
            Color::DarkGray
        } else {
            Color::Blue
        }
    }

    fn base_style(self) -> Style {
        Style::default().fg(self.text_fg())
    }
}

// ---------------------------------------------------------------------------
// pulldown-cmark event → ratatui Text adapter
// ---------------------------------------------------------------------------

struct StyleFrame {
    fg: Option<Color>,
    modifiers: Modifier,
}

struct MarkdownRenderer {
    theme: Theme,
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    buf: String,
    style_stack: Vec<StyleFrame>,
    in_code_block: bool,
    code_block_buf: String,
    ordered_list_index: Option<u64>,
}

impl MarkdownRenderer {
    fn new(dim: bool) -> Self {
        Self {
            theme: Theme { dim },
            lines: Vec::new(),
            spans: Vec::new(),
            buf: String::new(),
            style_stack: Vec::new(),
            in_code_block: false,
            code_block_buf: String::new(),
            ordered_list_index: None,
        }
    }

    fn render(mut self, source: &str) -> Text<'static> {
        let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
        let parser = Parser::new_ext(source, opts);

        for event in parser {
            self.handle_event(event);
        }

        if self.in_code_block {
            self.finish_code_block();
        }
        self.flush_buf();
        if !self.spans.is_empty() {
            self.flush_line();
        }

        Text::from(self.lines)
    }

    fn handle_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.on_start(tag),
            Event::End(tag_end) => self.on_end(tag_end),
            Event::Text(text) => self.on_text(text.into_string()),
            Event::Code(text) => self.on_inline_code(text.into_string()),
            Event::Html(html) => self.on_text(html.into_string()),
            Event::SoftBreak => self.buf.push(' '),
            Event::HardBreak => {
                self.flush_buf();
                self.flush_line();
            }
            Event::Rule => {
                self.flush_buf();
                self.flush_line();
            }
            Event::FootnoteReference(_) | Event::TaskListMarker(_) => {}
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                self.on_inline_code(text.into_string());
            }
            Event::InlineHtml(html) => self.on_text(html.into_string()),
        }
    }

    fn on_start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.style_stack.push(StyleFrame {
                    fg: Some(self.theme.heading_fg(level)),
                    modifiers: Modifier::BOLD,
                });
            }
            Tag::BlockQuote(_) => {
                self.buf.push_str("│ ");
            }
            Tag::CodeBlock(kind) => {
                self.flush_buf();
                self.in_code_block = true;
                self.code_block_buf.clear();
                if let CodeBlockKind::Fenced(lang) = kind
                    && !lang.is_empty()
                {
                    self.code_block_buf
                        .push_str(&format!("// {lang}\n"));
                }
            }
            Tag::List(start) => {
                self.ordered_list_index = start;
            }
            Tag::Item => {
                self.flush_buf();
                if !self.spans.is_empty() {
                    self.flush_line();
                }
                let prefix = match self.ordered_list_index {
                    Some(n) => {
                        let p = format!("{n}. ");
                        self.ordered_list_index = Some(n + 1);
                        p
                    }
                    None => "• ".to_string(),
                };
                self.buf.push_str(&prefix);
            }
            Tag::Emphasis => {
                self.style_stack.push(StyleFrame {
                    fg: None,
                    modifiers: Modifier::ITALIC,
                });
            }
            Tag::Strong => {
                self.style_stack.push(StyleFrame {
                    fg: None,
                    modifiers: Modifier::BOLD,
                });
            }
            Tag::Strikethrough => {
                self.style_stack.push(StyleFrame {
                    fg: None,
                    modifiers: Modifier::CROSSED_OUT,
                });
            }
            Tag::Link { .. } => {
                self.style_stack.push(StyleFrame {
                    fg: Some(self.theme.link_fg()),
                    modifiers: Modifier::UNDERLINED,
                });
            }
            Tag::Image { .. } | Tag::MetadataBlock(_) | Tag::HtmlBlock => {}
            Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
            Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition => {}
            Tag::FootnoteDefinition(_) | Tag::Superscript | Tag::Subscript => {}
        }
    }

    fn on_end(&mut self, tag_end: TagEnd) {
        match tag_end {
            TagEnd::Paragraph => {
                self.flush_buf();
                self.flush_line();
            }
            TagEnd::Heading(_) => {
                self.flush_buf();
                self.flush_line();
                self.style_stack.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.flush_buf();
                self.flush_line();
            }
            TagEnd::CodeBlock => {
                self.finish_code_block();
            }
            TagEnd::List(_) => {
                self.ordered_list_index = None;
            }
            TagEnd::Item => {
                self.flush_buf();
                self.flush_line();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.flush_buf();
                self.style_stack.pop();
            }
            TagEnd::Table
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::TableCell => {}
            TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition => {}
            TagEnd::MetadataBlock(_)
            | TagEnd::HtmlBlock
            | TagEnd::FootnoteDefinition
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Image => {}
        }
    }

    fn on_text(&mut self, text: String) {
        if self.in_code_block {
            self.code_block_buf.push_str(&text);
        } else {
            self.buf.push_str(&text);
        }
    }

    fn on_inline_code(&mut self, code: String) {
        self.flush_buf();
        self.spans.push(Span::styled(
            code,
            Style::default().fg(self.theme.code_fg()),
        ));
    }

    fn finish_code_block(&mut self) {
        if !self.in_code_block {
            return;
        }
        self.in_code_block = false;
        let style = Style::default().fg(self.theme.code_block_fg());
        let content = std::mem::take(&mut self.code_block_buf);
        for line in content.lines() {
            self.lines.push(Line::from(Span::styled(line.to_string(), style)));
        }
        self.code_block_buf.clear();
    }

    fn current_style(&self) -> Style {
        let mut style = self.theme.base_style();
        for frame in &self.style_stack {
            if let Some(fg) = frame.fg {
                style = style.fg(fg);
            }
            style = style.add_modifier(frame.modifiers);
        }
        style
    }

    fn flush_buf(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.buf);
        self.spans.push(Span::styled(text, self.current_style()));
    }

    fn flush_line(&mut self) {
        if self.spans.is_empty() {
            return;
        }
        self.lines
            .push(Line::from(std::mem::take(&mut self.spans)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_levels() {
        let t = parse_markdown("# H1\n## H2\n### H3");
        assert_eq!(t.lines.len(), 3);
        assert_eq!(t.lines[0].spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(t.lines[0].spans[0].content, "H1");
        assert_eq!(t.lines[1].spans[0].style.fg, Some(Color::Cyan));
        assert_eq!(t.lines[2].spans[0].style.fg, Some(Color::Green));
    }

    #[test]
    fn inline_bold_italic_code() {
        let t = parse_markdown("hello **world** and *nice* and `code`");
        let combined: String = t.lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(combined, "hello world and nice and code");
        assert!(
            t.lines[0]
                .spans
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(
            t.lines[0]
                .spans
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::ITALIC))
        );
        assert!(t.lines[0].spans.iter().any(|s| s.style.fg == Some(Color::Green)));
    }

    #[test]
    fn code_block_content() {
        let t = parse_markdown("```\nlet x = 1;\n```");
        assert_eq!(t.lines.len(), 1);
        assert_eq!(t.lines[0].spans[0].content, "let x = 1;");
        assert_eq!(t.lines[0].spans[0].style.fg, Some(Color::Cyan));
    }

    #[test]
    fn dimmed_header_is_dark_gray() {
        let t = parse_markdown_dimmed("# H1");
        assert_eq!(t.lines[0].spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn dimmed_code_block_is_dark_gray() {
        let t = parse_markdown_dimmed("```\nlet x = 1;\n```");
        assert_eq!(t.lines[0].spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn unordered_list() {
        let t = parse_markdown("- one\n- two");
        assert!(t.lines[0].spans[0].content.starts_with("• "));
        assert!(t.lines[1].spans[0].content.starts_with("• "));
    }

    #[test]
    fn blockquote_prefix() {
        let t = parse_markdown("> quoted");
        assert!(t.lines[0].spans[0].content.starts_with("│ "));
    }
}
