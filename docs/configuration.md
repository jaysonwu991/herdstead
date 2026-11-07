# Configuration

`herdstead init` writes a commented starter file; `herdstead config` prints the
resolved configuration, the paths in use, and any warnings.

## Files and layering

| file | scope |
|---|---|
| `~/.config/herdstead/config.toml` | your defaults |
| `<project>/.herdstead/config.toml` | a project's overrides |
| `<project>/herdstead.toml` | the same, for people who prefer a root file |

They are merged in that order — later files win, table by table, so a project can
set one key without repeating the rest. Missing files are fine. Unknown keys are
ignored; `herdstead config` shows what was understood.

A project config is read whenever herdstead starts in that folder. There is no
trust prompt: a repository you run `herdstead` in can change its own theme,
keybindings and shell.

## Settings

```toml
theme = "midnight"
# shell = "/bin/zsh"         # optional; otherwise use $SHELL, then /bin/sh
scrollback = 5000

links = true
link-modifier = "super"

mouse = true
sidebar = true
sidebar-width = 28
composer-markdown-preview = false
status-bar = true

restore = true

tree-ignores = ["target", "node_modules", ".git", "dist", "build", ".venv"]
tree-max-entries = 4000

keybind-prefix = "ctrl+b"

[keybinds]
[theme-colors]
```

| key | type | default | meaning |
|---|---|---|---|
| `theme` | string | `"midnight"` | palette name or theme file |
| `shell` | string or omitted | `$SHELL`, then `/bin/sh` | shell for new panes |
| `scrollback` | integer | `5000` | rows of history kept per pane (clamped to 100–200000) |
| `links` | bool | `true` | detect URLs and paths in pane output |
| `link-modifier` | `"super"`, `"ctrl"`, `"alt"` | `"super"` | held to open a link or path |
| `mouse` | bool | `true` | use the mouse (capture is disabled entirely when false) |
| `sidebar` | bool | `true` | show the sidebar |
| `sidebar-width` | integer | `28` | sidebar columns (clamped to 12–80, and to the screen) |
| `composer-markdown-preview` | bool | `false` | open the composer with the preview on |
| `status-bar` | bool | `true` | show the bottom status row |
| `restore` | bool | `true` | restore the saved layout when a session starts again |
| `tree-ignores` | array of strings | `target`, `node_modules`, `.git`, `dist`, `build`, `.venv` | folders tree viewers skip |
| `tree-max-entries` | integer | `4000` | cap on tree entries (50–100000) |
| `keybind-prefix` | string | `"ctrl+b"` | the multiplexer prefix |
| `keybinds` | table | empty | action → key spec; see [keys](keys.md) |
| `theme-colors` | table | empty | slot → colour; see [themes](themes.md) |

Out-of-range numbers are clamped rather than rejected, and an empty `theme`
falls back to `midnight`.

`composer-markdown-preview` only decides the initial state: `ctrl+p` toggles the
preview in the current client either way.

## Applying changes

In the TUI, `prefix+r` reloads the client config and theme from the focused
pane's folder. Shell, scrollback and restore settings are read by the daemon
when it starts; restart the session to apply those changes. Existing processes
are stopped by a restart. Mouse capture is set when the TUI attaches, so changing
`mouse` also requires a new attach. The desktop keeps its appearance preferences
locally; see [Desktop](desktop.md).

## Environment

| variable | meaning |
|---|---|
| `HERDSTEAD_SESSION` | session name when `--session` is not given |
| `HERDSTEAD_THEME` | theme name when `--theme` is not given |
| `HERDSTEAD_CONFIG_DIR` | replaces `~/.config/herdstead` (themes live in `themes/`) |
| `HERDSTEAD_STATE_DIR` | replaces `~/.local/state/herdstead` (layouts and logs) |
| `HERDSTEAD_RUNTIME_DIR` | replaces the socket folder |

The directory variables are used exactly as given — they are not appended to a
`herdstead` subfolder. Without them, the XDG variables are honoured
(`XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR`), falling back to
`~/.config/herdstead`, `~/.local/state/herdstead` and a `herdstead-<uid>`
folder in the system temporary directory (`$TMPDIR` on macOS, usually `/tmp`
on Linux).

Panes also receive `TERM`, `COLORTERM`, `HERDSTEAD=1`, `HERDSTEAD_PANE` and
`HERDSTEAD_SESSION_PID` — see [terminal](terminal.md).

## Warnings

herdstead reports configuration problems instead of failing: an invalid TOML
file, an unknown keybinding action, an unparsable key spec, or an unknown theme
slot. Warnings are printed to stderr before the interface starts (and the
interface shows how many config warnings there were), and a daemon writes its
configuration warnings to its session log. A TOML syntax error skips that file;
a type error in the merged configuration falls back to all defaults.

`herdstead config` is the quickest way to see what a run actually resolved, and
`herdstead keys` what the keymap became.

## Examples

A light setup with a project-local palette and fewer scrollback rows:

```toml
# ~/.config/herdstead/config.toml
theme = "daylight"
link-modifier = "ctrl"
scrollback = 2000

[theme-colors]
selection = "#dbe7ff"
```

```toml
# <project>/.herdstead/config.toml
theme = "nord"

[keybinds]
split-vertical = "prefix+|"
close-pane = "prefix+shift+q"
```

See [keys](keys.md) for the action names and the key syntax, and
[themes](themes.md) for the slots.
