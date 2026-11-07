# Troubleshooting

## Start here

```bash
herdstead config      # resolved settings, paths, warnings (stderr)
herdstead ls          # live sessions and their state
herdstead themes      # did your theme load?
herdstead keys        # what the keymap became
```

The session log is at `~/.local/state/herdstead/logs/<session>.log`. Daemon
startup errors and configuration warnings end up there; a client prints its
warnings to stderr before the interface takes over.

## The install script failed

```bash
curl -fsSL https://raw.githubusercontent.com/jaysonwu991/herdstead/main/install.sh | sh
```

Each message names the step that broke, and the download error prints the URL
it tried. The usual ones:

- **`could not download …`** — that release published no archive for your
  platform. `releases/latest/download/herdstead-<platform>.tar.gz` is a permanent
  URL per platform, so this means the newest release is incomplete, or your
  platform is not one of the four: `HERDSTEAD_VERSION=v0.1.1` pins an older
  release that has it.
- **`unsupported architecture` / `unsupported operating system`** — the script
  knows macOS arm64 and x64 and Linux x64 and arm64. Anything else builds from
  source: `cargo install --git https://github.com/jaysonwu991/herdstead.git herdstead`.
- **`this is a musl system`** — the release binaries link glibc, so Alpine and
  other musl distributions need their own build (the same `cargo install`).
- **`checksum verification failed`** — the archive and the `.sha256` beside it
  disagree, usually a truncated download. Nothing was installed; run it again.
- **`cannot write to …`** — the folder is not yours to write. Set
  `HERDSTEAD_INSTALL_DIR=$HOME/.local/bin`.
- **`is not on your PATH`** — the binary is installed and the script still
  succeeded; add the line it printed to your shell's startup file.

## An upgrade did not take effect

A session daemon is a copy of the binary installed when that session started, so
replacing the file on disk leaves it running the old build, and attaching to it
reuses it:

```bash
herdstead --version          # the client you run
herdstead ls                 # sessions, each with its own daemon
herdstead kill <name>        # stops the daemon, keeps the layout
```

Attaching again starts a new daemon from the new binary and restores the panes
from the saved layout.

## "herdstead needs an interactive terminal to attach"

herdstead refuses to start a session when stdin or stdout is not a terminal —
in a pipe, a script, a CI job, or an editor's command runner. That is deliberate:
attaching without a TTY used to start a daemon and then fail. Run it from a real
terminal, or use the non-interactive commands (`cat`, `tree`, `md`, `preview`,
`themes`, `keys`, `languages`, `config`), which work anywhere.

## A session is listed but does not respond

`herdstead ls` prints `(not responding: …)` when the socket exists but the
daemon behind it does not answer. That is usually a daemon that crashed without
cleaning up, or one from a different user. Stop it and remove the file:

```bash
herdstead kill <name>            # sends Kill, then removes the socket
herdstead config                # find the runtime directory in use
```

Attaching to a stale socket also recovers: the client sees that nobody is
listening, removes the stale file and starts a fresh daemon.

## My session vanished

Sessions are named after the folder, so moving or renaming the project changes
the name. Check what is on disk and attach by the old name:

```bash
ls ~/.local/state/herdstead/sessions/
herdstead -s old-name
```

If `restore = false`, a new daemon starts with one fresh shell and saves that
fresh layout over the previous file. Copy a saved layout before starting if you
need to preserve it. A missing or unreadable file starts fresh; invalid JSON is
recorded in the daemon's warnings, which are printed on shutdown.

## Panes came back in the wrong folder

A pane's folder is reported by its shell through OSC 7. A shell or prompt that
does not emit it leaves herdstead with the folder the pane was created in. New
panes then start there too. Any prompt that shows the working directory can emit
OSC 7; enable your shell or prompt's directory-reporting hook if needed.
The bundled zsh integration emits it automatically.

## The theme did not change

