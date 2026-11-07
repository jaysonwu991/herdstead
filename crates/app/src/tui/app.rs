//! Client-side input routing, the composer, overlays and presentation state.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use herdstead_core::config::{Binding, Config, LinkModifier};
use herdstead_core::geom::{Dir, Rect};
use herdstead_core::layout::SplitDir;
use herdstead_core::mention;
use herdstead_core::model::{PaneKind, Size, ViewerKind};
use herdstead_core::protocol::{Action, Key, KeyInput, Mods, PaneFrame, Snapshot};
use herdstead_core::text;
use herdstead_core::Theme;
use herdstead_render::markdown;
use herdstead_render::{Line as RLine, Link};

use crate::tui::layout as ui_layout;
use crate::tui::layout::Chrome;
use crate::tui::viewer;

/// Where keystrokes go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Pane,
    Composer,
}

/// A floating panel above the panes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    ThemeMenu {
        index: usize,
    },
    Help,
    Prompt {
        title: String,
        text: String,
        action: PromptAction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAction {
    RenameTab,
    RenameWorkspace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composer {
    pub open: bool,
    pub text: String,
    /// Byte offset of the caret.
    pub cursor: usize,
    pub preview: bool,
    pub completion: Option<Completion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub items: Vec<String>,
    pub index: usize,
    pub token_start: usize,
    pub token_end: usize,
}

/// A divider being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drag {
    pub pane: herdstead_core::ids::PaneId,
    pub dir: Dir,
    pub last: (u16, u16),
}

pub struct App {
    pub session: String,
    pub theme: Theme,
    pub themes: Vec<Theme>,
    pub theme_index: usize,
    /// The theme to return to when the theme menu is cancelled.
    theme_before_menu: Option<Theme>,
    pub config: Config,
    pub keymap: BTreeMap<String, Binding>,
    pub prefix: Binding,
    pub snapshot: Option<Snapshot>,
    /// The terminal's size, kept current from resize events.
    pub screen: Rect,
    pub warnings: Vec<String>,
    pub status: Option<(String, Instant)>,
    pub focus: Focus,
    pub composer: Composer,
    pub overlay: Option<Overlay>,
    pub sidebar: bool,
    pub sidebar_width: u16,
    pub prefix_armed: bool,
    pub quit: bool,
    pub detach_requested: bool,
    pub viewer_scroll: BTreeMap<herdstead_core::ids::PaneId, usize>,
    viewer_cache: HashMap<herdstead_core::ids::PaneId, ViewerCache>,
    pub completion_entries: Vec<String>,
    pub completion_root: PathBuf,
    pub last_pane_area: Option<Rect>,
    pub hover_link: Option<Link>,
    pub drag: Option<Drag>,
    /// Paths opened through the client, newest last.
    pub opened: Vec<PathBuf>,
}

struct ViewerCache {
    at: Instant,
    width: u16,
    path: PathBuf,
    lines: Vec<RLine>,
}

/// Events the run loop feeds the app.
pub enum AppEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
    Snapshot(Box<Snapshot>),
    Detached(String),
    ServerError(String),
    Disconnected(String),
}

