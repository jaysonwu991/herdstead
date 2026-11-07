//! One PTY-backed terminal pane: the process, its screen and its readers.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Instant;

use anyhow::{Context, Result};
use herdstead_core::ids::PaneId;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};

use super::Msg;

pub struct PanePty {
    pub parser: vt100::Parser,
    pub last_output: Instant,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    pub exit_code: Option<i32>,
    pub closed: bool,
}

impl PanePty {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        pane: PaneId,
        shell: &str,
        cwd: &Path,
        rows: u16,
        cols: u16,
        scrollback: usize,
        tx: Sender<Msg>,
    ) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("opening a pty")?;
        let mut command = CommandBuilder::new(shell);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("HERDSTEAD", "1");
        command.env("HERDSTEAD_PANE", pane.to_string());
        command.env("HERDSTEAD_SESSION_PID", std::process::id().to_string());
        if std::env::var("HERDSTEAD_SHELL_INTEGRATION").as_deref() == Ok("1")
            && Path::new(shell)
                .file_name()
                .is_some_and(|name| name == "zsh")
        {
            command.arg("-l");
            let dir = crate::shell::prepare(&herdstead_core::paths::Paths::discover())?;
            let user_dir = std::env::var_os("ZDOTDIR")
                .or_else(|| std::env::var_os("HOME"))
                .unwrap_or_default();
            command.env("HERDSTEAD_USER_ZDOTDIR", user_dir);
            command.env("HERDSTEAD_INTEGRATION_DIR", &dir);
            command.env("ZDOTDIR", &dir);
            command.env("HERDSTEAD_EXECUTABLE", std::env::current_exe()?);
        }
        let mut child = pair
            .slave
            .spawn_command(command)
            .with_context(|| format!("starting {shell} in {}", cwd.display()))?;
        let killer = child.clone_killer();
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .context("cloning the pty reader")?;
        let writer = pair.master.take_writer().context("taking the pty writer")?;

        // Output: feed the tracker, then hand the bytes to the runtime.
        let output_tx = tx.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            let mut tracker = OscTracker::default();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        let (title, cwd) = tracker.feed(&buffer[..read]);
                        if output_tx
                            .send(Msg::PaneOutput {
                                pane,
                                bytes: buffer[..read].to_vec(),
                                title,
                                cwd,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let _ = output_tx.send(Msg::PaneClosed);
        });

        // Reaping the child reports the exit code once the process ends.
        let exit_tx = tx;
        std::thread::spawn(move || {
            let code = child
                .wait()
                .ok()
                .map(|status| i32::try_from(status.exit_code()).unwrap_or(-1));
            let _ = exit_tx.send(Msg::PaneExited { pane, code });
        });

        Ok(Self {
            parser: parser(rows, cols, scrollback),
            last_output: Instant::now(),
            master: pair.master,
            writer,
            killer,
            exit_code: None,
            closed: false,
        })
    }

    pub fn write(&mut self, bytes: &[u8]) {
        if self.closed {
            return;
        }
        if self.writer.write_all(bytes).is_err() {
            self.closed = true;
        }
        let _ = self.writer.flush();
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        if self.closed {
            return;
        }
        let rows = rows.max(1);
        let cols = cols.max(1);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.parser.set_size(nonzero(rows), nonzero(cols));
    }

    pub fn kill(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        let _ = self.killer.kill();
    }

    /// The visible screen as plain text, newest lines last.
    pub fn text(&self) -> String {
        let screen = self.parser.screen();
        let (_, cols) = screen.size();
        let mut out = String::new();
        for row in screen.rows(0, u16::from(cols)) {
            out.push_str(&row);
            out.push('\n');
        }
        out
    }

    pub fn text_tail(&self, lines: usize) -> String {
        let text = self.text();
        let all: Vec<&str> = text.lines().collect();
        all[all.len().saturating_sub(lines)..].join("\n")
    }

    pub fn scroll(&mut self, delta: i32, page: bool) -> bool {
        let screen = self.parser.screen_mut();
        let current = screen.scrollback() as i32;
        let total = screen.scrollback_len() as i32;
        let step = if page {
            delta.signum() * i32::from(u16::from(screen.size().0))
        } else {
            delta
        };
        let target = (current + step).clamp(0, total);
        if target == current {
            return false;
        }
        screen.set_scrollback(target as usize);
        true
    }

    pub fn scroll_to_bottom(&mut self) -> bool {
        let screen = self.parser.screen_mut();
        let changed = screen.scrollback() != 0;
        screen.set_scrollback(0);
        changed
    }

    pub fn scroll_to_top(&mut self) -> bool {
        let screen = self.parser.screen_mut();
        let total = screen.scrollback_len();
        let changed = screen.scrollback() != total;
        screen.set_scrollback(total);
        changed
    }
}

