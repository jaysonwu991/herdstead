![herdstead](assets/logo-mark.svg)

# herdstead

**A desktop workspace for coding agents — a homestead for your herd.**

Herdstead is a Tauri desktop app for macOS and Linux. Keep agents running in
persistent terminal panes, tabs and workspaces, with rich Markdown, clickable
links, shell `@path` completion, highlighted files and folder trees.

[![CI](https://github.com/jaysonwu991/herdstead/actions/workflows/ci.yml/badge.svg)](https://github.com/jaysonwu991/herdstead/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-666666.svg)](LICENSE)

## Download and install

Download an installer from [Releases](https://github.com/jaysonwu991/herdstead/releases/latest):

| computer | installer | installation |
|---|---|---|
| macOS Apple Silicon | `Herdstead-macos-arm64.dmg` | open and drag Herdstead into Applications |
| macOS Intel | `Herdstead-macos-x64.dmg` | open and drag Herdstead into Applications |
| Linux x64 | `Herdstead-linux-x64.AppImage` or `.deb` | make the AppImage executable and run it, or install the Debian package |
| Linux arm64 | `Herdstead-linux-arm64.AppImage` or `.deb` | make the AppImage executable and run it, or install the Debian package |

The app includes its session runtime and shell helpers. No separate CLI install
is needed. Windows is not supported because the session runtime uses Unix sockets.
Linux requires glibc and the system webview; see [getting started](docs/getting-started.md).
Downloads include `SHA256SUMS` for verification.

Open **Appearance → Check for updates**, then **Install update and restart**.
Updates are signed and verified before installation. Linux in-app updates use
the AppImage; Debian installations update by installing a newer `.deb`.
macOS packages are not currently Apple Developer ID signed or notarised; see
[troubleshooting](docs/troubleshooting.md) for first launch.

## Use a workspace

Launch Herdstead and use **+** beside Workspaces to choose your project folder.
Start your coding agent in a terminal. Split panes with Cmd/Ctrl+D, open a tab
with Cmd/Ctrl+T, and find all commands with Cmd/Ctrl+Shift+P. Closing the window
leaves the session's shells running; reopening reconnects to the session.

The terminal loads your zsh startup files, Starship and Oh My Zsh configuration.
Type `@` for path completion. Use `cat README.md` for rendered Markdown,
`cat src/main.rs` for highlighted code, and `tree .` for a directory tree.
Existing aliases and piped `cat` commands retain their normal behaviour.

Choose a theme and terminal font in **Appearance**. The bundled JetBrains Mono Nerd Font includes prompt symbols. Click files in the explorer to open a viewer; use
Cmd/Ctrl-click for links in terminal text. **Rich output** toggles Markdown
interpretation without changing the terminal's bytes or row coordinates.

[Documentation](docs/README.md) · [Desktop controls](docs/desktop.md) ·
[Configuration](docs/configuration.md) · [Architecture](docs/architecture.md)

## Build from source

Install a current stable Rust toolchain, Node.js 26, pnpm 12 and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.
Linux also needs zsh for the rich shell integration.

```sh
git clone https://github.com/jaysonwu991/herdstead.git
cd herdstead/desktop
pnpm install --frozen-lockfile
pnpm run dev
# Local app, without signed updater artifacts:
pnpm run build --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

Release CI builds native installers for all four supported platforms and signs
update artifacts. See [Contributing](CONTRIBUTING.md) for verification and release setup.
The standalone TUI and curl installer have been retired; the Rust executable is
a bundled runtime and shell helper.

## Development checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 -m unittest discover -s scripts -p 'test_*.py'
```

Desktop host checks and browser integration tests are described in
[Desktop](docs/desktop.md#verification).

## Limitations

Sessions and scrollback are local. Saved layouts restore fresh processes after
a daemon restart; they do not restore process memory or past terminal output.
One session has one layout, so multiple windows with different sizes resize it
for one another. Mouse reporting is reserved for the desktop controls, selection
and scrollback. File viewers require UTF-8 and refuse files over 4 MiB.

## Name and licence

**Herd** + **stead**: a homestead for your agents. The mark is a house whose
window is a pane grid, with one pane lit by the hearth. See [Brand](docs/brand.md).
Inspired by [herdr](https://github.com/herdrdev/herdr). Apache-2.0 — see [LICENSE](LICENSE).
