# Desktop

Herdstead Desktop is a Tauri 2 application for macOS and Linux. It uses the
system webview and the existing Rust session daemon. Closing the window leaves
shells and agents running; reopening the same session reattaches to them.

## Run from source

Install a current stable Rust toolchain, Node.js 26, pnpm 12 and the
[Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/).
Install the pinned pnpm version with `npm install -g pnpm@12.18.0`.
Local installs use your configured registry (including Skyscanner Artifactory);
CI and release builds use the public npm registry.
The bundled runtime builds separately; the user interface requires the webview.

From the repository root:

```sh
cd desktop
pnpm install --frozen-lockfile
pnpm run dev
```

This builds the runtime, copies it into the desktop bundle's resources, and
opens the native window. No web server or separate daemon setup is required.
The desktop is an independent Cargo workspace under `desktop/src-tauri`; the
terminal workspace remains independently buildable.

To open a particular project and session:

```sh
HERDSTEAD_CWD=/path/to/project HERDSTEAD_SESSION=work pnpm run dev
```

The default folder is the launch directory (or the home folder when launched
without one), and the session defaults to that folder's name. The **+** beside
Spaces opens a native folder picker and adds a workspace to that session.
The home workspace is shown as `~`; home paths are abbreviated throughout the
desktop. Custom workspace names are preserved. Pane labels show `shell` when no
recognised agent owns the foreground job, and the herd uses readable agent
names such as `opencode`.

## Native builds and downloads

For prebuilt DMGs, AppImages and Debian packages, installation and in-app updates,
see [getting started](getting-started.md). Desktop is the only distributed app.
The runtime is bundled; there is no separate CLI installer.

Local unsigned builds can disable update artifacts:

