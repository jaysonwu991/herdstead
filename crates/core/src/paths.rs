//! Where herdstead keeps configuration, themes, persistence and sockets.

use std::path::{Path, PathBuf};

/// Config, state and runtime roots for one installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config: PathBuf,
    pub state: PathBuf,
    pub runtime: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self::discover()
    }
}

impl Paths {
    /// Honours `HERDSTEAD_*` first, then the XDG variables and their usual
    /// fallbacks, on every platform — an explicit, predictable layout:
    ///
    /// - config: `$XDG_CONFIG_HOME/herdstead` (default `~/.config/herdstead`)
    /// - state:  `$XDG_STATE_HOME/herdstead` (default `~/.local/state/herdstead`)
    /// - runtime: `$XDG_RUNTIME_DIR/herdstead` (default `/tmp/herdstead-<uid>`)
    pub fn discover() -> Self {
        Self::from_env(|key| std::env::var_os(key))
    }

    /// The same resolution with an injectable environment, for tests.
    pub fn from_env(get: impl Fn(&str) -> Option<std::ffi::OsString>) -> Self {
        let home = get("HOME").map(PathBuf::from);
        let join = |base: Option<PathBuf>, fallback: Option<PathBuf>| {
            base.or(fallback).map(|dir| dir.join("herdstead"))
        };
        let config = get("HERDSTEAD_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                join(
                    get("XDG_CONFIG_HOME").map(PathBuf::from),
                    home.as_ref().map(|home| home.join(".config")),
                )
            })
            .unwrap_or_else(|| PathBuf::from(".herdstead"));
        let state = get("HERDSTEAD_STATE_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                join(
                    get("XDG_STATE_HOME").map(PathBuf::from),
                    home.as_ref().map(|home| home.join(".local/state")),
                )
            })
            .unwrap_or_else(|| config.join("state"));
        let runtime = get("HERDSTEAD_RUNTIME_DIR")
            .map(PathBuf::from)
            .or_else(|| join(get("XDG_RUNTIME_DIR").map(PathBuf::from), None))
            .unwrap_or_else(|| {
                let uid = current_uid();
                std::env::temp_dir().join(format!("herdstead-{uid}"))
            });
        Self {
            config,
            state,
            runtime,
        }
    }

    pub fn with_roots(
        config: impl Into<PathBuf>,
        state: impl Into<PathBuf>,
        runtime: impl Into<PathBuf>,
    ) -> Self {
        Self {
            config: config.into(),
            state: state.into(),
            runtime: runtime.into(),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    pub fn themes_dir(&self) -> PathBuf {
        self.config.join("themes")
    }

    pub fn socket(&self, session: &str) -> PathBuf {
        self.runtime
            .join(format!("{}.sock", sanitize_session(session)))
    }

    pub fn lock_file(&self, session: &str) -> PathBuf {
        self.runtime
            .join(format!("{}.lock", sanitize_session(session)))
    }

    pub fn session_file(&self, session: &str) -> PathBuf {
        self.state
            .join("sessions")
            .join(format!("{}.json", sanitize_session(session)))
    }

    pub fn log_file(&self, session: &str) -> PathBuf {
        self.state
            .join("logs")
            .join(format!("{}.log", sanitize_session(session)))
    }

    /// Creates the runtime directory with owner-only permissions.
    pub fn ensure_runtime(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.runtime)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&self.runtime)?.permissions();
            permissions.set_mode(0o700);
            std::fs::set_permissions(&self.runtime, permissions)?;
        }
        Ok(())
    }

    pub fn ensure_state(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.state.join("sessions"))?;
        std::fs::create_dir_all(self.state.join("logs"))?;
        Ok(())
    }

    pub fn ensure_config(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.themes_dir())
    }

    /// Writes a documented starter config when none exists.
    pub fn ensure_default_config(&self) -> std::io::Result<bool> {
        self.ensure_config()?;
        let path = self.config_file();
        if path.exists() {
            return Ok(false);
        }
        std::fs::write(&path, DEFAULT_CONFIG)?;
        Ok(true)
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        self.ensure_runtime()?;
        self.ensure_state()?;
        self.ensure_config()
    }

    /// Session names present on disk (sockets that still exist).
    pub fn sessions(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.runtime) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("sock") {
                    continue;
                }
                if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
                    names.push(name.to_string());
                }
            }
        }
        names.sort();
        names
    }

    pub fn remove_session_files(&self, session: &str) {
        std::fs::remove_file(self.socket(session)).ok();
        std::fs::remove_file(self.session_file(session)).ok();
    }
}

#[cfg(unix)]
fn current_uid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

/// Restricts a session name to characters that are safe in a file name.
pub fn sanitize_session(name: &str) -> String {
    let mut out: String = name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .take(48)
        .collect();
    while out.starts_with('-') || out.starts_with('.') {
        out.remove(0);
    }
    if out.is_empty() {
        out.push_str("main");
    }
    out
}

