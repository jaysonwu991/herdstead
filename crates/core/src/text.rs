//! Text utilities: link and path detection, width handling, truncation.

use std::ops::Range;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// A URL found in a line, with the byte range it occupies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlHit {
    pub span: Range<usize>,
    pub url: String,
}

/// A file-system-looking token found in a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathHit {
    pub span: Range<usize>,
    /// The path without any `:12:3` / `#L12` location suffix.
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

const SCHEMES: [&str; 4] = ["https://", "http://", "file://", "mailto:"];

/// Byte offsets of the URLs in `line`. Trailing sentence punctuation stays with
/// the surrounding text, so a link ends at the URL.
pub fn find_urls(line: &str) -> Vec<UrlHit> {
    let mut hits = Vec::new();
    let mut offset = 0;
    while offset < line.len() {
        let rest = &line[offset..];
        let scheme = SCHEMES.iter().find(|scheme| rest.starts_with(**scheme));
        let is_www = rest.starts_with("www.") && starts_word(&line[..offset]);
        if scheme.is_none() && !is_www {
            offset += rest.chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let mut end = offset;
        for (index, ch) in rest.char_indices() {
            if ch.is_whitespace() || ch == '"' || ch == '<' || ch == '>' || ch == '`' {
                break;
            }
            end = offset + index + ch.len_utf8();
        }
        let raw = &line[offset..end];
        let trimmed = trim_url(raw);
        if trimmed.len() > 4 && !trimmed.ends_with("://") {
            let url = if is_www {
                format!("https://{trimmed}")
            } else {
                trimmed.to_string()
            };
            hits.push(UrlHit {
                span: offset..offset + trimmed.len(),
                url,
            });
        }
        offset = end.max(offset + 1);
    }
    hits
}

fn starts_word(prefix: &str) -> bool {
    prefix
        .chars()
        .next_back()
        .is_none_or(|ch| ch.is_whitespace() || "([{'\"<".contains(ch))
}

fn trim_url(url: &str) -> &str {
    let mut end = url.len();
    while let Some(last) = url[..end].chars().next_back() {
        let trim = matches!(
            last,
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '*' | '_' | '}' | ']' | '>'
        );
        let unbalanced =
            last == ')' && url[..end].matches('(').count() < url[..end].matches(')').count();
        if trim || unbalanced {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    &url[..end]
}

/// True when `text` is a URL (used for Markdown link targets).
pub fn is_url(text: &str) -> bool {
    let text = text.trim();
    SCHEMES.iter().any(|scheme| text.starts_with(scheme)) || text.starts_with("www.")
}

/// File extensions worth treating as a path when the token has no `/`.
const PATH_EXTENSIONS: &[&str] = &[
    "rs", "toml", "json", "jsonl", "yaml", "yml", "md", "markdown", "txt", "lock", "sh", "bash",
    "zsh", "fish", "py", "ts", "tsx", "js", "jsx", "mjs", "cjs", "css", "scss", "html", "htm",
    "go", "java", "kt", "c", "h", "cc", "cpp", "hpp", "rb", "php", "swift", "sql", "csv", "env",
    "cfg", "ini", "lua",
];

/// Path-like tokens in `line`. Detection is deliberately liberal — the caller
/// checks the file system before making a candidate clickable.
pub fn find_paths(line: &str) -> Vec<PathHit> {
    let urls = find_urls(line);
    let mut hits = Vec::new();
    let mut offset = 0;
    while offset < line.len() {
        let ch = line[offset..].chars().next().unwrap_or(' ');
        if ch.is_whitespace() || is_delimiter(ch) {
            offset += ch.len_utf8();
            continue;
        }
        let mut end = offset;
        for (index, c) in line[offset..].char_indices() {
            if c.is_whitespace() || is_delimiter(c) {
                break;
            }
            end = offset + index + c.len_utf8();
        }
        let token = &line[offset..end];
        if let Some(hit) = path_of_token(token, offset) {
            let overlaps_url = urls
                .iter()
                .any(|url| hit.span.start < url.span.end && url.span.start < hit.span.end);
            if !overlaps_url {
                hits.push(hit);
            }
        }
        offset = end.max(offset + 1);
    }
    hits
}

fn is_delimiter(ch: char) -> bool {
    matches!(
        ch,
        '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\'' | '`' | ',' | ';' | '=' | '|'
    )
}

fn path_of_token(token: &str, offset: usize) -> Option<PathHit> {
    let trimmed = token.trim_end_matches(['.', ',', ';', ':', '!', '?']);
    if trimmed.len() < 3 || trimmed.len() > 300 {
        return None;
    }
    let (body, line_no, col_no) = split_location(trimmed);
    if body.is_empty() || body.len() > 260 {
        return None;
    }
    if !looks_like_path(body) {
        return None;
    }
    Some(PathHit {
        span: offset..offset + trimmed.len(),
        path: body.to_string(),
        line: line_no,
        column: col_no,
    })
}

fn looks_like_path(token: &str) -> bool {
    if token.contains("://") || token.contains('=') {
        return false;
    }
    if token.starts_with('-') || token.starts_with('@') {
        return false;
    }
    if !token.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    let has_slash = token.contains('/');
    let prefixed = token.starts_with("./")
        || token.starts_with("../")
        || token.starts_with("~/")
        || token.starts_with('/');
    if has_slash {
        // Reject ratios like `24/7`.
        if token.chars().all(|c| c.is_ascii_digit() || c == '/') {
            return false;
        }
        return prefixed || plausible_slash_token(token);
    }
    match token.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            PATH_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
        }
        _ => false,
    }
}

/// Splits a trailing `:12`, `:12:3` or `#L12-L14` location off a path.
pub fn split_location(value: &str) -> (&str, Option<u32>, Option<u32>) {
    if let Some((path, fragment)) = value.rsplit_once("#L") {
        let split = fragment
            .char_indices()
            .find(|(_, ch)| matches!(ch, '-' | 'C' | 'c'))
            .map(|(index, ch)| (index, ch.len_utf8()));
        let (line_part, column_part) = match split {
            Some((index, width)) => (&fragment[..index], Some(&fragment[index + width..])),
            None => (fragment, None),
        };
        if let Ok(line) = line_part.parse::<u32>() {
            let column = column_part.and_then(|part| part.trim_start_matches('L').parse().ok());
            return (path, Some(line), column);
        }
    }
    // `rsplitn` yields the rightmost field first: column, then line, then path.
    let mut parts = value.rsplitn(3, ':');
    let Some(last) = parts.next() else {
        return (value, None, None);
    };
    let Some(middle) = parts.next() else {
        return (value, None, None);
    };
    match (last.parse::<u32>(), middle.parse::<u32>()) {
        (Ok(column), Ok(line)) => match parts.next() {
            Some(head) => (head, Some(line), Some(column)),
            None => (value, None, None),
        },
        (Ok(line), Err(_)) => (middle, Some(line), None),
        _ => (value, None, None),
    }
}

/// Whether a slash-separated token is a path rather than prose like `and/or`.
fn plausible_slash_token(token: &str) -> bool {
    const COMMON: [&str; 34] = [
        "src",
        "lib",
        "docs",
        "doc",
        "tests",
        "test",
        "crates",
        "crate",
        "app",
        "apps",
        "bin",
        "pkg",
        "packages",
        "scripts",
        "script",
        "assets",
        "config",
        "examples",
        "benches",
        "target",
        "node_modules",
        ".github",
        ".agents",
        ".herdstead",
        "skills",
        "tmp",
        "var",
        "usr",
        "opt",
        "etc",
        "Users",
        "home",
        "editors",
        "shared",
    ];
    let trailing_slash = token.ends_with('/');
    let segments: Vec<&str> = token.split('/').filter(|part| !part.is_empty()).collect();
    if segments.is_empty() {
        return false;
    }
    if segments.len() == 1 && trailing_slash {
        return COMMON.contains(&segments[0])
            || segments[0].contains(['-', '.', '_'])
            || PATH_EXTENSIONS.contains(&segments[0].to_ascii_lowercase().as_str());
    }
    if segments.len() < 2 {
        return false;
    }
    if segments.len() >= 3 {
        return true;
    }
    if segments[0].contains(['-', '.', '_']) {
        return true;
    }
    segments.iter().any(|segment| {
        COMMON.contains(segment)
            || segment.rsplit_once('.').is_some_and(|(stem, ext)| {
                !stem.is_empty() && PATH_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
            })
    })
}

/// Strips quotes and a location suffix, leaving a usable path.
pub fn clean_path(value: &str) -> String {
    let value = value.trim().trim_start_matches('@');
    let value = value.trim_matches(['"', '\'', '`']);
    split_location(value).0.to_string()
}

pub fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `$HOME` is replaced by `~` for display.
pub fn abbreviate_home(path: &str, home: Option<&str>) -> String {
    match home {
        Some(home) if !home.is_empty() && path.starts_with(home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_string(),
    }
}

pub fn display_width(text: &str) -> usize {
    text.width()
}

pub fn char_width(ch: char) -> usize {
    ch.width().unwrap_or(0)
}

/// Truncates to `width` columns, appending `…` when something was cut.
pub fn truncate(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = char_width(ch);
        if used + w > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// Pads with spaces to `width` columns (never truncates).
pub fn pad(text: &str, width: usize) -> String {
    let mut out = truncate(text, width);
    let used = display_width(&out);
    out.extend(std::iter::repeat_n(' ', width.saturating_sub(used)));
    out
}

/// Wraps plain text on word boundaries. Inline styles are the renderers' job;
/// this is for titles, help text and other single-style strings.
pub fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let word_width = display_width(word);
        if line.is_empty() {
            if word_width > width {
                for chunk in chunk_width(word, width) {
                    lines.push(chunk);
                }
            } else {
                line.push_str(word);
            }
        } else if display_width(&line) + 1 + word_width <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(std::mem::take(&mut line));
            if word_width > width {
                for chunk in chunk_width(word, width) {
                    lines.push(chunk);
                }
            } else {
                line.push_str(word);
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn chunk_width(text: &str, width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = char_width(ch);
        if used + w > width {
            chunks.push(std::mem::take(&mut current));
            used = 0;
        }
        current.push(ch);
        used += w;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_urls_and_keeps_punctuation_out() {
        let hits = find_urls("see https://example.com/a?b=c, and (https://x.dev/p).");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://example.com/a?b=c");
        assert_eq!(hits[0].span, 4..29);
        assert_eq!(
            &"see https://example.com/a?b=c, and (https://x.dev/p)."[hits[0].span.clone()],
            "https://example.com/a?b=c"
        );
        assert_eq!(hits[1].url, "https://x.dev/p");
    }

    #[test]
    fn trims_balanced_and_unbalanced_parens() {
        let hits = find_urls("(https://example.com/a)");
        assert_eq!(hits[0].url, "https://example.com/a");
        let hits = find_urls("https://en.wikipedia.org/wiki/Rust_(language)");
        assert_eq!(hits[0].url, "https://en.wikipedia.org/wiki/Rust_(language)");
    }

    #[test]
    fn www_links_get_a_scheme() {
        let hits = find_urls("go to www.herdr.dev/docs now");
        assert_eq!(hits[0].url, "https://www.herdr.dev/docs");
        assert_eq!(hits[0].span, 6..24);
        assert!(find_urls("awww.example.com").is_empty());
    }

    #[test]
    fn finds_paths_with_locations() {
        let hits = find_paths("error in src/main.rs:42:7 and @README.md, plus ./scripts/run.sh");
        let paths: Vec<&str> = hits.iter().map(|hit| hit.path.as_str()).collect();
        assert_eq!(paths, vec!["src/main.rs", "./scripts/run.sh"]);
        assert_eq!(hits[0].line, Some(42));
        assert_eq!(hits[0].column, Some(7));
    }

    #[test]
    fn ignores_prose_and_flags() {
        for line in [
            "and/or either 24/7 --flag=value",
            "https://example.com/a/b.rs",
            "a.b is not a path",
            "x",
        ] {
            assert!(find_paths(line).is_empty(), "unexpected hit in {line:?}");
        }
        assert_eq!(find_paths("open Cargo.toml").len(), 1);
        assert_eq!(find_paths("ls src/").len(), 1);
    }

    #[test]
    fn hash_locations_are_split_off() {
        let (path, line, column) = split_location("src/lib.rs#L12-L14");
        assert_eq!(path, "src/lib.rs");
        assert_eq!(line, Some(12));
        assert_eq!(column, Some(14));
        assert_eq!(split_location("src/lib.rs").0, "src/lib.rs");
        assert_eq!(split_location("12:30").0, "12:30");
    }

    #[test]
    fn widths_respect_wide_characters() {
        assert_eq!(display_width("ab"), 2);
        assert_eq!(display_width("日本"), 4);
        assert_eq!(truncate("日本語", 4), "日…");
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(pad("ab", 4), "ab  ");
        assert_eq!(
            wrap_words("one two three four", 7),
            vec!["one two", "three", "four"]
        );
    }

    #[test]
    fn home_is_abbreviated() {
        assert_eq!(
            abbreviate_home("/Users/me/proj", Some("/Users/me")),
            "~/proj"
        );
        assert_eq!(abbreviate_home("/tmp/x", Some("/Users/me")), "/tmp/x");
    }
}
