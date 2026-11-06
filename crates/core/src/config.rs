//! Configuration: `config.toml` layers, keybindings and their defaults.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths::{Paths, DEFAULT_CONFIG};
use crate::protocol::{Key, Mods};

/// Which modifier opens a link or path under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LinkModifier {
    #[default]
    Super,
    Ctrl,
    Alt,
}

/// One parsed key combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    /// Reached through the multiplexer prefix (default `ctrl+b`).
    pub prefix: bool,
    pub mods: Mods,
    pub key: Key,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    pub theme: String,
    pub shell: Option<String>,
    pub scrollback: usize,
    pub links: bool,
    pub link_modifier: LinkModifier,
    pub mouse: bool,
    pub sidebar: bool,
    pub sidebar_width: u16,
    pub composer: bool,
    pub composer_markdown_preview: bool,
    pub status_bar: bool,
    pub restore: bool,
    pub tree_ignores: Vec<String>,
    pub tree_max_entries: usize,
    pub keybind_prefix: String,
    /// Action name → key spec, layered over the defaults.
    pub keybinds: BTreeMap<String, String>,
    /// Theme slot → colour, applied over the built-in palette.
    pub theme_colors: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "midnight".to_string(),
            shell: None,
            scrollback: 5_000,
            links: true,
            link_modifier: LinkModifier::Super,
            mouse: true,
            sidebar: true,
            sidebar_width: 28,
            composer: true,
            composer_markdown_preview: true,
            status_bar: true,
            restore: true,
            tree_ignores: vec![
                "target".into(),
                "node_modules".into(),
                ".git".into(),
                "dist".into(),
                "build".into(),
                ".venv".into(),
            ],
            tree_max_entries: 4_000,
            keybind_prefix: "ctrl+b".into(),
            keybinds: BTreeMap::new(),
            theme_colors: BTreeMap::new(),
        }
    }
}

impl Config {
    /// The shell a new pane runs: the configured one, `$SHELL`, or `/bin/sh`.
    pub fn shell_command(&self) -> String {
        self.shell
            .clone()
            .or_else(|| std::env::var("SHELL").ok())
            .filter(|shell| !shell.trim().is_empty())
            .unwrap_or_else(|| "/bin/sh".to_string())
    }

    /// Clamps out-of-range values so a bad config cannot break the layout.
    pub fn sanitized(mut self) -> Self {
        if self.theme.trim().is_empty() {
            self.theme = "midnight".into();
        }
        self.scrollback = self.scrollback.clamp(100, 200_000);
        self.sidebar_width = self.sidebar_width.clamp(12, 80);
        self.tree_max_entries = self.tree_max_entries.clamp(50, 100_000);
        self.tree_ignores.retain(|entry| !entry.trim().is_empty());
        self
    }

    /// Defaults, then the global config file, then a project config file.
    pub fn load(project: &Path, paths: &Paths) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut table = toml::Table::new();
        for path in [
            Some(paths.config_file()),
            Some(project.join(".herdr-plus/config.toml")),
            Some(project.join("herdr-plus.toml")),
        ]
        .into_iter()
        .flatten()
        {
            match std::fs::read_to_string(&path) {
                Ok(text) => match text.parse::<toml::Table>() {
                    Ok(layer) => deep_merge(&mut table, layer),
                    Err(err) => warnings.push(format!("{}: {err}", path.display())),
                },
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => warnings.push(format!("{}: {err}", path.display())),
            }
        }
        let config = match toml::Value::Table(table).try_into::<Config>() {
            Ok(config) => config.sanitized(),
            Err(err) => {
                warnings.push(format!("config: {err}"));
                Self::default()
            }
        };
        for (action, spec) in &config.keybinds {
            if !is_known_action(action) {
                warnings.push(format!("keybinds: unknown action `{action}`"));
            }
            if let Err(err) = parse_binding(spec) {
                warnings.push(format!("keybinds.{action}: {err}"));
            }
        }
        if let Err(err) = parse_binding(&config.keybind_prefix) {
            warnings.push(format!("keybind-prefix: {err}"));
        }
        for slot in config.theme_colors.keys() {
            if !crate::theme::Theme::SLOTS.contains(&slot.as_str()) {
                warnings.push(format!("theme-colors: unknown slot `{slot}`"));
            }
        }
        (config, warnings)
    }

    /// The complete keymap: defaults overlaid with the configured bindings.
    pub fn keymap(&self) -> BTreeMap<String, Binding> {
        let mut map = BTreeMap::new();
        for action in ACTIONS {
            if let Some(spec) = action.default {
                if let Ok(binding) = parse_binding(spec) {
                    map.insert(action.name.to_string(), binding);
                }
            }
        }
        if let Ok(prefix) = parse_binding(&self.keybind_prefix) {
            // The prefix key itself is matched by the client; keep it here so
            // `herdr-plus keys` can print it.
            map.insert("prefix".to_string(), prefix);
        }
        for (action, spec) in &self.keybinds {
            if let Ok(binding) = parse_binding(spec) {
                map.insert(action.clone(), binding);
            }
        }
        map
    }

    /// The prefix binding, defaulting to `ctrl+b`.
    pub fn prefix_binding(&self) -> Binding {
        parse_binding(&self.keybind_prefix)
            .unwrap_or_else(|_| parse_binding("ctrl+b").expect("default prefix parses"))
    }
}