/// The session name a directory maps to when none was given: the folder name.
pub fn default_session_name(cwd: &Path) -> String {
    cwd.file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_session)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "main".to_string())
}

pub const DEFAULT_CONFIG: &str = r##"# herdstead configuration
# Reload with `prefix+r` (or restart the session) after editing.

# Theme: midnight, daylight, nord, gruvbox, mono, solarized,
# or a name from ~/.config/herdstead/themes/<name>.json
theme = "midnight"

# Shell started for a new pane. Defaults to $SHELL.
# shell = "/bin/zsh"

# Lines of scrollback kept per terminal pane.
scrollback = 5000

# Wrap and interpret URLs and paths in terminal output.
links = true
# Modifier for opening a link or path under the pointer: "super", "ctrl", "alt".
link-modifier = "super"

# Interface
mouse = true
sidebar = true
sidebar-width = 28
# Open the composer with the Markdown preview already on (ctrl+p toggles it).
composer-markdown-preview = false
status-bar = true

# Detached sessions are restored (layout, folders) when you reattach.
restore = true

# Folders a tree viewer collapses by default.
tree-ignores = ["target", "node_modules", ".git", "dist", "build", ".venv"]
tree-max-entries = 4000

# Prefix for multiplexer shortcuts.
keybind-prefix = "ctrl+b"

# Override any keybinding, e.g. `split-vertical = "prefix+|"`.
# Run `herdstead keys` for the full list.
[keybinds]

[theme-colors]
# Override individual slots of the active theme, e.g.:
# accent = "#ff8800"
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_names_are_safe() {
        assert_eq!(sanitize_session("my project/../x"), "my-project-..-x");
        assert_eq!(sanitize_session("  "), "main");
        assert_eq!(sanitize_session("a b"), "a-b");
        assert_eq!(sanitize_session("../etc"), "etc");
    }

    #[test]
    fn paths_map_sessions_to_files() {
        let paths = Paths::with_roots("/c", "/s", "/r");
        assert_eq!(paths.socket("main"), PathBuf::from("/r/main.sock"));
        assert_eq!(
            paths.session_file("a b"),
            PathBuf::from("/s/sessions/a-b.json")
        );
        assert_eq!(paths.themes_dir(), PathBuf::from("/c/themes"));
    }

    #[test]
    fn default_roots_follow_the_xdg_variables() {
        let paths = Paths::from_env(|key| match key {
            "HOME" => Some("/home/me".into()),
            "XDG_CONFIG_HOME" => Some("/cfg".into()),
            "XDG_RUNTIME_DIR" => Some("/run/user/501".into()),
            _ => None,
        });
        assert_eq!(paths.config, PathBuf::from("/cfg/herdstead"));
        assert_eq!(
            paths.state,
            PathBuf::from("/home/me/.local/state/herdstead")
        );
        assert_eq!(paths.runtime, PathBuf::from("/run/user/501/herdstead"));
    }

    #[test]
    fn without_xdg_the_home_fallbacks_apply() {
        let paths = Paths::from_env(|key| match key {
            "HOME" => Some("/home/me".into()),
            _ => None,
        });
        assert_eq!(paths.config, PathBuf::from("/home/me/.config/herdstead"));
        assert_eq!(
            paths.state,
            PathBuf::from("/home/me/.local/state/herdstead")
        );
        assert!(paths
            .runtime
            .ends_with(format!("herdstead-{}", current_uid())));
    }

    #[test]
    fn herdstead_variables_win_over_xdg() {
        let paths = Paths::from_env(|key| match key {
            "HERDSTEAD_CONFIG_DIR" => Some("/c".into()),
            "HERDSTEAD_STATE_DIR" => Some("/s".into()),
            "HERDSTEAD_RUNTIME_DIR" => Some("/r".into()),
            "HOME" => Some("/home/me".into()),
            "XDG_CONFIG_HOME" => Some("/cfg".into()),
            _ => None,
        });
        assert_eq!(paths.config, PathBuf::from("/c"));
        assert_eq!(paths.state, PathBuf::from("/s"));
        assert_eq!(paths.runtime, PathBuf::from("/r"));
    }

    #[test]
    fn an_explicit_runtime_dir_is_used_as_given() {
        // The env override is an exact directory, not a parent to join onto.
        // (Regression: it used to become `<dir>/herdstead`.)
        let paths = Paths::with_roots("/c", "/s", "/r");
        assert_eq!(paths.socket("main"), PathBuf::from("/r/main.sock"));
    }

    #[test]
    fn sanitised_socket_paths_stay_short() {
        let paths = Paths::with_roots("/c", "/s", "/tmp/herdstead-501");
        let long = "x".repeat(200);
        assert!(paths.socket(&long).as_os_str().len() < 100);
    }

    #[test]
    fn default_session_name_uses_the_folder() {
        assert_eq!(
            default_session_name(Path::new("/Users/me/herdstead")),
            "herdstead"
        );
        assert_eq!(default_session_name(Path::new("/")), "main");
    }
}