impl App {
    pub fn new(
        session: String,
        config: Config,
        theme: Theme,
        themes: Vec<Theme>,
        warnings: Vec<String>,
        cwd: &Path,
    ) -> Self {
        let theme_index = themes
            .iter()
            .position(|candidate| candidate.name == theme.name)
            .unwrap_or(0);
        let keymap = config.keymap();
        let prefix = config.prefix_binding();
        Self {
            session,
            theme,
            themes,
            theme_index,
            theme_before_menu: None,
            sidebar: config.sidebar,
            sidebar_width: config.sidebar_width,
            composer: Composer {
                preview: config.composer_markdown_preview,
                ..Composer::default()
            },
            config,
            focus: Focus::Pane,
            overlay: None,
            snapshot: None,
            screen: Rect::new(0, 0, 80, 24),
            warnings,
            status: None,
            keymap,
            prefix,
            prefix_armed: false,
            quit: false,
            detach_requested: false,
            viewer_scroll: BTreeMap::new(),
            viewer_cache: HashMap::new(),
            completion_entries: Vec::new(),
            completion_root: cwd.to_path_buf(),
            last_pane_area: None,
            hover_link: None,
            drag: None,
            opened: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    pub fn focused_pane(&self) -> Option<&PaneFrame> {
        let snapshot = self.snapshot.as_ref()?;
        let focused = snapshot.focused?;
        snapshot.panes.iter().find(|pane| pane.id == focused)
    }

    pub fn focused_cwd(&self) -> PathBuf {
        self.focused_pane()
            .map(|pane| PathBuf::from(&pane.cwd))
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.completion_root.clone())
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now()));
    }

    /// Draws chromeless: the theme list comes from the real theme folder.
    pub fn refresh_theme_list(&mut self, themes: Vec<Theme>) {
        self.themes = themes;
        self.theme_index = self
            .themes
            .iter()
            .position(|candidate| candidate.name == self.theme.name)
            .unwrap_or(0);
    }

    pub fn handle(&mut self, event: AppEvent) -> Vec<Action> {
        match event {
            AppEvent::Key(key) => self.on_key(key),
            AppEvent::Mouse(mouse) => self.on_mouse(mouse),
            AppEvent::Paste(text) => {
                if self.focus == Focus::Composer {
                    let at = self.composer.cursor;
                    self.composer.text.insert_str(at, &text);
                    self.composer.cursor += text.len();
                    self.refresh_completion();
                    Vec::new()
                } else {
                    vec![Action::Paste { text }]
                }
            }
            AppEvent::Resize(_, _) => Vec::new(),
            AppEvent::Snapshot(snapshot) => {
                self.clamp_viewers(&snapshot);
                self.snapshot = Some(*snapshot);
                Vec::new()
            }
            AppEvent::Detached(reason) => {
                self.set_status(reason);
                self.quit = true;
                Vec::new()
            }
            AppEvent::ServerError(message) => {
                self.set_status(message);
                Vec::new()
            }
            AppEvent::Disconnected(reason) => {
                self.set_status(reason);
                self.quit = true;
                Vec::new()
            }
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.kind == KeyEventKind::Release {
            return Vec::new();
        }
        if let Some(overlay) = self.overlay.clone() {
            return self.on_overlay_key(overlay, key);
        }
        if self.focus == Focus::Composer {
            return self.on_composer_key(key);
        }
        if !sendable(key.code) {
            return Vec::new();
        }
        let input = key_input(&key);
        if self.prefix_armed {
            self.prefix_armed = false;
            return self.dispatch_binding(&input);
        }
        if self.is_prefix(&input) {
            self.prefix_armed = true;
            return Vec::new();
        }
        // A viewer pane scrolls with the usual navigation keys.
        if let Some(pane) = self.focused_pane().cloned() {
            if pane.kind == PaneKind::Viewer {
                if let Some(action) = self.viewer_key(&pane, &input) {
                    return vec![action];
                }
            }
        }
        vec![Action::Key { input }]
    }

    fn is_prefix(&self, input: &KeyInput) -> bool {
        keys_match(&self.prefix, input)
    }

    pub fn dispatch_binding(&mut self, input: &KeyInput) -> Vec<Action> {
        // Digits select tabs; with shift they select workspaces.
        if let Key::Char(ch) = input.key {
            let with_shift = input.mods.shift || !ch.is_ascii_digit();
            let digit = if ch.is_ascii_digit() {
                ch.to_digit(10)
            } else if input.mods.shift {
                shifted_digit(ch)
            } else {
                None
            };
            if let Some(digit) = digit {
                if (1..=9).contains(&digit) {
                    let index = digit as usize - 1;
                    return if with_shift {
                        vec![Action::SelectWorkspace { index }]
                    } else {
                        vec![Action::SelectTab { index }]
                    };
                }
            }
        }
        let name = self
            .keymap
            .iter()
            .find(|(action, binding)| *action != "prefix" && keys_match(binding, input))
            .map(|(action, _)| action.clone());
        match name {
            Some(name) => self.run_action(&name),
            None => {
                self.set_status(format!(
                    "no binding for {} — `prefix+?` lists keys",
                    describe_key(input)
                ));
                Vec::new()
            }
        }
    }

    fn run_action(&mut self, name: &str) -> Vec<Action> {
        let focused = self.snapshot.as_ref().and_then(|snapshot| snapshot.focused);
        match name {
            "split-vertical" => vec![Action::Split {
                dir: SplitDir::Vertical,
            }],
            "split-horizontal" => vec![Action::Split {
                dir: SplitDir::Horizontal,
            }],
            "close-pane" => vec![Action::ClosePane],
            "focus-left" => vec![Action::FocusDir { dir: Dir::Left }],
            "focus-right" => vec![Action::FocusDir { dir: Dir::Right }],
            "focus-up" => vec![Action::FocusDir { dir: Dir::Up }],
            "focus-down" => vec![Action::FocusDir { dir: Dir::Down }],
            "focus-next" => vec![Action::FocusCycle { step: 1 }],
            "focus-prev" => vec![Action::FocusCycle { step: -1 }],
            "zoom" => vec![Action::ToggleZoom],
            "new-tab" => vec![Action::NewTab { cwd: None }],
            "next-tab" => vec![Action::CycleTab { step: 1 }],
            "prev-tab" => vec![Action::CycleTab { step: -1 }],
            "close-tab" => vec![Action::CloseTab],
            "rename-tab" => {
                self.overlay = Some(Overlay::Prompt {
                    title: "rename tab".into(),
                    text: String::new(),
                    action: PromptAction::RenameTab,
                });
                Vec::new()
            }
            "new-workspace" => vec![Action::NewWorkspace { dir: None }],
            "next-workspace" => vec![Action::CycleWorkspace { step: 1 }],
            "prev-workspace" => vec![Action::CycleWorkspace { step: -1 }],
            "close-workspace" => vec![Action::CloseWorkspace],
            "rename-workspace" => {
                self.overlay = Some(Overlay::Prompt {
                    title: "rename workspace".into(),
                    text: String::new(),
                    action: PromptAction::RenameWorkspace,
                });
                Vec::new()
            }
            "composer" => {
                self.open_composer();
                Vec::new()
            }
            "theme-menu" => {
                let index = self.theme_index;
                self.theme_before_menu = Some(self.theme.clone());
                self.overlay = Some(Overlay::ThemeMenu { index });
                Vec::new()
            }
            "open-file" => {
                self.open_composer_with("/open ");
                Vec::new()
            }
            "open-tree" => {
                self.open_composer_with("/tree ");
                Vec::new()
            }
            "markdown-preview" => {
                self.composer.preview = !self.composer.preview;
                self.open_composer();
                if self.composer.preview {
                    self.set_status("Markdown preview on");
                }
                Vec::new()
            }
            "sidebar" => {
                self.sidebar = !self.sidebar;
                Vec::new()
            }
            "help" => {
                self.overlay = Some(Overlay::Help);
                Vec::new()
            }
            "reload" => {
                self.reload();
                Vec::new()
            }
            "detach" => {
                self.detach_requested = true;
                Vec::new()
            }
            "scroll-page-up" => focused
                .map(|pane| Action::ScrollPane {
                    pane,
                    delta: -1,
                    page: true,
                })
                .into_iter()
                .collect(),
            "scroll-page-down" => focused
                .map(|pane| Action::ScrollPane {
                    pane,
                    delta: 1,
                    page: true,
                })
                .into_iter()
                .collect(),
            "scroll-top" => focused
                .map(|pane| Action::ScrollToTop { pane })
                .into_iter()
                .collect(),
            "scroll-bottom" => focused
                .map(|pane| Action::ScrollToBottom { pane })
                .into_iter()
                .collect(),
            other => {
                self.set_status(format!("`{other}` is not wired up yet"));
                Vec::new()
            }
        }
    }

    fn reload(&mut self) {
        let cwd = self.completion_root.clone();
        let paths = herdstead_core::paths::Paths::discover();
        let (config, warnings) = Config::load(&cwd, &paths);
        self.theme = crate::load_theme(&None, &config, &paths);
        self.refresh_theme_list(Theme::catalog(&paths.themes_dir()));
        self.keymap = config.keymap();
        self.prefix = config.prefix_binding();
        self.sidebar = config.sidebar;
        self.sidebar_width = config.sidebar_width;
        self.config = config;
        self.warnings = warnings;
        self.set_status(format!("reloaded {} theme", self.theme.name));
    }

    fn open_composer(&mut self) {
        self.composer.open = true;
        self.focus = Focus::Composer;
        // Rescan so files created since the last prompt show up.
        self.completion_entries.clear();
        self.refresh_completion();
    }

    fn open_composer_with(&mut self, prefix: &str) {
        self.composer.text = prefix.to_string();
        self.composer.cursor = self.composer.text.len();
        self.open_composer();
    }

    fn close_composer(&mut self) {
        self.composer.open = false;
        self.composer.completion = None;
        self.focus = Focus::Pane;
    }

    fn on_composer_key(&mut self, key: KeyEvent) -> Vec<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Esc => {
                self.close_composer();
                Vec::new()
            }
            KeyCode::Enter if shift => {
                self.insert_text("\n");
                Vec::new()
            }
            KeyCode::Enter => {
                if self.accept_completion() {
                    return Vec::new();
                }
                self.submit_composer()
            }
            KeyCode::Tab => {
                if !self.accept_completion() {
                    self.insert_text("  ");
                }
                Vec::new()
            }
            KeyCode::Backspace => {
                self.delete_backward(alt);
                Vec::new()
            }
            KeyCode::Delete => {
                self.delete_forward();
                Vec::new()
            }
            KeyCode::Left => {
                self.move_cursor(-1, ctrl);
                Vec::new()
            }
            KeyCode::Right => {
                self.move_cursor(1, ctrl);
                Vec::new()
            }
            KeyCode::Up => {
                if !self.completion_up() && self.composer.text.contains('\n') {
                    self.move_vertical(-1);
                }
                Vec::new()
            }
            KeyCode::Down => {
                if !self.completion_down() && self.composer.text.contains('\n') {
                    self.move_vertical(1);
                }
                Vec::new()
            }
            KeyCode::Home => {
                self.composer.cursor = 0;
                self.refresh_completion();
                Vec::new()
            }
            KeyCode::End => {
                self.composer.cursor = self.composer.text.len();
                self.refresh_completion();
                Vec::new()
            }
            KeyCode::Char('p') if ctrl => {
                self.composer.preview = !self.composer.preview;
                Vec::new()
            }
            KeyCode::Char('a') if ctrl => {
                self.composer.cursor = 0;
                Vec::new()
            }
            KeyCode::Char('e') if ctrl => {
                self.composer.cursor = self.composer.text.len();
                Vec::new()
            }
            KeyCode::Char('u') if ctrl => {
                let cursor = self.composer.cursor;
                self.composer.text.drain(..cursor);
                self.composer.cursor = 0;
                self.refresh_completion();
                Vec::new()
            }
            KeyCode::Char('k') if ctrl => {
                let cursor = self.composer.cursor;
                self.composer.text.truncate(cursor);
                self.refresh_completion();
                Vec::new()
            }
            KeyCode::Char('w') if ctrl => {
                self.delete_word();
                Vec::new()
            }
            KeyCode::Char(ch) if !ctrl || ch == ' ' => {
                self.insert_text(&ch.to_string());
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn insert_text(&mut self, text: &str) {
        let at = self.composer.cursor.min(self.composer.text.len());
        self.composer.text.insert_str(at, text);
        self.composer.cursor = at + text.len();
        self.refresh_completion();
    }

    fn delete_backward(&mut self, word: bool) {
        if self.composer.cursor == 0 {
            return;
        }
        if word {
            self.delete_word();
            return;
        }
        let mut index = self.composer.cursor;
        while index > 0 && !self.composer.text.is_char_boundary(index) {
            index -= 1;
        }
        let start = self.composer.text[..index]
            .char_indices()
            .next_back()
            .map(|(offset, _)| offset)
            .unwrap_or(0);
        self.composer.text.drain(start..index);
        self.composer.cursor = start;
        self.refresh_completion();
    }

    fn delete_forward(&mut self) {
        let cursor = self.composer.cursor;
        if cursor >= self.composer.text.len() {
            return;
        }
        let end = self.composer.text[cursor..]
            .char_indices()
            .nth(1)
            .map(|(offset, _)| cursor + offset)
            .unwrap_or(self.composer.text.len());
        self.composer.text.drain(cursor..end);
        self.refresh_completion();
    }

    fn delete_word(&mut self) {
        let cursor = self.composer.cursor;
        let before = &self.composer.text[..cursor];
        let boundary = before
            .char_indices()
            .rev()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(offset, _)| offset + 1)
            .unwrap_or(0);
        self.composer.text.drain(boundary..cursor);
        self.composer.cursor = boundary;
        self.refresh_completion();
    }

    fn move_cursor(&mut self, direction: i32, word: bool) {
        let text = &self.composer.text;
        let cursor = self.composer.cursor;
        if direction < 0 {
            if word {
                let before = &text[..cursor];
                self.composer.cursor = before
                    .char_indices()
                    .rev()
                    .find(|(_, ch)| ch.is_whitespace())
                    .map(|(offset, _)| offset + 1)
                    .unwrap_or(0);
            } else {
                self.composer.cursor = text[..cursor]
                    .char_indices()
                    .next_back()
                    .map(|(offset, _)| offset)
                    .unwrap_or(0);
            }
        } else if word {
            let after = &text[cursor..];
            self.composer.cursor = after
                .char_indices()
                .find(|(_, ch)| ch.is_whitespace())
                .map(|(offset, _)| cursor + offset + 1)
                .unwrap_or(text.len());
        } else {
            self.composer.cursor = text[cursor..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| cursor + offset)
                .unwrap_or(text.len());
        }
        self.refresh_completion();
    }

    fn move_vertical(&mut self, direction: i32) {
        let text = &self.composer.text;
        let cursor = self.composer.cursor;
        let line_start = text[..cursor]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let column = cursor - line_start;
        let target_start = if direction < 0 {
            if line_start == 0 {
                return;
            }
            text[..line_start - 1]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0)
        } else {
            match text[line_start..].find('\n') {
                Some(offset) => line_start + offset + 1,
                None => return,
            }
        };
        let target_line_end = text[target_start..]
            .find('\n')
            .map(|offset| target_start + offset)
            .unwrap_or(text.len());
        self.composer.cursor = (target_start + column).min(target_line_end);
        self.refresh_completion();
    }

    fn submit_composer(&mut self) -> Vec<Action> {
        let text = self.composer.text.trim_end().to_string();
        if text.is_empty() {
            self.close_composer();
            return Vec::new();
        }
        if !text.starts_with('/') && !self.can_prompt_focused_pane() {
            // Keep the text: silently dropping a prompt is worse than saying
            // which pane is in the way.
            self.set_status(
                "the focused pane is not a running shell — focus a shell pane, or use /split",
            );
            return Vec::new();
        }
        self.composer.text.clear();
        self.composer.cursor = 0;
        self.composer.completion = None;
        if text.starts_with('/') {
            return self.run_command(&text);
        }
        vec![Action::SendText { text, submit: true }]
    }

    /// A prompt only makes sense for a live shell pane.
    fn can_prompt_focused_pane(&self) -> bool {
        self.focused_pane()
            .is_some_and(|pane| pane.kind == PaneKind::Shell && pane.exit_code.is_none())
    }

    /// Slash commands the composer understands.
    fn run_command(&mut self, command: &str) -> Vec<Action> {
        let mut parts = command
            .trim_start_matches('/')
            .splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("").trim();
        let argument = parts.next().unwrap_or("").trim().to_string();
        let focused = self.snapshot.as_ref().and_then(|snapshot| snapshot.focused);
        match name {
            "" => Vec::new(),
            "split" => vec![Action::Split {
                dir: if argument.starts_with('h') {
                    SplitDir::Horizontal
                } else {
                    SplitDir::Vertical
                },
            }],
            "close" => vec![Action::ClosePane],
            "tab" => vec![Action::NewTab {
                cwd: if argument.is_empty() {
                    None
                } else {
                    Some(argument)
                },
            }],
            "workspace" => vec![Action::NewWorkspace { dir: None }],
            "select" => match argument.parse::<usize>() {
                Ok(number) => vec![Action::SelectTab {
                    index: number.saturating_sub(1),
                }],
                Err(_) => {
                    self.set_status("usage: /select <tab number>");
                    Vec::new()
                }
            },
            "theme" => {
                if argument.is_empty() {
                    let index = self.theme_index;
                    self.overlay = Some(Overlay::ThemeMenu { index });
                } else {
                    self.apply_theme(&argument);
                }
                Vec::new()
            }
            "open" if argument.is_empty() => {
                self.set_status("usage: /open <path>");
                Vec::new()
            }
            "open" => vec![Action::OpenViewer {
                path: argument,
                kind: None,
                split: None,
            }],
            "tree" => vec![Action::OpenViewer {
                path: if argument.is_empty() {
                    ".".into()
                } else {
                    argument
                },
                kind: Some(ViewerKind::Tree),
                split: None,
            }],
            "cat" if argument.is_empty() => {
                self.set_status("usage: /cat <file>");
                Vec::new()
            }
            "cat" => vec![Action::OpenViewer {
                path: argument,
                kind: Some(ViewerKind::Code),
                split: None,
            }],
            "md" if argument.is_empty() => {
                self.set_status("usage: /md <file>");
                Vec::new()
            }
            "md" => vec![Action::OpenViewer {
                path: argument,
                kind: Some(ViewerKind::Markdown),
                split: None,
            }],
            "rename-tab" => {
                self.overlay = Some(Overlay::Prompt {
                    title: "rename tab".into(),
                    text: argument,
                    action: PromptAction::RenameTab,
                });
                Vec::new()
            }
            "rename-workspace" => {
                self.overlay = Some(Overlay::Prompt {
                    title: "rename workspace".into(),
                    text: argument,
                    action: PromptAction::RenameWorkspace,
                });
                Vec::new()
            }
            "scroll-top" => focused
                .map(|pane| Action::ScrollToTop { pane })
                .into_iter()
                .collect(),
            "scroll-bottom" => focused
                .map(|pane| Action::ScrollToBottom { pane })
                .into_iter()
                .collect(),
            "help" => {
                self.overlay = Some(Overlay::Help);
                Vec::new()
            }
            "detach" => {
                self.detach_requested = true;
                Vec::new()
            }
            other => {
                self.set_status(format!("unknown command `/{other}` — try /help"));
                Vec::new()
            }
        }
    }

    fn apply_theme(&mut self, name: &str) {
        if let Some(index) = self
            .themes
            .iter()
            .position(|theme| theme.name.eq_ignore_ascii_case(name))
        {
            self.theme_index = index;
            self.theme = self.themes[index].clone();
            let override_names = self.config.theme_colors.clone();
            if let Err(err) = self.theme.apply_overrides(&override_names) {
                self.set_status(err);
            } else {
                self.set_status(format!("theme: {}", self.theme.name));
            }
            self.viewer_cache.clear();
        } else {
            self.set_status(format!("no theme `{name}`"));
        }
    }

    fn on_overlay_key(&mut self, overlay: Overlay, key: KeyEvent) -> Vec<Action> {
        match overlay {
            Overlay::ThemeMenu { mut index } => match key.code {
                KeyCode::Esc => {
                    // Cancelling puts back the theme that was active when the
                    // menu opened, undoing the live preview.
                    if let Some(previous) = self.theme_before_menu.take() {
                        self.theme = previous;
                        self.theme_index = self
                            .themes
                            .iter()
                            .position(|candidate| candidate.name == self.theme.name)
                            .unwrap_or(self.theme_index);
                        self.viewer_cache.clear();
                    }
                    self.overlay = None;
                    Vec::new()
                }
                KeyCode::Enter | KeyCode::Char('q') => {
                    if let Some(theme) = self.themes.get(index).cloned() {
                        self.theme = theme;
                        self.theme_index = index;
                        self.viewer_cache.clear();
                        self.set_status(format!("theme: {}", self.theme.name));
                    }
                    self.theme_before_menu = None;
                    self.overlay = None;
                    Vec::new()
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    index = index.saturating_sub(1);
                    self.overlay = Some(Overlay::ThemeMenu { index });
                    self.preview_theme(index);
                    Vec::new()
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    index = (index + 1).min(self.themes.len().saturating_sub(1));
                    self.overlay = Some(Overlay::ThemeMenu { index });
                    self.preview_theme(index);
                    Vec::new()
                }
                _ => Vec::new(),
            },
            Overlay::Help => {
                if matches!(
                    key.code,
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')
                ) {
                    self.overlay = None;
                }
                Vec::new()
            }
            Overlay::Prompt {
                title,
                mut text,
                action,
            } => match key.code {
                KeyCode::Esc => {
                    self.overlay = None;
                    Vec::new()
                }
                KeyCode::Enter => {
                    self.overlay = None;
                    self.finish_prompt(action, &text)
                }
                KeyCode::Backspace => {
                    text.pop();
                    self.overlay = Some(Overlay::Prompt {
                        title,
                        text,
                        action,
                    });
                    Vec::new()
                }
                KeyCode::Char(ch) => {
                    text.push(ch);
                    self.overlay = Some(Overlay::Prompt {
                        title,
                        text,
                        action,
                    });
                    Vec::new()
                }
                _ => Vec::new(),
            },
        }
    }

    fn preview_theme(&mut self, index: usize) {
        if let Some(theme) = self.themes.get(index).cloned() {
            self.theme = theme;
            self.viewer_cache.clear();
        }
    }

    fn finish_prompt(&mut self, action: PromptAction, text: &str) -> Vec<Action> {
        match action {
            PromptAction::RenameTab => vec![Action::RenameTab {
                name: text.trim().to_string(),
            }],
            PromptAction::RenameWorkspace => vec![Action::RenameWorkspace {
                name: text.trim().to_string(),
            }],
        }
    }

    /// Recomputes the `@path` completion for the caret.
    fn refresh_completion(&mut self) {
        let text = self.composer.text.clone();
        let cursor = self.composer.cursor;
        let Some(token) = mention::token(&text, cursor) else {
            self.composer.completion = None;
            return;
        };
        let root = self.focused_cwd();
        // Refresh the candidate list when the folder changed or it is empty.
        if self.completion_entries.is_empty() || root != self.completion_root {
            let ignores = self.config.tree_ignores.clone();
            self.completion_entries = completion_entries(&root, &ignores, 2_000);
            self.completion_root = root;
        }
        let suggestions = mention::suggestions(&self.completion_entries, &token, 8);
        if suggestions.is_empty() {
            self.composer.completion = None;
            return;
        }
        let index = self
            .composer
            .completion
            .as_ref()
            .map(|completion| completion.index.min(suggestions.len() - 1))
            .unwrap_or(0);
        self.composer.completion = Some(Completion {
            items: suggestions,
            index,
            token_start: token.start,
            token_end: token.end,
        });
    }

    fn accept_completion(&mut self) -> bool {
        let Some(completion) = self.composer.completion.clone() else {
            return false;
        };
        let Some(item) = completion.items.get(completion.index).cloned() else {
            return false;
        };
        let replacement = mention::replacement(&item);
        let start = completion.token_start.min(self.composer.text.len());
        let end = completion.token_end.min(self.composer.text.len());
        self.composer.text.replace_range(start..end, &replacement);
        self.composer.cursor = start + replacement.len();
        self.composer.completion = None;
        true
    }

    fn completion_up(&mut self) -> bool {
        let Some(mut completion) = self.composer.completion.clone() else {
            return false;
        };
        completion.index = completion.index.saturating_sub(1);
        self.composer.completion = Some(completion);
        true
    }

    fn completion_down(&mut self) -> bool {
        let Some(mut completion) = self.composer.completion.clone() else {
            return false;
        };
        completion.index = (completion.index + 1).min(completion.items.len().saturating_sub(1));
        self.composer.completion = Some(completion);
        true
    }

    fn on_mouse(&mut self, mouse: MouseEvent) -> Vec<Action> {
        if !self.config.mouse {
            return Vec::new();
        }
        let Some(snapshot) = self.snapshot.clone() else {
            return Vec::new();
        };
        let chrome = ui_layout::chrome(self.screen, &self.chrome_state());

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.overlay.is_some() {
                    self.overlay_click(mouse.column, mouse.row);
                    return Vec::new();
                }
                if chrome.composer.contains(mouse.column, mouse.row) {
                    self.open_composer();
                    let content_x = chrome.composer.x + 3;
                    let column = usize::from(mouse.column.saturating_sub(content_x));
                    self.composer.cursor = byte_at_column(&self.composer.text, column);
                    return Vec::new();
                }
                if chrome.tab_bar.contains(mouse.column, mouse.row) {
                    if let Some((_, index)) = ui_layout::tab_hits(&chrome, &snapshot)
                        .into_iter()
                        .find(|(rect, _)| rect.contains(mouse.column, mouse.row))
                    {
                        return vec![Action::SelectTab { index }];
                    }
                    return Vec::new();
                }
                if chrome
                    .sidebar
                    .is_some_and(|rect| rect.contains(mouse.column, mouse.row))
                {
                    return self.sidebar_click(&snapshot, &chrome, mouse.column, mouse.row);
                }
                if let Some(pane) = snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.rect.contains(mouse.column, mouse.row))
                {
                    if let Some(divider) = ui_layout::divider_at(&snapshot, mouse.column, mouse.row)
                    {
                        self.drag = Some(Drag {
                            pane: divider.pane,
                            dir: divider.dir,
                            last: (mouse.column, mouse.row),
                        });
                        return Vec::new();
                    }
                    if self.link_modifier_held(mouse.modifiers) {
                        if let Some(link) = self.link_at(pane, mouse.column, mouse.row) {
                            let cwd = PathBuf::from(&pane.cwd);
                            return self.open_link(link, &cwd);
                        }
                    }
                    if snapshot.focused != Some(pane.id) {
                        return vec![Action::FocusPane { pane: pane.id }];
                    }
                }
                Vec::new()
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(drag) = self.drag else {
                    return Vec::new();
                };
                let (last_x, last_y) = drag.last;
                let delta = match drag.dir {
                    Dir::Left | Dir::Right => i32::from(mouse.column) - i32::from(last_x),
                    Dir::Up | Dir::Down => i32::from(mouse.row) - i32::from(last_y),
                };
                if delta == 0 {
                    return Vec::new();
                }
                self.drag = Some(Drag {
                    last: (mouse.column, mouse.row),
                    ..drag
                });
                vec![Action::ResizePane {
                    dir: drag.dir,
                    cells: delta.clamp(-40, 40) as i16,
                }]
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.drag = None;
                Vec::new()
            }
            MouseEventKind::Moved => {
                self.hover_link = if self.link_modifier_held(mouse.modifiers) {
                    snapshot
                        .panes
                        .iter()
                        .find(|pane| pane.rect.contains(mouse.column, mouse.row))
                        .and_then(|pane| self.link_at(pane, mouse.column, mouse.row))
                } else {
                    None
                };
                Vec::new()
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let pane = snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.rect.contains(mouse.column, mouse.row));
                let Some(pane) = pane else {
                    return Vec::new();
                };
                let delta = if matches!(mouse.kind, MouseEventKind::ScrollUp) {
                    -3
                } else {
                    3
                };
                if pane.kind == PaneKind::Viewer {
                    self.scroll_viewer(pane.id, delta);
                    return Vec::new();
                }
                vec![Action::ScrollPane {
                    pane: pane.id,
                    delta,
                    page: false,
                }]
            }
            _ => Vec::new(),
        }
    }

    fn overlay_click(&mut self, column: u16, row: u16) {
        let overlay = self.overlay.clone();
        match overlay {
            Some(Overlay::ThemeMenu { index: _ }) => {
                let panel = ui_layout::overlay_rect(self.screen, self.themes.len());
                if !panel.contains(column, row) {
                    self.overlay = None;
                    return;
                }
                let row_in_panel = usize::from(row.saturating_sub(panel.y + 2));
                if let Some(theme) = self.themes.get(row_in_panel).cloned() {
                    self.theme = theme;
                    self.theme_index = row_in_panel;
                    self.viewer_cache.clear();
                    self.overlay = None;
                }
            }
            Some(Overlay::Help) => self.overlay = None,
            Some(Overlay::Prompt { .. }) => {}
            None => {}
        }
    }

    fn sidebar_click(
        &mut self,
        snapshot: &Snapshot,
        chrome: &Chrome,
        column: u16,
        row: u16,
    ) -> Vec<Action> {
        let Some(sidebar) = chrome.sidebar else {
            return Vec::new();
        };
        let hits = ui_layout::sidebar_hits(sidebar, snapshot);
        for hit in hits {
            if hit.rect.contains(column, row) {
                return match hit.target {
                    ui_layout::SidebarTarget::Workspace(index) => {
                        vec![Action::SelectWorkspace { index }]
                    }
                    ui_layout::SidebarTarget::Tab(index) => vec![Action::SelectTab { index }],
                    ui_layout::SidebarTarget::Pane(pane) => vec![Action::FocusPane { pane }],
                };
            }
        }
        Vec::new()
    }

    fn link_modifier_held(&self, modifiers: KeyModifiers) -> bool {
        match self.config.link_modifier {
            LinkModifier::Super => modifiers.contains(KeyModifiers::SUPER),
            LinkModifier::Ctrl => modifiers.contains(KeyModifiers::CONTROL),
            LinkModifier::Alt => modifiers.contains(KeyModifiers::ALT),
        }
    }

    /// The link under a screen position inside a pane.
    fn link_at(&mut self, pane: &PaneFrame, column: u16, row: u16) -> Option<Link> {
        let inner = pane.rect.inner();
        if !inner.contains(column, row) {
            return None;
        }
        let lines = self.pane_lines(pane, inner.width)?;
        let line = lines.get(usize::from(row - inner.y))?;
        line.link_at(usize::from(column - inner.x))
    }

    fn open_link(&mut self, link: Link, base: &Path) -> Vec<Action> {
        match link {
            Link::Url(url) => {
                if let Err(err) = open_url(&url) {
                    self.set_status(format!("could not open {url}: {err}"));
                }
                Vec::new()
            }
            Link::Path { path, line, column } => {
                let resolved = route_path(base, &path);
                if !resolved.exists() {
                    self.set_status(format!("{}: no such file or folder", resolved.display()));
                    return Vec::new();
                }
                self.opened.push(resolved.clone());
                let _ = (line, column);
                self.set_status(format!("opening {}", resolved.display()));
                vec![Action::OpenViewer {
                    path: resolved.display().to_string(),
                    kind: None,
                    split: None,
                }]
            }
        }
    }

    fn viewer_key(&mut self, pane: &PaneFrame, input: &KeyInput) -> Option<Action> {
        let height = usize::from(pane.rect.inner().height.max(1));
        let delta = match input.key {
            Key::Up => 1,
            Key::Down => -1,
            Key::PageUp | Key::Char('b') => height as i32,
            Key::PageDown | Key::Char(' ') => -(height as i32),
            Key::Home | Key::Char('g') => i32::MAX / 2,
            Key::End | Key::Char('G') => -(i32::MAX / 2),
            Key::Char('q') | Key::Esc => {
                return Some(Action::ClosePane);
            }
            _ => return None,
        };
        self.scroll_viewer(pane.id, delta);
        None
    }

    /// Scrolls a viewer pane; the runtime never sees this.
    pub fn scroll_viewer(&mut self, pane: herdstead_core::ids::PaneId, delta: i32) {
        let Some(frame) = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.panes.iter().find(|candidate| candidate.id == pane))
        else {
            return;
        };
        let inner = frame.rect.inner();
        let width = inner.width.max(1);
        let height = usize::from(inner.height.max(1));
        let lines = match self.viewer_cache.get(&pane) {
            Some(cache) if cache.width == width => {
                let _ = height;
                cache.lines.len()
            }
            _ => {
                let lines =
                    viewer::render(frame, width, &self.theme, &self.config).unwrap_or_default();
                let count = lines.len();
                self.viewer_cache.insert(
                    pane,
                    ViewerCache {
                        at: Instant::now(),
                        width,
                        path: frame
                            .viewer
                            .as_ref()
                            .map(|spec| spec.path.clone())
                            .unwrap_or_default(),
                        lines,
                    },
                );
                count
            }
        };
        let max = lines.saturating_sub(height.saturating_sub(1));
        let current = self.viewer_scroll.get(&pane).copied().unwrap_or(0);
        let next = if delta > 0 {
            current.saturating_add(delta as usize).min(max)
        } else {
            current.saturating_sub((-delta) as usize)
        };
        self.viewer_scroll.insert(pane, next);
    }

    fn clamp_viewers(&mut self, snapshot: &Snapshot) {
        for pane in &snapshot.panes {
            if pane.kind != PaneKind::Viewer {
                continue;
            }
            let height = usize::from(pane.rect.inner().height.max(1));
            let max = self
                .viewer_cache
                .get(&pane.id)
                .map(|cache| cache.lines.len().saturating_sub(height.saturating_sub(1)))
                .unwrap_or(0);
            if let Some(offset) = self.viewer_scroll.get_mut(&pane.id) {
                *offset = (*offset).min(max);
            }
        }
    }

    /// Rendered rows for a pane, or `None` when there is nothing to draw yet.
    pub fn pane_lines(&mut self, pane: &PaneFrame, width: u16) -> Option<Vec<RLine>> {
        match pane.kind {
            PaneKind::Shell => {
                let grid = pane.grid.as_ref()?;
                let mut lines = herdstead_render::term::rich_lines(grid, &self.theme);
                let cwd = PathBuf::from(&pane.cwd);
                herdstead_render::term::decorate_input(
                    grid,
                    &mut lines,
                    &self.theme,
                    self.config.links,
                    &|path| route_path(&cwd, path).exists(),
                );
                if self.config.links {
                    Some(
                        lines
                            .into_iter()
                            .map(|line| {
                                herdstead_render::link::decorate(
                                    &line,
                                    &self.theme,
                                    &|path: &str| route_path(&cwd, path).exists(),
                                )
                            })
                            .collect(),
                    )
                } else {
                    Some(lines)
                }
            }
            PaneKind::Viewer => {
                let wanted = pane
                    .viewer
                    .as_ref()
                    .map(|spec| spec.path.clone())
                    .unwrap_or_default();
                let fresh = self
                    .viewer_cache
                    .get(&pane.id)
                    .map(|cache| {
                        cache.width == width
                            && cache.at.elapsed() < Duration::from_millis(750)
                            && cache.path == wanted
                    })
                    .unwrap_or(false);
                if !fresh {
                    let lines = viewer::render(pane, width, &self.theme, &self.config)
                        .unwrap_or_else(|err| {
                            vec![RLine::styled(
                                err,
                                herdstead_render::Style::plain().fg(self.theme.error),
                            )]
                        });
                    self.viewer_cache.insert(
                        pane.id,
                        ViewerCache {
                            at: Instant::now(),
                            width,
                            path: pane
                                .viewer
                                .as_ref()
                                .map(|spec| spec.path.clone())
                                .unwrap_or_default(),
                            lines,
                        },
                    );
                }
                self.viewer_cache
                    .get(&pane.id)
                    .map(|cache| cache.lines.clone())
            }
        }
    }

    pub fn viewer_offset(&self, pane: herdstead_core::ids::PaneId) -> usize {
        self.viewer_scroll.get(&pane).copied().unwrap_or(0)
    }

    pub fn chrome_state(&self) -> ui_layout::ChromeState {
        ui_layout::ChromeState {
            sidebar: self.sidebar,
            sidebar_width: self.sidebar_width,
            composer: self.composer.open,
            preview: self.composer.open && self.composer.preview,
            composer_lines: self.composer.text.lines().count().clamp(1, 4) as u16,
            status: self.config.status_bar,
        }
    }

    /// The Markdown preview rows for the composer.
    pub fn preview_lines(&self, width: usize) -> Vec<RLine> {
        markdown::render_width(&self.composer.text, width, &self.theme)
    }

    /// A one-line hint for the status bar.
    pub fn hint(&self) -> String {
        if let Some(link) = &self.hover_link {
            return format!("open {} ", link.target());
        }
        if self.prefix_armed {
            return format!(
                "prefix {} … (esc cancels)",
                herdstead_core::config::format_binding(&self.prefix)
            );
        }
        if self.focus == Focus::Composer {
            return "enter send · shift+enter newline · tab complete · ctrl+p preview · esc close"
                .into();
        }
        if let Some(pane) = self.focused_pane() {
            if pane.kind == PaneKind::Viewer {
                return "j/k scroll · g/G top/end · q close".into();
            }
        }
        format!(
            "{} prompt · prefix+? help",
            herdstead_core::config::format_binding(&self.prefix)
        )
    }

    /// Sends a resize when the chrome changed, so panes fill their space.
    pub fn resize_action(&mut self, area: Rect, size: Size) -> Option<Action> {
        if self.last_pane_area == Some(area) {
            return None;
        }
        self.last_pane_area = Some(area);
        Some(Action::Resize { size, area })
    }

    /// The row the composer's caret sits on, for cursor placement.
    pub fn composer_layout(&self, width: usize) -> (Vec<RLine>, usize, usize) {
        let cwd = self.focused_cwd();
        herdstead_render::composer::layout(
            &self.composer.text,
            self.composer.cursor,
            width,
            &self.theme,
            &|path: &str| route_path(&cwd, path).exists(),
        )
    }
}

