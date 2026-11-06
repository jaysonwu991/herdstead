//! The terminal client: input, socket framing and the draw loop.

pub mod app;
pub mod layout;
pub mod ui;
pub mod viewer;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event as CrosstermEvent, KeyEventKind,
};
use crossterm::{execute, terminal};
use herdr_plus_core::config::Config;
use herdr_plus_core::geom::Rect;
use herdr_plus_core::model::Size;
use herdr_plus_core::paths::Paths;
use herdr_plus_core::protocol::{decode_line, encode_line, Action, Event, Request};
use herdr_plus_core::Theme;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::client;
use app::{App, AppEvent};

type Tui = Terminal<CrosstermBackend<std::io::Stdout>>;

pub fn run(
    session: String,
    cwd: PathBuf,
    paths: Paths,
    config: Config,
    theme: Theme,
    warnings: Vec<String>,
) -> Result<()> {
    // Take over the terminal first: if there is no usable TTY (a pipe, a
    // script, an unattended shell), fail before a daemon is started.
    let mut terminal = setup_terminal()?;
    let result = attach_and_run(&mut terminal, session, cwd, paths, config, theme, warnings);
    restore_terminal(&mut terminal);
    result
}

fn attach_and_run(
    terminal: &mut Tui,
    session: String,
    cwd: PathBuf,
    paths: Paths,
    config: Config,
    theme: Theme,
    warnings: Vec<String>,
) -> Result<()> {
    let themes = Theme::catalog(&paths.themes_dir());
    let mut connection = client::connect_or_spawn(&paths, &session, &cwd)?;
    let mut app = App::new(session.clone(), config, theme, themes, warnings, &cwd);
    if let Ok((cols, rows)) = terminal::size() {
        app.screen = Rect::new(0, 0, cols, rows);
    }
    if !app.warnings.is_empty() {
        let count = app.warnings.len();
        app.set_status(format!(
            "{count} configuration warning(s) — `herdr-plus config` shows them"
        ));
    }
    let chrome = layout::chrome(app.screen, &app.chrome_state());
    connection.send(&Request::Hello {
        session: session.clone(),
        size: Size {
            rows: app.screen.height,
            cols: app.screen.width,
        },
        area: chrome.panes,
    })?;
    app.last_pane_area = Some(chrome.panes);

    let mut write = connection
        .stream
        .try_clone()
        .context("cloning the socket")?;
    let read = connection
        .stream
        .try_clone()
        .context("cloning the socket")?;
    let (tx, rx) = mpsc::channel::<AppEvent>();
    spawn_input(tx.clone());
    spawn_reader(read, tx);

    event_loop(terminal, &mut app, &mut write, rx)
}

