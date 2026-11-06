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
ls /tmp/herdstead-$(id -u)/      # check what is left
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

If `restore = false`, a new daemon starts with one fresh shell and leaves the old
layout file untouched. If the layout file is unreadable, the daemon says so in its
log and starts fresh rather than failing.

## Panes came back in the wrong folder

A pane's folder is reported by its shell through OSC 7. A shell or prompt that
does not emit it leaves herdstead with the folder the pane was created in. New
panes then start there too. Any prompt that shows the working directory can emit
OSC 7; most do.

## The theme did not change

- An unknown name falls back to `midnight` with a warning on stderr.
- A theme file that fails to parse is reported by name (broken JSON, unknown slot,
  unparsable colour) — the rest of the theme is still applied when only one slot
  is wrong.
- `theme-colors` overrides apply to whichever theme is active; a typo in a slot
  name is a warning, not an error.
- `prefix+r` reloads the config and theme; choosing in `prefix+t` applies
  immediately. `Esc` in that menu restores the theme you had.

## A key does nothing

- While the **composer** has focus it takes every key; press `Esc` first.
- An unknown action name or an unparsable spec is a warning and that action keeps
  its default. `herdstead keys` shows the result.
- Letters compare case-sensitively for `shift`: `prefix+shift+r` needs `R`.
  Punctuation ignores shift.
- If two actions share a spec, the alphabetically first action name wins.
- The prefix itself must match exactly (`ctrl+b`, not `ctrl+B`).

## The composer will not send

The composer sends to the **focused shell pane**. If a viewer has focus, or the
pane's process has exited, it says so and keeps your text. Focus a shell
(`prefix+h/j/k/l`) or start one (`prefix+v`). Slash commands still work from
anywhere.

## Links are not clickable

- Hold the link modifier: `super` (Cmd) by default, or `ctrl`/`alt` per
  `link-modifier`.
- A path is only a link when it exists relative to that pane's folder.
- `links = false` turns the whole thing off.
- For `herdstead cat` output, the terminal must support OSC 8 hyperlinks
  (iTerm2, Ghostty, Kitty, WezTerm, VS Code). Elsewhere the text is still
  readable, just not clickable.

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

## Reporting a bug

Include:

- herdstead's version (`herdstead --version`) and your OS
- the output of `herdstead config`
- the session log, if the daemon is involved
- what you did, and what happened instead

If it panics, `RUST_BACKTRACE=1 herdstead` adds a backtrace; the panic message
also lands in the session log or on stderr.
