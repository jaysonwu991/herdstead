//! Markdown inline parsing: emphasis, code, links, bare URLs and `@mentions`.

use crate::style::{Link, Span, Style};
use herdstead_core::text::{self, find_urls};
use herdstead_core::Theme;

const MAX_DEPTH: usize = 6;

/// Parses one run of inline Markdown into styled spans.
pub fn parse_inline(input: &str, base: Style, theme: &Theme) -> Vec<Span> {
    let mut out = Vec::new();
    parse_into(input, base, theme, &mut out, 0);
    out
}

/// Plain spans with URL, path and mention links attached but no Markdown.
pub fn linkify_text(input: &str, base: Style, theme: &Theme) -> Vec<Span> {
    let mut hits: Vec<(std::ops::Range<usize>, Link, herdstead_core::theme::Rgb)> = Vec::new();
    for hit in find_urls(input) {
        hits.push((hit.span, Link::Url(hit.url), theme.link));
    }
    for hit in text::find_paths(input) {
        hits.push((
            hit.span,
            Link::Path {
                path: hit.path,
                line: hit.line,
                column: hit.column,
            },
            theme.path,
        ));
    }
    for range in herdstead_core::mention::mention_ranges(input) {
        let cleaned = text::clean_path(&input[range.clone()]);
        let (path, line, column) = text::split_location(&cleaned);
        if path.is_empty() {
            continue;
        }
        hits.push((
            range,
            Link::Path {
                path: path.to_string(),
                line,
                column,
            },
            theme.mention,
        ));
    }
    hits.sort_by_key(|(range, _, _)| range.start);
    let mut out = Vec::new();
    let mut cursor = 0;
    for (range, link, color) in hits {
        if range.start < cursor {
            continue;
        }
        if range.start > cursor {
            out.push(Span::new(&input[cursor..range.start], base));
        }
        out.push(Span {
            text: input[range.clone()].to_string(),
            style: base.fg(color).underline(),
            link: Some(link),
        });
        cursor = range.end;
    }
    if cursor < input.len() {
        out.push(Span::new(&input[cursor..], base));
    }
    if out.is_empty() {
        out.push(Span::new(input, base));
    }
    out
}

fn parse_into(input: &str, base: Style, theme: &Theme, out: &mut Vec<Span>, depth: usize) {
    if depth > MAX_DEPTH {
        out.push(Span::new(input, base));
        return;
    }
    let mut literal = String::new();
    let mut index = 0;
    while index < input.len() {
        let rest = &input[index..];
        let ch = rest.chars().next().unwrap_or(' ');

        // Escapes keep the escaped character literal.
        if ch == '\\' && rest.len() > ch.len_utf8() {
            let next = rest[ch.len_utf8()..].chars().next().unwrap_or(' ');
            if next.is_ascii_punctuation() {
                literal.push(next);
                index += ch.len_utf8() + next.len_utf8();
                continue;
            }
        }

        // `code`
        if ch == '`' {
            if let Some(spans) = take_code(rest, base, theme) {
                flush(&mut literal, base, out);
                out.extend(spans.spans);
                index += spans.consumed;
                continue;
            }
        }

        // ![alt](target)
        if ch == '!' {
            if let Some((consumed, span)) = take_image(rest, base, theme) {
                flush(&mut literal, base, out);
                out.push(span);
                index += consumed;
                continue;
            }
        }

        // [label](target)
        if ch == '[' {
            if let Some((consumed, spans)) = take_link(rest, base, theme) {
                flush(&mut literal, base, out);
                out.extend(spans);
                index += consumed;
                continue;
            }
        }

        // <https://autolink>
        if ch == '<' {
            if let Some((consumed, span)) = take_autolink(rest, base, theme) {
                flush(&mut literal, base, out);
                out.push(span);
                index += consumed;
                continue;
            }
        }

        // **bold** / __bold__
        if rest.starts_with("**") || rest.starts_with("__") {
            let marker = &rest[..2];
            let underscore = marker == "__";
            if let Some(end) = find_closer(&rest[2..], marker, underscore) {
                if end > 0 {
                    flush(&mut literal, base, out);
                    parse_into(&rest[2..2 + end], base.bold(), theme, out, depth + 1);
                    index += 2 + end + 2;
                    continue;
                }
            }
        }

        // ~~strike~~
        if let Some(after) = rest.strip_prefix("~~") {
            if let Some(end) = find_closer(after, "~~", false) {
                if end > 0 {
                    flush(&mut literal, base, out);
                    parse_into(
                        &rest[2..2 + end],
                        base.strikethrough(),
                        theme,
                        out,
                        depth + 1,
                    );
                    index += 2 + end + 2;
                    continue;
                }
            }
        }

        // *italic* / _italic_
        if ch == '*' || ch == '_' {
            let marker = if ch == '*' { "*" } else { "_" };
            // `snake_case` stays literal: `_` opens only at a word boundary.
            let boundary = ch == '*' || at_word_boundary(&input[..index]);
            if boundary && !rest[1..].starts_with(ch) {
                if let Some(end) = find_closer(&rest[1..], marker, ch == '_') {
                    if end > 0 && !rest[1..].starts_with(char::is_whitespace) {
                        flush(&mut literal, base, out);
                        parse_into(&rest[1..1 + end], base.italic(), theme, out, depth + 1);
                        index += 1 + end + 1;
                        continue;
                    }
                }
            }
        }

        // Bare URL
        if ch == 'h' || ch == 'w' {
            if let Some(span) = take_bare_url(rest, base, theme) {
                flush(&mut literal, base, out);
                let consumed = span.text.len();
                out.push(span);
                index += consumed;
                continue;
            }
        }

        // @mention
        if ch == '@' && at_word_boundary(&input[..index]) {
            if let Some(span) = take_mention(rest, base, theme) {
                flush(&mut literal, base, out);
                let consumed = span.text.len();
                out.push(span);
                index += consumed;
                continue;
            }
        }

        literal.push(ch);
        index += ch.len_utf8();
    }
    flush(&mut literal, base, out);
}

