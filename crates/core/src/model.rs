//! The session tree: workspaces hold tabs, tabs hold a pane tree.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::detect::{AgentKind, PaneStatus};
use crate::geom::Dir;
use crate::ids::{Ids, PaneId, TabId, WorkspaceId};
use crate::layout::{Node, PanePlacement, SplitDir};

/// Terminal size of an attached client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub rows: u16,
    pub cols: u16,
}

impl Default for Size {
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

impl Size {
    /// The pane area left after the tab bar, status bar and composer.
    pub fn content(self, chrome_rows: u16) -> Self {
        Self {
            rows: self.rows.saturating_sub(chrome_rows).max(1),
            cols: self.cols.max(1),
        }
    }
}

/// A pane is either a live terminal or a rich viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaneKind {
    #[default]
    Shell,
    Viewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerKind {
    /// Syntax-highlighted file, VS Code style.
    Code,
    /// Rendered Markdown.
    Markdown,
    /// A folder tree.
    Tree,
}

impl ViewerKind {
    /// Chooses the viewer that suits a path: folders get a tree, Markdown gets
    /// rendering, everything else gets syntax highlighting.
    pub fn for_path(path: &Path) -> Self {
        if path.is_dir() {
            return Self::Tree;
        }
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("md" | "markdown" | "mdx") => Self::Markdown,
            _ => Self::Code,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerSpec {
    pub path: PathBuf,
    pub kind: ViewerKind,
}

/// Runtime facts about one pane. The PTY or viewer content itself lives in the
/// server; this is what both server and client show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneMeta {
    pub id: PaneId,
    pub title: String,
    pub cwd: PathBuf,
    pub command: String,
    pub kind: PaneKind,
    pub viewer: Option<ViewerSpec>,
    pub agent: Option<AgentKind>,
    pub status: PaneStatus,
    pub bell: bool,
    pub exit_code: Option<i32>,
    /// Unix milliseconds of the last output, for the quiet-time heuristic.
    pub last_output_ms: u64,
}

impl PaneMeta {
    pub fn shell(id: PaneId, cwd: impl Into<PathBuf>, command: impl Into<String>) -> Self {
        let cwd = cwd.into();
        let command = command.into();
        let title = cwd
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| cwd.display().to_string());
        Self {
            id,
            title,
            cwd,
            command,
            kind: PaneKind::Shell,
            viewer: None,
            agent: None,
            status: PaneStatus::Idle,
            bell: false,
            exit_code: None,
            last_output_ms: 0,
        }
    }

    pub fn viewer(id: PaneId, spec: ViewerSpec) -> Self {
        let title = spec
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| spec.path.display().to_string());
        let cwd = if spec.path.is_dir() {
            spec.path.clone()
        } else {
            spec.path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        };
        Self {
            id,
            title,
            cwd,
            command: String::new(),
            kind: PaneKind::Viewer,
            viewer: Some(spec),
            agent: None,
            status: PaneStatus::Idle,
            bell: false,
            exit_code: None,
            last_output_ms: 0,
        }
    }

    pub fn is_shell(&self) -> bool {
        self.kind == PaneKind::Shell
    }
}

/// One tab: a pane tree plus the pane that has focus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub id: TabId,
    pub name: String,
    pub layout: Node,
    pub focused: PaneId,
    pub zoomed: bool,
    pub dir: PathBuf,
}

impl Tab {
    pub fn new(id: TabId, name: impl Into<String>, pane: PaneId, dir: impl Into<PathBuf>) -> Self {
        Self {
            id,
            name: name.into(),
            layout: Node::leaf(pane),
            focused: pane,
            zoomed: false,
            dir: dir.into(),
        }
    }

    pub fn panes(&self) -> Vec<PaneId> {
        self.layout.leaves()
    }

    pub fn placements(&self, area: crate::geom::Rect) -> Vec<PanePlacement> {
        if self.zoomed {
            return vec![PanePlacement {
                pane: self.focused,
                rect: area,
            }];
        }
        self.layout.placements(area)
    }

    pub fn split(&mut self, dir: SplitDir, new_pane: PaneId) -> bool {
        self.zoomed = false;
        if self.layout.insert_split(self.focused, dir, new_pane) {
            self.focused = new_pane;
            true
        } else {
            false
        }
    }

