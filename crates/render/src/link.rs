//! Turning plain text into clickable links: URLs, file paths and mentions.

use herdr_plus_core::text;
use herdr_plus_core::Theme;

use crate::style::{Line, Link, Span, Style};

/// Restyles every URL and existing path in `line`, attaching the target.
///
/// `resolve` decides whether a path candidate exists, so a word that merely
/// looks like a path never becomes a link.
pub fn decorate(line: &Line, theme: &Theme, resolve: &dyn Fn(&str) -> bool) -> Line {
    let mut out = Line::new();
    for span in &line.spans {
        for piece in split_span(span, theme, resolve) {
            out.push(piece);
        }
    }
    out
}

fn split_span(span: &Span, theme: &Theme, resolve: &dyn Fn(&str) -> bool) -> Vec<Span> {
    if span.link.is_some() {
        return vec![span.clone()];
    }
    let content = &span.text;
    let mut hits: Vec<(usize, usize, Link)> = text::find_urls(content)
        .into_iter()
        .map(|hit| (hit.span.start, hit.span.end, Link::Url(hit.url)))
        .chain(text::find_paths(content).into_iter().filter_map(|hit| {
            if !resolve(&hit.path) {
                return None;
            }
            Some((
                hit.span.start,
                hit.span.end,
                Link::Path {
                    path: hit.path,
                    line: hit.line,
                    column: hit.column,
                },
            ))
        }))
        .collect();
    if hits.is_empty() {
        return vec![span.clone()];
    }
    hits.sort_by_key(|(start, _, _)| *start);
    let mut out = Vec::new();
    let mut cursor = 0;
    for (start, end, link) in hits {
        if start < cursor {
            continue;
        }
        if start > cursor {
            out.push(Span {
                text: content[cursor..start].to_string(),
                style: span.style,
                link: None,
            });
        }
        let style = match &link {
            Link::Url(_) => span.style.fg(theme.link).underline(),
            Link::Path { .. } => span.style.fg(theme.path).underline(),
        };
        out.push(Span {
            text: content[start..end].to_string(),
            style,
            link: Some(link),
        });
        cursor = end;
    }
    if cursor < content.len() {
        out.push(Span {
            text: content[cursor..].to_string(),
            style: span.style,
            link: None,
        });
    }
    out
}

/// Highlights the mention at `range` in a composer line as existing or not.
pub fn mark_mention(spans: &mut [Span], path: &str, exists: bool, theme: &Theme) {
    let color = if exists { theme.mention } else { theme.error };
    for span in spans {
        if let Some(Link::Path { path: target, .. }) = &span.link {
            if target == path {
                span.style = span.style.fg(color).underline();
            }
        }
    }
}

/// Style for a selected link, used when the pointer hovers one.
pub fn hover_style(style: Style, theme: &Theme) -> Style {
    style.fg(theme.accent_alt).underline().reversed()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::midnight()
    }

    #[test]
    fn urls_become_clickable_and_keep_their_text() {
        let line = Line::styled("see https://x.dev/a for more", Style::plain().bold());
        let decorated = decorate(&line, &theme(), &|_| false);
        assert_eq!(decorated.text(), "see https://x.dev/a for more");
        let links = decorated.links();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, Link::Url("https://x.dev/a".into()));
        assert_eq!(links[0].1, 4);
        assert!(decorated.spans[1].style.bold, "original style is preserved");
        assert!(decorated.spans[1].style.underline);
        assert_eq!(decorated.spans[1].style.fg, Some(theme().link));
    }

    #[test]
    fn paths_only_link_when_the_resolver_accepts_them() {
        let line = Line::plain("open src/main.rs and src/missing.rs");
        let existing = decorate(&line, &theme(), &|path: &str| path == "src/main.rs");
        assert_eq!(existing.links().len(), 1);
        assert_eq!(existing.links()[0].0.target(), "src/main.rs");
        // The accepted span is restyled; the rejected one keeps its colour.
        let missing = existing
            .spans
            .iter()
            .find(|span| span.text.contains("missing"))
            .unwrap();
        assert!(missing.link.is_none());
        assert_eq!(missing.style.fg, None);
    }

    #[test]
    fn mentions_are_coloured_by_existence() {
        let mut spans = crate::inline::parse_inline("@src/a.rs", Style::plain(), &theme());
        mark_mention(&mut spans, "src/a.rs", false, &theme());
        assert_eq!(spans[0].style.fg, Some(theme().error));
        mark_mention(&mut spans, "src/a.rs", true, &theme());
        assert_eq!(spans[0].style.fg, Some(theme().mention));
    }
}
