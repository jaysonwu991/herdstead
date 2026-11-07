//! Mapping a terminal pane's cells onto styled rows.

use herdstead_core::protocol::{GridFrame, StyledLine};
use herdstead_core::Theme;

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

/// Decorate the editable logical line while retaining its terminal columns,
/// including references whose text crosses a soft wrap.
pub fn decorate_input(
    frame: &GridFrame,
    lines: &mut [Line],
    theme: &Theme,
    links: bool,
    resolve: &dyn Fn(&str) -> bool,
) {
    let Some(cursor) = frame
        .cursor
        .filter(|_| !frame.alt_screen && frame.offset == 0)
    else {
        return;
    };
    let cursor = usize::from(cursor.row);
    if cursor >= lines.len() || lines.len() != frame.lines.len() || frame.cols == 0 {
        return;
    }
    let mut first = cursor;
    while first > 0 && frame.lines[first - 1].wrapped {
        first -= 1;
    }
    let mut last = cursor;
    while last + 1 < lines.len() && frame.lines[last].wrapped {
        last += 1;
    }
    let mut combined = Line::new();
    for (row, original) in lines.iter().enumerate().take(last + 1).skip(first) {
        let mut line = original.clone();
        if frame.lines[row].wrapped {
            line.padded(usize::from(frame.cols), Style::plain());
        }
        combined.extend(line.spans);
    }
    let mut styled = crate::input::decorate(&combined, theme, links, resolve);
    if links {
        styled = crate::link::decorate(&styled, theme, resolve);
    }
    let mut rows = vec![Line::new(); last - first + 1];
    let mut row = 0;
    let mut column = 0;
    for span in styled.spans {
        for ch in span.text.chars() {
            let width = herdstead_core::text::char_width(ch);
            if column + width > usize::from(frame.cols) {
                row += 1;
                column = 0;
            }
            let Some(target) = rows.get_mut(row) else {
                break;
            };
            target.push(Span {
                text: ch.to_string(),
                style: span.style,
                link: span.link.clone(),
            });
            column += width;
        }
    }
    for (target, mut line) in lines[first..=last].iter_mut().zip(rows) {
        trim_trailing_space(&mut line);
        *target = line;
    }
}

/// Rich ordinary output without changing terminal row coordinates. The live
/// cursor row, wrapped rows, ANSI-styled output and alternate screen are skipped.
/// Input decoration is a separate pass.
pub fn rich_lines(frame: &GridFrame, theme: &Theme) -> Vec<Line> {
    let mut lines = term_lines(frame, theme);
    if frame.alt_screen {
        return lines;
    }
    let eligible = |row: usize| {
        let line = &frame.lines[row];
        !line.wrapped
            && (row == 0 || !frame.lines[row - 1].wrapped)
            && frame
                .cursor
                .is_none_or(|cursor| row != usize::from(cursor.row))
            && line.runs.iter().all(|run| run.style.is_plain())
    };
    let mut row = 0;
    while row < frame.lines.len() {
        if !eligible(row) {
            row += 1;
            continue;
        }
        let source = frame.lines[row].text();
        let trimmed = source.trim_start();
        if let Some((marker, language)) = fence(trimmed) {
            let start = row + 1;
            let mut end = start;
            while end < frame.lines.len()
                && eligible(end)
                && !frame.lines[end].text().trim_start().starts_with(marker)
            {
                end += 1;
            }
            let body = frame.lines[start..end]
                .iter()
                .map(StyledLine::text)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            let options = crate::code::CodeOptions {
                width: usize::from(frame.cols),
                line_numbers: false,
                background: false,
                max_lines: end - start,
                ..Default::default()
            };
            let highlighted =
                crate::code::highlight_snippet(Some(language), &body, &options, theme);
            for (target, line) in lines[start..end].iter_mut().zip(highlighted) {
                *target = line;
            }
            lines[row] = Line::styled(
                format!("╭─ {language}"),
                Style::plain().fg(theme.code_border),
            );
            if end < frame.lines.len()
                && eligible(end)
                && frame.lines[end].text().trim_start().starts_with(marker)
            {
                lines[end] = Line::styled("╰─", Style::plain().fg(theme.code_border));
                row = end + 1;
            } else {
                row = end;
            }
        } else {
            let heading = trimmed.starts_with('#')
                && trimmed.chars().take_while(|ch| *ch == '#').count() <= 6
                && trimmed.trim_start_matches('#').starts_with(' ');
            if heading
                || trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("> ")
            {
                if let Some(line) =
                    crate::markdown::render_width(&source, usize::from(frame.cols), theme)
                        .into_iter()
                        .find(|line| !line.is_blank())
                {
                    lines[row] = line;
                }
            } else if source.contains("**")
                || source.contains("__")
                || source.matches('`').count() >= 2
                || (source.contains("](") && source.contains('['))
            {
                lines[row] = Line {
                    spans: crate::inline::parse_inline(&source, Style::plain(), theme),
                };
            }
            row += 1;
        }
    }
    for line in &mut lines {
        line.truncate(usize::from(frame.cols), false);
    }
    lines
}

