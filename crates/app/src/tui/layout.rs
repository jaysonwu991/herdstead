//! Where the chrome sits: tab bar, sidebar, panes, composer and status bar.
//!
//! The same functions are used to draw and to hit-test, so a click always lands
//! on the row the user sees.

use herdstead_core::detect::{AgentKind, PaneStatus};
use herdstead_core::geom::{Dir, Rect};
use herdstead_core::ids::PaneId;
use herdstead_core::protocol::Snapshot;
use herdstead_core::text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChromeState {
    pub sidebar: bool,
    pub sidebar_width: u16,
    pub composer: bool,
    pub preview: bool,
    pub composer_lines: u16,
    pub status: bool,
}

/// Rows the Markdown preview occupies above the composer input.
pub const PREVIEW_ROWS: u16 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chrome {
    pub tab_bar: Rect,
    pub sidebar: Option<Rect>,
    pub panes: Rect,
    pub composer: Rect,
    pub status: Rect,
}

/// Splits the screen into the areas the client draws.
pub fn chrome(area: Rect, state: &ChromeState) -> Chrome {
    let area = Rect::new(area.x, area.y, area.width.max(1), area.height.max(1));
    let tab_bar = Rect::new(area.x, area.y, area.width, 1.min(area.height));
    let status_h = if state.status { 1 } else { 0 };
    // The composer always keeps a top rule; an open one adds its input rows
    // and, when previewing, a Markdown panel above the input.
    let composer_h = if state.composer {
        let preview = if state.preview { PREVIEW_ROWS } else { 0 };
        (state.composer_lines.max(1) + 1 + preview).min(area.height.saturating_sub(2))
    } else {
        // A closed composer still shows its top rule and one hint row.
        2.min(area.height.saturating_sub(2))
    };
    let body_y = tab_bar.bottom();
    let body_h = area.height.saturating_sub(1 + composer_h + status_h).max(1);
    let sidebar_w = if state.sidebar {
        state
            .sidebar_width
            .min(area.width.saturating_sub(20))
            .max(12)
    } else {
        0
    };
    let sidebar = (sidebar_w > 0).then(|| Rect::new(area.x, body_y, sidebar_w, body_h));
    let panes = Rect::new(
        area.x + sidebar_w,
        body_y,
        area.width.saturating_sub(sidebar_w).max(1),
        body_h,
    );
    let composer = Rect::new(
        area.x,
        body_y + body_h,
        area.width,
        composer_h.min(area.height.saturating_sub(body_y + body_h)),
    );
    let status = Rect::new(
        area.x,
        composer.bottom(),
        area.width,
        status_h.min(area.height.saturating_sub(composer.bottom())),
    );
    Chrome {
        tab_bar,
        sidebar,
        panes,
        composer,
        status,
    }
}

/// One clickable tab in the tab strip, with the screen rect it occupies.
pub fn tab_hits(chrome: &Chrome, snapshot: &Snapshot) -> Vec<(Rect, usize)> {
    let mut hits = Vec::new();
    let Some(workspace) = snapshot.workspaces.get(snapshot.active_workspace) else {
        return hits;
    };
    let mut x = chrome.tab_bar.x + text::display_width(&chip_label(&workspace.name)) as u16;
    for (index, tab) in workspace.tabs.iter().enumerate() {
        let label = format!(" {}{} ", index + 1, text::truncate(&tab.name, 18));
        let width = text::display_width(&label) as u16;
        if x + width > chrome.tab_bar.right() {
            break;
        }
        hits.push((Rect::new(x, chrome.tab_bar.y, width, 1), index));
        x += width;
    }
    hits
}

pub fn chip_label(name: &str) -> String {
    format!(" ▦ {} ", text::truncate(name, 16))
}

