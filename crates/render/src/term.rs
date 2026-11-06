//! Mapping a terminal pane's cells onto styled rows.

use herdr_plus_core::protocol::{GridFrame, StyledLine};
use herdr_plus_core::Theme;

use crate::style::{Line, Span, Style};

/// One screen row of a terminal, with colours resolved through the theme.
pub fn term_line(line: &StyledLine, theme: &Theme) -> Line {
    let mut out = Line::new();
    for run in &line.runs {
        let style = term_style(&run.style, theme);
        out.push(Span::new(run.text.clone(), style));
    }
    trim_trailing_space(&mut out);
    out
}

/// All visible rows of a frame.
pub fn term_lines(frame: &GridFrame, theme: &Theme) -> Vec<Line> {
    frame
        .lines
        .iter()
        .map(|line| term_line(line, theme))
        .collect()
}

pub fn term_style(style: &herdr_plus_core::protocol::TermStyle, theme: &Theme) -> Style {
    let mut out = Style::plain().fg(theme.resolve(style.fg));
    if !matches!(style.bg, herdr_plus_core::protocol::TermColor::Default) {
        out = out.bg(theme.resolve_bg(style.bg));
    }
    if style.bold {
        out = out.bold();
    }
    if style.dim {
        out = out.dim();
    }
    if style.italic {
        out = out.italic();
    }
    if style.underline {
        out = out.underline();
    }
    if style.inverse {
        out = out.reversed();
    }
    out
}

/// Drops trailing blanks that carry no background, which a terminal pads rows
/// with; keeping the background matters for vim-style blocks.
fn trim_trailing_space(line: &mut Line) {
    while let Some(last) = line.spans.last_mut() {
        if last.style.bg.is_some() {
            break;
        }
        let trimmed = last.text.trim_end_matches(' ').len();
        if trimmed == last.text.len() {
            break;
        }
        last.text.truncate(trimmed);
        if last.text.is_empty() {
            line.spans.pop();
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdr_plus_core::protocol::{StyledLine, StyledRun, TermColor, TermStyle};

    fn run(text: &str, style: TermStyle) -> StyledRun {
        StyledRun {
            text: text.into(),
            style,
        }
    }

    #[test]
    fn resolves_colours_and_modifiers() {
        let theme = Theme::midnight();
        let line = StyledLine {
            runs: vec![
                run(
                    "ok",
                    TermStyle {
                        fg: TermColor::Rgb(1, 2, 3),
                        bold: true,
                        ..TermStyle::default()
                    },
                ),
                run(
                    " warn",
                    TermStyle {
                        fg: TermColor::Idx(3),
                        dim: true,
                        ..TermStyle::default()
                    },
                ),
            ],
            wrapped: false,
        };
        let rendered = term_line(&line, &theme);
        assert_eq!(rendered.text(), "ok warn");
        assert_eq!(
            rendered.spans[0].style.fg,
            Some(herdr_plus_core::theme::Rgb::new(1, 2, 3))
        );
        assert!(rendered.spans[0].style.bold);
        assert_eq!(
            rendered.spans[1].style.fg,
            Some(herdr_plus_core::theme::xterm256(3))
        );
        assert!(rendered.spans[1].style.dim);
    }

    #[test]
    fn drops_unpainted_trailing_space_but_keeps_backgrounds() {
        let theme = Theme::midnight();
        let plain = StyledLine {
            runs: vec![run("hi     ", TermStyle::default())],
            wrapped: false,
        };
        assert_eq!(term_line(&plain, &theme).text(), "hi");

        let block = StyledLine {
            runs: vec![run(
                "hi     ",
                TermStyle {
                    bg: TermColor::Idx(4),
                    ..TermStyle::default()
                },
            )],
            wrapped: false,
        };
        assert_eq!(term_line(&block, &theme).text(), "hi     ");
    }

    #[test]
    fn frames_render_one_line_per_row() {
        let theme = Theme::midnight();
        let frame = GridFrame::plain(
            2,
            4,
            vec![
                StyledLine {
                    runs: vec![run("ab", TermStyle::default())],
                    wrapped: false,
                },
                StyledLine::default(),
            ],
        );
        let lines = term_lines(&frame, &theme);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text(), "ab");
        assert_eq!(lines[1].text(), "");
    }
}
