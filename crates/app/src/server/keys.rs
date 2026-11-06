//! Encoding a key press for a pane, honouring its terminal modes.

use herdstead_core::protocol::{Key, KeyInput, Mods};

/// The bytes a pane expects for `input`.
pub fn encode(input: &KeyInput, screen: &vt100::Screen) -> Vec<u8> {
    let mods = input.mods;
    let mut out = Vec::new();
    match input.key {
        Key::Char(ch) => {
            if mods.ctrl {
                if let Some(byte) = ctrl_byte(ch) {
                    if mods.alt {
                        out.push(0x1b);
                    }
                    out.push(byte);
                    return out;
                }
            }
            if mods.alt {
                out.push(0x1b);
            }
            let mut buffer = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
        }
        Key::Enter => {
            if mods.alt {
                out.push(0x1b);
            }
            out.push(b'\r');
        }
        Key::Tab => {
            if mods.alt {
                out.push(0x1b);
            }
            out.push(b'\t');
        }
        Key::BackTab => out.extend_from_slice(b"\x1b[Z"),
        Key::Esc => out.push(0x1b),
        Key::Backspace => {
            if mods.ctrl || mods.alt {
                out.push(0x1b);
            }
            out.push(0x7f);
        }
        Key::Delete => out.extend(csi(b"3~", mods)),
        Key::Insert => out.extend(csi(b"2~", mods)),
        Key::Left => out.extend(cursor(b'D', mods, screen)),
        Key::Right => out.extend(cursor(b'C', mods, screen)),
        Key::Up => out.extend(cursor(b'A', mods, screen)),
        Key::Down => out.extend(cursor(b'B', mods, screen)),
        Key::Home => out.extend(cursor(b'H', mods, screen)),
        Key::End => out.extend(cursor(b'F', mods, screen)),
        Key::PageUp => out.extend(csi(b"5~", mods)),
        Key::PageDown => out.extend(csi(b"6~", mods)),
        Key::F(number) => out.extend(function_key(number, mods)),
    }
    out
}

/// Pastes text, wrapping it in bracketed-paste markers when the pane asked.
pub fn encode_paste(text: &str, screen: &vt100::Screen) -> Vec<u8> {
    let mut out = Vec::new();
    if screen.bracketed_paste() {
        out.extend_from_slice(b"\x1b[200~");
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
    } else {
        out.extend_from_slice(text.as_bytes());
    }
    out
}

fn ctrl_byte(ch: char) -> Option<u8> {
    let byte = ch as u32;
    if (0x40..=0x7f).contains(&byte) {
        return Some((byte as u8) & 0x1f);
    }
    match ch {
        ' ' => Some(0x00),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        _ => None,
    }
}

fn modifier(m: Mods) -> u8 {
    1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.ctrl)
}

fn cursor(final_byte: u8, mods: Mods, screen: &vt100::Screen) -> Vec<u8> {
    let m = modifier(mods);
    if m == 1 {
        if screen.application_cursor() {
            return vec![0x1b, b'O', final_byte];
        }
        return vec![0x1b, b'[', final_byte];
    }
    let mut out = vec![0x1b, b'['];
    out.push(b'1');
    out.push(b';');
    out.extend_from_slice(m.to_string().as_bytes());
    out.push(final_byte);
    out
}

fn csi(body: &[u8], mods: Mods) -> Vec<u8> {
    let m = modifier(mods);
    let mut out = vec![0x1b, b'['];
    if m == 1 {
        out.extend_from_slice(body);
        return out;
    }
    // Insert `;{m}` before the final byte.
    let (head, tail) = body.split_at(body.len() - 1);
    out.extend_from_slice(head);
    out.push(b';');
    out.extend_from_slice(m.to_string().as_bytes());
    out.extend_from_slice(tail);
    out
}

fn function_key(number: u8, mods: Mods) -> Vec<u8> {
    let code: &[u8] = match number {
        1 => b"11~",
        2 => b"12~",
        3 => b"13~",
        4 => b"14~",
        5 => b"15~",
        6 => b"17~",
        7 => b"18~",
        8 => b"19~",
        9 => b"20~",
        10 => b"21~",
        11 => b"23~",
        12 => b"24~",
        13 => b"25~",
        14 => b"26~",
        15 => b"28~",
        16 => b"29~",
        17 => b"31~",
        18 => b"32~",
        19 => b"33~",
        20 => b"34~",
        _ => b"2~",
    };
    csi(code, mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> vt100::Parser {
        vt100::Parser::new(
            std::num::NonZeroU16::new(24).unwrap(),
            std::num::NonZeroU16::new(80).unwrap(),
            0,
        )
    }

    fn key(key: Key) -> Vec<u8> {
        encode(&KeyInput::new(key), screen().screen())
    }

    #[test]
    fn plain_characters_are_utf8() {
        assert_eq!(key(Key::Char('a')), b"a");
        assert_eq!(key(Key::Char('é')), "é".as_bytes());
    }

    #[test]
    fn control_characters_map_to_their_bytes() {
        assert_eq!(
            encode(
                &KeyInput::with_mods(Key::Char('c'), Mods::ctrl()),
                screen().screen()
            ),
            b"\x03"
        );
        assert_eq!(
            encode(
                &KeyInput::with_mods(Key::Char('['), Mods::ctrl()),
                screen().screen()
            ),
            b"\x1b"
        );
        assert_eq!(
            encode(
                &KeyInput::with_mods(Key::Char(' '), Mods::ctrl()),
                screen().screen()
            ),
            b"\x00"
        );
    }

    #[test]
    fn alt_prefixes_escape() {
        let mods = Mods {
            alt: true,
            ..Mods::none()
        };
        assert_eq!(
            encode(
                &KeyInput::with_mods(Key::Char('x'), mods),
                screen().screen()
            ),
            b"\x1bx"
        );
    }

    #[test]
    fn arrows_follow_the_application_cursor_mode() {
        assert_eq!(key(Key::Up), b"\x1b[A");
        let mut parser = vt100::Parser::new(
            std::num::NonZeroU16::new(24).unwrap(),
            std::num::NonZeroU16::new(80).unwrap(),
            0,
        );
        parser.process(b"\x1b[?1h");
        assert_eq!(encode(&KeyInput::new(Key::Up), parser.screen()), b"\x1bOA");
        let mods = Mods {
            shift: true,
            ..Mods::none()
        };
        assert_eq!(
            encode(&KeyInput::with_mods(Key::Right, mods), parser.screen()),
            b"\x1b[1;2C"
        );
    }

    #[test]
    fn editing_keys_and_function_keys() {
        assert_eq!(key(Key::Enter), b"\r");
        assert_eq!(key(Key::Tab), b"\t");
        assert_eq!(key(Key::BackTab), b"\x1b[Z");
        assert_eq!(key(Key::Backspace), b"\x7f");
        assert_eq!(key(Key::Delete), b"\x1b[3~");
        assert_eq!(key(Key::PageUp), b"\x1b[5~");
        assert_eq!(key(Key::F(1)), b"\x1b[11~");
        assert_eq!(key(Key::F(12)), b"\x1b[24~");
    }

    #[test]
    fn paste_respects_bracketed_paste() {
        let mut parser = vt100::Parser::new(
            std::num::NonZeroU16::new(24).unwrap(),
            std::num::NonZeroU16::new(80).unwrap(),
            0,
        );
        assert_eq!(encode_paste("hi", parser.screen()), b"hi");
        parser.process(b"\x1b[?2004h");
        assert_eq!(encode_paste("hi", parser.screen()), b"\x1b[200~hi\x1b[201~");
    }
}
