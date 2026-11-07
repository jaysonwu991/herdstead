//! Inline Markdown styling for editable shell text. Every source byte stays put.

use std::ops::Range;
use std::sync::OnceLock;

use herdstead_core::Theme;
use regex::Regex;

use crate::{Line, Link, Span};

#[derive(Clone, Copy)]
enum Mark {
    Bold,
    Italic,
    Code,
    Strike,
    Link,
}

struct Decoration {
    range: Range<usize>,
    content: Range<usize>,
    mark: Mark,
    link: Option<Link>,
}

fn patterns() -> &'static [(Regex, Mark)] {
    static PATTERNS: OnceLock<Vec<(Regex, Mark)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            (Regex::new(r"\*\*([^*\n]+)\*\*").unwrap(), Mark::Bold),
            (Regex::new(r"__([^_\n]+)__").unwrap(), Mark::Bold),
            (Regex::new(r"\*([^*\n]+)\*").unwrap(), Mark::Italic),
            (Regex::new(r"_([^_\n]+)_").unwrap(), Mark::Italic),
            (Regex::new(r"`([^`\n]+)`").unwrap(), Mark::Code),
            (Regex::new(r"~~([^~\n]+)~~").unwrap(), Mark::Strike),
            (
                Regex::new(r"\[([^\]\n]+)\]\(([^\s()]+)\)").unwrap(),
                Mark::Link,
            ),
        ]
    })
}

/// Style editable syntax, including Markdown link labels, without removing its
/// delimiters. Existing zsh/terminal styles survive outside decorated ranges.
pub fn decorate(line: &Line, theme: &Theme, links: bool, resolve: &dyn Fn(&str) -> bool) -> Line {
    let source = line.text();
    let mut decorations: Vec<Decoration> = Vec::new();
    for (pattern, mark) in patterns() {
        let mut search = 0;
        while let Some(hit) = pattern.captures_at(&source, search) {
            let whole = hit.get(0).unwrap();
            let body = hit.get(1).unwrap();
            search = if matches!(mark, Mark::Italic) {
                whole.start() + source[whole.start()..].chars().next().unwrap().len_utf8()
            } else {
                whole.end()
            };
            // Emphasis must not turn snake_case identifiers or the inside of
            // **bold** into italic text. Escaped syntax remains literal.
            if source[..whole.start()]
                .chars()
                .rev()
                .take_while(|ch| *ch == '\\')
                .count()
                % 2
                == 1
            {
                continue;
            }
            if matches!(mark, Mark::Italic) {
                let delimiter = source[whole.start()..].chars().next().unwrap();
                let before = source[..whole.start()].chars().next_back();
                let after = source[whole.end()..].chars().next();
                if before.is_some_and(|ch| ch == delimiter || ch.is_alphanumeric())
                    || after.is_some_and(|ch| ch == delimiter || ch.is_alphanumeric())
                {
                    continue;
                }
            }
            if !matches!(mark, Mark::Code)
                && decorations.iter().any(|item| {
                    matches!(item.mark, Mark::Code) && item.range.contains(&whole.start())
                })
            {
                continue;
            }
            let target = if matches!(mark, Mark::Link) && links {
                let target = hit.get(2).unwrap().as_str();
                if ["https://", "http://", "mailto:", "file://"]
                    .iter()
                    .any(|scheme| target.starts_with(scheme))
                {
                    Some(Link::Url(target.to_string()))
                } else if resolve(target) {
                    Some(Link::path(target))
                } else {
                    None
                }
            } else {
                None
            };
            decorations.push(Decoration {
                range: whole.range(),
                content: body.range(),
                mark: *mark,
                link: target,
            });
        }
    }
    let code_ranges: Vec<_> = decorations
        .iter()
        .filter(|item| matches!(item.mark, Mark::Code))
        .map(|item| item.range.clone())
        .collect();
    let mut out = Line::new();
    let mut offset = 0;
    for span in &line.spans {
        for (index, ch) in span.text.char_indices() {
            let at = offset + index;
            let in_code = code_ranges.iter().any(|range| range.contains(&at));
            let mut style = span.style;
            let mut link = span.link.clone();
            for item in &decorations {
                if !item.range.contains(&at) || (in_code && !matches!(item.mark, Mark::Code)) {
                    continue;
                }
                if !item.content.contains(&at) {
                    style = style.fg(theme.muted);
                } else {
                    match item.mark {
                        Mark::Bold => style.bold = true,
                        Mark::Italic => style.italic = true,
                        Mark::Code => {
                            style.fg = Some(theme.inline_code_fg);
                            style.bg = Some(theme.code_bg);
                        }
                        Mark::Strike => style.strikethrough = true,
                        Mark::Link if item.link.is_some() => {
                            style.fg = Some(theme.link);
                            style.underline = true;
                        }
                        Mark::Link => {}
                    }
                }
                if let Some(target) = &item.link {
                    link = Some(target.clone());
                }
            }
            out.push(Span {
                text: ch.to_string(),
                style,
                link,
            });
        }
        offset += span.text.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Style;

    #[test]
    fn editable_markdown_retains_source_width_and_original_styles() {
        let theme = Theme::midnight();
        let source = "printf '%s' **bold** *italic* ~~old~~ `code` 日本語";
        let line = Line::styled(source, Style::plain().fg(theme.success));
        let styled = decorate(&line, &theme, true, &|_| false);
        assert_eq!(styled.text(), source);
        assert_eq!(styled.width(), line.width());
        assert!(styled
            .spans
            .iter()
            .any(|span| span.text == "bold" && span.style.bold));
        assert!(styled
            .spans
            .iter()
            .any(|span| span.text == "italic" && span.style.italic));
        assert!(styled
            .spans
            .iter()
            .any(|span| span.text == "old" && span.style.strikethrough));
        assert!(styled
            .spans
            .iter()
            .any(|span| span.text == "code" && span.style.bg == Some(theme.code_bg)));
        assert_eq!(styled.spans[0].style.fg, Some(theme.success));
    }

    #[test]
    fn markdown_links_keep_editable_syntax_and_safe_targets() {
        let theme = Theme::midnight();
        let source =
            "echo [GitHub](https://github.com) [file](docs/desktop.md) [bad](javascript:alert)";
        let styled = decorate(&Line::plain(source), &theme, true, &|path| {
            path == "docs/desktop.md"
        });
        assert_eq!(styled.text(), source);
        assert!(styled.spans.iter().any(|span| span.text == "GitHub"
            && span.link == Some(Link::Url("https://github.com".into()))));
        assert!(styled
            .spans
            .iter()
            .any(|span| span.link == Some(Link::path("docs/desktop.md"))));
        assert!(!styled.spans.iter().any(|span| span
            .link
            .as_ref()
            .is_some_and(|link| link.target().starts_with("javascript:"))));
        assert!(decorate(&Line::plain(source), &theme, false, &|_| true)
            .spans
            .iter()
            .all(|span| span.link.is_none()));
    }

    #[test]
    fn identifiers_escaped_markers_and_code_contents_stay_literal() {
        let theme = Theme::midnight();
        let source = "snake_case_name \\**literal** `**not bold**`";
        let styled = decorate(&Line::plain(source), &theme, true, &|_| false);
        assert_eq!(styled.text(), source);
        assert!(styled
            .spans
            .iter()
            .all(|span| !span.style.italic && !span.style.bold));
    }
}
