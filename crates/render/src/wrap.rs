//! Word wrapping that keeps styles and links on the characters they belong to.

use crate::style::{Line, Link, Span, Style};
use herdr_plus_core::text::char_width;

const NO_LINK: u32 = u32::MAX;

#[derive(Debug, Clone, Copy)]
struct Cell {
    ch: char,
    style: Style,
    link: u32,
}

struct Cells {
    cells: Vec<Cell>,
    links: Vec<Link>,
}

impl Cells {
    fn from_spans(spans: &[Span]) -> Self {
        let mut links: Vec<Link> = Vec::new();
        let mut cells = Vec::new();
        for span in spans {
            let index = span.link.as_ref().map_or(NO_LINK, |link| {
                match links.iter().position(|existing| existing == link) {
                    Some(index) => index as u32,
                    None => {
                        links.push(link.clone());
                        (links.len() - 1) as u32
                    }
                }
            });
            for ch in span.text.chars() {
                cells.push(Cell {
                    ch,
                    style: span.style,
                    link: index,
                });
            }
        }
        Self { cells, links }
    }

    fn line(&self, cells: impl IntoIterator<Item = Cell>) -> Line {
        let mut line = Line::new();
        for cell in cells {
            let link = if cell.link == NO_LINK {
                None
            } else {
                self.links.get(cell.link as usize).cloned()
            };
            line.push(Span {
                text: cell.ch.to_string(),
                style: cell.style,
                link,
            });
        }
        line
    }
}

/// Wraps spans to `width` columns, breaking on spaces and hard-breaking words
/// that cannot fit. Existing newlines start a new line.
pub fn wrap_spans(spans: &[Span], width: usize) -> Vec<Line> {
    let width = width.max(1);
    let cells = Cells::from_spans(spans);
    let mut out = Vec::new();
    let mut current: Vec<Cell> = Vec::new();
    let mut column = 0usize;

    let flush = |current: &mut Vec<Cell>, out: &mut Vec<Line>, cells: &Cells| {
        while current
            .last()
            .is_some_and(|cell| cell.ch == ' ' || cell.ch == '\t')
        {
            current.pop();
        }
        out.push(cells.line(std::mem::take(current)));
    };

    for cell in cells.cells.iter().copied() {
        if cell.ch == '\n' {
            flush(&mut current, &mut out, &cells);
            column = 0;
            continue;
        }
        let ch_width = char_width(cell.ch);
        if ch_width == 0 {
            // Combining marks add no width and merge into the previous span.
            current.push(cell);
            continue;
        }
        if column + ch_width > width {
            if let Some(index) = current.iter().rposition(|cell| cell.ch == ' ') {
                let remainder: Vec<Cell> = current.split_off(index + 1);
                flush(&mut current, &mut out, &cells);
                column = remainder.iter().map(|cell| char_width(cell.ch)).sum();
                current = remainder;
                while current.first().is_some_and(|cell| cell.ch == ' ') {
                    column -= char_width(' ');
                    current.remove(0);
                }
                if column + ch_width > width {
                    flush(&mut current, &mut out, &cells);
                    column = 0;
                }
            } else {
                flush(&mut current, &mut out, &cells);
                column = 0;
            }
        }
        if column == 0 && cell.ch == ' ' {
            continue;
        }
        current.push(cell);
        column += ch_width;
    }
    if !current.is_empty() || out.is_empty() {
        flush(&mut current, &mut out, &cells);
    }
    out
}

/// Wraps a single-style paragraph.
pub fn wrap_text(text: &str, width: usize) -> Vec<Line> {
    wrap_spans(&[Span::plain(text)], width)
}