/// Every action a keybinding can name, with its default and one-line help.
pub struct ActionSpec {
    pub name: &'static str,
    pub default: Option<&'static str>,
    pub help: &'static str,
}

pub const ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "split-vertical",
        default: Some("prefix+v"),
        help: "split the pane side by side",
    },
    ActionSpec {
        name: "split-horizontal",
        default: Some("prefix+s"),
        help: "split the pane top and bottom",
    },
    ActionSpec {
        name: "close-pane",
        default: Some("prefix+x"),
        help: "close the focused pane",
    },
    ActionSpec {
        name: "focus-left",
        default: Some("prefix+h"),
        help: "focus the pane to the left",
    },
    ActionSpec {
        name: "focus-down",
        default: Some("prefix+j"),
        help: "focus the pane below",
    },
    ActionSpec {
        name: "focus-up",
        default: Some("prefix+k"),
        help: "focus the pane above",
    },
    ActionSpec {
        name: "focus-right",
        default: Some("prefix+l"),
        help: "focus the pane to the right",
    },
    ActionSpec {
        name: "focus-next",
        default: Some("prefix+o"),
        help: "focus the next pane",
    },
    ActionSpec {
        name: "focus-prev",
        default: Some("prefix+i"),
        help: "focus the previous pane",
    },
    ActionSpec {
        name: "zoom",
        default: Some("prefix+z"),
        help: "zoom the focused pane",
    },
    ActionSpec {
        name: "new-tab",
        default: Some("prefix+c"),
        help: "new tab",
    },
    ActionSpec {
        name: "next-tab",
        default: Some("prefix+n"),
        help: "next tab",
    },
    ActionSpec {
        name: "prev-tab",
        default: Some("prefix+p"),
        help: "previous tab",
    },
    ActionSpec {
        name: "close-tab",
        default: Some("prefix+w"),
        help: "close the tab",
    },
    ActionSpec {
        name: "rename-tab",
        default: Some("prefix+shift+r"),
        help: "rename the tab",
    },
    ActionSpec {
        name: "new-workspace",
        default: Some("prefix+shift+n"),
        help: "new workspace",
    },
    ActionSpec {
        name: "next-workspace",
        default: Some("prefix+]"),
        help: "next workspace",
    },
    ActionSpec {
        name: "prev-workspace",
        default: Some("prefix+["),
        help: "previous workspace",
    },
    ActionSpec {
        name: "close-workspace",
        default: Some("prefix+shift+x"),
        help: "close the workspace",
    },
    ActionSpec {
        name: "rename-workspace",
        default: Some("prefix+shift+w"),
        help: "rename the workspace",
    },
    ActionSpec {
        name: "composer",
        default: Some("prefix+:"),
        help: "open the composer",
    },
    ActionSpec {
        name: "theme-menu",
        default: Some("prefix+t"),
        help: "pick a theme",
    },
    ActionSpec {
        name: "open-file",
        default: Some("prefix+e"),
        help: "open a file in a viewer pane",
    },
    ActionSpec {
        name: "open-tree",
        default: Some("prefix+shift+e"),
        help: "open a folder tree",
    },
    ActionSpec {
        name: "markdown-preview",
        default: Some("prefix+m"),
        help: "preview the composer as Markdown",
    },
    ActionSpec {
        name: "sidebar",
        default: Some("prefix+|"),
        help: "show or hide the sidebar",
    },
    ActionSpec {
        name: "help",
        default: Some("prefix+?"),
        help: "show keybindings",
    },
    ActionSpec {
        name: "reload",
        default: Some("prefix+r"),
        help: "reload config and theme",
    },
    ActionSpec {
        name: "detach",
        default: Some("prefix+q"),
        help: "detach, leaving panes running",
    },
    ActionSpec {
        name: "scroll-page-up",
        default: Some("prefix+pageup"),
        help: "scroll the pane up a page",
    },
    ActionSpec {
        name: "scroll-page-down",
        default: Some("prefix+pagedown"),
        help: "scroll the pane down a page",
    },
    ActionSpec {
        name: "scroll-top",
        default: Some("prefix+home"),
        help: "scroll to the top of scrollback",
    },
    ActionSpec {
        name: "scroll-bottom",
        default: Some("prefix+end"),
        help: "scroll to the live view",
    },
];

