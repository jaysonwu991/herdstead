//! Styled text primitives shared by every renderer.
//!
//! A [`Line`] is a sequence of [`Span`]s; a span carries an optional link so a
//! click can be answered from the characters under the pointer, which is how
//! URLs and `@path` references stay clickable after wrapping.

use herdstead_core::theme::Rgb;
use herdstead_core::Theme;

/// Where a link points.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Link {
    Url(String),
    Path {
        path: String,
        line: Option<u32>,
        column: Option<u32>,
    },
}

impl Link {
    pub fn path(path: impl Into<String>) -> Self {
        Self::Path {
            path: path.into(),
            line: None,
            column: None,
        }
    }

    pub fn target(&self) -> &str {
        match self {
            Self::Url(url) => url,
            Self::Path { path, .. } => path,
        }
    }

    pub fn is_url(&self) -> bool {
        matches!(self, Self::Url(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub reversed: bool,
}

impl Style {
    pub const fn plain() -> Self {
        Self {
            fg: None,
            bg: None,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            strikethrough: false,
            reversed: false,
        }
    }

    pub fn fg(mut self, color: Rgb) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn bg(mut self, color: Rgb) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    pub fn reversed(mut self) -> Self {
        self.reversed = true;
        self
    }

    /// Resolves unset colours from the theme so ANSI output is complete.
    pub fn resolved(self, theme: &Theme) -> Self {
        Self {
            fg: self.fg.or(Some(theme.fg)),
            bg: self.bg.or(Some(theme.bg)),
            ..self
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub link: Option<Link>,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            link: None,
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self::new(text, Style::plain())
    }

    pub fn linked(mut self, link: Link) -> Self {
        self.link = Some(link);
        self
    }

    pub fn width(&self) -> usize {
        herdstead_core::text::display_width(&self.text)
    }

    /// Same text and link, new style.
    pub fn restyled(&self, style: Style) -> Self {
        Self {
            text: self.text.clone(),
            style,
            link: self.link.clone(),
        }
    }
}

/// One rendered row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    pub fn new() -> Self {
        Self { spans: Vec::new() }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        if text.is_empty() {
            return Self::new();
        }
        Self {
            spans: vec![Span::plain(text)],
        }
    }

    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        let text = text.into();
        if text.is_empty() {
            return Self::new();
        }
        Self {
            spans: vec![Span::new(text, style)],
        }
    }

    pub fn push(&mut self, span: Span) {
        if span.text.is_empty() {
            return;
        }
        // Merge neighbours that share style and link, which keeps frames small.
        if let Some(last) = self.spans.last_mut() {
            if last.style == span.style && last.link == span.link {
                last.text.push_str(&span.text);
                return;
            }
        }
        self.spans.push(span);
    }

    pub fn extend(&mut self, spans: impl IntoIterator<Item = Span>) {
        for span in spans {
            self.push(span);
        }
    }

    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(Span::width).sum()
    }

    pub fn is_blank(&self) -> bool {
        self.spans.iter().all(|span| span.text.trim().is_empty())
    }

    /// Links in the order they appear, with the column they start at.
    pub fn links(&self) -> Vec<(Link, usize, usize)> {
        let mut out = Vec::new();
        let mut column = 0;
        for span in &self.spans {
            let width = span.width();
            if let Some(link) = &span.link {
                match out.last_mut() {
                    Some((last_link, _, last_end)) if last_link == link && *last_end == column => {
                        *last_end += width;
                    }
                    _ => out.push((link.clone(), column, column + width)),
                }
            }
            column += width;
        }
        out
    }

    /// The link covering `column`, used for click handling.
    pub fn link_at(&self, column: usize) -> Option<Link> {
        self.links()
            .into_iter()
            .find(|(_, start, end)| column >= *start && column < *end)
            .map(|(link, _, _)| link)
    }

    /// Pads with spaces to `width` columns.
    pub fn padded(&mut self, width: usize, style: Style) {
        let used = self.width();
        if used < width {
            self.push(Span::new(" ".repeat(width - used), style));
        }
    }

    /// Truncates the line to `width` columns without splitting a wide glyph.
    pub fn truncate(&mut self, width: usize, ellipsis: bool) {
        if self.width() <= width {
            return;
        }
        let limit = if ellipsis {
            width.saturating_sub(1)
        } else {
            width
        };
        let mut used = 0;
        let mut spans = Vec::new();
        for span in std::mem::take(&mut self.spans) {
            if used >= limit {
                break;
            }
            let mut text = String::new();
            for ch in span.text.chars() {
                let ch_width = herdstead_core::text::char_width(ch);
                if used + ch_width > limit {
                    break;
                }
                text.push(ch);
                used += ch_width;
            }
            if !text.is_empty() {
                spans.push(Span {
                    text,
                    style: span.style,
                    link: span.link,
                });
            }
        }
        self.spans = spans;
        if ellipsis {
            self.push(Span::plain("…"));
        }
    }

    /// Applies a style to all spans (often a background).
    pub fn map_style(&mut self, f: impl Fn(Style) -> Style) {
        for span in &mut self.spans {
            span.style = f(span.style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_merge_when_style_and_link_match() {
        let mut line = Line::new();
        line.push(Span::plain("ab"));
        line.push(Span::plain("cd"));
        line.push(Span::new("ef", Style::plain().bold()));
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.text(), "abcdef");
        assert_eq!(line.width(), 6);
    }

    #[test]
    fn links_report_columns_across_spans() {
        let mut line = Line::plain("see ");
        line.push(
            Span::new("docs", Style::plain().underline()).linked(Link::Url("https://x.dev".into())),
        );
        let links = line.links();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].1, 4);
        assert_eq!(links[0].2, 8);
        assert_eq!(line.link_at(4), Some(Link::Url("https://x.dev".into())));
        assert_eq!(line.link_at(8), None);
    }

    #[test]
    fn truncation_counts_wide_characters() {
        let mut line = Line::plain("日本語です");
        line.truncate(5, true);
        assert_eq!(line.text(), "日本…");
        let mut line = Line::plain("abcdef");
        line.truncate(4, false);
        assert_eq!(line.text(), "abcd");
    }

    #[test]
    fn padding_adds_a_single_span() {
        let mut line = Line::plain("ab");
        line.padded(5, Style::plain());
        assert_eq!(line.width(), 5);
        assert_eq!(line.text(), "ab   ");
    }
}
