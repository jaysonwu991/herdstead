//! A Markdown renderer for the terminal: headings, lists, task lists, quotes,
//! tables, rules, fenced code and inline styles, wrapped to a column width.

use herdr_plus_core::text;
use herdr_plus_core::Theme;

use crate::code::{highlight_snippet, CodeOptions};
use crate::inline::parse_inline;
use crate::style::{Line, Span, Style};
use crate::wrap::wrap_spans;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownOptions {
    pub width: usize,
    /// Fenced code longer than this is truncated by the highlighter.
    pub max_code_lines: usize,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            width: 80,
            max_code_lines: 400,
        }
    }
}

impl MarkdownOptions {
    pub fn width(mut self, width: usize) -> Self {
        self.width = width.max(8);
        self
    }
}

/// Renders Markdown to styled rows.
pub fn render(source: &str, options: &MarkdownOptions, theme: &Theme) -> Vec<Line> {
    let mut out = Vec::new();
    let base = Style::plain();
    render_blocks(source, options, theme, base, &mut out, 0, 0);
    while out.last().is_some_and(Line::is_blank) {
        out.pop();
    }
    out
}

/// Convenience wrapper for a fixed width.
pub fn render_width(source: &str, width: usize, theme: &Theme) -> Vec<Line> {
    render(source, &MarkdownOptions::default().width(width), theme)
}

fn render_blocks(
    source: &str,
    options: &MarkdownOptions,
    theme: &Theme,
    base: Style,
    out: &mut Vec<Line>,
    depth: usize,
    list_depth: usize,
) {
    if depth > 8 {
        out.extend(crate::wrap::wrap_spans(
            &[Span::new(source, base)],
            options.width,
        ));
        return;
    }
    let width = options.width.max(8);
    let lines: Vec<&str> = source.split('\n').collect();
    let mut index = 0;
    let mut last_blank = false;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            if !last_blank && !out.is_empty() {
                out.push(Line::new());
            }
            last_blank = true;
            index += 1;
            continue;
        }
        last_blank = false;

        if let Some((fence, language)) = fence_start(line) {
            let mut body = Vec::new();
            index += 1;
            while index < lines.len() && !is_fence_end(lines[index], fence) {
                body.push(lines[index]);
                index += 1;
            }
            index += 1; // closing fence
            render_code(&language, &body, width, options, theme, out);
            continue;
        }

        if let Some((level, heading)) = heading(line) {
            render_heading(level, heading, width, theme, base, out);
            index += 1;
            continue;
        }

        if is_rule(trimmed) {
            out.push(Line::styled(
                "─".repeat(width),
                Style::plain().fg(theme.border),
            ));
            index += 1;
            continue;
        }

        if trimmed.trim_start().starts_with('>') {
            let mut inner = String::new();
            while index < lines.len() {
                let quoted = lines[index].trim_start();
                let Some(rest) = quoted.strip_prefix('>') else {
                    break;
                };
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                inner.push_str(rest);
                inner.push('\n');
                index += 1;
            }
            let mut inner_lines = Vec::new();
            let quote_base = base.fg(theme.quote).italic();
            render_blocks(
                &inner,
                &MarkdownOptions {
                    width: width.saturating_sub(2),
                    ..options.clone()
                },
                theme,
                quote_base,
                &mut inner_lines,
                depth + 1,
                list_depth,
            );
            for line in inner_lines {
                let mut row = Line::new();
                row.push(Span::new("▎ ", Style::plain().fg(theme.quote)));
                row.extend(line.spans);
                row.truncate(width, true);
                out.push(row);
            }
            continue;
        }

        if is_table_start(&lines, index) {
            index = render_table(&lines, index, width, theme, base, out);
            continue;
        }

        if let Some(item) = parse_list_item(line) {
            index = render_list_item(
                &lines, index, item, width, options, theme, base, out, depth, list_depth,
            );
            continue;
        }

        // Paragraph: gather until a blank line or another block starts.
        let mut paragraph = String::new();
        while index < lines.len() {
            let candidate = lines[index];
            if candidate.trim().is_empty()
                || fence_start(candidate).is_some()
                || heading(candidate).is_some()
                || is_rule(candidate.trim_end())
                || candidate.trim_start().starts_with('>')
                || parse_list_item(candidate).is_some()
                || is_table_start(&lines, index)
            {
                break;
            }
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(candidate.trim());
            index += 1;
        }
        let spans = parse_inline(&paragraph, base, theme);
        out.extend(wrap_spans(&spans, width));
    }
}