- An unknown name falls back to `midnight` with a warning on stderr.
- A custom theme file with broken JSON, an unknown base or slot, or an
  unparsable colour is rejected as a whole and omitted from the menu. Fix the
  file and reload. `extends` accepts built-in palettes only.
- Config `theme-colors` overrides are applied in sorted slot order and stop at
  the first invalid slot or colour. Earlier overrides remain applied; later
  ones do not. Correct the reported entry and reload.
- `prefix+r` reloads the config and theme; choosing in `prefix+t` applies
  immediately. `Esc` in that menu restores the theme you had.

## A key does nothing

- While the **composer** has focus it takes every key; press `Esc` first.
- An unparsable spec warns and keeps the default. Remove unknown action names:
  a valid binding for one can consume its key without performing an action.
  `herdstead keys` shows supported actions.
- Direct bindings without `prefix` are parsed, but not dispatched as global
  shortcuts by the current TUI. Configure `prefix+…` instead.
- Letters compare case-sensitively for `shift`: `prefix+shift+r` needs `R`.
  Punctuation ignores shift.
- If two actions share a spec, the alphabetically first action name wins.
- Key specs are parsed case-insensitively; use `shift` explicitly for an
  uppercase letter binding. Modifiers must match the incoming event.

## The composer will not send

The composer sends to the **focused shell pane**. If a viewer has focus, or the
pane's process has exited, it says so and keeps your text. Focus a shell
(`prefix+h/j/k/l`) or start one (`prefix+v`). Slash commands still work from
anywhere.

## Composer Markdown markers disappear

The TUI composer currently uses the inline display renderer, which hides
recognised Markdown delimiters. Its stored message still includes those bytes
and sends them to the pane. The desktop shell input uses a separate renderer
that keeps delimiters visible.

## Links are not clickable

- Hold the link modifier: `super` (Cmd) by default, or `ctrl`/`alt` per
  `link-modifier`.
- A path is only a link when it exists relative to that pane's folder.
- `links = false` disables terminal-output link detection; rendered Markdown
  and tree viewers can still carry links.
- Markdown and tree CLI output use OSC 8 hyperlinks, which require a
  supporting terminal. Highlighted source (`herdstead cat` and code viewers)
  currently carries no link targets of its own. In a terminal pane, the client
  can still detect links in that output.
- A location suffix is recognised for linking, but opening a TUI viewer does
  not currently jump to that source line.

## A viewer says "binary file"

The code viewer refuses files containing a NUL byte instead of dumping control
characters. Use a hex or image tool for those; everything textual (including SVG
and XML) opens normally.

## The terminal's own text selection is gone

Mouse capture belongs to herdstead while it runs. Set `mouse = false` to hand the
mouse back to your terminal and the interface stops responding to clicks. Most
terminals also have a modifier for native selection (often Shift or Option).

## Output lags in a huge pane

- Lower `scrollback` (5000 rows is the default per pane).
- Lower `tree-max-entries` if tree viewers are slow on a monorepo.
- A pane's emission rate is the other half: a program that prints megabytes per
  second costs more to parse no matter what draws it.

## Platform limits

macOS and Linux only: the client and daemon talk over a Unix socket. Windows has
no support today. Mouse events are not forwarded into panes, so programs that
expect them (htop's mouse, some TUIs) will not receive them.

## Protocol limits

The desktop bridge caps incoming JSON messages at 32 MiB. The Unix socket
readers currently have no equivalent frame-size check, and incoming runtime
messages use an unbounded channel. Keep the runtime directory private and use
the normal clients; the outgoing two-frame queues only bound queued snapshots.

## Reporting a bug

Include:

- herdstead's version (`herdstead --version`) and your OS
- the output of `herdstead config`
- the session log, if the daemon is involved
- what you did, and what happened instead

If it panics, `RUST_BACKTRACE=1 herdstead` adds a backtrace; the panic message
also lands in the session log or on stderr.