/// Indents every line by `indent` columns, adding a styled prefix per line.
/// `first` is used on the first line only, which keeps a bullet or a speaker
/// label sharing the line with the text.
pub fn indent_lines(
    lines: Vec<Line>,
    width: usize,
    first: Option<Vec<Span>>,
    rest: Option<Vec<Span>>,
) -> Vec<Line> {
    let mut out = Vec::new();
    for (index, mut line) in lines.into_iter().enumerate() {
        let prefix = if index == 0 {
            first.as_ref()
        } else {
            rest.as_ref()
        };
        let prefix_width: usize = prefix
            .map(|spans| spans.iter().map(Span::width).sum())
            .unwrap_or(0);
        let mut result = Line::new();
        if let Some(spans) = prefix {
            result.extend(spans.iter().cloned());
        }
        result.truncate(width, false);
        // Re-wrap the body against what the prefix left of the row.
        if index == 0 && prefix_width > 0 {
            let available = width.saturating_sub(prefix_width);
            if line.width() > available {
                let wrapped = wrap_spans(&line.spans, available);
                if let Some(head) = wrapped.first() {
                    for span in &head.spans {
                        result.push(span.clone());
                    }
                }
                out.push(result);
                for extra in wrapped.into_iter().skip(1) {
                    let mut row = Line::new();
                    if let Some(spans) = rest.as_ref() {
                        row.extend(spans.iter().cloned());
                    }
                    for span in &extra.spans {
                        row.push(span.clone());
                    }
                    out.push(row);
                }
                continue;
            }
        }
        for span in std::mem::take(&mut line.spans) {
            result.push(span);
        }
        out.push(result);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdr_plus_core::theme::Rgb;

    fn link_line() -> Line {
        let mut line = Line::plain("alpha beta ");
        line.push(
            Span::new(
                "https://example.com",
                Style::plain().underline().fg(Rgb::new(1, 2, 3)),
            )
            .linked(Link::Url("https://example.com".into())),
        );
        line
    }

    #[test]
    fn wraps_on_spaces_and_keeps_every_word() {
        let lines = wrap_text("the quick brown fox jumps", 10);
        assert_eq!(
            lines.iter().map(Line::text).collect::<Vec<_>>(),
            vec!["the quick", "brown fox", "jumps"]
        );
    }

    #[test]
    fn hard_breaks_a_long_word() {
        let lines = wrap_text("abcdefghijklmno", 6);
        assert_eq!(
            lines.iter().map(Line::text).collect::<Vec<_>>(),
            vec!["abcdef", "ghijkl", "mno"]
        );
    }

    #[test]
    fn existing_newlines_and_empty_lines_are_preserved() {
        let lines = wrap_text("a\n\nb", 20);
        assert_eq!(
            lines.iter().map(Line::text).collect::<Vec<_>>(),
            vec!["a", "", "b"]
        );
    }

    #[test]
    fn links_survive_wrapping_at_the_right_columns() {
        let line = link_line();
        let wrapped = wrap_spans(&line.spans, 12);
        assert!(wrapped.len() >= 2);
        let mut linked_columns = Vec::new();
        for row in &wrapped {
            assert!(row.width() <= 12, "{:?}", row.text());
            for (link, start, _end) in row.links() {
                assert_eq!(link, Link::Url("https://example.com".into()));
                linked_columns.push(start);
            }
        }
        assert!(!linked_columns.is_empty());
        // Every linked column really holds the start of the URL text.
        let linked: String = wrapped
            .iter()
            .flat_map(|row| {
                row.links().into_iter().map(move |(_, start, end)| {
                    row.text()
                        .chars()
                        .skip(start)
                        .take(end - start)
                        .collect::<String>()
                })
            })
            .collect();
        assert_eq!(linked, "https://example.com");
    }

    #[test]
    fn wide_characters_do_not_overflow() {
        let lines = wrap_text("日本語です", 4);
        for line in &lines {
            assert!(line.width() <= 4, "{:?}", line.text());
        }
    }

    #[test]
    fn indent_lines_shares_the_first_row_with_a_prefix() {
        let lines = wrap_text("one two three four", 12);
        let out = indent_lines(
            lines,
            12,
            Some(vec![Span::plain("• ")]),
            Some(vec![Span::plain("  ")]),
        );
        assert_eq!(out[0].text(), "• one two");
        assert!(out.iter().all(|line| line.width() <= 12));
        assert_eq!(out.last().unwrap().text(), "  three four");
    }
}
