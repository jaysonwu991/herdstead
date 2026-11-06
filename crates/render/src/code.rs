//! VS Code-style syntax highlighting for files, snippets and fenced blocks.

use std::path::Path;
use std::sync::OnceLock;

use herdstead_core::text;
use herdstead_core::theme::Rgb;
use herdstead_core::Theme;
use syntect::highlighting::{FontStyle, Theme as SyntectTheme};
use syntect::parsing::SyntaxSet;
use two_face::theme::LazyThemeSet;

use crate::style::{Line, Span, Style};

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEMES: OnceLock<LazyThemeSet> = OnceLock::new();

/// bat's syntax set, which covers the languages an editor highlights (TOML,
/// TypeScript, Dockerfile, …) rather than syntect's small default list.
fn syntaxes() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(two_face::syntax::extra_newlines)
}

fn syntect_themes() -> &'static LazyThemeSet {
    THEMES.get_or_init(|| two_face::theme::extra().into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeOptions {
    pub width: usize,
    pub line_numbers: bool,
    pub start_line: usize,
    pub max_lines: usize,
    /// Fill the row with the theme's code background.
    pub background: bool,
    /// Draw a `┃` bar down the left edge.
    pub rail: bool,
}

impl Default for CodeOptions {
    fn default() -> Self {
        Self {
            width: 80,
            line_numbers: true,
            start_line: 1,
            max_lines: 5_000,
            background: true,
            rail: false,
        }
    }
}

impl CodeOptions {
    pub fn width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    pub fn snippet(mut self) -> Self {
        self.line_numbers = false;
        self
    }
}

/// The language name syntect recognises for a path, if any.
pub fn language_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    syntaxes()
        .find_syntax_by_extension(&name)
        .or_else(|| {
            let extension = path.extension()?.to_string_lossy();
            syntaxes()
                .find_syntax_by_extension(&extension)
                .or_else(|| syntaxes().find_syntax_by_token(&extension))
        })
        .map(|syntax| syntax.name.clone())
}

/// Every language syntect can highlight, for `herdstead languages`.
pub fn supported_languages() -> Vec<String> {
    let mut names: Vec<String> = syntaxes()
        .syntaxes()
        .iter()
        .map(|syntax| syntax.name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Highlights a file from disk.
pub fn highlight_file(
    path: &Path,
    options: &CodeOptions,
    theme: &Theme,
) -> Result<Vec<Line>, String> {
    let source =
        std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    if source.contains('\0') {
        return Err(format!("{}: binary file", path.display()));
    }
    let syntax = syntax_for_path(path);
    Ok(highlight_with(syntax, &source, options, theme))
}

/// Highlights a snippet in the named language (a fence label or extension).
pub fn highlight_snippet(
    language: Option<&str>,
    source: &str,
    options: &CodeOptions,
    theme: &Theme,
) -> Vec<Line> {
    let syntax = language
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .and_then(|name| {
            syntaxes()
                .find_syntax_by_token(name)
                .or_else(|| syntaxes().find_syntax_by_extension(name))
        })
        .unwrap_or_else(|| syntaxes().find_syntax_plain_text());
    highlight_with(syntax, source, options, theme)
}

fn syntax_for_path(path: &Path) -> &'static syntect::parsing::SyntaxReference {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    syntaxes()
        .find_syntax_for_file(path)
        .ok()
        .flatten()
        .or_else(|| syntaxes().find_syntax_by_extension(file_name))
        .or_else(|| syntaxes().find_syntax_by_extension(extension))
        .or_else(|| syntaxes().find_syntax_by_token(extension))
        .unwrap_or_else(|| syntaxes().find_syntax_plain_text())
}

fn syntect_theme(theme: &Theme) -> &'static SyntectTheme {
    let themes = syntect_themes();
    let fallback = if theme.is_dark() {
        "base16-ocean.dark"
    } else {
        "base16-ocean.light"
    };
    themes
        .get(&theme.syntax)
        .or_else(|| themes.get(fallback))
        .or_else(|| themes.get("Nord"))
        .or_else(|| themes.get("InspiredGitHub"))
        .expect("the bundled theme set carries a fallback")
}

fn highlight_with(
    syntax: &syntect::parsing::SyntaxReference,
    source: &str,
    options: &CodeOptions,
    theme: &Theme,
) -> Vec<Line> {
    let width = options.width.max(8);
    let mut highlighter = syntect::easy::HighlightLines::new(syntax, syntect_theme(theme));
    let source = source.replace("\r\n", "\n").replace('\r', "\n");
    let source = source.replace('\t', "    ");
    let mut lines: Vec<&str> = source.split('\n').collect();
    // A trailing newline yields one empty final line; drop it.
    if lines.len() > 1 && lines.last() == Some(&"") {
        lines.pop();
    }
    let total = lines.len();
    let shown = total.min(options.max_lines);
    let number_width = format!("{}", options.start_line + shown.saturating_sub(1))
        .len()
        .max(2);

    let mut out = Vec::with_capacity(shown);
    for (index, raw) in lines.iter().take(shown).enumerate() {
        let with_newline = format!("{raw}\n");
        let ranges = highlighter
            .highlight_line(&with_newline, syntaxes())
            .unwrap_or_default();
        let mut line = Line::new();
        if options.rail {
            line.push(Span::new("┃ ", Style::plain().fg(theme.code_border)));
        }
        if options.line_numbers {
            let number = options.start_line + index;
            line.push(Span::new(
                format!("{number:>number_width$} "),
                Style::plain().fg(theme.gutter),
            ));
        }
        for (style, piece) in ranges {
            let piece = piece.trim_end_matches('\n');
            if piece.is_empty() {
                continue;
            }
            line.push(Span::new(piece, convert_style(style, theme)));
        }
        finish_line(&mut line, options, theme, width);
        out.push(line);
    }
    if total > shown {
        out.push(Line::styled(
            format!("… {} more lines", total - shown),
            Style::plain().fg(theme.muted).italic(),
        ));
    }
    out
}

