//! End-to-end checks against a real session daemon: a shell pane, splits,
//! scrolling, viewers and shutdown, all over the socket protocol.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use herdstead_core::geom::Rect;
use herdstead_core::model::{PaneKind, Size, ViewerKind};
use herdstead_core::paths::Paths;
use herdstead_core::protocol::{
    decode_line, encode_line, Action, Event, Key, KeyInput, Request, Snapshot,
};

struct Session {
    root: PathBuf,
    child: Child,
    name: String,
}

impl Session {
    fn start(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("herdstead-it-{}-{tag}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::create_dir_all(root.join("state")).unwrap();
        std::fs::create_dir_all(root.join("run")).unwrap();
        std::fs::create_dir_all(root.join("project")).unwrap();
        std::fs::write(
            root.join("config/config.toml"),
            "shell = \"/bin/sh\"\nrestore = true\nscrollback = 2000\n",
        )
        .unwrap();
        std::fs::write(
            root.join("project/README.md"),
            "# Demo\n\nsee `src/main.rs`\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("project/src")).unwrap();
        std::fs::write(root.join("project/src/main.rs"), "fn main() {}\n").unwrap();
        let name = "it".to_string();
        let child = Command::new(env!("CARGO_BIN_EXE_herdstead"))
            .args(["--server", "--session", &name])
            .current_dir(root.join("project"))
            .env("HERDSTEAD_CONFIG_DIR", root.join("config"))
            .env("HERDSTEAD_STATE_DIR", root.join("state"))
            .env("HERDSTEAD_RUNTIME_DIR", root.join("run"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("starting the daemon");
        let session = Self { root, child, name };
        session.wait_for_socket();
        session
    }

    fn paths(&self) -> Paths {
        Paths::with_roots(
            self.root.join("config"),
            self.root.join("state"),
            self.root.join("run"),
        )
    }

    fn wait_for_socket(&self) {
        let socket = self.paths().socket(&self.name);
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if UnixStream::connect(&socket).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the daemon never bound {}", socket.display());
    }

    fn connect(&self) -> Conn {
        let socket = self.paths().socket(&self.name);
        let stream = UnixStream::connect(&socket).expect("connecting");
        // A short timeout lets `wait` poll instead of blocking on one read.
        stream
            .set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        let mut conn = Conn { stream, reader };
        conn.send(&Request::Hello {
            session: self.name.clone(),
            size: Size {
                rows: 30,
                cols: 100,
            },
            area: Rect::new(0, 1, 100, 27),
        })
        .unwrap();
        conn
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = ConnectionGuard::kill(&self.paths(), &self.name);
        std::fs::remove_dir_all(&self.root).ok();
    }
}

struct Conn {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Conn {
    fn send(&mut self, request: &Request) -> std::io::Result<()> {
        let bytes = encode_line(request).unwrap();
        self.stream.write_all(&bytes)?;
        self.stream.flush()
    }

    fn action(&mut self, action: Action) -> std::io::Result<()> {
        self.send(&Request::Action { action })
    }

    /// Reads one snapshot, or `None` when none arrived before the socket's
    /// read timeout.
    fn next_snapshot(&mut self) -> Option<Snapshot> {
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => panic!("the daemon closed the connection"),
                Ok(_) if line.trim().is_empty() => continue,
                Ok(_) => match decode_line::<Event>(line.trim_end()).expect("decoding a frame") {
                    Event::Snapshot { snapshot } => return Some(*snapshot),
                    Event::Error { message } => panic!("server error: {message}"),
                    Event::Detached { reason } => panic!("detached: {reason}"),
                },
                Err(err)
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return None;
                }
                Err(err) => panic!("reading a frame: {err}"),
            }
        }
    }

    fn snapshot(&mut self) -> Snapshot {
        self.wait("a snapshot", |_| true)
    }

    fn wait<F: Fn(&Snapshot) -> bool>(&mut self, what: &str, predicate: F) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if let Some(snapshot) = self.next_snapshot() {
                if predicate(&snapshot) {
                    return snapshot;
                }
            }
        }
        panic!("timed out waiting for {what}");
    }
}

/// A minimal killer, so `Drop` can stop the daemon without the app crate.
struct ConnectionGuard;

impl ConnectionGuard {
    fn kill(paths: &Paths, session: &str) -> std::io::Result<()> {
        let mut stream = UnixStream::connect(paths.socket(session))?;
        stream.write_all(&encode_line(&Request::Kill).unwrap())?;
        stream.flush()?;
        Ok(())
    }
}

fn pane_text(snapshot: &Snapshot, id: herdstead_core::ids::PaneId) -> String {
    snapshot
        .panes
        .iter()
        .find(|pane| pane.id == id)
        .and_then(|pane| pane.grid.as_ref())
        .map(|grid| grid.text())
        .unwrap_or_default()
}