/// Resolves a reference the way a pane does: relative to its working folder.
pub fn route_path(base: &Path, path: &str) -> PathBuf {
    let path = text::clean_path(path);
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    let candidate = PathBuf::from(&path);
    if candidate.is_absolute() {
        candidate
    } else {
        base.join(candidate)
    }
}

fn byte_at_column(text: &str, column: usize) -> usize {
    let mut width = 0;
    for (index, ch) in text.char_indices() {
        if width >= column {
            return index;
        }
        width += text::char_width(ch);
        if width >= column {
            return index + ch.len_utf8();
        }
    }
    text.len()
}

/// Lists files and folders under `root`, relative, folders first, capped.
pub fn completion_entries(root: &Path, ignores: &[String], limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    walk_entries(root, root, 0, ignores, limit, &mut out);
    out.sort();
    out
}

fn walk_entries(
    root: &Path,
    dir: &Path,
    depth: usize,
    ignores: &[String],
    limit: usize,
    out: &mut Vec<String>,
) {
    if depth > 5 || out.len() >= limit {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    children.sort();
    for path in children {
        if out.len() >= limit {
            return;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == ".DS_Store" {
            continue;
        }
        let is_dir = path.is_dir();
        if is_dir && ignores.iter().any(|ignore| ignore == &name) {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            let mut label = relative.to_string_lossy().into_owned();
            if is_dir {
                label.push('/');
            }
            out.push(label);
        }
        if is_dir {
            walk_entries(root, &path, depth + 1, ignores, limit, out);
        }
    }
}

/// Opens a URL in the user's browser.
pub fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(target_os = "macos"))]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