    /// Focuses `pane` if it belongs to this tab.
    pub fn focus(&mut self, pane: PaneId) -> bool {
        if self.layout.contains(pane) {
            self.focused = pane;
            true
        } else {
            false
        }
    }

    /// Moves focus by geometry, leaving it unchanged at an edge.
    pub fn focus_dir(&mut self, dir: Dir, area: crate::geom::Rect) -> bool {
        let placements = self.layout.placements(area);
        match crate::layout::neighbor(&placements, self.focused, dir) {
            Some(pane) => {
                self.focused = pane;
                true
            }
            None => false,
        }
    }

    /// Cycles focus through the tab's panes in layout order.
    pub fn focus_cycle(&mut self, step: i32) -> bool {
        let panes = self.panes();
        if panes.len() < 2 {
            return false;
        }
        let current = panes.iter().position(|p| *p == self.focused).unwrap_or(0);
        let len = panes.len() as i32;
        let next = ((current as i32 + step) % len + len) % len;
        self.focused = panes[next as usize];
        true
    }

    pub fn resize(&mut self, dir: Dir, cells: f32, area: crate::geom::Rect) -> bool {
        self.layout.resize(self.focused, dir, cells, area)
    }

    /// Status shown on the tab strip: the most urgent pane wins.
    pub fn status(&self, panes: &BTreeMap<PaneId, PaneMeta>) -> PaneStatus {
        self.panes()
            .iter()
            .filter_map(|pane| panes.get(pane).map(|meta| meta.status))
            .min_by_key(|status| status.priority())
            .unwrap_or(PaneStatus::Idle)
    }
}

/// One workspace: a project directory with its own set of tabs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub dir: PathBuf,
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
}

impl Workspace {
    pub fn new(
        id: WorkspaceId,
        name: impl Into<String>,
        dir: impl Into<PathBuf>,
        tab: Tab,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            dir: dir.into(),
            tabs: vec![tab],
            active_tab: 0,
        }
    }

    pub fn tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    pub fn tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    pub fn panes(&self) -> Vec<PaneId> {
        self.tabs.iter().flat_map(Tab::panes).collect()
    }

    pub fn status(&self, panes: &BTreeMap<PaneId, PaneMeta>) -> PaneStatus {
        self.tabs
            .iter()
            .map(|tab| tab.status(panes))
            .min_by_key(|status| status.priority())
            .unwrap_or(PaneStatus::Idle)
    }

    pub fn active_panes(&self) -> Vec<PaneId> {
        self.tab().map(Tab::panes).unwrap_or_default()
    }
}

/// Everything the runtime knows about a session. Serialisable as a whole, so a
/// detached session survives a restart of the daemon and the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub name: String,
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub panes: BTreeMap<PaneId, PaneMeta>,
    pub ids: Ids,
    /// Bumped on every structural change so clients can ignore stale frames.
    pub seq: u64,
    /// Last size an attached client reported; panes are laid out for it.
    pub size: Size,
}

/// What a close actually changed, so the caller can focus sensibly and spawn a
/// replacement shell when the last pane went away.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CloseOutcome {
    pub tab_closed: bool,
    pub workspace_closed: bool,
    pub focus: Option<PaneId>,
    pub empty: bool,
}

impl SessionState {
    /// A session with one workspace and one shell pane, whose meta the caller
    /// supplies after spawning it.
    pub fn new(name: impl Into<String>, dir: impl Into<PathBuf>, pane: PaneMeta) -> Self {
        let dir = dir.into();
        let mut ids = Ids::new();
        let workspace_id = ids.workspace();
        let tab_id = ids.tab();
        let pane_id = pane.id;
        let ids_next = ids.pane();
        debug_assert_eq!(ids_next, pane_id);
        let name = name.into();
        let mut panes = BTreeMap::new();
        panes.insert(pane_id, pane);
        Self {
            name: name.clone(),
            workspaces: vec![Workspace::new(
                workspace_id,
                name,
                dir.clone(),
                Tab::new(tab_id, "shell", pane_id, dir),
            )],
            active_workspace: 0,
            panes,
            ids,
            seq: 1,
            size: Size::default(),
        }
    }

    pub fn touch(&mut self) {
        self.seq += 1;
    }

    pub fn workspace(&self) -> Option<&Workspace> {
        self.workspaces.get(self.active_workspace)
    }

    pub fn workspace_mut(&mut self) -> Option<&mut Workspace> {
        self.workspaces.get_mut(self.active_workspace)
    }