#[test]
fn a_session_runs_a_shell_and_streams_its_output() {
    let session = Session::start("shell");
    let mut conn = session.connect();
    let snapshot = conn.snapshot();
    assert_eq!(snapshot.session, "it");
    assert_eq!(snapshot.workspaces.len(), 1);
    assert_eq!(snapshot.panes.len(), 1);
    let pane = &snapshot.panes[0];
    assert_eq!(pane.kind, PaneKind::Shell);
    assert_eq!(snapshot.focused, Some(pane.id));

    // Type a command into the focused pane.
    conn.action(Action::SendText {
        text: "echo marker-ok\n".into(),
        submit: false,
    })
    .unwrap();
    let snapshot = conn.wait("the echoed marker", |snapshot| {
        pane_text(snapshot, pane.id).contains("marker-ok")
    });
    assert!(pane_text(&snapshot, pane.id).contains("marker-ok"));
}

#[test]
fn splits_focus_and_close_keep_the_tree_consistent() {
    let session = Session::start("split");
    let mut conn = session.connect();
    let first = conn.snapshot().panes[0].id;

    conn.action(Action::Split {
        dir: herdstead_core::layout::SplitDir::Vertical,
    })
    .unwrap();
    let snapshot = conn.wait("a second pane", |snapshot| snapshot.panes.len() == 2);
    assert_eq!(snapshot.panes.len(), 2);
    let framed = snapshot.focused.expect("focus follows the new pane");
    assert_ne!(framed, first);
    // Panes are laid out side by side inside the area the client sent.
    let mut rects: Vec<_> = snapshot.panes.iter().map(|pane| pane.rect).collect();
    rects.sort_by_key(|rect| rect.x);
    assert_eq!(rects[0].y, rects[1].y);
    assert!(rects[1].x > rects[0].x);

    conn.action(Action::ClosePane).unwrap();
    let snapshot = conn.wait("one pane again", |snapshot| snapshot.panes.len() == 1);
    assert_eq!(snapshot.panes[0].id, first);

    // Tabs and workspaces.
    conn.action(Action::NewTab { cwd: None }).unwrap();
    let snapshot = conn.wait("a second tab", |snapshot| {
        snapshot.workspaces[0].tabs.len() == 2
    });
    assert_eq!(snapshot.workspaces[0].tabs.len(), 2);
    conn.action(Action::NewWorkspace { dir: None }).unwrap();
    conn.wait("a second workspace", |snapshot| {
        snapshot.workspaces.len() == 2
    });
}

#[test]
fn scrollback_and_links_reach_the_client() {
    let session = Session::start("scroll");
    let mut conn = session.connect();
    let pane = conn.snapshot().panes[0].id;
    conn.action(Action::SendText {
        text: "for i in $(seq 1 80); do echo line-$i; done\n".into(),
        submit: false,
    })
    .unwrap();
    conn.wait("the numbered lines", |snapshot| {
        pane_text(snapshot, pane).contains("line-80")
    });

    conn.action(Action::ScrollToTop { pane }).unwrap();
    let snapshot = conn.wait("a scrolled view", |snapshot| {
        snapshot
            .panes
            .iter()
            .find(|frame| frame.id == pane)
            .and_then(|frame| frame.grid.as_ref())
            .map(|grid| grid.offset > 0)
            .unwrap_or(false)
    });
    let grid = snapshot.panes[0].grid.as_ref().unwrap();
    assert!(grid.offset > 0);
    assert!(!grid.lines.is_empty());

    conn.action(Action::ScrollToBottom { pane }).unwrap();
    conn.wait("the live view", |snapshot| {
        snapshot.panes[0]
            .grid
            .as_ref()
            .map(|grid| grid.offset == 0)
            .unwrap_or(false)
    });
}

#[test]
fn viewers_open_files_and_folders_without_a_pty() {
    let session = Session::start("viewer");
    let mut conn = session.connect();
    conn.action(Action::OpenViewer {
        path: "README.md".into(),
        kind: None,
        split: None,
    })
    .unwrap();
    let snapshot = conn.wait("a markdown viewer", |snapshot| {
        snapshot
            .panes
            .iter()
            .any(|pane| pane.kind == PaneKind::Viewer)
    });
    let viewer = snapshot
        .panes
        .iter()
        .find(|pane| pane.kind == PaneKind::Viewer)
        .expect("a viewer pane");
    let spec = viewer.viewer.as_ref().expect("a viewer spec");
    assert_eq!(spec.kind, ViewerKind::Markdown);
    assert!(spec.path.ends_with("README.md"));
    assert!(viewer.grid.is_none(), "viewers render on the client");

    // Reopening in the focused viewer reuses the pane.
    conn.action(Action::OpenViewer {
        path: "src".into(),
        kind: None,
        split: None,
    })
    .unwrap();
    let snapshot = conn.wait("a tree viewer", |snapshot| {
        snapshot.panes.iter().any(|pane| {
            pane.viewer
                .as_ref()
                .is_some_and(|spec| spec.kind == ViewerKind::Tree)
        })
    });
    let viewers = snapshot
        .panes
        .iter()
        .filter(|pane| pane.kind == PaneKind::Viewer)
        .count();
    assert_eq!(viewers, 1, "the viewer pane is reused");
}

