//! The session daemon: it owns panes, layout and focus, and streams snapshots
//! to whichever clients are attached. Detaching leaves the panes running.

mod grid;
mod keys;
mod pane;

use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use herdstead_core::config::Config;
use herdstead_core::detect::{self, AgentKind};
use herdstead_core::geom::Rect;
use herdstead_core::ids::PaneId;
use herdstead_core::layout::SplitDir;
use herdstead_core::model::{PaneMeta, SessionState, Size, ViewerKind, ViewerSpec};
use herdstead_core::paths::Paths;
use herdstead_core::protocol::{
    decode_line, encode_line, Action, Event, PaneFrame, Request, Snapshot, TabView, WorkspaceView,
};

use pane::PanePty;

/// Set by the signal handler, checked by the main loop.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);
static SIGNAL_HANDLER_READY: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_signal(_: i32) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

fn install_signal_handlers() {
    if SIGNAL_HANDLER_READY.swap(true, Ordering::SeqCst) {
        return;
    }
    // Writing to a closed socket must not kill the daemon.
    unsafe {
        libc_signal(libc_sigpipe(), 1);
        libc_signal(libc_sigterm(), handle_signal as *const () as usize);
        libc_signal(libc_sigint(), handle_signal as *const () as usize);
        libc_signal(libc_sighup(), 1);
    }
}

extern "C" {
    #[link_name = "signal"]
    fn libc_signal(signal: i32, handler: usize) -> usize;
}

fn libc_sigpipe() -> i32 {
    13
}

fn libc_sigterm() -> i32 {
    15
}

fn libc_sigint() -> i32 {
    2
}

fn libc_sighup() -> i32 {
    1
}

/// How often pane activity is re-classified.
const TICK: Duration = Duration::from_millis(250);
/// How long structural changes wait before being written to disk.
const SAVE_INTERVAL: Duration = Duration::from_secs(2);

pub fn run(session: String) -> Result<()> {
    let paths = Paths::discover();
    paths.ensure().context("creating herdstead folders")?;
    let cwd = std::env::current_dir().context("reading the current folder")?;
    let (config, mut warnings) = Config::load(&cwd, &paths);

    let socket = paths.socket(&session);
    if UnixStream::connect(&socket).is_ok() {
        // Another daemon already owns the session; nothing to do.
        return Ok(());
    }
    std::fs::remove_file(&socket).ok();
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let listener =
        UnixListener::bind(&socket).with_context(|| format!("binding {}", socket.display()))?;
    install_signal_handlers();

    let (tx, rx) = mpsc::channel::<Msg>();
    let mut runtime = Runtime::new(session.clone(), cwd, paths.clone(), config, tx.clone())?;
    runtime.warnings.append(&mut warnings);

    let accept_tx = tx.clone();
    std::thread::spawn(move || {
        let mut next_id: u64 = 1;
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    spawn_client(stream, next_id, accept_tx.clone());
                    next_id += 1;
                }
                Err(_) => break,
            }
        }
    });

    runtime.main_loop(rx);
    runtime.shutdown();
    std::fs::remove_file(&socket).ok();
    Ok(())
}

/// Messages from client threads and pane readers into the runtime.
enum Msg {
    Connected {
        id: u64,
        events: SyncSender<Vec<u8>>,
    },
    Request {
        id: u64,
        request: Request,
    },
    ProtocolError {
        id: u64,
        message: String,
    },
    Disconnected {
        id: u64,
    },
    PaneOutput {
        pane: PaneId,
        bytes: Vec<u8>,
        title: Option<String>,
        cwd: Option<PathBuf>,
    },
    PaneExited {
        pane: PaneId,
        code: Option<i32>,
    },
    PaneClosed,
}

struct ClientHandle {
    /// Bounded: a client that cannot keep up drops frames rather than growing
    /// the daemon's memory. Every frame is a whole snapshot, so the next one is
    /// always a complete picture.
    events: SyncSender<Vec<u8>>,
    size: Size,
    attached: bool,
}