fn convert_style(style: syntect::highlighting::Style, theme: &Theme) -> Style {
    let fg = Rgb::new(style.foreground.r, style.foreground.g, style.foreground.b);
    let mut out = Style::plain().fg(fg);
    if style.font_style.contains(FontStyle::BOLD) {
        out = out.bold();
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        out = out.italic();
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        out = out.underline();
    }
    if theme.name == "mono" {
        // A monochrome theme keeps the structure but drops the hues.
        let shade = fg.luminance();
        let value = (shade * 235.0).clamp(30.0, 235.0) as u8;
        out = out.fg(Rgb::new(value, value, value));
    }
    out
}

fn finish_line(line: &mut Line, options: &CodeOptions, theme: &Theme, width: usize) {
    line.truncate(width, false);
    if options.background {
        line.map_style(|style| style.bg(theme.code_bg));
        line.padded(width, Style::plain().bg(theme.code_bg));
    }
}

/// A short language label for a fence or file, for the viewer header.
pub fn label_for_path(path: &Path) -> String {
    language_name(path).unwrap_or_else(|| "text".to_string())
}

/// A one-line summary of a file, used in the viewer title.
pub fn summarize(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(metadata) => format!(
            "{} · {}",
            text::basename(&path.display().to_string()),
            bytes(metadata.len())
        ),
        Err(_) => text::basename(&path.display().to_string()).to_string(),
    }
}

pub fn bytes(size: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{size} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::midnight()
    }

    #[test]
    fn highlights_rust_and_keeps_the_text_intact() {
        let source = "fn main() {\n    println!(\"hi\");\n}\n";
        let lines = highlight_snippet(
            Some("rust"),
            source,
            &CodeOptions::default().width(40),
            &theme(),
        );
        assert_eq!(lines.len(), 3);
        let text: String = lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("fn main()"));
        // More than one colour means tokens were actually classified.
        let colors: std::collections::HashSet<_> = lines
            .iter()
            .flat_map(|line| line.spans.iter().filter_map(|span| span.style.fg))
            .map(|rgb| (rgb.r, rgb.g, rgb.b))
            .collect();
        assert!(
            colors.len() > 1,
            "expected several token colours: {colors:?}"
        );
    }

    #[test]
    fn line_numbers_are_right_aligned_and_start_where_asked() {
        let options = CodeOptions {
            start_line: 98,
            width: 20,
            rail: false,
            ..CodeOptions::default()
        };
        let lines = highlight_snippet(Some("text"), "a\nb\nc", &options, &theme());
        assert!(lines[0].text().starts_with(" 98 "), "{:?}", lines[0].text());
        assert!(lines[2].text().starts_with("100 "), "{:?}", lines[2].text());
    }

    #[test]
    fn rows_are_padded_and_never_wider_than_requested() {
        let options = CodeOptions::default().width(24);
        let lines = highlight_snippet(Some("rust"), "fn main() {}", &options, &theme());
        assert_eq!(lines[0].width(), 24);
        let long = "x".repeat(200);
        let lines = highlight_snippet(Some("text"), &long, &options, &theme());
        assert_eq!(lines[0].width(), 24);
    }

    #[test]
    fn tabs_expand_and_long_files_are_truncated() {
        let options = CodeOptions {
            max_lines: 2,
            ..CodeOptions::default().width(40)
        };
        let lines = highlight_snippet(Some("text"), "\ta\nb\nc", &options, &theme());
        assert!(lines[0].text().contains("    a"));
        assert_eq!(lines.len(), 3);
        assert!(lines[2].text().contains("more lines"));
    }

    #[test]
    fn unknown_languages_still_render() {
        let lines = highlight_snippet(None, "hello <world>", &CodeOptions::default(), &theme());
        assert_eq!(lines[0].text().trim(), "1 hello <world>");
        assert_eq!(lines[0].width(), 80);
    }

    #[test]
    fn language_names_come_from_the_extension() {
        assert_eq!(
            language_name(Path::new("src/main.rs")).as_deref(),
            Some("Rust")
        );
        assert_eq!(
            language_name(Path::new("a/b.toml")).as_deref(),
            Some("TOML")
        );
        assert!(language_name(Path::new("noext")).is_none());
        assert!(supported_languages().iter().any(|name| name == "Rust"));
    }

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(2048), "2.0 KB");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0 MB");
    }
}