/// A sidebar line, rendered and hit-tested from the same list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarRow {
    Blank,
    Header(&'static str),
    Workspace {
        index: usize,
        name: String,
        status: PaneStatus,
        panes: usize,
        active: bool,
    },
    Tab {
        index: usize,
        name: String,
        status: PaneStatus,
        panes: usize,
        active: bool,
        agent: Option<AgentKind>,
    },
    Pane {
        id: PaneId,
        title: String,
        status: PaneStatus,
        agent: Option<AgentKind>,
        active: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTarget {
    Workspace(usize),
    Tab(usize),
    Pane(PaneId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarHit {
    pub rect: Rect,
    pub target: SidebarTarget,
}

pub fn sidebar_rows(snapshot: &Snapshot) -> Vec<SidebarRow> {
    let mut rows = Vec::new();
    rows.push(SidebarRow::Blank);
    rows.push(SidebarRow::Header("workspaces"));
    for (index, workspace) in snapshot.workspaces.iter().enumerate() {
        rows.push(SidebarRow::Workspace {
            index,
            name: workspace.name.clone(),
            status: workspace.status,
            panes: workspace.panes,
            active: index == snapshot.active_workspace,
        });
    }
    if let Some(workspace) = snapshot.workspaces.get(snapshot.active_workspace) {
        rows.push(SidebarRow::Blank);
        rows.push(SidebarRow::Header("tabs"));
        for (index, tab) in workspace.tabs.iter().enumerate() {
            rows.push(SidebarRow::Tab {
                index,
                name: tab.name.clone(),
                status: tab.status,
                panes: tab.panes,
                active: index == workspace.active_tab,
                agent: tab.agent.clone(),
            });
        }
    }
    rows.push(SidebarRow::Blank);
    rows.push(SidebarRow::Header("panes"));
    for pane in &snapshot.panes {
        rows.push(SidebarRow::Pane {
            id: pane.id,
            title: pane.title.clone(),
            status: pane.status,
            agent: pane.agent.clone(),
            active: snapshot.focused == Some(pane.id),
        });
    }
    rows
}

/// Rows are clickable where they name a workspace, tab or pane.
pub fn sidebar_hits(sidebar: Rect, snapshot: &Snapshot) -> Vec<SidebarHit> {
    let mut hits = Vec::new();
    for (offset, row) in sidebar_rows(snapshot).into_iter().enumerate() {
        if offset >= usize::from(sidebar.height) {
            break;
        }
        let rect = Rect::new(sidebar.x, sidebar.y + offset as u16, sidebar.width, 1);
        let target = match row {
            SidebarRow::Workspace { index, .. } => Some(SidebarTarget::Workspace(index)),
            SidebarRow::Tab { index, .. } => Some(SidebarTarget::Tab(index)),
            SidebarRow::Pane { id, .. } => Some(SidebarTarget::Pane(id)),
            _ => None,
        };
        if let Some(target) = target {
            hits.push(SidebarHit { rect, target });
        }
    }
    hits
}

/// A divider the pointer is on, and the pane it resizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divider {
    pub pane: PaneId,
    pub dir: Dir,
}

pub fn divider_at(snapshot: &Snapshot, column: u16, row: u16) -> Option<Divider> {
    for left in &snapshot.panes {
        for right in &snapshot.panes {
            if left.id == right.id {
                continue;
            }
            // A one-cell gap between siblings is the divider.
            if left.rect.right() + 1 == right.rect.x && column == left.rect.right() {
                let top = left.rect.y.max(right.rect.y);
                let bottom = left.rect.bottom().min(right.rect.bottom());
                if row >= top && row < bottom {
                    return Some(Divider {
                        pane: left.id,
                        dir: Dir::Right,
                    });
                }
            }
            if left.rect.bottom() + 1 == right.rect.y && row == left.rect.bottom() {
                let start = left.rect.x.max(right.rect.x);
                let end = left.rect.right().min(right.rect.right());
                if column >= start && column < end {
                    return Some(Divider {
                        pane: left.id,
                        dir: Dir::Down,
                    });
                }
            }
        }
    }
    None
}

/// A centred panel for the theme menu and help.
pub fn overlay_rect(area: Rect, rows: usize) -> Rect {
    let width = area.width.saturating_sub(8).clamp(24, 64).min(area.width);
    let height = (rows as u16 + 4).min(area.height.saturating_sub(2)).max(5);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdstead_core::model::Size;
    use herdstead_core::protocol::{PaneFrame, Snapshot, TabView, WorkspaceView};

    fn snapshot() -> Snapshot {
        Snapshot {
            session: "test".into(),
            version: "0.1.0".into(),
            seq: 1,
            size: Size { rows: 24, cols: 80 },
            area: Rect::new(28, 1, 52, 18),
            workspaces: vec![WorkspaceView {
                id: herdstead_core::ids::WorkspaceId(1),
                name: "herdstead".into(),
                dir: "/tmp".into(),
                status: PaneStatus::Idle,
                panes: 1,
                agent: Some(AgentKind::Claude),
                tabs: vec![TabView {
                    id: herdstead_core::ids::TabId(1),
                    name: "shell".into(),
                    status: PaneStatus::Idle,
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
                rect: Rect::new(28, 1, 52, 18),
                title: "shell".into(),
                cwd: "/tmp".into(),
                command: "zsh".into(),
                kind: herdstead_core::model::PaneKind::Shell,
                viewer: None,
                agent: Some(AgentKind::Claude),
                status: PaneStatus::Working,
                bell: false,
                exit_code: None,
                scroll: 0,
                grid: None,
            }],
        }
    }

    fn state() -> ChromeState {
        ChromeState {
            sidebar: true,
            sidebar_width: 28,
            composer: false,
            preview: false,
            composer_lines: 1,
            status: true,
        }
    }

    #[test]
    fn chrome_fills_the_screen_without_overlap() {
        let area = Rect::new(0, 0, 80, 24);
        let chrome = chrome(area, &state());
        assert_eq!(chrome.tab_bar.height, 1);
        assert_eq!(chrome.status.height, 1);
        assert_eq!(chrome.status.y, 23);
        assert_eq!(chrome.composer.y + chrome.composer.height, 23);
        assert_eq!(chrome.panes.height, 20);
        assert_eq!(chrome.panes.x, 28);
        assert_eq!(chrome.panes.width, 52);
        assert_eq!(chrome.panes.y + chrome.panes.height, chrome.composer.y);
    }

    #[test]
    fn opening_the_composer_takes_rows_from_the_panes() {
        let area = Rect::new(0, 0, 80, 24);
        let closed = chrome(area, &state());
        let open = chrome(
            area,
            &ChromeState {
                composer: true,
                preview: true,
                composer_lines: 3,
                ..state()
            },
        );
        assert!(open.panes.height < closed.panes.height);
        assert_eq!(open.composer.height, 14);
        assert_eq!(open.status.y, 23);
    }

    #[test]
    fn small_terminals_still_get_a_pane() {
        let chrome = chrome(Rect::new(0, 0, 20, 6), &state());
        assert!(chrome.panes.width >= 1);
        assert!(chrome.panes.height >= 1);
    }

    #[test]
    fn tab_hits_line_up_with_the_strip() {
        let area = Rect::new(0, 0, 80, 24);
        let chrome = chrome(area, &state());
        let hits = tab_hits(&chrome, &snapshot());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1, 0);
        assert!(hits[0].0.y == 0);
        assert!(hits[0].0.x >= text::display_width(&chip_label("herdstead")) as u16);
    }

    #[test]
    fn sidebar_rows_are_clickable_where_they_name_something() {
        let area = Rect::new(0, 0, 80, 24);
        let chrome = chrome(area, &state());
        let sidebar = chrome.sidebar.unwrap();
        let snapshot = snapshot();
        let hits = sidebar_hits(sidebar, &snapshot);
        assert!(hits
            .iter()
            .any(|hit| matches!(hit.target, SidebarTarget::Workspace(0))));
        assert!(hits
            .iter()
            .any(|hit| matches!(hit.target, SidebarTarget::Tab(0))));
        assert!(hits
            .iter()
            .any(|hit| matches!(hit.target, SidebarTarget::Pane(PaneId(1)))));
        assert!(sidebar_rows(&snapshot)
            .iter()
            .any(|row| matches!(row, SidebarRow::Header("workspaces"))));
    }

    #[test]
    fn dividers_are_found_between_neighbours() {
        let mut snapshot = snapshot();
        snapshot.panes.push(PaneFrame {
            id: PaneId(2),
            rect: Rect::new(28, 10, 52, 9),
            title: "logs".into(),
            cwd: "/tmp".into(),
            command: "zsh".into(),
            kind: herdstead_core::model::PaneKind::Shell,
            viewer: None,
            agent: None,
            status: PaneStatus::Idle,
            bell: false,
            exit_code: None,
            scroll: 0,
            grid: None,
        });
        snapshot.panes[0].rect = Rect::new(28, 1, 52, 8);
        let divider = divider_at(&snapshot, 50, 9).unwrap();
        assert_eq!(divider.pane, PaneId(1));
        assert_eq!(divider.dir, Dir::Down);
        assert!(divider_at(&snapshot, 10, 4).is_none());
    }

    #[test]
    fn overlay_is_centred_and_inside_the_screen() {
        let area = Rect::new(0, 0, 80, 24);
        let panel = overlay_rect(area, 6);
        assert!(panel.x > 0 && panel.right() < 80);
        assert!(panel.y > 0 && panel.bottom() < 24);
        assert!(panel.contains(40, 12));
    }
}