fn setup_terminal() -> Result<Tui> {
    terminal::enable_raw_mode().context("enabling raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        terminal::EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )
    .context("entering the alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("starting the terminal")?;
    terminal.clear().ok();
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Tui) {
    let _ = terminal.show_cursor();
    let mut stdout = std::io::stdout();
    let _ = execute!(
        stdout,
        DisableBracketedPaste,
        DisableMouseCapture,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}

fn send(write: &mut UnixStream, request: &Request) -> Result<()> {
    let bytes = encode_line(request).context("encoding a request")?;
    write.write_all(&bytes).context("writing to the session")?;
    write.flush().context("flushing to the session")
}

fn event_loop(
    terminal: &mut Tui,
    app: &mut App,
    write: &mut UnixStream,
    rx: mpsc::Receiver<AppEvent>,
) -> Result<()> {
    loop {
        while let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::Resize(cols, rows) => {
                    app.screen = Rect::new(0, 0, cols, rows);
                }
                other => {
                    for action in app.handle(other) {
                        send(write, &Request::Action { action })?;
                    }
                }
            }
        }
        if app.detach_requested {
            send(
                write,
                &Request::Action {
                    action: Action::Detach,
                },
            )?;
            break;
        }
        if app.quit {
            break;
        }
        let chrome = layout::chrome(app.screen, &app.chrome_state());
        let size = Size {
            rows: app.screen.height,
            cols: app.screen.width,
        };
        if let Some(action) = app.resize_action(chrome.panes, size) {
            send(write, &Request::Action { action })?;
        }
        terminal.draw(|frame| ui::draw(frame, app))?;
        if let Some((_, at)) = &app.status {
            if at.elapsed() > Duration::from_secs(4) {
                app.status = None;
            }
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    terminal.show_cursor().ok();
    Ok(())
}

fn spawn_input(tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || loop {
        match event::poll(Duration::from_millis(60)) {
            Ok(true) => match event::read() {
                Ok(CrosstermEvent::Key(key)) if key.kind != KeyEventKind::Release => {
                    if tx.send(AppEvent::Key(key)).is_err() {
                        break;
                    }
                }
                Ok(CrosstermEvent::Mouse(mouse)) => {
                    if tx.send(AppEvent::Mouse(mouse)).is_err() {
                        break;
                    }
                }
                Ok(CrosstermEvent::Paste(text)) => {
                    if tx.send(AppEvent::Paste(text)).is_err() {
                        break;
                    }
                }
                Ok(CrosstermEvent::Resize(cols, rows))
                    if tx.send(AppEvent::Resize(cols, rows)).is_err() =>
                {
                    break;
                }
                _ => {}
            },
            Ok(false) => {}
            Err(_) => break,
        }
    });
}

fn spawn_reader(stream: UnixStream, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    let _ = tx.send(AppEvent::Disconnected(
                        "the session daemon closed the connection".into(),
                    ));
                    break;
                }
                Ok(_) if line.trim().is_empty() => continue,
                Ok(_) => match decode_line::<Event>(line.trim_end()) {
                    Ok(Event::Snapshot { snapshot }) => {
                        if tx.send(AppEvent::Snapshot(snapshot)).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Detached { reason }) => {
                        let _ = tx.send(AppEvent::Detached(reason));
                        break;
                    }
                    Ok(Event::Error { message }) => {
                        let _ = tx.send(AppEvent::ServerError(message));
                    }
                    Err(err) => {
                        let _ = tx.send(AppEvent::ServerError(format!("bad frame: {err}")));
                    }
                },
                Err(_) => {
                    let _ = tx.send(AppEvent::Disconnected("the connection was lost".into()));
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdr_plus_core::detect::PaneStatus;
    use herdr_plus_core::ids::PaneId;
    use herdr_plus_core::model::PaneKind;
    use herdr_plus_core::protocol::{
        GridFrame, PaneFrame, Snapshot, StyledLine, StyledRun, TabView, TermStyle, WorkspaceView,
    };
    use ratatui::backend::TestBackend;

    fn demo_app() -> App {
        let config = Config::default();
        let themes = Theme::catalog(std::path::Path::new("/nonexistent"));
        let theme = themes[0].clone();
        let mut app = App::new(
            "herdr-plus".into(),
            config,
            theme,
            themes,
            Vec::new(),
            std::path::Path::new("/tmp"),
        );
        app.screen = Rect::new(0, 0, 90, 26);
        let chrome = layout::chrome(app.screen, &app.chrome_state());
        let area = chrome.panes;
        let grid = GridFrame::plain(
            3,
            30,
            vec![
                StyledLine {
                    runs: vec![StyledRun {
                        text: "✓ cargo test — 59 passed".into(),
                        style: TermStyle::default(),
                    }],
                    wrapped: false,
                },
                StyledLine {
                    runs: vec![StyledRun {
                        text: "see https://herdr.dev/docs and src/main.rs".into(),
                        style: TermStyle::default(),
                    }],
                    wrapped: false,
                },
                StyledLine {
                    runs: vec![StyledRun {
                        text: "$ ".into(),
                        style: TermStyle::default(),
                    }],
                    wrapped: false,
                },
            ],
        );
        app.handle(AppEvent::Snapshot(Box::new(Snapshot {
            session: "herdr-plus".into(),
            version: "0.1.0".into(),
            seq: 3,
            size: Size { rows: 26, cols: 90 },
            area,
            workspaces: vec![WorkspaceView {
                id: herdr_plus_core::ids::WorkspaceId(1),
                name: "herdr-plus".into(),
                dir: "/tmp/project".into(),
                status: PaneStatus::Working,
                panes: 1,
                agent: None,
                tabs: vec![TabView {
                    id: herdr_plus_core::ids::TabId(1),
                    name: "shell".into(),
                    status: PaneStatus::Working,
                    panes: 1,
                    agent: None,
                }],
                active_tab: 0,
            }],
            active_workspace: 0,
            focused: Some(PaneId(1)),
            zoomed: false,
            panes: vec![PaneFrame {
                id: PaneId(1),
                rect: area,
                title: "project".into(),
                cwd: "/tmp/project".into(),
                command: "zsh".into(),
                kind: PaneKind::Shell,
                viewer: None,
                agent: None,
                status: PaneStatus::Working,
                bell: false,
                exit_code: None,
                scroll: 0,
                grid: Some(grid),
            }],
        })));
        app
    }

    fn buffer_text(app: &mut App) -> String {
        let backend = TestBackend::new(90, 26);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| ui::draw(frame, app))
            .expect("drawing a frame");
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn draws_the_tab_bar_sidebar_panes_and_status() {
        let mut app = demo_app();
        let text = buffer_text(&mut app);
        assert!(text.contains("herdr-plus"), "{text}");
        assert!(text.contains("shell"), "{text}");
        assert!(text.contains("WORKSPACES"), "{text}");
        assert!(text.contains("PANES"), "{text}");
        assert!(text.contains("cargo test"), "{text}");
        assert!(text.contains("https://herdr.dev/docs"), "{text}");
        assert!(text.contains("prompt"), "{text}");
        assert!(text.contains("working"), "{text}");
        // The active pane is drawn with a rounded frame and a title.
        assert!(text.contains("✓") || text.contains("✳"));
        assert!(!text.contains("panicked"));
    }

    #[test]
    fn composer_draws_a_caret_line_and_completion() {
        let mut app = demo_app();
        app.handle(AppEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('b'),
            crossterm::event::KeyModifiers::CONTROL,
        )));
        app.handle(AppEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(':'),
            crossterm::event::KeyModifiers::NONE,
        )));
        for ch in "look at @".chars() {
            app.handle(AppEvent::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(ch),
                crossterm::event::KeyModifiers::NONE,
            )));
        }
        let text = buffer_text(&mut app);
        assert!(text.contains("look at @"), "{text}");
        assert!(text.contains("@path"), "completion popup missing:\n{text}");
    }

    #[test]
    fn viewer_panes_draw_file_content() {
        let mut app = demo_app();
        let root = std::env::temp_dir().join(format!("herdr-plus-ui-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("README.md");
        std::fs::write(&path, "# Herdr Plus\n\nrender **markdown** here").unwrap();
        let mut snapshot = app.snapshot().unwrap().clone();
        let chrome = layout::chrome(app.screen, &app.chrome_state());
        snapshot.panes[0] = PaneFrame {
            id: PaneId(1),
            rect: chrome.panes,
            title: "README.md".into(),
            cwd: root.display().to_string(),
            command: String::new(),
            kind: PaneKind::Viewer,
            viewer: Some(herdr_plus_core::model::ViewerSpec {
                path: path.clone(),
                kind: herdr_plus_core::model::ViewerKind::Markdown,
            }),
            agent: None,
            status: PaneStatus::Idle,
            bell: false,
            exit_code: None,
            scroll: 0,
            grid: None,
        };
        app.handle(AppEvent::Snapshot(Box::new(snapshot)));
        let text = buffer_text(&mut app);
        assert!(text.contains("Herdr Plus"), "{text}");
        assert!(text.contains("markdown"), "{text}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn theme_menu_lists_themes() {
        let mut app = demo_app();
        app.handle(AppEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('b'),
            crossterm::event::KeyModifiers::CONTROL,
        )));
        app.handle(AppEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('t'),
            crossterm::event::KeyModifiers::NONE,
        )));
        let text = buffer_text(&mut app);
        assert!(text.contains("themes"), "{text}");
        assert!(text.contains("midnight"), "{text}");
        assert!(text.contains("nord"), "{text}");
    }
}
