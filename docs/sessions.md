# Sessions

A session is a named workspace with its own daemon. The daemon owns the panes,
their processes and the layout; the terminal you are in is just one client
attached to it. That is the whole reason detaching works.

## Starting, attaching, detaching

```bash
herdstead                 # attach to the session named after the folder
herdstead -s work         # attach to (or start) the session "work"
HERDSTEAD_SESSION=work herdstead
```

When no daemon is listening on the session's socket, the client starts one
(`herdstead --server --session <name>`, detached with its own session id, output
to the session log) and waits for the socket. The daemon keeps running when the
last client leaves.

`ctrl+b q` (or the `detach` action, or `/detach` in the composer) closes the
client and leaves everything running. The next attach draws the current state.

The daemon stops when it receives `SIGTERM` or `SIGINT` (it saves the layout
first) or when it is asked to:

```bash
herdstead kill work              # stop the panes, keep the saved layout
herdstead kill work --forget     # also delete the saved layout
herdstead ls                     # live sessions, pane counts, focused pane
```

`herdstead ls` inspects a session without attaching to it, so it never changes
anyone's layout.

## Naming

The session name comes from, in order:

1. `--session` / `-s`
2. `$HERDSTEAD_SESSION`
3. the current folder's name, sanitised (`/`, spaces and most punctuation become
   `-`, at most 48 characters, `main` when nothing usable remains)

The name decides the socket (`<runtime>/<name>.sock`) and the saved layout
(`<state>/sessions/<name>.json`). Renaming the folder you start from therefore
renames the session: use `-s` to keep an old one.

## What survives a detach, and what survives a restart

The daemon keeps everything while it runs. Across a daemon restart — a reboot,
`herdstead kill`, a crash — herdstead restores the layout when `restore = true`:

| kept | not kept |
|---|---|
| workspaces, tabs and their names | the text in each pane |
| the pane tree, split ratios and zoom | scrollback |
| the focused pane and active tab | the client's theme, scroll positions and composer |
| each pane's working folder | environment changes |
| viewer panes and the files they show | running processes |

Restored shell panes start a fresh shell in their recorded folder. A folder that
no longer exists falls back to the session's first folder. Agent panes are
detected again from what they draw.

The layout is written two seconds after a structural change and on shutdown.
Nothing is written per frame, so a busy pane does not hammer the disk.

## Working folders

A pane's folder is reported by the shell through OSC 7 (`\e]7;file://host/path`),
which bash, zsh, fish and most prompts emit. herdstead reads it from the pane's
output, so new panes, the composer's `@path` completion and the viewer all resolve
relative paths against what the pane is actually showing — not the folder the
pane was created in.

Terminal titles (OSC 0 and 2) are tracked the same way, and a title an
application sets is left alone unless an agent takes over the pane.

## Agent status

The sidebar and the tab strip show one glyph per pane, tab and workspace:

| glyph | status | meaning |
|---|---|---|
| `○` | idle | quiet for at least 900 ms |
| `●` | working | output within the last 900 ms |
| `◆` | blocked | the bottom of the screen is asking a question |
| `✕` | exited | the process ended |

An agent is recognised first from its command or the pane title (`claude`,
`codex`, `gemini`, `aider`, `opencode`, `oxide`, `cursor-agent`, `goose`, `amp`,
`qwen`) and then from signatures in the screen text (`Welcome to Claude Code`,
`OpenAI Codex`, `aider v…`, and so on). When one is recognised, the pane title
becomes the agent's name and the sidebar shows its glyph.

"Blocked" is a heuristic over the last eight non-empty lines: prompts such as
`Do you want to…`, `[y/N]`, `Press Enter to continue`, `❯ 1. Yes`, an "allow"
question, or a bare question ending a line. It is deliberately conservative —
a question that has scrolled away does not block the pane.

## More than one client

Several clients may attach to one session; each gets the same snapshots. The
**most recently attached client's size wins**: the runtime lays panes out for the
last size it was told, so two clients of very different sizes will re-layout for
each other. Attach from one terminal at a time unless you want that.

## Where a session lives

| what | path |
|---|---|
| socket | `$HERDSTEAD_RUNTIME_DIR` / `$XDG_RUNTIME_DIR/herdstead` / `/tmp/herdstead-<uid>` + `<name>.sock` |
| layout | `$HERDSTEAD_STATE_DIR` / `$XDG_STATE_HOME/herdstead` / `~/.local/state/herdstead` + `sessions/<name>.json` |
| log | the same state folder + `logs/<name>.log` |

The runtime folder is created with mode `0700`. A session's log is where the
daemon's warnings and startup errors go — see [troubleshooting](troubleshooting.md).

## Signals

| signal | effect |
|---|---|
| `SIGTERM`, `SIGINT` | save the layout, stop the panes, remove the socket |
| `SIGHUP` | ignored (the daemon is already in its own session) |
| `SIGPIPE` | ignored (a client disappearing is normal) |

Panes are children of the daemon: stopping it stops them. Detaching does not.