pub fn is_known_action(name: &str) -> bool {
    ACTIONS.iter().any(|action| action.name == name)
}

pub fn action_help(name: &str) -> Option<&'static str> {
    ACTIONS
        .iter()
        .find(|action| action.name == name)
        .map(|action| action.help)
}

/// Parses `ctrl+shift+p`, `prefix+v`, `alt+left`, `f5`, `prefix++`.
pub fn parse_binding(spec: &str) -> Result<Binding, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("empty binding".to_string());
    }
    let mut parts: Vec<String> = spec
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .collect();
    let mut plus_key = false;
    let last = parts.pop().unwrap_or_default();
    let key_part = if last.is_empty() {
        // A trailing `+` means the `+` key itself was escaped by a second `+`.
        plus_key = true;
        while parts.last().is_some_and(|part| part.is_empty()) {
            parts.pop();
        }
        "+".to_string()
    } else {
        last
    };
    let mut binding = Binding {
        prefix: false,
        mods: Mods::none(),
        key: Key::Char(' '),
    };
    for part in parts {
        match part.as_str() {
            "" => return Err(format!("`{spec}`: stray `+`")),
            "prefix" => binding.prefix = true,
            "ctrl" | "control" => binding.mods.ctrl = true,
            "alt" | "option" | "opt" => binding.mods.alt = true,
            "shift" => binding.mods.shift = true,
            "super" | "cmd" | "command" | "meta" => binding.mods.sup = true,
            other => return Err(format!("`{spec}`: unknown modifier `{other}`")),
        }
    }
    binding.key = if plus_key {
        Key::Char('+')
    } else {
        parse_key(&key_part).ok_or_else(|| format!("`{spec}`: unknown key `{key_part}`"))?
    };
    Ok(binding)
}

fn parse_key(name: &str) -> Option<Key> {
    if name.chars().count() == 1 {
        let ch = name.chars().next()?;
        return Some(Key::Char(ch));
    }
    Some(match name {
        "space" => Key::Char(' '),
        "tab" => Key::Tab,
        "backtab" => Key::BackTab,
        "enter" | "return" => Key::Enter,
        "esc" | "escape" => Key::Esc,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "insert" | "ins" => Key::Insert,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" | "pagedn" => Key::PageDown,
        _ => {
            let digits = name.strip_prefix('f')?;
            let number: u8 = digits.parse().ok()?;
            if (1..=24).contains(&number) {
                Key::F(number)
            } else {
                return None;
            }
        }
    })
}

/// Renders a binding back into a spec, for help and `herdr-plus keys`.
pub fn format_binding(binding: &Binding) -> String {
    let mut parts = Vec::new();
    if binding.prefix {
        parts.push("prefix".to_string());
    }
    if binding.mods.ctrl {
        parts.push("ctrl".into());
    }
    if binding.mods.alt {
        parts.push("alt".into());
    }
    if binding.mods.shift {
        parts.push("shift".into());
    }
    if binding.mods.sup {
        parts.push("super".into());
    }
    parts.push(match binding.key {
        Key::Char(' ') => "space".to_string(),
        Key::Char(ch) => ch.to_string(),
        Key::Tab => "tab".into(),
        Key::BackTab => "backtab".into(),
        Key::Enter => "enter".into(),
        Key::Esc => "esc".into(),
        Key::Backspace => "backspace".into(),
        Key::Delete => "delete".into(),
        Key::Insert => "insert".into(),
        Key::Left => "left".into(),
        Key::Right => "right".into(),
        Key::Up => "up".into(),
        Key::Down => "down".into(),
        Key::Home => "home".into(),
        Key::End => "end".into(),
        Key::PageUp => "pageup".into(),
        Key::PageDown => "pagedown".into(),
        Key::F(number) => format!("f{number}"),
    });
    parts.join("+")
}