/// Keys that carry input, as opposed to modifier-only or media keys.
fn sendable(code: KeyCode) -> bool {
    !matches!(
        code,
        KeyCode::Null
            | KeyCode::CapsLock
            | KeyCode::ScrollLock
            | KeyCode::NumLock
            | KeyCode::PrintScreen
            | KeyCode::Pause
            | KeyCode::Menu
            | KeyCode::KeypadBegin
            | KeyCode::Media(_)
            | KeyCode::Modifier(_)
    )
}

/// Translates a crossterm key into the wire form.
pub fn key_input(key: &KeyEvent) -> KeyInput {
    let mods = Mods {
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        sup: key.modifiers.contains(KeyModifiers::SUPER),
    };
    let code = match key.code {
        KeyCode::Char(ch) => Key::Char(ch),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::F(number) => Key::F(number),
        _ => Key::Esc,
    };
    KeyInput {
        key: code,
        mods,
        repeat: key.kind == KeyEventKind::Repeat,
    }
}

fn describe_key(input: &KeyInput) -> String {
    herdstead_core::config::format_binding(&Binding {
        prefix: false,
        mods: input.mods,
        key: input.key,
    })
}

/// Binding matching. For letters, `shift` is carried by the character's case,
/// so `shift+r` matches `R` and a plain `v` never matches `V`. For punctuation
/// the shift flag is inherent to typing the character and is ignored.
pub fn keys_match(binding: &Binding, input: &KeyInput) -> bool {
    let mut bmods = binding.mods;
    let mut imods = input.mods;
    match (binding.key, input.key) {
        (Key::Char(expected), Key::Char(actual)) if expected.is_ascii_alphabetic() => {
            let shifted = input.mods.shift || actual.is_ascii_uppercase();
            if !expected.eq_ignore_ascii_case(&actual) || shifted != binding.mods.shift {
                return false;
            }
            bmods.shift = false;
            imods.shift = false;
        }
        (Key::Char(expected), Key::Char(actual)) if !expected.is_alphanumeric() => {
            if expected != actual {
                return false;
            }
            bmods.shift = false;
            imods.shift = false;
        }
        (expected, actual) => {
            if expected != actual {
                return false;
            }
        }
    }
    bmods == imods
}