fn spawn_client(stream: UnixStream, id: u64, tx: Sender<Msg>) {
    let Ok(read_stream) = stream.try_clone() else {
        return;
    };
    let (events_tx, events_rx) = mpsc::sync_channel::<Vec<u8>>(2);
    // Writer: one thread per client so a slow reader cannot stall the runtime.
    std::thread::spawn(move || {
        let mut stream = stream;
        for bytes in events_rx {
            if stream.write_all(&bytes).is_err() {
                break;
            }
            let _ = stream.flush();
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
    });
    // Reader: frames in.
    let reader_tx = tx.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(read_stream);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match decode_line::<Request>(line.trim_end()) {
                Ok(request) => {
                    if reader_tx.send(Msg::Request { id, request }).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = reader_tx.send(Msg::ProtocolError {
                        id,
                        message: err.to_string(),
                    });
                }
            }
        }
        let _ = reader_tx.send(Msg::Disconnected { id });
    });
    let _ = tx.send(Msg::Connected {
        id,
        events: events_tx,
    });
}

struct Runtime {
    session: String,
    state: SessionState,
    panes: BTreeMap<PaneId, PanePty>,
    clients: BTreeMap<u64, ClientHandle>,
    paths: Paths,
    config: Config,
    tx: Sender<Msg>,
    pane_area: Rect,
    start_cwd: PathBuf,
    /// Panes whose title an application set through OSC; those are left alone.
    titled: HashSet<PaneId>,
    dirty: bool,
    quit: bool,
    last_tick: Instant,
    last_save: Instant,
    saved_seq: u64,
    warnings: Vec<String>,
}

impl Runtime {
    fn new(
        session: String,
        cwd: PathBuf,
        paths: Paths,
        config: Config,
        tx: Sender<Msg>,
    ) -> Result<Self> {
        let mut runtime = Self {
            session,
            state: SessionState::new(
                String::new(),
                cwd.clone(),
                PaneMeta::shell(PaneId(1), cwd.clone(), config.shell_command()),
            ),
            panes: BTreeMap::new(),
            clients: BTreeMap::new(),
            paths,
            config,
            tx,
            pane_area: Rect::new(0, 1, 78, 37),
            start_cwd: cwd.clone(),
            titled: HashSet::new(),
            dirty: true,
            quit: false,
            last_tick: Instant::now(),
            last_save: Instant::now(),
            saved_seq: 0,
            warnings: Vec::new(),
        };
        runtime.state.name = runtime.session.clone();
        runtime.state.size = Size {
            rows: 40,
            cols: 120,
        };
        let restored = if runtime.config.restore {
            runtime.restore()
        } else {
            false
        };
        if !restored {
            runtime.state = SessionState::new(
                runtime.session.clone(),
                cwd.clone(),
                PaneMeta {
                    command: runtime.config.shell_command(),
                    last_output_ms: now_ms(),
                    ..PaneMeta::shell(PaneId(1), cwd, runtime.config.shell_command())
                },
            );
            runtime.state.size = Size {
                rows: 40,
                cols: 120,
            };
        }
        runtime.relayout();
        runtime.reconcile();
        runtime.save();
        Ok(runtime)
    }

