//! ANSI output for the CLI (`herdstead cat`, `tree`, `md`).

use std::io::Write;

use herdstead_core::Theme;

use crate::style::{Line, Link, Style};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsiOptions {
    /// Emit OSC 8 hyperlinks so supporting terminals make links clickable.
    pub hyperlinks: bool,
    /// Reset styles after each row so they do not leak into following output.
    pub reset_lines: bool,
}

impl Default for AnsiOptions {
    fn default() -> Self {
        Self {
            hyperlinks: true,
            reset_lines: true,
        }
    }
}

/// Renders styled lines as one ANSI string.
pub fn render(lines: &[Line], theme: &Theme, options: &AnsiOptions) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(&render_line(line, theme, options));
        out.push('\n');
    }
    out
}

pub fn render_line(line: &Line, theme: &Theme, options: &AnsiOptions) -> String {
    let mut out = String::new();
    for span in &line.spans {
        if span.text.is_empty() {
            continue;
        }
        out.push_str(&style_prefix(span.style, theme));
        let link = span.link.as_ref().filter(|_| options.hyperlinks);
        if let Some(link) = link {
            out.push_str(&osc8_open(link));
        }
        out.push_str(&span.text);
        if link.is_some() {
            out.push_str("\x1b]8;;\x1b\\");
        }
    }
    if options.reset_lines {
        out.push_str("\x1b[0m");
    }
    out
}

/// Writes rendered lines to a writer, propagating IO errors.
pub fn write(
    writer: &mut impl Write,
    lines: &[Line],
    theme: &Theme,
    options: &AnsiOptions,
) -> std::io::Result<()> {
    writer.write_all(render(lines, theme, options).as_bytes())
}

fn style_prefix(style: Style, theme: &Theme) -> String {
    let style = style.resolved(theme);
    let mut out = String::from("\x1b[0m");
    if let Some(fg) = style.fg {
        out.push_str(&fg.ansi_fg());
    }
    if let Some(bg) = style.bg {
        out.push_str(&bg.ansi_bg());
    }
    if style.bold {
        out.push_str("\x1b[1m");
    }
    if style.dim {
        out.push_str("\x1b[2m");
    }
    if style.italic {
        out.push_str("\x1b[3m");
    }
    if style.underline {
        out.push_str("\x1b[4m");
    }
    if style.strikethrough {
        out.push_str("\x1b[9m");
    }
    if style.reversed {
        out.push_str("\x1b[7m");
    }
    out
}

fn osc8_open(link: &Link) -> String {
    let target = match link {
        Link::Url(url) => url.clone(),
        Link::Path { path, line, column } => {
            let mut target = format!("file://{path}");
            if let Some(line) = line {
                target.push_str(&format!("#L{line}"));
                if let Some(column) = column {
                    target.push_str(&format!("C{column}"));
                }
            }
            target
        }
    };
    format!("\x1b]8;;{target}\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdstead_core::theme::Rgb;

    fn theme() -> Theme {
        Theme::midnight()
    }

    #[test]
    fn emits_truecolour_and_modifiers() {
        let line = Line::styled(
            "hi",
            Style::plain().fg(Rgb::new(1, 2, 3)).bold().underline(),
        );
        let out = render_line(&line, &theme(), &AnsiOptions::default());
        assert!(out.contains("\x1b[38;2;1;2;3m"));
        assert!(out.contains("\x1b[1m"));
        assert!(out.contains("\x1b[4m"));
        assert!(out.ends_with("\x1b[0m"));
        assert!(out.contains("hi"));
    }

    #[test]
    fn links_become_osc8_hyperlinks() {
        let mut line = Line::plain("docs");
        line.spans[0].link = Some(Link::Url("https://x.dev".into()));
        let out = render_line(&line, &theme(), &AnsiOptions::default());
        assert!(out.contains("\x1b]8;;https://x.dev\x1b\\"));

        let mut line = Line::plain("main");
        line.spans[0].link = Some(Link::Path {
            path: "src/main.rs".into(),
            line: Some(3),
            column: None,
        });
        let out = render_line(&line, &theme(), &AnsiOptions::default());
        assert!(out.contains("file://src/main.rs#L3"));
    }

    #[test]
    fn hyperlinks_can_be_disabled() {
        let options = AnsiOptions {
            hyperlinks: false,
            reset_lines: false,
        };
        let mut line = Line::plain("docs");
        line.spans[0].link = Some(Link::Url("https://x.dev".into()));
        let out = render_line(&line, &theme(), &options);
        assert!(!out.contains("\x1b]8;"));
        assert!(!out.ends_with("\x1b[0m"));
    }

    #[test]
    fn renders_one_line_each() {
        let lines = vec![Line::plain("a"), Line::plain("b")];
        let out = render(&lines, &theme(), &AnsiOptions::default());
        assert_eq!(out.lines().count(), 2);
    }
}