fn fence_start(line: &str) -> Option<(&str, Option<String>)> {
    let trimmed = line.trim_start();
    for fence in ["```", "~~~"] {
        if let Some(rest) = trimmed.strip_prefix(fence) {
            if rest.starts_with(fence) {
                return None;
            }
            let language = rest
                .split_whitespace()
                .next()
                .filter(|token| !token.is_empty())
                .map(str::to_string);
            return Some((fence, language));
        }
    }
    None
}

fn is_fence_end(line: &str, fence: &str) -> bool {
    line.trim_start().starts_with(fence)
}

fn render_code(
    language: &Option<String>,
    body: &[&str],
    width: usize,
    options: &MarkdownOptions,
    theme: &Theme,
    out: &mut Vec<Line>,
) {
    if let Some(language) = language {
        out.push(Line::styled(
            format!("▸ {language}"),
            Style::plain().fg(theme.muted).dim(),
        ));
    }
    let source = body.join("\n");
    let code_options = CodeOptions {
        width,
        line_numbers: false,
        start_line: 1,
        max_lines: options.max_code_lines,
        background: true,
        rail: true,
    };
    out.extend(highlight_snippet(
        language.as_deref(),
        &source,
        &code_options,
        theme,
    ));
}

fn render_heading(
    level: usize,
    heading: &str,
    width: usize,
    theme: &Theme,
    base: Style,
    out: &mut Vec<Line>,
) {
    let (prefix, style) = match level {
        1 => ("▌ ", base.fg(theme.heading).bold()),
        2 => ("▎ ", base.fg(theme.heading).bold()),
        3 => ("", base.fg(theme.heading).bold()),
        4 => ("", base.fg(theme.fg).bold()),
        _ => ("", base.fg(theme.muted).bold()),
    };
    let prefix_width = text::display_width(prefix);
    let spans = parse_inline(heading, style, theme);
    let mut wrapped = wrap_spans(&spans, width.saturating_sub(prefix_width));
    for (index, line) in wrapped.iter_mut().enumerate() {
        if index == 0 && !prefix.is_empty() {
            line.spans
                .insert(0, Span::new(prefix, Style::plain().fg(theme.heading)));
        }
        out.push(line.clone());
    }
    if level <= 2 && !wrapped.is_empty() {
        out.push(Line::new());
    }
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some((level, rest.trim()))
}

fn is_rule(line: &str) -> bool {
    let line = line.trim();
    if line.len() < 3 {
        return false;
    }
    [('-', "---"), ('*', "***"), ('_', "___")]
        .iter()
        .any(|(ch, _)| {
            line.chars().all(|c| c == *ch || c.is_whitespace()) && line.matches(*ch).count() >= 3
        })
}

struct ListItem {
    indent: usize,
    ordered: bool,
    number: u64,
    content: String,
    task: Option<bool>,
}

fn parse_list_item(line: &str) -> Option<ListItem> {
    let indent = indentation(line);
    let trimmed = line.trim_start();
    let (ordered, number, rest) = if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
    {
        (false, 0, rest)
    } else {
        let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() || digits.len() > 9 {
            return None;
        }
        let after = &trimmed[digits.len()..];
        let rest = after
            .strip_prefix(". ")
            .or_else(|| after.strip_prefix(") "))?;
        (true, digits.parse().unwrap_or(1), rest)
    };
    let (task, content) = if let Some(rest) = rest.strip_prefix("[ ] ") {
        (Some(false), rest)
    } else if let Some(rest) = rest
        .strip_prefix("[x] ")
        .or_else(|| rest.strip_prefix("[X] "))
    {
        (Some(true), rest)
    } else {
        (None, rest)
    };
    Some(ListItem {
        indent,
        ordered,
        number,
        content: content.to_string(),
        task,
    })
}

