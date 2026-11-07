//! `@path` mentions: parsing them out of a composer line, highlighting their
//! ranges, and completing the one being typed.
//!
//! A reference starts a token with `@`, supports quoted paths and an optional
//! `#L12-L14` location. Callers handle file-system resolution.

use std::ops::Range;

use crate::text;

/// The mention the cursor sits in, ready to be completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtToken {
    pub start: usize,
    /// End of the `@...` token in the input (exclusive).
    pub end: usize,
    /// What the user has typed after `@`, without quotes.
    pub query: String,
    pub quoted: bool,
    /// `@/absolute/path` rather than a path relative to the pane.
    pub absolute: bool,
}

impl AtToken {
    pub fn replace_range(&self) -> Range<usize> {
        self.start..self.end
    }
}

/// The mention token under `cursor`, if the cursor is inside one.
pub fn token(input: &str, cursor: usize) -> Option<AtToken> {
    let cursor = cursor.min(input.len());
    let head = &input[..cursor];
    let mut start = None;
    for (index, ch) in head.char_indices().rev() {
        if ch == '@' {
            let at_word_start = index == 0
                || input[..index]
                    .chars()
                    .next_back()
                    .is_some_and(|previous| previous.is_whitespace() || previous == '(');
            if at_word_start {
                start = Some(index);
                break;
            }
        }
        if ch.is_whitespace() {
            break;
        }
    }
    let start = start?;
    let after = &input[start + 1..];
    let quote = after.chars().next().filter(|ch| *ch == '"' || *ch == '\'');
    if let Some(quote) = quote {
        let body = &input[start + 2..];
        let end = body
            .find(quote)
            .map(|index| start + 2 + index + quote.len_utf8());
        if let Some(end) = end {
            if cursor > end {
                return None;
            }
        }
        return Some(AtToken {
            start,
            end: end.unwrap_or(input.len()),
            query: body.to_string(),
            quoted: true,
            absolute: body.starts_with('/'),
        });
    }
    let end = after
        .find(char::is_whitespace)
        .map(|index| start + 1 + index)
        .unwrap_or(input.len());
    let query = &input[start + 1..end];
    if query.contains('@') {
        return None;
    }
    Some(AtToken {
        start,
        end,
        query: query.to_string(),
        quoted: false,
        absolute: query.starts_with('/'),
    })
}

/// Byte ranges of every mention in a composer line, for highlighting.
pub fn mention_ranges(input: &str) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut offset = 0;
    while offset < input.len() {
        let rest = &input[offset..];
        let Some(index) = rest.find('@') else {
            break;
        };
        let at = offset + index;
        let boundary = at == 0
            || input[..at]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_whitespace() || ch == '(');
        if !boundary {
            offset = at + 1;
            continue;
        }
        let after = &input[at + 1..];
        let quote = after.chars().next().filter(|ch| *ch == '"' || *ch == '\'');
        let end = if let Some(quote) = quote {
            match input[at + 2..].find(quote) {
                Some(index) => at + 2 + index + quote.len_utf8(),
                None => input.len(),
            }
        } else {
            after
                .find(char::is_whitespace)
                .map(|index| at + 1 + index)
                .unwrap_or(input.len())
        };
        let mut end = end;
        // Keep a location suffix attached: `@src/lib.rs#L12-L14`.
        if let Some(fragment) = input[end..].strip_prefix("#L") {
            let take = fragment.find(char::is_whitespace).unwrap_or(fragment.len());
            end += 2 + take;
        }
        if end > at + 1 {
            ranges.push(at..end);
        }
        offset = end.max(at + 1);
    }
    ranges
}

/// Paths a composer line references, in order, cleaned of quotes and
/// locations. Empty for no mentions.
pub fn referenced(input: &str) -> Vec<String> {
    mention_ranges(input)
        .into_iter()
        .map(|range| text::clean_path(&input[range]))
        .filter(|path| !path.is_empty())
        .collect()
}

/// Ranks `entries` (relative paths, folders ending in `/`) against the token.
/// Prefix matches on the last segment win, then path prefixes, then substrings.
pub fn suggestions<S: AsRef<str>>(entries: &[S], token: &AtToken, limit: usize) -> Vec<String> {
    let query = token.query.trim_start_matches("./");
    let query_lower = query.to_ascii_lowercase();
    let last = query.rsplit('/').next().unwrap_or(query);
    let last_lower = last.to_ascii_lowercase();
    let mut ranked: Vec<(u8, String, String)> = Vec::new();
    for entry in entries {
        let value = entry.as_ref();
        let lower = value.to_ascii_lowercase();
        let name = text::basename(&lower).to_string();
        let rank = if !last_lower.is_empty() && name.starts_with(&last_lower) {
            0
        } else if lower.starts_with(&query_lower) {
            1
        } else if !last_lower.is_empty() && name.contains(&last_lower) {
            2
        } else if lower.contains(&query_lower) {
            3
        } else {
            continue;
        };
        ranked.push((rank, name, value.to_string()));
    }
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    ranked.dedup_by(|a, b| a.2 == b.2);
    ranked
        .into_iter()
        .map(|(_, _, value)| value)
        .take(limit)
        .collect()
}

/// Turns a suggestion into the token that replaces the one being typed.
pub fn replacement(entry: &str) -> String {
    if entry.contains(char::is_whitespace) {
        format!("@\"{entry}\"")
    } else {
        format!("@{entry}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_mention_under_the_cursor() {
        let input = "explain @src/main.rs please";
        let at = token(input, 16).unwrap();
        assert_eq!(at.query, "src/main.rs");
        assert_eq!(at.start, 8);
        assert_eq!(&input[at.replace_range()], "@src/main.rs");
        assert!(token(input, 5).is_none());
        assert!(token(input, 40).is_none());
    }

    #[test]
    fn quoted_mentions_keep_spaces_together() {
        let input = "read @\"docs/design notes.md\" now";
        let ranges = mention_ranges(input);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&input[ranges[0].clone()], "@\"docs/design notes.md\"");
        assert_eq!(referenced(input), vec!["docs/design notes.md"]);
    }

    #[test]
    fn locations_are_kept_with_the_mention_but_not_the_path() {
        let input = "look at @src/lib.rs#L12-L14";
        let ranges = mention_ranges(input);
        assert_eq!(&input[ranges[0].clone()], "@src/lib.rs#L12-L14");
        assert_eq!(referenced(input), vec!["src/lib.rs"]);
    }

    #[test]
    fn only_word_start_at_signs_count() {
        let ranges = mention_ranges("mail me at user@example.com or @README.md");
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            referenced("mail me at user@example.com or @README.md"),
            vec!["README.md"]
        );
    }

    #[test]
    fn suggestions_rank_names_before_substrings() {
        let entries = vec![
            "src/main.rs",
            "src/markdown.rs",
            "docs/marks.md",
            "Cargo.toml",
        ];
        let at = token("@ma", 3).unwrap();
        assert_eq!(
            suggestions(&entries, &at, 3),
            vec!["src/main.rs", "src/markdown.rs", "docs/marks.md"]
        );
        let at = token("@src/", 5).unwrap();
        assert_eq!(
            suggestions(&entries, &at, 5),
            vec!["src/main.rs", "src/markdown.rs"]
        );
    }

    #[test]
    fn replacements_quote_spaces() {
        assert_eq!(replacement("src/main.rs"), "@src/main.rs");
        assert_eq!(replacement("a b/c.md"), "@\"a b/c.md\"");
    }
}