```sh
pnpm run build --config '{"bundle":{"createUpdaterArtifacts":false}}'
# macOS app only:
pnpm run build --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

Bundles appear under `desktop/src-tauri/target/release/bundle/`. To generate
signed update artifacts, set `TAURI_SIGNING_PRIVATE_KEY` to the release signing
key and its optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, then run `pnpm run build`.
The public key in `tauri.conf.json` must match. These update signatures are
separate from Apple Developer ID signing and notarisation.

The macOS DMG opens a compact installer window with Herdstead on the left and
Applications on the right. Release builds set `TAURI_BUNDLER_DMG_IGNORE_CI=true`
so Tauri saves this Finder layout even in CI; otherwise the DMG opens with
Finder's default window size and icon arrangement.

## The shell and rich terminal

The terminal is the input surface. There is no separate desktop composer.
New desktop daemons prefer zsh and add shell integration after loading your
normal startup files. An explicit `shell` in Herdstead's config takes precedence.
A zsh pane is a login shell, so `.zprofile` PATH setup is available too.

Your `.zshrc`, prompt, aliases and plugins remain yours. Existing
[Starship](https://starship.rs/guide/) or [Oh My Zsh](https://ohmyz.sh/) setups
load normally. Herdstead does not install them or rewrite your configuration.
For example, a Starship installation still uses its normal zshrc line:

```zsh
eval "$(starship init zsh)"
```

**Settings** sets the desktop palette, terminal font and rich-output preference.
The default is the bundled JetBrains Mono Nerd Font, including Powerline and
Nerd Font prompt symbols. An installed font can override it; the bundled font
stays in the fallback chain for symbols missing from that font. Prompt themes
come from zsh/Starship/Oh My Zsh; their ANSI colours are preserved. The desktop
palette is separate from that shell configuration.

### References at the shell prompt

Type `@` to see matching paths in the terminal. Tab completes them; multiple
matches use zsh's completion menu. Completed folders keep a trailing slash.
References are highlighted in the command line, with missing paths shown in red
(zsh 5.9 or newer provides the highlight tags). Ordinary completion outside a
reference delegates to your existing Tab widget.

```zsh
cat @crates/app/src/main.rs
cd @docs/
printf '%s\n' @"a file with spaces.rs"
```

On Enter, standalone references become safely quoted absolute path arguments.
A missing reference stays in the editor with an error. Shell metacharacters in a
filename are literal; reference expansion never evaluates them. References
inside an already quoted string stay text, so `claude 'Review @src/main.rs'`
continues to pass a reference to the agent. Comments, escaped `\@` and
here-documents also retain their literal contents.

This is zsh line-editor integration. An agent or another program that owns its
own input UI uses its own completion behaviour. Existing running daemons keep
their original environment: start a new session to enable the integration.
Other shells remain usable as ordinary terminals.

### Markdown and links while typing

The live input line styles **bold**, *italic*, ~~strikethrough~~, inline code and
Markdown links as you type. Syntax markers remain visible and editable; the
styling does not change shell quoting or the bytes zsh executes. Soft-wrapped
input retains its cursor columns and complete link targets.

Bare URLs and `[label](https://example.com)` links are recognised before you
submit. A local logo appears ahead of a link, using an available space or the
terminal gutter. GitHub, web, file and mail icons require no favicon/network
requests. Click the icon or Cmd/Ctrl-click the linked text to open it; a focused
link also opens with Enter. Opening a link does not submit the command. Icons
are excluded from copied text and never become part of shell input.

### Files, code and command output

Interactive single-file `cat` reads render in place:

```zsh
cat README.md                    # rendered Markdown
cat crates/app/src/main.rs        # programming syntax highlighting
cat @crates/app/src/main.rs       # completion + safe file reference
md README.md                     # explicit Markdown renderer
git show HEAD:README.md | md      # render Markdown from command output
tree .                           # folder tree
```

Existing `cat` and `md` aliases/functions are preserved; `hcat file` and
`hmd [file]` always select the rich readers. Oh My Zsh's `md` directory shortcut
therefore continues to work. `command cat` bypasses the wrapper. Multiple files, flags and piped or
redirected `cat` preserve ordinary byte-for-byte behaviour. Existing `tree`
aliases/functions are also preserved. The bundled runtime is on the shell PATH,
so `herdstead cat`, `herdstead md`, `herdstead render` and `herdstead tree` work
without a separate CLI installation. `command | herdstead render` renders a full
Markdown document, including tables and fenced code.

**Rich output** automatically recognises headings, emphasis, inline code,
lists, quotes, links and language-labelled code fences in ordinary terminal
output. It preserves the terminal's row coordinates: no rows are inserted or
removed. Block interpretation skips the cursor row, soft-wrapped rows, existing
ANSI-styled output and alternate-screen applications. Input styling is applied
separately, with source delimiters preserved. For complete document layout,
including tables and reflowing paragraphs, use `md` or `herdstead render`.
Disable **Rich terminal output** in Settings whenever literal output is preferable.

Links in output remain Cmd/Ctrl-clickable; URLs open externally and file paths
open in a viewer. Folder trees, code and Markdown viewers remain available from
Files and the command palette.

The PTY still supports truecolour, wide characters, alternate-screen programs,
semantic control/function keys, bracketed paste and daemon-owned scrollback.
Native selection/copy and IME input remain available. Mouse reporting is not
forwarded to applications; the mouse controls panes, selection and scrollback.

## Controls

The shortcut modifier is Cmd on macOS and Ctrl on Linux.

| shortcut | action |
|---|---|
| Cmd/Ctrl+Shift+P | command palette |
| Cmd/Ctrl+T | new terminal tab |
| Cmd/Ctrl+D | split right |
| Cmd/Ctrl+Shift+D | split down |
| Cmd/Ctrl+Shift+Enter | zoom focused pane |
| Cmd/Ctrl+Shift+W | close focused pane |
| Cmd/Ctrl+1–9 | select tab |
| Cmd/Ctrl+B | toggle Files |
| Cmd/Ctrl+F | find matching rows in visible output |
| Cmd/Ctrl+, | settings |
| Cmd/Ctrl+plus/minus | change terminal font size |

Each pane header has **split right**, **split down**, **zoom/restore** and
**close** controls. They focus and act on that pane, including when another pane
was previously focused.

Drag between panes to resize. Scroll over a shell to move through daemon
scrollback, and click its scroll offset to return to live output. Viewer panes
scroll independently in the client. Settings includes **Check for updates** and **Install update and restart**.
On macOS, the **Herdstead** application menu also offers **Check for Updates…**
and **Settings…** (Cmd+,), alongside About, Hide and Quit. Checking opens Settings
and reports the result; installing an available update remains an explicit action.
Linux exposes Settings and update checking in the Herdstead menu too.
The command palette also offers rename,
focus cycling, a folder tree viewer, scrolling and copying the visible output.
Search covers the current viewport, not the entire scrollback. Files over
4 MiB are refused by desktop viewers with an explanation; terminal commands
remain available for those files.

Desktop palette, terminal font, rich-output preference and Files visibility are saved locally by the webview.
They do not add daemon settings or change the shared config file. Native folder
access and external URL opening use narrow Rust commands; remote pages cannot
navigate the application, and there is no general shell IPC command.

## Verification

From the repository root, first run the standard workspace checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build                     # debug binary used by pnpm test
```

Then:

```sh
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml --check
cd desktop
pnpm install --frozen-lockfile
pnpm run prepare:runtime
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm test
pnpm run test:e2e
```

`pnpm test` uses `target/debug/herdstead` (or `HERDSTEAD_BIN`) for real daemon
integration tests, isolated from user configuration and state. Build it with
`cargo build` first. The browser E2E test needs Chrome installed, drives the real
page and real daemon, and substitutes only Tauri's invoke/event transport. It
checks input Markdown/source preservation, link-logo activation, splits, tabs,
in-terminal Markdown, file viewers, shell `@path`, themes,
the command palette and resizing. Native webview/IPC and folder picking also
need a smoke test in the actual Tauri window, along with the native application
menu and Cmd+, accelerator.