    pub fn tab(&self) -> Option<&Tab> {
        self.workspace()?.tab()
    }

    pub fn tab_mut(&mut self) -> Option<&mut Tab> {
        let index = self.active_workspace;
        self.workspaces.get_mut(index)?.tab_mut()
    }

    pub fn focused(&self) -> Option<PaneId> {
        self.tab().map(|tab| tab.focused)
    }

    pub fn focused_meta(&self) -> Option<&PaneMeta> {
        self.panes.get(&self.focused()?)
    }

    pub fn pane(&self, pane: PaneId) -> Option<&PaneMeta> {
        self.panes.get(&pane)
    }

    pub fn pane_location(&self, pane: PaneId) -> Option<(usize, usize)> {
        for (wi, workspace) in self.workspaces.iter().enumerate() {
            for (ti, tab) in workspace.tabs.iter().enumerate() {
                if tab.layout.contains(pane) {
                    return Some((wi, ti));
                }
            }
        }
        None
    }

    pub fn insert_pane(&mut self, pane: PaneMeta) {
        self.panes.insert(pane.id, pane);
        self.touch();
    }

    /// Splits the focused pane and focuses the new pane.
    pub fn split_focused(&mut self, dir: SplitDir, new_pane: PaneId) -> bool {
        let Some(tab) = self.tab_mut() else {
            return false;
        };
        let ok = tab.split(dir, new_pane);
        if ok {
            self.touch();
        }
        ok
    }

    pub fn focus_pane(&mut self, pane: PaneId) -> bool {
        let Some((wi, ti)) = self.pane_location(pane) else {
            return false;
        };
        if let Some(workspace) = self.workspaces.get_mut(wi) {
            workspace.active_tab = ti;
            if let Some(tab) = workspace.tabs.get_mut(ti) {
                tab.focused = pane;
            }
        }
        self.active_workspace = wi;
        self.touch();
        true
    }

    pub fn focus_dir(&mut self, dir: Dir, area: crate::geom::Rect) -> bool {
        let ok = self
            .tab_mut()
            .map(|tab| tab.focus_dir(dir, area))
            .unwrap_or(false);
        if ok {
            self.touch();
        }
        ok
    }

    pub fn focus_cycle(&mut self, step: i32) -> bool {
        let ok = self
            .tab_mut()
            .map(|tab| tab.focus_cycle(step))
            .unwrap_or(false);
        if ok {
            self.touch();
        }
        ok
    }

    pub fn toggle_zoom(&mut self) -> bool {
        let ok = self
            .tab_mut()
            .map(|tab| {
                if tab.panes().len() < 2 {
                    return false;
                }
                tab.zoomed = !tab.zoomed;
                true
            })
            .unwrap_or(false);
        if ok {
            self.touch();
        }
        ok
    }

    pub fn resize(&mut self, dir: Dir, cells: f32, area: crate::geom::Rect) -> bool {
        let ok = self
            .tab_mut()
            .map(|tab| tab.resize(dir, cells, area))
            .unwrap_or(false);
        if ok {
            self.touch();
        }
        ok
    }

    /// Adds a tab to the active workspace and focuses its pane.
    pub fn add_tab(&mut self, name: impl Into<String>, pane: PaneId) -> bool {
        let id = self.ids.tab();
        let dir = self
            .workspace()
            .map(|workspace| workspace.dir.clone())
            .unwrap_or_default();
        let Some(workspace) = self.workspace_mut() else {
            return false;
        };
        let name = name.into();
        workspace.tabs.push(Tab::new(id, &name, pane, dir));
        workspace.active_tab = workspace.tabs.len() - 1;
        self.touch();
        true
    }

    pub fn select_tab(&mut self, index: usize) -> bool {
        let Some(workspace) = self.workspace_mut() else {
            return false;
        };
        if index < workspace.tabs.len() && workspace.active_tab != index {
            workspace.active_tab = index;
            self.touch();
            return true;
        }
        false
    }

    pub fn cycle_tab(&mut self, step: i32) -> bool {
        let Some(workspace) = self.workspace_mut() else {
            return false;
        };
        let len = workspace.tabs.len();
        if len < 2 {
            return false;
        }
        let len = len as i32;
        let next = ((workspace.active_tab as i32 + step) % len + len) % len;
        workspace.active_tab = next as usize;
        self.touch();
        true
    }