fn deep_merge(base: &mut toml::Table, overlay: toml::Table) {
    for (key, value) in overlay {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(base_table)), toml::Value::Table(overlay_table)) => {
                deep_merge(base_table, overlay_table);
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// Parses the starter config, which must always be valid.
pub fn default_config_parses() -> bool {
    match DEFAULT_CONFIG.parse::<toml::Table>() {
        Ok(table) => toml::Value::Table(table).try_into::<Config>().is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_and_modified_bindings() {
        let binding = parse_binding("prefix+v").unwrap();
        assert!(binding.prefix);
        assert_eq!(binding.key, Key::Char('v'));
        assert!(binding.mods.is_empty());

        let binding = parse_binding("ctrl+shift+p").unwrap();
        assert!(!binding.prefix);
        assert!(binding.mods.ctrl && binding.mods.shift);
        assert_eq!(binding.key, Key::Char('p'));

        let binding = parse_binding("alt+pageup").unwrap();
        assert_eq!(binding.key, Key::PageUp);
        assert!(binding.mods.alt);

        assert_eq!(parse_binding("f12").unwrap().key, Key::F(12));
        assert_eq!(parse_binding("prefix++").unwrap().key, Key::Char('+'));
        assert_eq!(parse_binding("space").unwrap().key, Key::Char(' '));
        assert!(parse_binding("hyper+x").is_err());
        assert!(parse_binding("ctrl+nosuch").is_err());
        assert!(parse_binding("").is_err());
    }

    #[test]
    fn bindings_round_trip_through_formatting() {
        for spec in [
            "prefix+v",
            "ctrl+shift+p",
            "alt+pageup",
            "prefix++",
            "f5",
            "shift+tab",
        ] {
            let binding = parse_binding(spec).unwrap();
            let formatted = format_binding(&binding);
            assert_eq!(
                parse_binding(&formatted).unwrap(),
                binding,
                "{spec} → {formatted}"
            );
        }
    }

    #[test]
    fn defaults_cover_every_action_and_parse() {
        let config = Config::default();
        let keymap = config.keymap();
        for action in ACTIONS {
            let name = action.name;
            let default = action
                .default
                .unwrap_or_else(|| panic!("{name} has no default"));
            let binding = parse_binding(default).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert_eq!(
                keymap.get(name),
                Some(&binding),
                "{name} missing from the keymap"
            );
            assert!(binding.prefix, "{name} should use the prefix");
        }
        assert_eq!(config.prefix_binding(), parse_binding("ctrl+b").unwrap());
    }

    #[test]
    fn config_layers_project_over_global() {
        let root = std::env::temp_dir().join(format!("herdr-plus-cfg-{}", std::process::id()));
        let config_dir = root.join("config");
        let global = config_dir.join("herdr-plus");
        let project = root.join("project");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::create_dir_all(project.join(".herdr-plus")).unwrap();
        std::fs::write(
            global.join("config.toml"),
            "theme = \"nord\"\nscrollback = 1000\n[keybinds]\nsplit-vertical = \"prefix+p\"\n",
        )
        .unwrap();
        std::fs::write(
            project.join(".herdr-plus/config.toml"),
            "theme = \"gruvbox\"\n[theme-colors]\naccent = \"#ff0000\"\n",
        )
        .unwrap();
        let paths = Paths::with_roots(&global, root.join("state"), root.join("run"));
        let (config, warnings) = Config::load(&project, &paths);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config.theme, "gruvbox");
        assert_eq!(config.scrollback, 1000);
        assert_eq!(
            config.theme_colors.get("accent").map(String::as_str),
            Some("#ff0000")
        );
        assert_eq!(
            config.keymap().get("split-vertical").map(|b| b.key),
            Some(Key::Char('p'))
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn invalid_config_warns_instead_of_failing() {
        let root = std::env::temp_dir().join(format!("herdr-plus-bad-{}", std::process::id()));
        let project = root.join("project");
        std::fs::create_dir_all(project.join(".herdr-plus")).unwrap();
        std::fs::write(
            project.join(".herdr-plus/config.toml"),
            "scrollback = 10\n[keybinds]\nno-such-action = \"prefix+9\"\n[theme-colors]\nnope = \"#fff\"\n",
        )
        .unwrap();
        let paths = Paths::with_roots(root.join("config"), root.join("state"), root.join("run"));
        let (config, warnings) = Config::load(&project, &paths);
        assert_eq!(config.scrollback, 100);
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("no-such-action")));
        assert!(warnings.iter().any(|warning| warning.contains("nope")));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn starter_config_is_valid() {
        assert!(default_config_parses());
    }
}