fn nonzero(value: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(value.max(1)).expect("clamped to at least one")
}

fn parser(rows: u16, cols: u16, scrollback: usize) -> vt100::Parser {
    vt100::Parser::new(nonzero(rows), nonzero(cols), scrollback)
}

/// Picks the last window title and working directory an application reports
/// through OSC 0/2 and OSC 7.
#[derive(Default)]
pub struct OscTracker {
    tail: Vec<u8>,
}

impl OscTracker {
    pub fn feed(&mut self, bytes: &[u8]) -> (Option<String>, Option<PathBuf>) {
        const KEEP: usize = 4_096;
        self.tail.extend_from_slice(bytes);
        if self.tail.len() > KEEP {
            let drain = self.tail.len() - KEEP;
            self.tail.drain(..drain);
        }
        let text = String::from_utf8_lossy(&self.tail).into_owned();
        let title = last_osc(&text, "2").or_else(|| last_osc(&text, "0"));
        let cwd = last_osc(&text, "7").and_then(|value| file_url_path(&value));
        (title, cwd)
    }
}

fn last_osc(text: &str, code: &str) -> Option<String> {
    let needle = format!("\x1b]{code};");
    let mut found = None;
    let mut search = 0;
    while let Some(index) = text[search..].find(&needle) {
        let start = search + index + needle.len();
        let rest = &text[start..];
        let end = rest
            .find('\x07')
            .or_else(|| rest.find("\x1b\\"))
            .unwrap_or(rest.len());
        if end < rest.len() {
            found = Some(rest[..end].to_string());
            search = start + end + 1;
        } else {
            break;
        }
    }
    found
}

fn file_url_path(value: &str) -> Option<PathBuf> {
    let rest = value.strip_prefix("file://")?;
    let slash = rest.find('/')?;
    let path = &rest[slash..];
    Some(PathBuf::from(percent_decode(path)))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_titles_and_folders() {
        let mut tracker = OscTracker::default();
        let (title, cwd) = tracker.feed(b"\x1b]2;my title\x07");
        assert_eq!(title.as_deref(), Some("my title"));
        assert_eq!(cwd, None);

        let (_, cwd) = tracker.feed(b"\x1b]7;file://host/Users/me/my%20project\x07");
        assert_eq!(cwd, Some(PathBuf::from("/Users/me/my project")));
    }

    #[test]
    fn handles_sequences_split_across_chunks() {
        let mut tracker = OscTracker::default();
        assert_eq!(tracker.feed(b"\x1b]7;file://host/Us").1, None);
        let (_, cwd) = tracker.feed(b"ers/me\x07");
        assert_eq!(cwd, Some(PathBuf::from("/Users/me")));
    }

    #[test]
    fn ignores_unterminated_sequences() {
        let mut tracker = OscTracker::default();
        let (title, _) = tracker.feed(b"\x1b]2;still typing");
        assert_eq!(title, None);
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("/a%20b/c"), "/a b/c");
        assert_eq!(percent_decode("/100%"), "/100%");
        assert_eq!(percent_decode("/%zz"), "/%zz");
    }
}