    pub fn rename_tab(&mut self, name: impl Into<String>) -> bool {
        let Some(tab) = self.tab_mut() else {
            return false;
        };
        tab.name = name.into();
        self.touch();
        true
    }

    pub fn add_workspace(
        &mut self,
        name: impl Into<String>,
        dir: impl Into<PathBuf>,
        pane: PaneId,
    ) -> bool {
        let id = self.ids.workspace();
        let tab_id = self.ids.tab();
        let dir = dir.into();
        let name = name.into();
        let workspace =
            Workspace::new(id, &name, dir.clone(), Tab::new(tab_id, "shell", pane, dir));
        self.workspaces.push(workspace);
        self.active_workspace = self.workspaces.len() - 1;
        self.touch();
        true
    }

    pub fn select_workspace(&mut self, index: usize) -> bool {
        if index < self.workspaces.len() && index != self.active_workspace {
            self.active_workspace = index;
            self.touch();
            return true;
        }
        false
    }

    pub fn cycle_workspace(&mut self, step: i32) -> bool {
        let len = self.workspaces.len() as i32;
        if len < 2 {
            return false;
        }
        let next = ((self.active_workspace as i32 + step) % len + len) % len;
        self.active_workspace = next as usize;
        self.touch();
        true
    }

    pub fn rename_workspace(&mut self, name: impl Into<String>) -> bool {
        let Some(workspace) = self.workspace_mut() else {
            return false;
        };
        workspace.name = name.into();
        self.touch();
        true
    }

    /// Removes a pane, its tab when it was the tab's last pane, and its
    /// workspace when it was the workspace's last tab.
    pub fn close_pane(&mut self, pane: PaneId) -> CloseOutcome {
        let Some((wi, ti)) = self.pane_location(pane) else {
            return CloseOutcome {
                focus: self.focused(),
                ..CloseOutcome::default()
            };
        };
        let mut outcome = CloseOutcome::default();
        self.panes.remove(&pane);
        let mut tab_empty = false;
        {
            let workspace = &mut self.workspaces[wi];
            let tab = &mut workspace.tabs[ti];
            if !tab.layout.remove(pane) {
                tab_empty = true;
            }
            if tab.focused == pane {
                if let Some(next) = tab.panes().first().copied() {
                    tab.focused = next;
                }
            }
            if tab_empty || tab.panes().is_empty() {
                workspace.tabs.remove(ti);
                outcome.tab_closed = true;
                if workspace.tabs.is_empty() {
                    outcome.workspace_closed = true;
                } else {
                    workspace.active_tab = ti.saturating_sub(1);
                }
            }
        }
        if outcome.workspace_closed {
            self.workspaces.remove(wi);
            if !self.workspaces.is_empty() {
                self.active_workspace = self.active_workspace.min(self.workspaces.len() - 1);
            }
        } else if outcome.tab_closed && wi < self.workspaces.len() {
            self.active_workspace = wi;
            if let Some(workspace) = self.workspaces.get_mut(wi) {
                workspace.active_tab = workspace.tabs.len().saturating_sub(1);
            }
        }
        outcome.empty = self.workspaces.is_empty();
        outcome.focus = self.focused();
        self.touch();
        outcome
    }

    /// Closes the active tab (and its workspace when it was the last tab).
    pub fn close_tab(&mut self) -> CloseOutcome {
        let mut outcome = CloseOutcome::default();
        let wi = self.active_workspace;
        let Some(workspace) = self.workspaces.get(wi) else {
            outcome.empty = self.workspaces.is_empty();
            return outcome;
        };
        if workspace.tabs.is_empty() {
            outcome.focus = None;
            return outcome;
        }
        let ti = workspace.active_tab.min(workspace.tabs.len() - 1);
        let closed = self.workspaces[wi].tabs.remove(ti);
        for pane in closed.panes() {
            self.panes.remove(&pane);
        }
        outcome.tab_closed = true;
        if self.workspaces[wi].tabs.is_empty() {
            outcome.workspace_closed = true;
        } else {
            let remaining = self.workspaces[wi].tabs.len();
            self.workspaces[wi].active_tab = ti.min(remaining - 1);
            outcome.focus = self.workspaces[wi].tab().map(|tab| tab.focused);
        }
        if outcome.workspace_closed {
            self.workspaces.remove(wi);
            if !self.workspaces.is_empty() {
                self.active_workspace = self.active_workspace.min(self.workspaces.len() - 1);
                outcome.focus = self.focused();
            }
        }
        outcome.empty = self.workspaces.is_empty();
        self.touch();
        outcome
    }