/// The digit a shifted number-row symbol stands for.
fn shifted_digit(ch: char) -> Option<u32> {
    ")!@#$%^&*("
        .chars()
        .position(|symbol| symbol == ch)
        .map(|index| index as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdstead_core::config::{parse_binding, Config};

    fn app() -> App {
        let themes = Theme::catalog(Path::new("/nonexistent"));
        let theme = themes[0].clone();
        App::new(
            "test".into(),
            Config::default(),
            theme,
            themes,
            Vec::new(),
            Path::new("/tmp"),
        )
    }

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    /// Attaches a snapshot with one focused shell pane, which is what the
    /// composer needs before it will send anything.
    fn with_shell(app: &mut App) -> &mut App {
        with_shell_in(app, "/tmp")
    }

    fn with_shell_in<'a>(app: &'a mut App, cwd: &str) -> &'a mut App {
        use herdstead_core::detect::PaneStatus;
        use herdstead_core::protocol::{PaneFrame, Snapshot, TabView, WorkspaceView};
        let area = herdstead_core::geom::Rect::new(0, 1, 80, 20);
        app.handle(AppEvent::Snapshot(Box::new(Snapshot {
            session: "test".into(),
            version: "0.1.0".into(),
            seq: 1,
            size: herdstead_core::model::Size { rows: 24, cols: 80 },
            area,
            workspaces: vec![WorkspaceView {
                id: herdstead_core::ids::WorkspaceId(1),
                name: "test".into(),
                dir: "/tmp".into(),
                status: PaneStatus::Idle,
                panes: 1,
                agent: None,
                tabs: vec![TabView {
                    id: herdstead_core::ids::TabId(1),
                    name: "shell".into(),
                    status: PaneStatus::Idle,
                    panes: 1,
                    agent: None,
                }],
                active_tab: 0,
            }],
            active_workspace: 0,
            focused: Some(herdstead_core::ids::PaneId(1)),
            zoomed: false,
            panes: vec![PaneFrame {
                id: herdstead_core::ids::PaneId(1),
                rect: area,
                title: "shell".into(),
                cwd: cwd.to_string(),
                command: "sh".into(),
                kind: PaneKind::Shell,
                viewer: None,
                agent: None,
                status: PaneStatus::Idle,
                bell: false,
                exit_code: None,
                scroll: 0,
                grid: None,
            }],
        })));
        app
    }

    fn with_viewer(app: &mut App) -> &mut App {
        with_shell(app);
        let mut snapshot = app.snapshot().unwrap().clone();
        let pane = &mut snapshot.panes[0];
        pane.kind = PaneKind::Viewer;
        pane.viewer = Some(herdstead_core::model::ViewerSpec {
            path: "/tmp/README.md".into(),
            kind: ViewerKind::Markdown,
        });
        app.handle(AppEvent::Snapshot(Box::new(snapshot)));
        app
    }

    #[test]
    fn bindings_match_with_shifted_characters() {
        let binding = parse_binding("prefix+shift+r").unwrap();
        let input = key_input(&press(KeyCode::Char('R'), KeyModifiers::SHIFT));
        assert!(keys_match(&binding, &input));
        let input = key_input(&press(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(!keys_match(&binding, &input));

        let binding = parse_binding("ctrl+p").unwrap();
        assert!(keys_match(
            &binding,
            &key_input(&press(KeyCode::Char('p'), KeyModifiers::CONTROL))
        ));
        assert!(!keys_match(
            &binding,
            &key_input(&press(KeyCode::Char('p'), KeyModifiers::NONE))
        ));
    }

    #[test]
    fn prefix_arms_and_dispatch_sends_actions() {
        let mut app = app();
        assert!(app
            .on_key(press(KeyCode::Char('b'), KeyModifiers::CONTROL))
            .is_empty());
        assert!(app.prefix_armed);
        let actions = app.on_key(press(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(
            actions,
            vec![Action::Split {
                dir: SplitDir::Vertical
            }]
        );
        assert!(!app.prefix_armed);
    }

    #[test]
    fn digits_select_tabs_and_workspaces() {
        let mut app = app();
        app.on_key(press(KeyCode::Char('b'), KeyModifiers::CONTROL));
        let actions = app.on_key(press(KeyCode::Char('3'), KeyModifiers::NONE));
        assert_eq!(actions, vec![Action::SelectTab { index: 2 }]);
        app.on_key(press(KeyCode::Char('b'), KeyModifiers::CONTROL));
        let actions = app.on_key(press(KeyCode::Char('#'), KeyModifiers::SHIFT));
        assert_eq!(actions, vec![Action::SelectWorkspace { index: 2 }]);
    }

    #[test]
    fn composer_edits_send_and_complete_paths() {
        let root = std::env::temp_dir().join(format!("herdstead-composer-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "").unwrap();
        let mut app = app();
        with_shell_in(&mut app, &root.display().to_string());
        app.completion_root = root.clone();
        app.completion_entries.clear();
        app.open_composer_with("look at @s");
        assert_eq!(app.focus, Focus::Composer);
        let completion = app.composer.completion.clone().expect("a completion");
        assert!(completion.items.iter().any(|item| item.starts_with("src")));
        // Tab accepts the completion instead of typing a tab.
        app.on_key(press(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.composer.text, "look at @src/");
        // A completion is no longer open, so Enter sends the line.
        let actions = app.on_key(press(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            actions,
            vec![Action::SendText {
                text: "look at @src/".into(),
                submit: true
            }]
        );
        assert!(app.composer.text.is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_composer_keeps_text_when_no_shell_has_focus() {
        let mut app = app();
        with_viewer(&mut app);
        app.open_composer_with("explain this");
        let actions = app.on_key(press(KeyCode::Enter, KeyModifiers::NONE));
        assert!(actions.is_empty());
        assert_eq!(
            app.composer.text, "explain this",
            "the prompt is kept so it is not lost"
        );
        let status = app
            .status
            .as_ref()
            .map(|(message, _)| message.clone())
            .unwrap_or_default();
        assert!(status.contains("not a running shell"), "{status}");

        // Slash commands still work from a viewer.
        app.composer.text.clear();
        app.composer.cursor = 0;
        app.open_composer_with("/close");
        let actions = app.on_key(press(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(actions, vec![Action::ClosePane]);
    }

    #[test]
    fn cancelling_the_theme_menu_restores_the_previous_theme() {
        let mut app = app();
        let before = app.theme.name.clone();
        app.on_key(press(KeyCode::Char('b'), KeyModifiers::CONTROL));
        app.on_key(press(KeyCode::Char('t'), KeyModifiers::NONE));
        app.on_key(press(KeyCode::Down, KeyModifiers::NONE));
        assert_ne!(app.theme.name, before, "the menu previews as it moves");
        app.on_key(press(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.theme.name, before, "esc cancels the preview");
        assert!(app.overlay.is_none());

        // Enter keeps the previewed theme.
        app.on_key(press(KeyCode::Char('b'), KeyModifiers::CONTROL));
        app.on_key(press(KeyCode::Char('t'), KeyModifiers::NONE));
        app.on_key(press(KeyCode::Down, KeyModifiers::NONE));
        let previewed = app.theme.name.clone();
        app.on_key(press(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.theme.name, previewed);
        assert!(app.overlay.is_none());
    }

    #[test]
    fn composer_commands_map_to_actions() {
        let mut app = app();
        app.open_composer_with("/split h");
        let actions = app.on_key(press(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            actions,
            vec![Action::Split {
                dir: SplitDir::Horizontal
            }]
        );

        app.open_composer_with("/theme nord");
        app.on_key(press(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.theme.name, "nord");

        app.open_composer_with("/open src/main.rs");
        let actions = app.on_key(press(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            actions,
            vec![Action::OpenViewer {
                path: "src/main.rs".into(),
                kind: None,
                split: None
            }]
        );
    }

    #[test]
    fn completion_lists_folders_first_and_is_capped() {
        let root = std::env::temp_dir().join(format!("herdstead-complete-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src/nested")).unwrap();
        std::fs::write(root.join("src/main.rs"), "").unwrap();
        std::fs::write(root.join("README.md"), "").unwrap();
        let entries = completion_entries(&root, &["target".into()], 100);
        assert!(entries.contains(&"src/".to_string()));
        assert!(entries.contains(&"src/main.rs".to_string()));
        assert!(entries.contains(&"README.md".to_string()));
        let capped = completion_entries(&root, &[], 1);
        assert_eq!(capped.len(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn viewer_keys_scroll_and_close() {
        let mut app = app();
        let mut frame = PaneFrame {
            id: herdstead_core::ids::PaneId(1),
            rect: Rect::new(0, 0, 20, 10),
            title: "README.md".into(),
            cwd: "/tmp".into(),
            command: String::new(),
            kind: PaneKind::Viewer,
            viewer: None,
            agent: None,
            status: herdstead_core::detect::PaneStatus::Idle,
            bell: false,
            exit_code: None,
            scroll: 0,
            grid: None,
        };
        let up = KeyInput::new(Key::Up);
        assert!(app.viewer_key(&frame, &up).is_none());
        assert_eq!(
            app.viewer_key(&frame, &KeyInput::new(Key::Char('q'))),
            Some(Action::ClosePane)
        );
        frame.kind = PaneKind::Shell;
        assert!(app.viewer_key(&frame, &up).is_none());
    }
}