fn flush(literal: &mut String, base: Style, out: &mut Vec<Span>) {
    if literal.is_empty() {
        return;
    }
    out.push(Span::new(std::mem::take(literal), base));
}

fn at_word_boundary(prefix: &str) -> bool {
    prefix
        .chars()
        .next_back()
        .is_none_or(|ch| ch.is_whitespace() || "([{\"'".contains(ch))
}

struct Taken {
    spans: Vec<Span>,
    consumed: usize,
}

fn take_code(rest: &str, base: Style, theme: &Theme) -> Option<Taken> {
    let ticks = rest.chars().take_while(|ch| *ch == '`').count();
    let delimiter = "`".repeat(ticks);
    let inner = &rest[ticks..];
    let end = inner.find(&delimiter)?;
    let content = &inner[..end];
    let style = base.fg(theme.inline_code_fg).bg(theme.code_bg);
    let text = if content.len() > 2 && content.starts_with(' ') && content.ends_with(' ') {
        &content[1..content.len() - 1]
    } else {
        content
    };
    Some(Taken {
        spans: vec![Span::new(text, style)],
        consumed: ticks + end + ticks,
    })
}

fn take_link(rest: &str, base: Style, theme: &Theme) -> Option<(usize, Vec<Span>)> {
    let close = rest.find(']')?;
    let after = &rest[close + 1..];
    let target = after.strip_prefix('(')?;
    let target_end = target.find(')')?;
    let label = &rest[1..close];
    let target = target[..target_end].trim();
    let link = if text::is_url(target) {
        Link::Url(target.to_string())
    } else {
        let (path, line, column) = text::split_location(target);
        Link::Path {
            path: path.to_string(),
            line,
            column,
        }
    };
    let style = match &link {
        Link::Url(_) => base.fg(theme.link).underline(),
        Link::Path { .. } => base.fg(theme.path).underline(),
    };
    let mut spans = parse_inline(label, style, theme);
    if spans.is_empty() {
        spans.push(Span::new(label, style));
    }
    let color = match &link {
        Link::Url(_) => theme.link,
        Link::Path { .. } => theme.path,
    };
    for span in &mut spans {
        // The link colour wins over the label's own inline styles.
        span.style = span.style.fg(color).underline();
        span.link = Some(link.clone());
    }
    Some((close + 1 + target_end + 2, spans))
}