    /// Closes a workspace and every pane in it.
    pub fn close_workspace(&mut self, index: usize) -> Vec<PaneId> {
        if index >= self.workspaces.len() {
            return Vec::new();
        }
        let workspace = self.workspaces.remove(index);
        let panes = workspace.panes();
        for pane in &panes {
            self.panes.remove(pane);
        }
        if !self.workspaces.is_empty() {
            self.active_workspace = self.active_workspace.min(self.workspaces.len() - 1);
        }
        self.touch();
        panes
    }

    /// Every pane of the active tab, with the screen area it occupies.
    pub fn active_placements(&self, area: crate::geom::Rect) -> Vec<PanePlacement> {
        self.tab()
            .map(|tab| tab.placements(area))
            .unwrap_or_default()
    }

    pub fn tab_dir(&self) -> PathBuf {
        self.tab()
            .map(|tab| tab.dir.clone())
            .or_else(|| self.workspace().map(|ws| ws.dir.clone()))
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;

    fn session() -> SessionState {
        SessionState::new("s", "/tmp", PaneMeta::shell(PaneId(1), "/tmp", "zsh"))
    }

    #[test]
    fn splitting_a_tab_focuses_the_new_pane() {
        let mut state = session();
        state.insert_pane(PaneMeta::shell(PaneId(2), "/tmp", "zsh"));
        assert!(state.split_focused(SplitDir::Vertical, PaneId(2)));
        assert_eq!(state.focused(), Some(PaneId(2)));
        assert_eq!(state.tab().unwrap().panes(), vec![PaneId(1), PaneId(2)]);
    }

    #[test]
    fn closing_the_last_pane_closes_tab_and_workspace() {
        let mut state = session();
        let outcome = state.close_pane(PaneId(1));
        assert!(outcome.tab_closed);
        assert!(outcome.workspace_closed);
        assert!(outcome.empty);
        assert_eq!(outcome.focus, None);
        assert!(state.workspaces.is_empty());
    }

    #[test]
    fn closing_a_pane_keeps_its_siblings_focused() {
        let mut state = session();
        state.insert_pane(PaneMeta::shell(PaneId(2), "/tmp", "zsh"));
        state.split_focused(SplitDir::Vertical, PaneId(2));
        let outcome = state.close_pane(PaneId(2));
        assert_eq!(outcome.focus, Some(PaneId(1)));
        assert_eq!(state.tab().unwrap().panes(), vec![PaneId(1)]);
    }

    #[test]
    fn tabs_and_workspaces_rotate() {
        let mut state = session();
        state.insert_pane(PaneMeta::shell(PaneId(2), "/tmp", "zsh"));
        state.add_tab("second", PaneId(2));
        assert_eq!(state.tab().unwrap().name, "second");
        assert!(state.cycle_tab(-1));
        assert_eq!(state.tab().unwrap().panes(), vec![PaneId(1)]);
        state.insert_pane(PaneMeta::shell(PaneId(3), "/tmp", "zsh"));
        state.add_workspace("other", "/tmp", PaneId(3));
        assert_eq!(state.active_workspace, 1);
        assert!(state.cycle_workspace(-1));
        assert_eq!(state.active_workspace, 0);
    }

    #[test]
    fn zoom_shows_only_the_focused_pane() {
        let mut state = session();
        state.insert_pane(PaneMeta::shell(PaneId(2), "/tmp", "zsh"));
        state.split_focused(SplitDir::Vertical, PaneId(2));
        assert!(state.toggle_zoom());
        let area = Rect::new(0, 0, 80, 24);
        let placements = state.active_placements(area);
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].pane, PaneId(2));
        assert_eq!(placements[0].rect, area);
    }

    #[test]
    fn viewer_kinds_follow_the_path() {
        assert_eq!(
            ViewerKind::for_path(Path::new("README.md")),
            ViewerKind::Markdown
        );
        assert_eq!(
            ViewerKind::for_path(Path::new("src/main.rs")),
            ViewerKind::Code
        );
        assert_eq!(ViewerKind::for_path(Path::new("src")), ViewerKind::Tree);
    }

    #[test]
    fn session_round_trips_through_json() {
        let state = session();
        let json = serde_json::to_string(&state).unwrap();
        let back: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);
    }
}
