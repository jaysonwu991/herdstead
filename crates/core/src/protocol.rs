//! The client ⇄ runtime wire protocol.
//!
//! Frames are newline-delimited JSON over a Unix socket. The runtime owns
//! panes, PTYs, layout and focus; the client owns presentation (theme, scroll
//! of viewer panes, overlays, the composer) and sends intents back.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::detect::{AgentKind, PaneStatus};
use crate::geom::{Dir, Rect};
use crate::ids::{PaneId, TabId, WorkspaceId};
use crate::layout::SplitDir;
use crate::model::{PaneKind, Size, ViewerKind, ViewerSpec};

/// A hard ceiling on one frame, so a client cannot exhaust the daemon's memory.
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TermColor {
    #[default]
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TermStyle {
    pub fg: TermColor,
    pub bg: TermColor,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub dim: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub inverse: bool,
}

impl TermStyle {
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyledRun {
    pub text: String,
    #[serde(default, skip_serializing_if = "TermStyle::is_plain")]
    pub style: TermStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StyledLine {
    pub runs: Vec<StyledRun>,
    /// True when the terminal soft-wrapped this row into the next one.
    #[serde(default, skip_serializing_if = "is_false")]
    pub wrapped: bool,
}

impl StyledLine {
    pub fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }

    pub fn is_blank(&self) -> bool {
        self.runs.iter().all(|run| run.text.trim().is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorPos {
    pub row: u16,
    pub col: u16,
}

/// The visible screen of one terminal pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridFrame {
    pub rows: u16,
    pub cols: u16,
    pub lines: Vec<StyledLine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<CursorPos>,
    /// Scrollback rows the view is offset by (0 = live).
    pub offset: usize,
    /// How many scrollback rows exist behind the viewport.
    pub scrollback: usize,
    pub alt_screen: bool,
}

impl GridFrame {
    pub fn plain(rows: u16, cols: u16, lines: Vec<StyledLine>) -> Self {
        Self {
            rows,
            cols,
            lines,
            cursor: None,
            offset: 0,
            scrollback: 0,
            alt_screen: false,
        }
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(&line.text());
            out.push('\n');
        }
        out
    }
}

/// One pane of the active tab, with the rect it was laid out in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneFrame {
    pub id: PaneId,
    pub rect: Rect,
    pub title: String,
    pub cwd: String,
    pub command: String,
    pub kind: PaneKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer: Option<ViewerSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentKind>,
    pub status: PaneStatus,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bell: bool,
    #[serde(default)]
    pub exit_code: Option<i32>,
    pub scroll: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<GridFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabView {
    pub id: TabId,
    pub name: String,
    pub status: PaneStatus,
    pub panes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceView {
    pub id: WorkspaceId,
    pub name: String,
    pub dir: String,
    pub status: PaneStatus,
    pub panes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentKind>,
    pub tabs: Vec<TabView>,
    pub active_tab: usize,
}

/// A complete picture of the session, sent on attach and whenever it changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub session: String,
    pub version: String,
    pub seq: u64,
    pub size: Size,
    /// The rect the active tab's panes were laid out in.
    pub area: Rect,
    pub workspaces: Vec<WorkspaceView>,
    pub active_workspace: usize,
    #[serde(default)]
    pub focused: Option<PaneId>,
    pub zoomed: bool,
    pub panes: Vec<PaneFrame>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Mods {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    /// Cmd on macOS, Super elsewhere.
    #[serde(default)]
    pub sup: bool,
}

impl Mods {
    pub const fn none() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            sup: false,
        }
    }

    pub const fn ctrl() -> Self {
        Self {
            ctrl: true,
            ..Self::none()
        }
    }

    pub fn is_empty(self) -> bool {
        !(self.shift || self.ctrl || self.alt || self.sup)
    }
}

/// Keys the client sends to the runtime, which encodes them for the pane's
/// current terminal modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyInput {
    pub key: Key,
    #[serde(default)]
    pub mods: Mods,
    /// Held-key repeat rather than the first press.
    #[serde(default)]
    pub repeat: bool,
}

impl KeyInput {
    pub fn new(key: Key) -> Self {
        Self {
            key,
            mods: Mods::none(),
            repeat: false,
        }
    }

    pub fn with_mods(key: Key, mods: Mods) -> Self {
        Self {
            key,
            mods,
            repeat: false,
        }
    }
}

/// An intent from the client. Everything here is a runtime change; purely
/// visual state (theme, overlays, viewer scroll) never leaves the client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Key {
        input: KeyInput,
    },
    Paste {
        text: String,
    },
    Split {
        dir: SplitDir,
    },
    ClosePane,
    CloseTab,
    CloseWorkspace,
    FocusPane {
        pane: PaneId,
    },
    FocusDir {
        dir: Dir,
    },
    FocusCycle {
        step: i32,
    },
    ToggleZoom,
    ResizePane {
        dir: Dir,
        cells: i16,
    },
    /// `delta` is in rows; `page` scrolls by a viewport instead.
    ScrollPane {
        pane: PaneId,
        delta: i32,
        #[serde(default)]
        page: bool,
    },
    ScrollToBottom {
        pane: PaneId,
    },
    ScrollToTop {
        pane: PaneId,
    },
    NewTab {
        #[serde(default)]
        cwd: Option<String>,
    },
    NewWorkspace {
        #[serde(default)]
        dir: Option<String>,
    },
    SelectTab {
        index: usize,
    },
    SelectWorkspace {
        index: usize,
    },
    CycleTab {
        step: i32,
    },
    CycleWorkspace {
        step: i32,
    },
    RenameTab {
        name: String,
    },
    RenameWorkspace {
        name: String,
    },
    RenamePane {
        name: String,
    },
    /// Prompts the focused pane, which is what the composer sends.
    SendText {
        text: String,
        submit: bool,
    },
    /// Opens a file or folder in a rich viewer pane.
    OpenViewer {
        path: String,
        #[serde(default)]
        kind: Option<ViewerKind>,
        #[serde(default)]
        split: Option<SplitDir>,
    },
    /// Closes a pane by id (the sidebar's close affordance).
    KillPane {
        pane: PaneId,
    },
    Resize {
        size: Size,
        /// The rect the client laid out for panes (below its chrome, right of
        /// its sidebar). The runtime lays panes out inside exactly this rect.
        area: Rect,
    },
    /// Shut the session down, keeping its layout on disk.
    Kill,
    Detach,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Hello {
        session: String,
        size: Size,
        area: Rect,
    },
    Action {
        action: Action,
    },
    /// Reads the current state without attaching, so listing sessions does
    /// not change anyone's layout.
    Inspect {
        session: String,
    },
    Refresh,
    Kill,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Snapshot {
        snapshot: Box<Snapshot>,
    },
    /// The runtime is going away; the client should leave the alternate screen.
    Detached {
        reason: String,
    },
    Error {
        message: String,
    },
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn encode_line<T: Serialize>(value: &T) -> serde_json::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn decode_line<T: DeserializeOwned>(line: &str) -> serde_json::Result<T> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Size;

    #[test]
    fn actions_round_trip_through_json() {
        let actions = vec![
            Action::Key {
                input: KeyInput::with_mods(Key::Char('c'), Mods::ctrl()),
            },
            Action::Split {
                dir: SplitDir::Horizontal,
            },
            Action::SendText {
                text: "explain @src/main.rs".into(),
                submit: true,
            },
            Action::ScrollPane {
                pane: PaneId(3),
                delta: -5,
                page: false,
            },
            Action::OpenViewer {
                path: "README.md".into(),
                kind: None,
                split: None,
            },
            Action::NewWorkspace { dir: None },
        ];
        for action in actions {
            let line = encode_line(&Request::Action {
                action: action.clone(),
            })
            .unwrap();
            let text = String::from_utf8(line).unwrap();
            assert!(text.ends_with('\n'));
            let back: Request = decode_line(text.trim_end()).unwrap();
            assert_eq!(back, Request::Action { action });
        }
    }

    #[test]
    fn snapshot_is_serialisable() {
        let snapshot = Snapshot {
            session: "main".into(),
            version: "0.1.0".into(),
            seq: 4,
            size: Size {
                rows: 40,
                cols: 120,
            },
            area: Rect::new(0, 2, 120, 35),
            workspaces: vec![WorkspaceView {
                id: WorkspaceId(1),
                name: "herdr-plus".into(),
                dir: "/tmp".into(),
                status: PaneStatus::Working,
                panes: 1,
                agent: Some(AgentKind::Claude),
                tabs: vec![TabView {
                    id: TabId(1),
                    name: "shell".into(),
                    status: PaneStatus::Working,
                    panes: 1,
                    agent: Some(AgentKind::Claude),
                }],
                active_tab: 0,
            }],
            active_workspace: 0,
            focused: Some(PaneId(1)),
            zoomed: false,
            panes: vec![PaneFrame {
                id: PaneId(1),
                rect: Rect::new(0, 2, 120, 35),
                title: "shell".into(),
                cwd: "/tmp".into(),
                command: "zsh".into(),
                kind: PaneKind::Shell,
                viewer: None,
                agent: None,
                status: PaneStatus::Idle,
                bell: false,
                exit_code: None,
                scroll: 0,
                grid: Some(GridFrame::plain(
                    1,
                    4,
                    vec![StyledLine {
                        runs: vec![StyledRun {
                            text: "test".into(),
                            style: TermStyle {
                                fg: TermColor::Rgb(1, 2, 3),
                                ..TermStyle::default()
                            },
                        }],
                        wrapped: true,
                    }],
                )),
            }],
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let back: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, back);
        assert!(json.contains("\"wrapped\":true"));
    }
}
