use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

/// Parse a markdown string into ratatui [`Text`] with basic styling.
pub fn parse_markdown(text: &str) -> Text<'static> {
    parse_markdown_inner(text, false)
}

/// Same as [`parse_markdown`] but renders all text in `DarkGray` — used for
/// thinking/internal-monologue sections to visually distinguish them.
pub fn parse_markdown_dimmed(text: &str) -> Text<'static> {
    parse_markdown_inner(text, true)
}

fn parse_markdown_inner(text: &str, dim: bool) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;

    for raw_line in text.lines() {
        if raw_line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default().fg(Color::DarkGray),
            )));
            continue;
        }

        if in_code_block {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default().fg(if dim { Color::DarkGray } else { Color::Cyan }),
            )));
            continue;
        }

        if let Some(rest) = raw_line.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(
                rest.to_string(),
                Style::default()
                    .fg(if dim { Color::DarkGray } else { Color::Green })
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = raw_line.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                rest.to_string(),
                Style::default()
                    .fg(if dim { Color::DarkGray } else { Color::Cyan })
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = raw_line.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                rest.to_string(),
                Style::default()
                    .fg(if dim { Color::DarkGray } else { Color::Yellow })
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }

        lines.push(parse_inline(raw_line, dim));
    }

    Text::from(lines)
}

fn parse_inline(line: &str, dim: bool) -> Line<'static> {
    let fg = if dim { Color::DarkGray } else { Color::White };
    let code_fg = if dim { Color::DarkGray } else { Color::Green };

    let mut spans: Vec<Span<'static>> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut buf = String::new();

    while i < len {
        // Bold: **...**
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            let start = i + 2;
            if let Some(end) = find_marker(&chars, start, "**") {
                flush_buf(&mut buf, &mut spans, Style::default().fg(fg));
                let content: String = chars[start..end].iter().collect();
                spans.push(Span::styled(
                    content,
                    Style::default().fg(fg).add_modifier(Modifier::BOLD),
                ));
                i = end + 2;
                continue;
            }
        }

        // Italic: *...*
        if chars[i] == '*' {
            let start = i + 1;
            if let Some(end) = find_marker(&chars, start, "*") {
                flush_buf(&mut buf, &mut spans, Style::default().fg(fg));
                let content: String = chars[start..end].iter().collect();
                spans.push(Span::styled(
                    content,
                    Style::default().fg(fg).add_modifier(Modifier::ITALIC),
                ));
                i = end + 1;
                continue;
            }
        }

        // Inline code: `...`
        if chars[i] == '`' {
            let start = i + 1;
            if let Some(end) = find_marker(&chars, start, "`") {
                flush_buf(&mut buf, &mut spans, Style::default().fg(fg));
                let content: String = chars[start..end].iter().collect();
                spans.push(Span::styled(content, Style::default().fg(code_fg)));
                i = end + 1;
                continue;
            }
        }

        buf.push(chars[i]);
        i += 1;
    }

    flush_buf(&mut buf, &mut spans, Style::default().fg(fg));
    Line::from(spans)
}

fn find_marker(chars: &[char], start: usize, marker: &str) -> Option<usize> {
    let marker_chars: Vec<char> = marker.chars().collect();
    let mlen = marker_chars.len();
    if mlen == 0 || start >= chars.len() {
        return None;
    }
    for i in start..=(chars.len().saturating_sub(mlen)) {
        if chars[i..i + mlen] == marker_chars[..] {
            return Some(i);
        }
    }
    None
}

fn flush_buf(buf: &mut String, spans: &mut Vec<Span<'static>>, style: Style) {
    if !buf.is_empty() {
        spans.push(Span::styled(buf.clone(), style));
        buf.clear();
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
    }

    #[test]
    fn inline_bold_italic_code() {
        let t = parse_markdown("hello **world** and *nice* and `code`");
        let combined: String = t.lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(combined, "hello world and nice and code");
    }

    #[test]
    fn code_block_toggle() {
        let t = parse_markdown("```\nlet x = 1;\n```");
        assert_eq!(t.lines.len(), 3);
        assert_eq!(t.lines[1].spans[0].style.fg, Some(Color::Cyan));
    }

    #[test]
    fn dimmed_header_is_dark_gray() {
        let t = parse_markdown_dimmed("# H1");
        assert_eq!(t.lines[0].spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn dimmed_code_block_is_dark_gray() {
        let t = parse_markdown_dimmed("```\nlet x = 1;\n```");
        assert_eq!(t.lines[1].spans[0].style.fg, Some(Color::DarkGray));
    }
}