/// `![alt](target)` renders as a labelled placeholder that links to the
/// image. Terminals cannot draw the bitmap, but the alt text and the path stay
/// useful — and the target opens like any other link.
fn take_image(rest: &str, base: Style, theme: &Theme) -> Option<(usize, Span)> {
    let after_bang = rest.strip_prefix("![")?;
    let close = after_bang.find(']')?;
    let alt = &after_bang[..close];
    let after = &after_bang[close + 1..];
    let target = after.strip_prefix('(')?;
    let target_end = target.find(')')?;
    let target = target[..target_end].trim();
    let label = if alt.trim().is_empty() {
        text::basename(target).to_string()
    } else {
        alt.trim().to_string()
    };
    let link = if text::is_url(target) {
        Link::Url(target.to_string())
    } else {
        let (path, line, column) = text::split_location(target);
        Link::Path {
            path: path.to_string(),
            line,
            column,
        }
    };
    let style = base.fg(theme.muted).italic();
    let text = format!("▧ {label}");
    Some((
        close + target_end + 5,
        Span {
            text,
            style,
            link: Some(link),
        },
    ))
}

fn take_autolink(rest: &str, base: Style, theme: &Theme) -> Option<(usize, Span)> {
    let close = rest.find('>')?;
    let target = &rest[1..close];
    if !text::is_url(target) {
        return None;
    }
    Some((
        close + 1,
        Span::new(target, base.fg(theme.link).underline()).linked(Link::Url(target.to_string())),
    ))
}

fn take_bare_url(rest: &str, base: Style, theme: &Theme) -> Option<Span> {
    let hit = find_urls(rest).into_iter().next()?;
    if hit.span.start != 0 {
        return None;
    }
    let styled = base.fg(theme.link).underline();
    // A URL ending a sentence keeps the punctuation with the prose.
    let len = hit
        .url
        .trim_end_matches(['.', ',', ';', ':', '!', '?'])
        .len();
    Some(Span::new(&rest[..len], styled).linked(Link::Url(rest[..len].to_string())))
}

fn take_mention(rest: &str, base: Style, theme: &Theme) -> Option<Span> {
    let ranges = herdstead_core::mention::mention_ranges(rest);
    let range = ranges.first()?;
    if range.start != 0 {
        return None;
    }
    let raw = &rest[range.clone()];
    let cleaned = text::clean_path(raw);
    let (path, line, column) = text::split_location(&cleaned);
    if path.is_empty() {
        return None;
    }
    Some(
        Span::new(raw, base.fg(theme.mention).underline()).linked(Link::Path {
            path: path.to_string(),
            line,
            column,
        }),
    )
}

