# Herdstead documentation

Herdstead is a Tauri desktop workspace for coding agents on macOS and Linux.
Start with [getting started](getting-started.md) to download and install it.

| page | contents |
|---|---|
| [Getting started](getting-started.md) | installers, first workspace, signed updates |
| [Desktop](desktop.md) | native builds, rich terminal, zsh, controls and verification |
| [Sessions](sessions.md) | persistent processes, saved layouts and session maintenance |
| [Panes and terminals](terminal.md) | splits, scrollback, links and terminal behaviour |
| [Shell input](composer.md) | agent input, Markdown and shell `@path` completion |
| [Viewers](viewers.md) | Markdown, code and folder trees |
| [Themes](themes.md) | palettes, custom themes and colour slots |
| [Configuration](configuration.md) | runtime settings and environment |
| [Keys](keys.md) | desktop shortcuts |
| [Architecture](architecture.md) | native host, bridge, Unix daemon and rendering |
| [Troubleshooting](troubleshooting.md) | installation, updates, runtime logs and sessions |
| [Brand](brand.md) | name, mark and assets |

Desktop is the only front-end. The bundled Rust executable provides the daemon,
JSON bridge and rich shell commands; it has no interactive TUI.