#[allow(clippy::too_many_arguments)]
fn render_list_item(
    lines: &[&str],
    index: usize,
    item: ListItem,
    width: usize,
    options: &MarkdownOptions,
    theme: &Theme,
    base: Style,
    out: &mut Vec<Line>,
    depth: usize,
    list_depth: usize,
) -> usize {
    let mut cursor = index + 1;
    // Nested content: blank lines and anything indented deeper than this item.
    let mut nested = String::new();
    let mut nested_from = cursor;
    while cursor < lines.len() {
        let candidate = lines[cursor];
        let blank = candidate.trim().is_empty();
        if blank {
            // A blank line only continues the item when more indented content
            // follows; otherwise it ends it.
            let next = lines[cursor + 1..]
                .iter()
                .find(|line| !line.trim().is_empty());
            match next {
                Some(next) if indentation(next) > item.indent => {
                    nested.push('\n');
                    cursor += 1;
                    continue;
                }
                _ => break,
            }
        }
        if indentation(candidate) > item.indent {
            let dedent = (item.indent + 2).min(candidate.len());
            nested.push_str(&candidate[dedent..]);
            nested.push('\n');
            cursor += 1;
            if nested_from == cursor {
                nested_from = cursor;
            }
        } else {
            break;
        }
    }
    let _ = nested_from;

    let bullet_depth = list_depth + item.indent / 2;
    let marker = item_marker(&item, bullet_depth, theme);
    let marker_width: usize = marker.iter().map(Span::width).sum::<usize>() + 1;
    let prefix_width = item.indent + marker_width;
    let span = parse_inline(&item.content, base, theme);
    let wrapped = wrap_spans(&span, width.saturating_sub(prefix_width).max(4));
    for (line_index, line) in wrapped.into_iter().enumerate() {
        let mut row = Line::new();
        row.push(Span::new(" ".repeat(item.indent), base));
        if line_index == 0 {
            row.extend(marker.iter().cloned());
            row.push(Span::new(" ", base));
        } else {
            row.push(Span::new(" ".repeat(marker_width), base));
        }
        row.extend(line.spans);
        row.truncate(width, true);
        out.push(row);
    }

    if !nested.trim().is_empty() {
        let mut nested_lines = Vec::new();
        render_blocks(
            &nested,
            &MarkdownOptions {
                width: width.saturating_sub(prefix_width).max(8),
                ..options.clone()
            },
            theme,
            base,
            &mut nested_lines,
            depth + 1,
            bullet_depth + 1,
        );
        for line in nested_lines {
            let mut row = Line::new();
            row.push(Span::new(" ".repeat(prefix_width), base));
            row.extend(line.spans);
            row.truncate(width, true);
            out.push(row);
        }
    }
    cursor
}

fn item_marker(item: &ListItem, depth: usize, theme: &Theme) -> Vec<Span> {
    if let Some(done) = item.task {
        let (glyph, color) = if done {
            ("☑", theme.success)
        } else {
            ("☐", theme.muted)
        };
        return vec![Span::new(glyph, Style::plain().fg(color))];
    }
    if item.ordered {
        vec![Span::new(
            format!("{}.", item.number),
            Style::plain().fg(theme.number),
        )]
    } else {
        let glyph = match depth % 3 {
            0 => "•",
            1 => "◦",
            _ => "▪",
        };
        vec![Span::new(glyph, Style::plain().fg(theme.bullet))]
    }
}

fn indentation(line: &str) -> usize {
    let mut columns = 0;
    for ch in line.chars() {
        match ch {
            ' ' => columns += 1,
            '\t' => columns += 4,
            _ => break,
        }
    }
    columns
}

