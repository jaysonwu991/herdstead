//! Turning a vt100 screen into the grid a client draws.

use herdstead_core::geom::Rect;
use herdstead_core::protocol::{CursorPos, GridFrame, StyledLine, StyledRun, TermColor, TermStyle};

/// Converts the visible screen of `parser` for a pane occupying `rect`.
pub fn frame(parser: &vt100::Parser, rect: Rect) -> GridFrame {
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let rows = u16::from(rows);
    let cols = u16::from(cols);
    let offset = screen.scrollback();
    let mut lines = Vec::with_capacity(usize::from(rows));
    for row in 0..rows {
        let mut runs: Vec<StyledRun> = Vec::new();
        for col in 0..cols {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }
            let text = if cell.has_contents() {
                cell.contents()
            } else {
                " "
            };
            let style = cell_style(cell);
            match runs.last_mut() {
                Some(last) if last.style == style => last.text.push_str(text),
                _ => runs.push(StyledRun {
                    text: text.to_string(),
                    style,
                }),
            }
        }
        trim_trailing(&mut runs);
        lines.push(StyledLine {
            runs,
            wrapped: screen.row_wrapped(row),
        });
    }
    let cursor = if screen.hide_cursor() || offset > 0 {
        None
    } else {
        let (row, col) = screen.cursor_position();
        Some(CursorPos { row, col })
    };
    let frame = GridFrame {
        rows,
        cols,
        lines,
        cursor,
        offset,
        scrollback: screen.scrollback_len(),
        alt_screen: screen.alternate_screen(),
    };
    fit(frame, rect)
}

/// Clips the grid to the pane's inner size, which keeps a stale pane from
/// drawing outside its frame after a resize.
fn fit(mut frame: GridFrame, rect: Rect) -> GridFrame {
    let width = rect.inner().width;
    let height = rect.inner().height;
    if width == 0 || height == 0 {
        frame.lines.clear();
        frame.cursor = None;
        return frame;
    }
    if frame.cols > width {
        for line in &mut frame.lines {
            let mut used = 0usize;
            let mut runs = Vec::new();
            for run in std::mem::take(&mut line.runs) {
                if used >= usize::from(width) {
                    break;
                }
                let mut text = String::new();
                for ch in run.text.chars() {
                    let ch_width = herdstead_core::text::char_width(ch);
                    if used + ch_width > usize::from(width) {
                        break;
                    }
                    text.push(ch);
                    used += ch_width;
                }
                if !text.is_empty() {
                    runs.push(StyledRun {
                        text,
                        style: run.style,
                    });
                }
            }
            line.runs = runs;
        }
    }
    if usize::from(frame.rows) > usize::from(height) {
        frame.lines.truncate(usize::from(height));
    }
    frame
}

fn cell_style(cell: &vt100::Cell) -> TermStyle {
    TermStyle {
        fg: color(cell.fgcolor()),
        bg: color(cell.bgcolor()),
        bold: cell.bold(),
        dim: cell.dim(),
        italic: cell.italic(),
        underline: cell.underline(),
        inverse: cell.inverse(),
    }
}

fn color(color: vt100::Color) -> TermColor {
    match color {
        vt100::Color::Default => TermColor::Default,
        vt100::Color::Idx(index) => TermColor::Idx(index),
        vt100::Color::Rgb(r, g, b) => TermColor::Rgb(r, g, b),
    }
}

fn trim_trailing(runs: &mut Vec<StyledRun>) {
    while let Some(last) = runs.last_mut() {
        if last.style.bg != TermColor::Default {
            break;
        }
        let trimmed = last.text.trim_end_matches(' ');
        if trimmed.len() == last.text.len() {
            break;
        }
        last.text.truncate(trimmed.len());
        if last.text.is_empty() {
            runs.pop();
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_text_and_colours() {
        let mut parser = vt100::Parser::new(
            std::num::NonZeroU16::new(3).unwrap(),
            std::num::NonZeroU16::new(10).unwrap(),
            100,
        );
        parser.process(b"\x1b[1;31mhi\x1b[0m there");
        let frame = frame(&parser, Rect::new(0, 0, 12, 5));
        assert_eq!(frame.rows, 3);
        assert_eq!(frame.cols, 10);
        assert_eq!(frame.lines[0].text(), "hi there");
        assert_eq!(frame.lines[0].runs[0].style.fg, TermColor::Idx(1));
        assert!(frame.lines[0].runs[0].style.bold);
        assert_eq!(frame.lines[0].runs[1].style, TermStyle::default());
        assert!(frame.lines[1].runs.is_empty());
        assert_eq!(frame.cursor.map(|c| (c.row, c.col)), Some((0, 8)));
    }

    #[test]
    fn wide_characters_are_kept_once() {
        let mut parser = vt100::Parser::new(
            std::num::NonZeroU16::new(2).unwrap(),
            std::num::NonZeroU16::new(10).unwrap(),
            0,
        );
        parser.process("日本語".as_bytes());
        let frame = frame(&parser, Rect::new(0, 0, 12, 4));
        assert_eq!(frame.lines[0].text(), "日本語");
    }

    #[test]
    fn scrollback_offsets_show_history_and_hide_the_cursor() {
        let mut parser = vt100::Parser::new(
            std::num::NonZeroU16::new(2).unwrap(),
            std::num::NonZeroU16::new(10).unwrap(),
            50,
        );
        for index in 0..10 {
            parser.process(format!("line{index}\r\n").as_bytes());
        }
        let live = frame(&parser, Rect::new(0, 0, 12, 4));
        assert!(live.scrollback >= 8);
        assert!(live.cursor.is_some());

        parser.screen_mut().set_scrollback(4);
        let scrolled = frame(&parser, Rect::new(0, 0, 12, 4));
        assert_eq!(scrolled.offset, 4);
        assert!(scrolled.cursor.is_none());
        assert_ne!(scrolled.lines[0].text(), live.lines[0].text());
    }

    #[test]
    fn the_grid_is_clipped_to_the_pane() {
        let mut parser = vt100::Parser::new(
            std::num::NonZeroU16::new(6).unwrap(),
            std::num::NonZeroU16::new(20).unwrap(),
            0,
        );
        parser.process(b"0123456789abcdefghij");
        let frame = frame(&parser, Rect::new(0, 0, 8, 3));
        assert!(frame.lines[0].width_text() <= 6);
        assert!(frame.lines.len() <= 1);
    }

    trait Width {
        fn width_text(&self) -> usize;
    }

    impl Width for StyledLine {
        fn width_text(&self) -> usize {
            self.runs
                .iter()
                .map(|run| herdstead_core::text::display_width(&run.text))
                .sum()
        }
    }
}