#[test]
fn keys_reach_the_shell_and_kill_stops_the_session() {
    let session = Session::start("keys");
    let mut conn = session.connect();
    let pane = conn.snapshot().panes[0].id;
    // Type `echo keystroke` one key at a time, then Enter.
    for ch in "echo keystroke".chars() {
        conn.action(Action::Key {
            input: KeyInput::new(Key::Char(ch)),
        })
        .unwrap();
    }
    conn.action(Action::Key {
        input: KeyInput::new(Key::Enter),
    })
    .unwrap();
    // The shell echoes the characters as they arrive, so wait for the second
    // mention, which is the command's own output.
    let snapshot = conn.wait("typed output", |snapshot| {
        pane_text(snapshot, pane).matches("keystroke").count() >= 2
    });
    let text = pane_text(&snapshot, pane);
    assert!(
        text.matches("keystroke").count() >= 2,
        "expected the command and its output:\n{text}"
    );

    conn.send(&Request::Kill).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !session.paths().socket(&session.name).exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the socket outlived the session");
}

#[test]
fn a_restored_session_keeps_its_layout() {
    let session = Session::start("restore");
    {
        let mut conn = session.connect();
        conn.action(Action::Split {
            dir: herdstead_core::layout::SplitDir::Horizontal,
        })
        .unwrap();
        conn.wait("two panes", |snapshot| snapshot.panes.len() == 2);
        conn.action(Action::RenameTab {
            name: "restored-tab".into(),
        })
        .unwrap();
        conn.wait("the renamed tab", |snapshot| {
            snapshot.workspaces[0].tabs[0].name == "restored-tab"
        });
        conn.send(&Request::Kill).unwrap();
    }
    // Wait for the daemon to save and exit.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && session.paths().socket(&session.name).exists() {
        std::thread::sleep(Duration::from_millis(20));
    }
    let saved = session.paths().session_file(&session.name);
    assert!(saved.exists(), "the layout was saved");

    // Start a second daemon against the same state directory.
    let child = Command::new(env!("CARGO_BIN_EXE_herdstead"))
        .args(["--server", "--session", "it"])
        .current_dir(session.root.join("project"))
        .env("HERDSTEAD_CONFIG_DIR", session.root.join("config"))
        .env("HERDSTEAD_STATE_DIR", session.root.join("state"))
        .env("HERDSTEAD_RUNTIME_DIR", session.root.join("run"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("restarting the daemon");
    let restart = RestartGuard { child: Some(child) };
    session.wait_for_socket();
    let mut conn = session.connect();
    let snapshot = conn.wait("the restored tab", |snapshot| {
        snapshot.workspaces[0].tabs[0].name == "restored-tab"
    });
    assert_eq!(snapshot.workspaces[0].tabs[0].name, "restored-tab");
    assert_eq!(snapshot.workspaces[0].tabs[0].panes, 2);
    conn.send(&Request::Kill).unwrap();
    drop(restart);
}

struct RestartGuard {
    child: Option<Child>,
}

impl Drop for RestartGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn unknown_requests_report_an_error_instead_of_dropping_the_client() {
    let session = Session::start("errors");
    let mut conn = session.connect();
    conn.send(&Request::Hello {
        session: "another-session".into(),
        size: Size { rows: 10, cols: 10 },
        area: Rect::new(0, 0, 10, 10),
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let mut line = String::new();
        match conn.reader.read_line(&mut line) {
            Ok(0) => panic!("the daemon closed the connection"),
            Ok(_) if line.trim().is_empty() => continue,
            Ok(_) => match decode_line::<Event>(line.trim_end()).unwrap() {
                Event::Error { message } => {
                    assert!(message.contains("another-session"));
                    return;
                }
                Event::Snapshot { .. } => continue,
                Event::Detached { reason } => panic!("detached: {reason}"),
            },
            Err(err) => panic!("read error: {err}"),
        }
    }
    panic!("no error arrived");
}

#[test]
fn the_layout_survives_a_restart_of_the_client() {
    let session = Session::start("reattach");
    let mut first = session.connect();
    let snapshot = first.snapshot();
    assert_eq!(snapshot.panes.len(), 1);
    drop(first);
    let mut second = session.connect();
    let snapshot = second.snapshot();
    assert_eq!(snapshot.panes.len(), 1);
    assert_eq!(snapshot.panes[0].kind, PaneKind::Shell);
}