/// Finds the closer for an emphasis marker, skipping empty and space-led runs.
fn find_closer(text: &str, delimiter: &str, underscore: bool) -> Option<usize> {
    let mut search = 0;
    while search <= text.len() {
        let index = text[search..].find(delimiter)? + search;
        let before = &text[..index];
        let after = &text[index + delimiter.len()..];
        let empty = index == 0;
        let space_before = before.ends_with(char::is_whitespace);
        let word_after = underscore && after.chars().next().is_some_and(|ch| ch.is_alphanumeric());
        // `__` also closes only at a boundary, so `snake_case` stays literal.
        let double_underscore = underscore && before.ends_with('_') && !before.ends_with("__");
        if !empty && !space_before && !word_after && !double_underscore {
            return Some(index);
        }
        search = index + delimiter.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdstead_core::theme::Rgb;

    fn theme() -> Theme {
        Theme::midnight()
    }

    fn text_of(spans: &[Span]) -> String {
        spans.iter().map(|span| span.text.as_str()).collect()
    }

    #[test]
    fn emphasis_produces_styled_runs() {
        let spans = parse_inline(
            "a **bold** and *it* plus ~~gone~~",
            Style::plain(),
            &theme(),
        );
        assert_eq!(text_of(&spans), "a bold and it plus gone");
        assert!(spans
            .iter()
            .any(|span| span.text == "bold" && span.style.bold));
        assert!(spans
            .iter()
            .any(|span| span.text == "it" && span.style.italic));
        assert!(spans
            .iter()
            .any(|span| span.text == "gone" && span.style.strikethrough));
    }

    #[test]
    fn underscores_leave_identifiers_alone() {
        let spans = parse_inline("snake_case and _emph_", Style::plain(), &theme());
        assert_eq!(text_of(&spans), "snake_case and emph");
        assert!(spans
            .iter()
            .any(|span| span.text == "emph" && span.style.italic));
    }

    #[test]
    fn code_spans_are_coloured_and_literal() {
        let spans = parse_inline("run `cargo **test**` now", Style::plain(), &theme());
        assert_eq!(text_of(&spans), "run cargo **test** now");
        let code = spans
            .iter()
            .find(|span| span.text.contains("cargo"))
            .unwrap();
        assert_eq!(code.style.fg, Some(theme().inline_code_fg));
        assert_eq!(code.style.bg, Some(theme().code_bg));
    }

    #[test]
    fn links_carry_their_target() {
        let spans = parse_inline(
            "see [the docs](https://herdr.dev/docs) and [main](src/main.rs#L4)",
            Style::plain(),
            &theme(),
        );
        let labels: Vec<&str> = spans
            .iter()
            .filter(|span| span.link.is_some())
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(labels, vec!["the docs", "main"]);
        assert_eq!(
            spans[1].link,
            Some(Link::Url("https://herdr.dev/docs".into()))
        );
        assert_eq!(
            spans[3].link,
            Some(Link::Path {
                path: "src/main.rs".into(),
                line: Some(4),
                column: None
            })
        );
    }

    #[test]
    fn bare_urls_become_links_without_the_sentence_period() {
        let spans = parse_inline("go to https://x.dev/a.", Style::plain(), &theme());
        assert_eq!(text_of(&spans), "go to https://x.dev/a.");
        let link = spans.iter().find(|span| span.link.is_some()).unwrap();
        assert_eq!(link.text, "https://x.dev/a");
        assert_eq!(link.link, Some(Link::Url("https://x.dev/a".into())));
    }

    #[test]
    fn mentions_become_path_links() {
        let spans = parse_inline(
            "read @src/main.rs and @\"docs/a b.md\"",
            Style::plain(),
            &theme(),
        );
        let linked: Vec<&Span> = spans.iter().filter(|span| span.link.is_some()).collect();
        assert_eq!(linked.len(), 2);
        assert_eq!(
            linked[0].link,
            Some(Link::Path {
                path: "src/main.rs".into(),
                line: None,
                column: None
            })
        );
        assert_eq!(linked[1].style.fg, Some(theme().mention));
    }

    #[test]
    fn images_become_labelled_links() {
        let spans = parse_inline("![The logo](assets/logo.svg)", Style::plain(), &theme());
        assert_eq!(text_of(&spans), "▧ The logo");
        assert_eq!(
            spans[0].link,
            Some(Link::Path {
                path: "assets/logo.svg".into(),
                line: None,
                column: None
            })
        );

        let spans = parse_inline("![](https://x.dev/a.png)", Style::plain(), &theme());
        assert_eq!(text_of(&spans), "▧ a.png");
        assert_eq!(spans[0].link, Some(Link::Url("https://x.dev/a.png".into())));

        // A lone exclamation mark is prose.
        let spans = parse_inline("wow! really", Style::plain(), &theme());
        assert_eq!(text_of(&spans), "wow! really");
    }

    #[test]
    fn escaped_markers_stay_literal() {
        let spans = parse_inline(r"\*not italic\*", Style::plain(), &theme());
        assert_eq!(text_of(&spans), "*not italic*");
    }

    #[test]
    fn autolinks_and_angle_text() {
        let spans = parse_inline("<https://x.dev> and <b>", Style::plain(), &theme());
        assert_eq!(text_of(&spans), "https://x.dev and <b>");
        assert_eq!(spans[0].link, Some(Link::Url("https://x.dev".into())));
    }

    #[test]
    fn nested_emphasis_inherits_styles() {
        let spans = parse_inline("**bold with `code`**", Style::plain(), &theme());
        let code = spans.iter().find(|span| span.text == "code").unwrap();
        assert!(code.style.bold, "inline code inside bold keeps bold");
        assert_eq!(code.style.fg, Some(theme().inline_code_fg));
    }

    #[test]
    fn linkify_text_marks_paths_and_urls_without_markdown() {
        let spans = linkify_text(
            "error in src/main.rs:3 and https://x.dev *literal*",
            Style::plain(),
            &theme(),
        );
        assert_eq!(
            text_of(&spans),
            "error in src/main.rs:3 and https://x.dev *literal*"
        );
        assert_eq!(spans.iter().filter(|span| span.link.is_some()).count(), 2);
        assert!(spans.iter().all(|span| !span.style.bold));
        let path = spans.iter().find(|span| span.link.is_some()).unwrap();
        assert_eq!(path.style.fg, Some(theme().path));
        assert_eq!(
            path.link,
            Some(Link::Path {
                path: "src/main.rs".into(),
                line: Some(3),
                column: None
            })
        );
    }

    #[test]
    fn unused_rgb_helper_is_available_to_renderers() {
        // Keeps the Rgb import meaningful for downstream modules.
        let _ = Rgb::new(1, 2, 3);
    }
}