fn fence(text: &str) -> Option<(&str, &str)> {
    let marker = if text.starts_with("```") {
        "```"
    } else if text.starts_with("~~~") {
        "~~~"
    } else {
        return None;
    };
    let language = text[3..].trim();
    if language
        .chars()
        .all(|ch| ch.is_alphanumeric() || "_+-#".contains(ch))
    {
        Some((marker, language))
    } else {
        None
    }
}

pub fn term_style(style: &herdstead_core::protocol::TermStyle, theme: &Theme) -> Style {
    let mut out = Style::plain().fg(theme.resolve(style.fg));
    if !matches!(style.bg, herdstead_core::protocol::TermColor::Default) {
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
    use herdstead_core::protocol::{StyledLine, StyledRun, TermColor, TermStyle};

    #[test]
    fn input_markdown_and_links_survive_soft_wraps_without_moving_cells() {
        let source = ["echo **bo", "ld** www.", "site.com"];
        let mut frame = GridFrame::plain(
            3,
            9,
            source
                .iter()
                .enumerate()
                .map(|(row, text)| StyledLine {
                    runs: vec![run(text, TermStyle::default())],
                    wrapped: row < 2,
                })
                .collect(),
        );
        frame.cursor = Some(herdstead_core::protocol::CursorPos { row: 2, col: 8 });
        let theme = Theme::midnight();
        let mut rows = term_lines(&frame, &theme);
        decorate_input(&frame, &mut rows, &theme, true, &|_| false);
        assert_eq!(
            rows.iter().map(Line::text).collect::<String>(),
            source.concat()
        );
        assert!(rows[0]
            .spans
            .iter()
            .any(|span| span.text == "bo" && span.style.bold));
        assert!(rows[1]
            .spans
            .iter()
            .any(|span| span.text == "ld" && span.style.bold));
        assert!(rows[2]
            .spans
            .iter()
            .all(|span| span.link == Some(crate::Link::Url("https://www.site.com".into()))));
        frame.alt_screen = true;
        let mut raw = term_lines(&frame, &theme);
        decorate_input(&frame, &mut raw, &theme, true, &|_| false);
        assert_eq!(raw, term_lines(&frame, &theme));
    }

    #[test]
    fn rich_output_formats_markdown_and_code_without_moving_input() {
        let source = [
            "# Heading",
            "**bold** and `code`",
            "```rust",
            "fn main() {}",
            "```",
            "$ echo **literal**",
        ];
        let mut frame = GridFrame::plain(
            6,
            60,
            source
                .iter()
                .map(|text| StyledLine {
                    runs: vec![run(text, TermStyle::default())],
                    wrapped: false,
                })
                .collect(),
        );
        frame.cursor = Some(herdstead_core::protocol::CursorPos { row: 5, col: 18 });
        let rows = rich_lines(&frame, &Theme::midnight());
        assert_eq!(rows.len(), source.len());
        assert!(rows[0].text().contains("Heading"));
        assert!(!rows[1].text().contains("**"));
        assert!(rows[1].spans.iter().any(|span| span.style.bold));
        assert_eq!(rows[3].text().trim_end(), "fn main() {}");
        assert!(rows[3].spans.iter().any(|span| span.style.fg.is_some()));
        assert_eq!(rows[5].text(), source[5]);
        frame.alt_screen = true;
        assert_eq!(
            rich_lines(&frame, &Theme::midnight()),
            term_lines(&frame, &Theme::midnight())
        );
    }

    #[test]
    fn rich_output_preserves_ansi_and_soft_wrapped_rows() {
        let frame = GridFrame::plain(
            3,
            30,
            vec![
                StyledLine {
                    runs: vec![run(
                        "**styled**",
                        TermStyle {
                            bold: true,
                            ..Default::default()
                        },
                    )],
                    wrapped: false,
                },
                StyledLine {
                    runs: vec![run("**wrapped**", TermStyle::default())],
                    wrapped: true,
                },
                StyledLine {
                    runs: vec![run("**continuation**", TermStyle::default())],
                    wrapped: false,
                },
            ],
        );
        assert_eq!(
            rich_lines(&frame, &Theme::midnight()),
            term_lines(&frame, &Theme::midnight())
        );
    }

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
            Some(herdstead_core::theme::Rgb::new(1, 2, 3))
        );
        assert!(rendered.spans[0].style.bold);
        assert_eq!(
            rendered.spans[1].style.fg,
            Some(herdstead_core::theme::xterm256(3))
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
