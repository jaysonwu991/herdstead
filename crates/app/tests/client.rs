//! Drives the real TUI in a PTY: typing, the prefix, the composer, `@path`
//! completion, detach and reattach.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use herdstead_core::paths::Paths;
use herdstead_core::protocol::{decode_line, encode_line, Event, Request};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    session: String,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("herdstead-client-{}-{tag}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::create_dir_all(root.join("state")).unwrap();
        std::fs::create_dir_all(root.join("run")).unwrap();
        let project = root.join("project");
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(
            root.join("config/config.toml"),
            "shell = \"/bin/sh\"\nsidebar = true\n",
        )
        .unwrap();
        std::fs::write(
            project.join("README.md"),
            "# Tui Demo\n\nhello from markdown\n",
        )
        .unwrap();
        std::fs::write(
            project.join("src/main.rs"),
            "fn main() { println!(\"hi\"); }\n",
        )
        .unwrap();
        Self {
            root,
            project,
            session: "tui".to_string(),
        }
    }

    fn paths(&self) -> Paths {
        Paths::with_roots(
            self.root.join("config"),
            self.root.join("state"),
            self.root.join("run"),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        kill_session(&self.paths(), &self.session);
        std::fs::remove_dir_all(&self.root).ok();
    }
}

struct Tui {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    buffer: Arc<Mutex<Vec<u8>>>,
    master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Tui {
    fn start(fixture: &Fixture) -> Self {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("opening a pty");
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_herdstead"));
        command.args(["--session", &fixture.session]);
        command.cwd(&fixture.project);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("HERDSTEAD_CONFIG_DIR", fixture.root.join("config"));
        command.env("HERDSTEAD_STATE_DIR", fixture.root.join("state"));
        command.env("HERDSTEAD_RUNTIME_DIR", fixture.root.join("run"));
        let child = pair.slave.spawn_command(command).expect("starting the tui");
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().expect("pty reader");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let sink = buffer.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => {
                        let mut buffer = sink.lock().unwrap();
                        buffer.extend_from_slice(&chunk[..read]);
                        if buffer.len() > 2 * 1024 * 1024 {
                            let drain = buffer.len() - 1024 * 1024;
                            buffer.drain(..drain);
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        let writer = pair.master.take_writer().expect("pty writer");
        Self {
            child,
            writer,
            buffer,
            master: pair.master,
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("writing to the tui");
        self.writer.flush().ok();
    }

    fn send_text(&mut self, text: &str) {
        self.send(text.as_bytes());
    }

    fn screen(&self) -> String {
        String::from_utf8_lossy(&self.buffer.lock().unwrap()).into_owned()
    }

    /// Waits until the raw output contains `needle`.
    fn wait_for(&self, needle: &str, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if self.screen().contains(needle) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!(
            "timed out waiting for {what} ({needle:?}).\nScreen so far:\n{}",
            self.screen()
        );
    }

    fn wait_exit(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if self.child.try_wait().expect("polling the tui").is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("the tui did not exit");
    }

    fn resize(&self, rows: u16, cols: u16) {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("resizing the pty");
    }
}

fn kill_session(paths: &Paths, session: &str) {
    let socket = paths.socket(session);
    if let Ok(mut stream) = UnixStream::connect(&socket) {
        let _ = stream.write_all(&encode_line(&Request::Kill).unwrap());
        let _ = stream.flush();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && socket.exists() {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    std::fs::remove_file(&socket).ok();
}

/// Reads the first snapshot from a live session.
fn inspect(paths: &Paths, session: &str) -> Option<herdstead_core::protocol::Snapshot> {
    let stream = UnixStream::connect(paths.socket(session)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(400)))
        .ok()?;
    let mut reader = std::io::BufReader::new(stream.try_clone().ok()?);
    let mut stream = stream;
    let inspect = encode_line(&Request::Inspect {
        session: session.to_string(),
    })
    .ok()?;
    stream.write_all(&inspect).ok()?;
    stream.flush().ok()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let mut line = String::new();
        match std::io::BufRead::read_line(&mut reader, &mut line) {
            Ok(0) => return None,
            Ok(_) if line.trim().is_empty() => continue,
            Ok(_) => {
                if let Ok(Event::Snapshot { snapshot }) = decode_line(line.trim_end()) {
                    return Some(*snapshot);
                }
            }
            Err(_) => continue,
        }
    }
    None
}

fn open_socket(paths: &Paths, session: &str) -> Option<UnixStream> {
    UnixStream::connect(paths.socket(session)).ok()
}

#[test]
fn attaching_without_a_terminal_fails_and_starts_nothing() {
    let fixture = Fixture::new("notty");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_herdstead"))
        .args(["--session", &fixture.session])
        .current_dir(&fixture.project)
        .env("HERDSTEAD_CONFIG_DIR", fixture.root.join("config"))
        .env("HERDSTEAD_STATE_DIR", fixture.root.join("state"))
        .env("HERDSTEAD_RUNTIME_DIR", fixture.root.join("run"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("running the binary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("needs an interactive terminal"),
        "unexpected error: {stderr}"
    );
    assert!(
        !fixture.paths().socket(&fixture.session).exists(),
        "a failed attach must not leave a daemon behind"
    );
}

#[test]
fn the_client_draws_chrome_and_runs_a_shell() {
    let fixture = Fixture::new("draw");
    let mut tui = Tui::start(&fixture);
    tui.wait_for("WORKSPACES", "the sidebar");
    tui.wait_for("prompt the agent", "the composer hint");
    tui.wait_for("shell", "the tab strip");

    tui.send_text("echo tui-marker\r");
    tui.wait_for("tui-marker", "the shell's output");
}

#[test]
fn the_prefix_splits_panes_and_the_runtime_agrees() {
    let fixture = Fixture::new("prefix");
    let mut tui = Tui::start(&fixture);
    tui.wait_for("prompt the agent", "the composer hint");
    // Ctrl+B then v splits side by side.
    tui.send(&[0x02]);
    tui.send_text("v");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = inspect(&fixture.paths(), &fixture.session);
        if let Some(snapshot) = snapshot {
            if snapshot.panes.len() == 2 {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "the split never reached the runtime"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // Closing the focused pane brings it back to one.
    tui.send(&[0x02]);
    tui.send_text("x");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = inspect(&fixture.paths(), &fixture.session);
        if let Some(snapshot) = snapshot {
            if snapshot.panes.len() == 1 {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "the close never reached the runtime"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_composer_sends_a_prompt_to_the_focused_pane() {
    let fixture = Fixture::new("composer");
    let mut tui = Tui::start(&fixture);
    tui.wait_for("prompt the agent", "the composer hint");
    // Open the composer, type a prompt, send it.
    tui.send(&[0x02]);
    tui.send_text(":");
    tui.wait_for("❯", "the composer prompt");
    tui.send_text("echo composer-ok\r");
    // The text is typed into the composer, so seeing it in the pane proves the
    // send path; the composer is cleared afterwards.
    tui.wait_for("composer-ok", "the composer's prompt reaching the shell");
}

#[test]
fn completion_inserts_a_path_reference() {
    let fixture = Fixture::new("at");
    let mut tui = Tui::start(&fixture);
    tui.wait_for("prompt the agent", "the composer hint");
    tui.send(&[0x02]);
    tui.send_text(":");
    tui.send_text("cat @REA");
    tui.wait_for("@path", "the completion popup");
    tui.wait_for("README.md", "the completion entry");
    // Tab accepts the completion, then Enter sends the line.
    tui.send(b"\t");
    tui.send(b"\r");
    tui.wait_for("@README.md", "the accepted reference reaching the shell");
}

#[test]
fn markdown_preview_renders_the_composer() {
    let fixture = Fixture::new("preview");
    let mut tui = Tui::start(&fixture);
    tui.wait_for("prompt the agent", "the composer hint");
    tui.send(&[0x02]);
    tui.send_text("m");
    tui.send_text("# Preview Title\r");
    tui.wait_for("Preview Title", "the typed heading");
    tui.send(&[0x10]); // Ctrl+P
    tui.wait_for("preview", "the preview label");
}

#[test]
fn composer_opens_markdown_and_tree_viewers() {
    let fixture = Fixture::new("viewers");
    let mut tui = Tui::start(&fixture);
    tui.wait_for("prompt the agent", "the composer hint");

    // Render README.md in a viewer pane beside the shell.
    tui.send(&[0x02]);
    tui.send_text(":");
    tui.send_text("/md README.md\r");
    // ratatui skips repainting blank cells, so wait for words rather than
    // runs that contain spaces.
    tui.wait_for("Demo", "the rendered Markdown heading");
    tui.wait_for("markdown", "the rendered Markdown body");

    // The composer stays open after a command, so the next one goes straight
    // into it; `/tree` reuses the viewer pane.
    tui.send_text("/tree .\r");
    tui.wait_for("README.md", "the tree entry");
    tui.wait_for("src/", "the tree folder");
}

#[test]
fn detach_leaves_the_panes_running_and_reattach_shows_them() {
    let fixture = Fixture::new("detach");
    let mut tui = Tui::start(&fixture);
    tui.wait_for("prompt the agent", "the composer hint");
    tui.send_text("echo survives-detach\r");
    tui.wait_for("survives-detach", "the shell output");
    tui.send(&[0x02]);
    tui.send_text("q");
    tui.wait_exit();
    assert!(
        open_socket(&fixture.paths(), &fixture.session).is_some(),
        "the daemon should keep running after detaching"
    );
    let snapshot = inspect(&fixture.paths(), &fixture.session).expect("a live session");
    let text: String = snapshot
        .panes
        .iter()
        .filter_map(|pane| pane.grid.as_ref())
        .map(|grid| grid.text())
        .collect();
    assert!(
        text.contains("survives-detach"),
        "the pane kept its output: {text}"
    );

    // A second client attaches to the same session and redraws its history.
    let mut again = Tui::start(&fixture);
    again.wait_for("WORKSPACES", "the reattached session");
    again.wait_for("survives-detach", "the preserved scrollback");
    again.send(&[0x02]);
    again.send_text("q");
    again.wait_exit();
}

#[test]
fn resizing_the_terminal_relayouts_panes() {
    let fixture = Fixture::new("resize");
    let tui = Tui::start(&fixture);
    tui.wait_for("prompt the agent", "the composer hint");
    tui.resize(20, 60);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(snapshot) = inspect(&fixture.paths(), &fixture.session) {
            if snapshot.size.cols == 60 {
                assert_eq!(snapshot.size.rows, 20);
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "the client never reported the new size"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
