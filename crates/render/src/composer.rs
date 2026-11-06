//! The composer: a prompt line that renders inline Markdown, links, `@paths`
//! and a live Markdown preview, the way Oxide's composer does.

use herdstead_core::mention;
use herdstead_core::text;
use herdstead_core::Theme;

use crate::inline::parse_inline;
use crate::link::mark_mention;
use crate::markdown;
use crate::style::{Line, Span, Style};

/// A composer can be drawn as plain styled text or as a Markdown preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComposerMode {
    #[default]
    Edit,
    Preview,
}

/// Highlights one composer line: Markdown inline styles, URLs and `@mentions`.
/// `resolve` reports whether a referenced path exists, so a typo reads as an
/// error instead of a reference.
pub fn decorate(line: &str, theme: &Theme, resolve: &dyn Fn(&str) -> bool) -> Vec<Span> {
    let mut spans = parse_inline(line, Style::plain(), theme);
    for path in mention::referenced(line) {
        mark_mention(&mut spans, &path, resolve(&path), theme);
    }
    spans
}

/// Wraps a composer's text to `width` columns, marking the cursor's row and
/// column so the caller can place the caret.
pub fn layout(
    text: &str,
    cursor: usize,
    width: usize,
    theme: &Theme,
    resolve: &dyn Fn(&str) -> bool,
) -> (Vec<Line>, usize, usize) {
    let width = width.max(1);
    let cursor = cursor.min(text.len());
    let mut lines: Vec<Line> = Vec::new();
    let mut cursor_row = 0;
    let mut cursor_col = 0;
    let mut offset = 0;
    for source in text.split('\n') {
        let spans = decorate(source, theme, resolve);
        let mut wrapped = crate::wrap::wrap_spans(&spans, width);
        if wrapped.is_empty() {
            wrapped.push(Line::new());
        }
        if cursor >= offset && cursor <= offset + source.len() {
            let target = display_width(&source[..cursor - offset]);
            let mut used = 0;
            let mut row = wrapped.len() - 1;
            for (index, line) in wrapped.iter().enumerate() {
                let line_width = line.width();
                if target <= used + line_width || index + 1 == wrapped.len() {
                    row = index;
                    cursor_col = target.saturating_sub(used).min(line_width);
                    break;
                }
                // A wrapped row drops the space it broke at.
                used += line_width + 1;
            }
            cursor_row = lines.len() + row;
        }
        lines.extend(wrapped);
        offset += source.len() + 1;
    }
    if lines.is_empty() {
        lines.push(Line::new());
    }
    (lines, cursor_row, cursor_col)
}

/// Renders the composer's text as Markdown, for the preview pane.
pub fn preview(text: &str, width: usize, theme: &Theme) -> Vec<Line> {
    if text.trim().is_empty() {
        return vec![Line::styled(
            "Nothing to preview yet — type Markdown above.",
            Style::plain().fg(theme.muted).italic(),
        )];
    }
    markdown::render_width(text, width, theme)
}

fn display_width(text: &str) -> usize {
    text::display_width(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::midnight()
    }

    #[test]
    fn composer_lines_show_markdown_and_references() {
        let spans = decorate(
            "explain **@src/main.rs** and [`docs`](https://x.dev)",
            &theme(),
            &|path: &str| path == "src/main.rs",
        );
        assert!(spans.iter().any(|span| span.style.bold));
        assert!(spans.iter().any(|span| span.style.fg == Some(theme().link)));
        let mention_span = spans
            .iter()
            .find(|span| span.text.starts_with("@src"))
            .unwrap();
        assert_eq!(mention_span.style.fg, Some(theme().mention));
        assert!(mention_span.style.underline);
        assert!(mention_span.style.bold, "bold wraps the mention");
    }

    #[test]
    fn a_missing_path_reads_as_an_error() {
        let spans = decorate("@nope/missing.rs", &theme(), &|_| false);
        assert_eq!(spans[0].style.fg, Some(theme().error));
    }

    #[test]
    fn layout_reports_the_cursor_inside_a_wrapped_composer() {
        let (lines, row, col) = layout("hello world", 11, 80, &theme(), &|_| false);
        assert_eq!(lines.len(), 1);
        assert_eq!(row, 0);
        assert_eq!(col, 11);
        assert_eq!(lines[0].text(), "hello world");
    }

    #[test]
    fn layout_handles_multiple_lines() {
        let text = "first\nsecond line";
        let (lines, row, col) = layout(text, text.len(), 40, &theme(), &|_| false);
        assert_eq!(lines.len(), 2);
        assert_eq!(row, 1);
        assert_eq!(col, "second line".len());
    }

    #[test]
    fn preview_renders_markdown_and_handles_empty_text() {
        let lines = preview("# Hi\n\n- a\n- b", 30, &theme());
        assert!(lines.iter().any(|line| line.text().contains("Hi")));
        let empty = preview("   ", 30, &theme());
        assert!(empty[0].text().contains("Nothing to preview"));
    }
}