    fn restore(&mut self) -> bool {
        let path = self.paths.session_file(&self.session);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return false;
        };
        let Ok(mut state) = serde_json::from_str::<SessionState>(&text) else {
            self.warnings.push(format!(
                "{}: unreadable session, starting fresh",
                path.display()
            ));
            return false;
        };
        if state.workspaces.is_empty() {
            return false;
        }
        state.name = self.session.clone();
        // Panes whose folder disappeared fall back to the session's first folder.
        for workspace in &mut state.workspaces {
            if !workspace.dir.is_dir() {
                workspace.dir = self.start_cwd.clone();
            }
            for tab in &mut workspace.tabs {
                if !tab.dir.is_dir() {
                    tab.dir = workspace.dir.clone();
                }
            }
        }
        for meta in state.panes.values_mut() {
            if !meta.cwd.is_dir() {
                meta.cwd = self.start_cwd.clone();
            }
            meta.status = detect::PaneStatus::Idle;
            meta.bell = false;
            meta.last_output_ms = now_ms();
        }
        self.state = state;
        true
    }

    fn content_area(&self) -> Rect {
        self.pane_area
    }

    /// Resizes every visible pane and starts any pane that has no process yet.
    fn relayout(&mut self) {
        let area = self.content_area();
        let placements = self.state.active_placements(area);
        let scrollback = self.config.scrollback;
        let shell = self.config.shell_command();
        for placement in placements {
            let Some(meta) = self.state.panes.get(&placement.pane) else {
                continue;
            };
            if !meta.is_shell() {
                continue;
            }
            let inner = placement.rect.inner();
            let rows = inner.height.max(1);
            let cols = inner.width.max(1);
            match self.panes.get_mut(&placement.pane) {
                Some(pty) => pty.resize(rows, cols),
                None => {
                    let cwd = meta.cwd.clone();
                    match PanePty::spawn(
                        placement.pane,
                        &shell,
                        &cwd,
                        rows,
                        cols,
                        scrollback,
                        self.tx.clone(),
                    ) {
                        Ok(pty) => {
                            self.panes.insert(placement.pane, pty);
                        }
                        Err(err) => {
                            self.warnings.push(format!("{err}"));
                            if let Some(meta) = self.state.panes.get_mut(&placement.pane) {
                                meta.status = detect::PaneStatus::Exited;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Kills panes that are no longer part of any tab.
    fn reconcile(&mut self) {
        let live: HashSet<PaneId> = self
            .state
            .workspaces
            .iter()
            .flat_map(|workspace| workspace.panes())
            .collect();
        let dead: Vec<PaneId> = self
            .state
            .panes
            .keys()
            .copied()
            .filter(|pane| !live.contains(pane))
            .collect();
        for pane in dead {
            self.state.panes.remove(&pane);
            if let Some(mut pty) = self.panes.remove(&pane) {
                pty.kill();
            }
        }
    }

    fn main_loop(&mut self, rx: Receiver<Msg>) {
        loop {
            if SHUTDOWN.load(Ordering::SeqCst) {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(msg) => {
                    self.handle(msg);
                    while let Ok(msg) = rx.try_recv() {
                        self.handle(msg);
                        if self.quit {
                            break;
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if self.quit {
                break;
            }
            if self.last_tick.elapsed() >= TICK {
                self.tick();
                self.last_tick = Instant::now();
            }
            if self.dirty {
                self.broadcast();
                self.dirty = false;
            }
            if self.state.seq != self.saved_seq && self.last_save.elapsed() >= SAVE_INTERVAL {
                self.save();
            }
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Connected { id, events } => {
                self.clients.insert(
                    id,
                    ClientHandle {
                        events,
                        size: self.state.size,
                        attached: false,
                    },
                );
            }
            Msg::Request { id, request } => self.handle_request(id, request),
            Msg::ProtocolError { id, message } => self.send_error(id, &message),
            Msg::Disconnected { id } => {
                self.clients.remove(&id);
            }
            Msg::PaneOutput {
                pane,
                bytes,
                title,
                cwd,
            } => {
                if let Some(pty) = self.panes.get_mut(&pane) {
                    pty.parser.process(&bytes);
                    pty.last_output = Instant::now();
                }
                if let Some(meta) = self.state.panes.get_mut(&pane) {
                    meta.last_output_ms = now_ms();
                    meta.bell = false;
                    if let Some(title) = title {
                        if !title.trim().is_empty() {
                            meta.title = title;
                            self.titled.insert(pane);
                        }
                    }
                    if let Some(cwd) = cwd {
                        if cwd.is_dir() {
                            meta.cwd = cwd;
                        }
                    }
                }
                self.dirty = true;
            }
            Msg::PaneExited { pane, code } => {
                if let Some(pty) = self.panes.get_mut(&pane) {
                    pty.exit_code = code;
                    pty.closed = true;
                }
                if let Some(meta) = self.state.panes.get_mut(&pane) {
                    meta.exit_code = code;
                    meta.status = detect::PaneStatus::Exited;
                }
                self.state.touch();
                self.dirty = true;
            }
            Msg::PaneClosed => {}
        }
    }

    fn handle_request(&mut self, id: u64, request: Request) {
        match request {
            Request::Hello {
                session,
                size,
                area,
            } => {
                if session != self.session {
                    self.send_error(
                        id,
                        &format!("this daemon serves `{}`, not `{session}`", self.session),
                    );
                    return;
                }
                if let Some(client) = self.clients.get_mut(&id) {
                    client.attached = true;
                    client.size = size;
                }
                self.state.size = size;
                self.pane_area = clamp_area(area, size);
                self.relayout();
                self.dirty = true;
            }
            Request::Action { action } => self.apply(action, Some(id)),
            Request::Inspect { session } => {
                if session != self.session {
                    self.send_error(
                        id,
                        &format!("this daemon serves `{}`, not `{session}`", self.session),
                    );
                    return;
                }
                self.send_snapshot(id);
            }
            Request::Refresh => self.dirty = true,
            Request::Kill => {
                self.quit = true;
            }
        }
    }

    fn send_error(&mut self, id: u64, message: &str) {
        if let Some(client) = self.clients.get(&id) {
            if let Ok(bytes) = encode_line(&Event::Error {
                message: message.to_string(),
            }) {
                let _ = client.events.try_send(bytes);
            }
        }
    }

    fn apply(&mut self, action: Action, from: Option<u64>) {
        match action {
            Action::Key { input } => {
                if let Some(pty) = self.focused_pty() {
                    let bytes = keys::encode(&input, pty.parser.screen());
                    pty.write(&bytes);
                }
            }
            Action::Paste { text } => {
                if let Some(pty) = self.focused_pty() {
                    let bytes = keys::encode_paste(&text, pty.parser.screen());
                    pty.write(&bytes);
                }
            }
            Action::Split { dir } => self.split(dir),
            Action::ClosePane => {
                if let Some(pane) = self.state.focused() {
                    self.close_pane(pane);
                }
            }
            Action::KillPane { pane } => self.close_pane(pane),
            Action::CloseTab => {
                let panes: Vec<PaneId> =
                    self.state.tab().map(|tab| tab.panes()).unwrap_or_default();
                let _ = self.state.close_tab();
                for pane in panes {
                    if let Some(mut pty) = self.panes.remove(&pane) {
                        pty.kill();
                    }
                }
                self.after_close();
            }
            Action::CloseWorkspace => {
                let panes: Vec<PaneId> = self
                    .state
                    .workspace()
                    .map(|workspace| workspace.panes())
                    .unwrap_or_default();
                let index = self.state.active_workspace;
                self.state.close_workspace(index);
                for pane in panes {
                    if let Some(mut pty) = self.panes.remove(&pane) {
                        pty.kill();
                    }
                }
                self.after_close();
            }
            Action::FocusPane { pane } => {
                self.state.focus_pane(pane);
            }
            Action::FocusDir { dir } => {
                let area = self.content_area();
                self.state.focus_dir(dir, area);
            }
            Action::FocusCycle { step } => {
                self.state.focus_cycle(step);
            }
            Action::ToggleZoom => {
                self.state.toggle_zoom();
                self.relayout();
            }
            Action::ResizePane { dir, cells } => {
                let area = self.content_area();
                if self.state.resize(dir, f32::from(cells), area) {
                    self.relayout();
                }
            }
            Action::ScrollPane { pane, delta, page } => {
                if let Some(pty) = self.panes.get_mut(&pane) {
                    if pty.scroll(delta, page) {
                        self.dirty = true;
                    }
                }
            }
            Action::ScrollToBottom { pane } => {
                if let Some(pty) = self.panes.get_mut(&pane) {
                    if pty.scroll_to_bottom() {
                        self.dirty = true;
                    }
                }
            }
            Action::ScrollToTop { pane } => {
                if let Some(pty) = self.panes.get_mut(&pane) {
                    if pty.scroll_to_top() {
                        self.dirty = true;
                    }
                }
            }
            Action::NewTab { cwd } => {
                let dir = cwd
                    .map(PathBuf::from)
                    .filter(|path| path.is_dir())
                    .unwrap_or_else(|| self.focused_cwd());
                self.new_tab(&dir);
            }
            Action::NewWorkspace { dir } => {
                let dir = dir
                    .map(PathBuf::from)
                    .filter(|path| path.is_dir())
                    .unwrap_or_else(|| self.focused_cwd());
                self.new_workspace(&dir);
            }
            Action::SelectTab { index } => {
                self.state.select_tab(index);
            }
            Action::SelectWorkspace { index } => {
                self.state.select_workspace(index);
                self.relayout();
            }
            Action::CycleTab { step } => {
                self.state.cycle_tab(step);
            }
            Action::CycleWorkspace { step } => {
                self.state.cycle_workspace(step);
                self.relayout();
            }
            Action::RenameTab { name } => {
                if !name.trim().is_empty() {
                    self.state.rename_tab(name.trim());
                }
            }
            Action::RenameWorkspace { name } => {
                if !name.trim().is_empty() {
                    self.state.rename_workspace(name.trim());
                }
            }
            Action::RenamePane { name } => {
                if let Some(pane) = self.state.focused() {
                    if let Some(meta) = self.state.panes.get_mut(&pane) {
                        meta.title = name;
                        self.state.touch();
                    }
                }
            }
            Action::SendText { text, submit } => {
                if let Some(pty) = self.focused_pty() {
                    pty.write(text.as_bytes());
                    if submit {
                        pty.write(b"\r");
                    }
                }
            }
            Action::OpenViewer { path, kind, split } => {
                self.open_viewer(&path, kind, split);
            }
            Action::Resize { size, area } => {
                self.state.size = size;
                self.pane_area = clamp_area(area, size);
                if let Some(client) = from.and_then(|id| self.clients.get_mut(&id)) {
                    client.size = size;
                }
                self.relayout();
                self.dirty = true;
            }
            Action::Kill => {
                self.quit = true;
            }
            Action::Detach => {
                if let Some(client) = from.and_then(|id| self.clients.remove(&id)) {
                    if let Ok(bytes) = encode_line(&Event::Detached {
                        reason: "detached".to_string(),
                    }) {
                        let _ = client.events.try_send(bytes);
                    }
                }
            }
        }
        self.dirty = true;
    }

    fn after_close(&mut self) {
        self.reconcile();
        if self.state.workspaces.is_empty() || self.state.panes.is_empty() {
            self.new_workspace(&self.start_cwd.clone());
        } else {
            self.relayout();
        }
        self.dirty = true;
    }

    fn focused_pty(&mut self) -> Option<&mut PanePty> {
        let pane = self.state.focused()?;
        self.panes.get_mut(&pane)
    }

    fn focused_cwd(&self) -> PathBuf {
        self.state
            .focused_meta()
            .map(|meta| meta.cwd.clone())
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.start_cwd.clone())
    }

    fn split(&mut self, dir: SplitDir) {
        let cwd = self.focused_cwd();
        let shell = self.config.shell_command();
        let pane = self.state.ids.pane();
        self.state.insert_pane(PaneMeta {
            last_output_ms: now_ms(),
            ..PaneMeta::shell(pane, cwd, shell)
        });
        if !self.state.split_focused(dir, pane) {
            self.state.panes.remove(&pane);
            return;
        }
        self.relayout();
        self.dirty = true;
    }

    fn new_tab(&mut self, dir: &Path) {
        let shell = self.config.shell_command();
        let pane = self.state.ids.pane();
        self.state.insert_pane(PaneMeta {
            last_output_ms: now_ms(),
            ..PaneMeta::shell(pane, dir.to_path_buf(), shell)
        });
        let name = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "shell".to_string());
        self.state.add_tab(name, pane);
        self.relayout();
        self.dirty = true;
    }

    fn new_workspace(&mut self, dir: &Path) {
        let shell = self.config.shell_command();
        let pane = self.state.ids.pane();
        self.state.insert_pane(PaneMeta {
            last_output_ms: now_ms(),
            ..PaneMeta::shell(pane, dir.to_path_buf(), shell)
        });
        let name = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "workspace".to_string());
        self.state.add_workspace(name, dir.to_path_buf(), pane);
        self.relayout();
        self.dirty = true;
    }

    fn close_pane(&mut self, pane: PaneId) {
        if let Some(mut pty) = self.panes.remove(&pane) {
            pty.kill();
        }
        let outcome = self.state.close_pane(pane);
        self.reconcile();
        if outcome.empty {
            self.new_workspace(&self.start_cwd.clone());
        } else {
            self.relayout();
        }
        self.dirty = true;
    }

    /// Opens a folder tree, a rendered Markdown file or a highlighted file.
    fn open_viewer(&mut self, path: &str, kind: Option<ViewerKind>, split: Option<SplitDir>) {
        let base = self.focused_cwd();
        let resolved = resolve_path(&base, path);
        let Ok(metadata) = std::fs::metadata(&resolved) else {
            self.warnings
                .push(format!("{}: no such file or folder", resolved.display()));
            return;
        };
        let kind = kind.unwrap_or_else(|| ViewerKind::for_path(&resolved));
        let spec = ViewerSpec {
            path: resolved.clone(),
            kind,
        };
        // Reusing the focused viewer keeps a viewer from multiplying.
        let focused = self.state.focused();
        if let Some(pane) = focused {
            let is_viewer = self
                .state
                .panes
                .get(&pane)
                .map(|meta| !meta.is_shell())
                .unwrap_or(false);
            if is_viewer {
                if let Some(meta) = self.state.panes.get_mut(&pane) {
                    meta.viewer = Some(spec.clone());
                    meta.title = resolved
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| resolved.display().to_string());
                    meta.cwd = if metadata.is_dir() {
                        resolved.clone()
                    } else {
                        resolved
                            .parent()
                            .map(Path::to_path_buf)
                            .unwrap_or_else(|| base.clone())
                    };
                }
                self.state.touch();
                self.dirty = true;
                return;
            }
        }
        let pane = self.state.ids.pane();
        self.state.insert_pane(PaneMeta::viewer(pane, spec));
        let dir = split.unwrap_or(SplitDir::Vertical);
        if !self.state.split_focused(dir, pane) {
            self.state.panes.remove(&pane);
        }
        self.relayout();
        self.dirty = true;
    }

    fn tick(&mut self) {
        let now = now_ms();
        let mut changed = false;
        let ids: Vec<PaneId> = self.state.panes.keys().copied().collect();
        for pane in ids {
            let Some(meta) = self.state.panes.get(&pane) else {
                continue;
            };
            if !meta.is_shell() {
                continue;
            }
            let text = self.panes.get(&pane).map(|pty| pty.text_tail(40));
            let Some(text) = text else {
                continue;
            };
            let exited = meta.exit_code.is_some()
                || self.panes.get(&pane).map(|pty| pty.closed).unwrap_or(true);
            let quiet = now.saturating_sub(meta.last_output_ms);
            let status = detect::classify(&text, quiet, exited);
            let agent = AgentKind::detect(&meta.command, &meta.title)
                .or_else(|| detect::agent_from_text(&text));
            let cwd_title = meta
                .cwd
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "shell".to_string());
            let app_titled = self.titled.contains(&pane);
            // An agent's name is more useful than the folder behind it.
            let title = match &agent {
                Some(agent) => Some(format!("{} {}", agent.glyph(), agent.label())),
                None if !app_titled => Some(cwd_title),
                None => None,
            };
            if let Some(meta) = self.state.panes.get_mut(&pane) {
                if meta.status != status || meta.agent != agent {
                    changed = true;
                }
                meta.status = status;
                meta.agent = agent;
                if let Some(title) = title {
                    if meta.title != title {
                        meta.title = title;
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.state.touch();
            self.dirty = true;
        }
    }

    fn snapshot(&self) -> Snapshot {
        let workspaces = self
            .state
            .workspaces
            .iter()
            .map(|workspace| WorkspaceView {
                id: workspace.id,
                name: workspace.name.clone(),
                dir: workspace.dir.display().to_string(),
                status: workspace.status(&self.state.panes),
                panes: workspace.panes().len(),
                agent: workspace
                    .panes()
                    .iter()
                    .filter_map(|pane| {
                        self.state
                            .panes
                            .get(pane)
                            .and_then(|meta| meta.agent.clone())
                    })
                    .next(),
                tabs: workspace
                    .tabs
                    .iter()
                    .map(|tab| TabView {
                        id: tab.id,
                        name: tab.name.clone(),
                        status: tab.status(&self.state.panes),
                        panes: tab.panes().len(),
                        agent: tab
                            .panes()
                            .iter()
                            .filter_map(|pane| {
                                self.state
                                    .panes
                                    .get(pane)
                                    .and_then(|meta| meta.agent.clone())
                            })
                            .next(),
                    })
                    .collect(),
                active_tab: workspace.active_tab,
            })
            .collect();
        let area = self.content_area();
        let placements = self.state.active_placements(area);
        let panes = placements
            .into_iter()
            .filter_map(|placement| {
                let meta = self.state.panes.get(&placement.pane)?;
                let grid = self
                    .panes
                    .get(&placement.pane)
                    .map(|pty| grid::frame(&pty.parser, placement.rect));
                let scroll = grid.as_ref().map(|grid| grid.offset).unwrap_or(0);
                Some(PaneFrame {
                    id: meta.id,
                    rect: placement.rect,
                    title: meta.title.clone(),
                    cwd: meta.cwd.display().to_string(),
                    command: meta.command.clone(),
                    kind: meta.kind,
                    viewer: meta.viewer.clone(),
                    agent: meta.agent.clone(),
                    status: meta.status,
                    bell: meta.bell,
                    exit_code: meta.exit_code,
                    scroll,
                    grid,
                })
            })
            .collect();
        Snapshot {
            session: self.session.clone(),
            version: herdstead_core::VERSION.to_string(),
            seq: self.state.seq,
            size: self.state.size,
            area,
            workspaces,
            active_workspace: self.state.active_workspace,
            focused: self.state.focused(),
            zoomed: self.state.tab().map(|tab| tab.zoomed).unwrap_or(false),
            panes,
        }
    }

    fn send_snapshot(&self, id: u64) {
        let Some(client) = self.clients.get(&id) else {
            return;
        };
        if let Ok(bytes) = encode_line(&Event::Snapshot {
            snapshot: Box::new(self.snapshot()),
        }) {
            let _ = client.events.try_send(bytes);
        }
    }

    fn broadcast(&mut self) {
        let snapshot = self.snapshot();
        let Ok(bytes) = encode_line(&Event::Snapshot {
            snapshot: Box::new(snapshot),
        }) else {
            return;
        };
        self.clients.retain(|_, client| {
            match client.events.try_send(bytes.clone()) {
                Ok(()) => true,
                // Behind, but alive: keep it and let the next snapshot catch up.
                Err(TrySendError::Full(_)) => true,
                Err(TrySendError::Disconnected(_)) => false,
            }
        });
    }

    fn save(&mut self) {
        let path = self.paths.session_file(&self.session);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let Ok(json) = serde_json::to_string_pretty(&self.state) else {
            return;
        };
        let temporary = path.with_extension("json.tmp");
        if std::fs::write(&temporary, json).is_ok() {
            std::fs::rename(&temporary, &path).ok();
        }
        self.saved_seq = self.state.seq;
        self.last_save = Instant::now();
    }

    fn shutdown(&mut self) {
        if let Ok(bytes) = encode_line(&Event::Detached {
            reason: "the session stopped".to_string(),
        }) {
            for client in self.clients.values() {
                let _ = client.events.try_send(bytes.clone());
            }
        }
        for pty in self.panes.values_mut() {
            pty.kill();
        }
        self.save();
        for warning in &self.warnings {
            eprintln!("herdstead: {warning}");
        }
    }
}

/// Keeps a client's pane area inside its terminal.
fn clamp_area(area: Rect, size: Size) -> Rect {
    let x = area.x.min(size.cols.saturating_sub(1));
    let y = area.y.min(size.rows.saturating_sub(1));
    let width = area.width.min(size.cols.saturating_sub(x)).max(1);
    let height = area.height.min(size.rows.saturating_sub(y)).max(1);
    Rect::new(x, y, width, height)
}

/// Resolves a path a client sent against the focused pane's folder.
pub fn resolve_path(base: &Path, path: &str) -> PathBuf {
    let expanded = if let Some(rest) = path.strip_prefix("~/") {
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .unwrap_or_else(|_| PathBuf::from(path))
    } else {
        PathBuf::from(path)
    };
    if expanded.is_absolute() {
        expanded
    } else {
        base.join(expanded)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_and_home_paths() {
        let base = Path::new("/tmp/project");
        assert_eq!(
            resolve_path(base, "src/main.rs"),
            PathBuf::from("/tmp/project/src/main.rs")
        );
        assert_eq!(
            resolve_path(base, "/etc/hosts"),
            PathBuf::from("/etc/hosts")
        );
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            resolve_path(base, "~/notes.md"),
            PathBuf::from(home).join("notes.md")
        );
    }

    #[test]
    fn snapshots_are_encodable() {
        let (tx, _rx) = mpsc::channel();
        let paths = Paths::with_roots("/c", "/s", "/r");
        let runtime = Runtime::new(
            "test".into(),
            PathBuf::from("/tmp"),
            paths,
            Config::default(),
            tx,
        )
        .unwrap();
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.session, "test");
        assert_eq!(snapshot.workspaces.len(), 1);
        assert_eq!(snapshot.panes.len(), 1);
        assert_eq!(
            snapshot.panes[0].kind,
            herdstead_core::model::PaneKind::Shell
        );
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("\"shell\""));
    }
}