fn is_table_start(lines: &[&str], index: usize) -> bool {
    let Some(header) = lines.get(index) else {
        return false;
    };
    if !header.contains('|') {
        return false;
    }
    let Some(separator) = lines.get(index + 1) else {
        return false;
    };
    let trimmed = separator.trim();
    if !trimmed.contains('-') || !trimmed.contains('|') {
        return false;
    }
    trimmed
        .chars()
        .all(|ch| matches!(ch, '|' | '-' | ':' | ' ' | '\t'))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

fn split_row(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('|').unwrap_or(trimmed);
    trimmed
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn render_table(
    lines: &[&str],
    index: usize,
    width: usize,
    theme: &Theme,
    base: Style,
    out: &mut Vec<Line>,
) -> usize {
    let header = split_row(lines[index]);
    let aligns: Vec<Align> = split_row(lines[index + 1])
        .iter()
        .map(|cell| {
            let left = cell.starts_with(':');
            let right = cell.ends_with(':');
            match (left, right) {
                (true, true) => Align::Center,
                (false, true) => Align::Right,
                _ => Align::Left,
            }
        })
        .collect();
    let mut rows: Vec<Vec<String>> = vec![header];
    let mut cursor = index + 2;
    while cursor < lines.len() {
        let line = lines[cursor];
        if line.trim().is_empty() || !line.contains('|') {
            break;
        }
        rows.push(split_row(line));
        cursor += 1;
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return cursor;
    }
    for row in &mut rows {
        row.resize(columns, String::new());
    }
    let available = width.saturating_sub(columns + 1).max(columns * 3);
    let mut widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .map(|row| text::display_width(&row[column]))
                .max()
                .unwrap_or(3)
                .clamp(3, available)
        })
        .collect();
    while widths.iter().sum::<usize>() > available {
        let widest = widths
            .iter()
            .enumerate()
            .max_by_key(|(_, value)| **value)
            .map(|(index, _)| index)
            .unwrap_or(0);
        if widths[widest] <= 3 {
            break;
        }
        widths[widest] -= 1;
    }

    for (row_index, row) in rows.iter().enumerate() {
        let mut line = Line::new();
        line.push(Span::new("│", Style::plain().fg(theme.table_border)));
        for column in 0..columns {
            let cell = &row[column];
            let cell_width = widths[column];
            let style = if row_index == 0 {
                base.fg(theme.fg).bold()
            } else {
                base
            };
            let spans = parse_inline(cell, style, theme);
            let mut content = Line::new();
            content.extend(spans);
            content.truncate(cell_width, true);
            let padding = cell_width.saturating_sub(content.width());
            let (left_pad, right_pad) = match aligns.get(column).copied().unwrap_or(Align::Left) {
                Align::Left => (0, padding),
                Align::Right => (padding, 0),
                Align::Center => (padding / 2, padding - padding / 2),
            };
            line.push(Span::new(" ".repeat(left_pad + 1), style));
            line.extend(content.spans);
            line.push(Span::new(" ".repeat(right_pad + 1), style));
            line.push(Span::new("│", Style::plain().fg(theme.table_border)));
        }
        line.truncate(width, true);
        out.push(line);
        if row_index == 0 {
            let mut rule = Line::new();
            rule.push(Span::new("├", Style::plain().fg(theme.table_border)));
            for (column, cell_width) in widths.iter().enumerate() {
                rule.push(Span::new(
                    "─".repeat(cell_width + 2),
                    Style::plain().fg(theme.table_border),
                ));
                let glyph = if column + 1 == widths.len() {
                    "┤"
                } else {
                    "┼"
                };
                rule.push(Span::new(glyph, Style::plain().fg(theme.table_border)));
            }
            out.push(rule);
        }
    }
    cursor
}

/// A compact one-line preview of Markdown, for titles and composer hints.
pub fn summarize(source: &str, width: usize) -> String {
    let plain = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find(|line| !line.starts_with("```"))
        .unwrap_or("")
        .trim_start_matches('#')
        .trim();
    text::truncate(plain, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::midnight()
    }

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(Line::text).collect()
    }

    #[test]
    fn headings_are_styled_and_labelled() {
        let lines = render_width("# Title\n\n## Sub\n\nbody", 30, &theme());
        let all = texts(&lines);
        assert_eq!(all[0], "▌ Title");
        assert!(all.iter().any(|line| line == "▎ Sub"));
        assert!(all.iter().any(|line| line == "body"));
        let heading = &lines[0];
        assert!(heading.spans.iter().any(|span| span.style.bold));
        assert_eq!(heading.spans[0].style.fg, Some(theme().heading));
    }

    #[test]
    fn bullet_and_ordered_lists_indent_and_wrap() {
        let source = "- alpha beta gamma delta\n- second\n\n1. one\n2. two";
        let lines = render_width(source, 20, &theme());
        let all = texts(&lines);
        assert!(all[0].starts_with("• alpha"), "{:?}", all[0]);
        assert!(
            all.iter().any(|line| line.starts_with("  delta")),
            "{all:?}"
        );
        assert!(all.iter().any(|line| line.starts_with("• second")));
        assert!(all.iter().any(|line| line.starts_with("1. one")));
        for line in &lines {
            assert!(line.width() <= 20, "{:?} too wide", line.text());
        }
    }

    #[test]
    fn task_lists_show_checkboxes() {
        let lines = render_width("- [ ] todo\n- [x] done", 30, &theme());
        let all = texts(&lines);
        assert_eq!(all[0], "☐ todo");
        assert_eq!(all[1], "☑ done");
        assert_eq!(lines[1].spans[0].style.fg, Some(theme().success));
    }

    #[test]
    fn nested_lists_are_indented_recursively() {
        let lines = render_width("- outer\n  - inner\n", 40, &theme());
        let all = texts(&lines);
        assert!(all.iter().any(|line| line.contains("outer")));
        assert!(
            all.iter()
                .any(|line| line.trim_start().starts_with("◦ inner")),
            "{all:?}"
        );
    }

    #[test]
    fn block_quotes_get_a_bar() {
        let lines = render_width("> quoted words\n> more", 30, &theme());
        assert_eq!(lines[0].text(), "▎ quoted words more");
        assert_eq!(lines[0].spans[0].style.fg, Some(theme().quote));
    }

    #[test]
    fn fenced_code_is_highlighted_and_railed() {
        let source = "```rust\nfn main() {}\n```";
        let lines = render_width(source, 30, &theme());
        let all = texts(&lines);
        assert!(all[0].contains("rust"));
        let code = lines
            .iter()
            .find(|line| line.text().contains("fn main"))
            .unwrap();
        assert!(code.text().starts_with("┃"), "{:?}", code.text());
        assert_eq!(code.width(), 30);
        assert!(code
            .spans
            .iter()
            .any(|span| span.style.fg.is_some_and(|c| c != theme().fg)));
    }

    #[test]
    fn tables_align_columns() {
        let source = "| Name | Size |\n| :--- | ---: |\n| main.rs | 12 KB |\n| lib.rs | 1 KB |";
        let lines = render_width(source, 40, &theme());
        let all = texts(&lines);
        assert!(all[0].starts_with("│ Name"), "{:?}", all[0]);
        assert!(all[1].starts_with("├"), "{:?}", all[1]);
        assert_eq!(all.len(), 4);
        for line in &lines {
            assert!(line.width() <= 40, "{:?}", line.text());
        }
        // Right-aligned size column ends with the same border position.
        let ends: Vec<usize> = lines.iter().map(Line::width).collect();
        assert!(ends.iter().all(|width| *width == ends[0]), "{ends:?}");
    }

    #[test]
    fn rules_and_blank_lines_are_collapsed() {
        let lines = render_width("a\n\n\n\nb\n\n---\n", 10, &theme());
        let all = texts(&lines);
        assert_eq!(all[0], "a");
        assert_eq!(all[1], "");
        assert_eq!(all[2], "b");
        assert_eq!(all[3], "");
        assert_eq!(all[4], "──────────");
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn links_and_mentions_keep_their_targets() {
        let lines = render_width(
            "see [docs](https://herdr.dev) and @src/main.rs",
            60,
            &theme(),
        );
        let links: Vec<_> = lines[0].links();
        assert_eq!(links.len(), 2);
        assert_eq!(
            links[0].0,
            crate::style::Link::Url("https://herdr.dev".into())
        );
        assert_eq!(links[1].0.target(), "src/main.rs");
    }

    #[test]
    fn long_paragraphs_wrap_within_the_width() {
        let source = "word ".repeat(60);
        let lines = render_width(&source, 24, &theme());
        assert!(lines.len() > 2);
        for line in &lines {
            assert!(line.width() <= 24, "{:?}", line.text());
        }
        let joined: String = lines.iter().map(Line::text).collect::<Vec<_>>().join(" ");
        assert_eq!(joined.split_whitespace().count(), 60);
    }

    #[test]
    fn summarize_takes_the_first_meaningful_line() {
        assert_eq!(summarize("# Title here\ntext", 20), "Title here");
        assert_eq!(summarize("```sh\nls\n```", 20), "ls");
    }
}
