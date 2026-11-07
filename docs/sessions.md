# Sessions

A session daemon owns the shells, PTYs, pane layout, focus and scrollback.
The desktop connects through its bundled JSON bridge. Closing the app stops
that bridge and leaves the daemon running. Reopening with the same session
reconnects to its processes and output.

The default session is named after the launch folder; a GUI launch uses its
working directory or home folder. Development launches can set `HERDSTEAD_CWD`
and `HERDSTEAD_SESSION` explicitly. Workspaces selected with the native folder
picker belong to that session.

With `restore = true`, a restarted daemon restores saved tabs, folders, splits
and focus. It starts fresh shell processes when tabs become visible. Process
memory and old terminal output do not survive a daemon restart.

Layouts live under `~/.local/state/herdstead/sessions/<name>.json`, and daemon
logs under `~/.local/state/herdstead/logs/<name>.log`. Sockets use
`$XDG_RUNTIME_DIR/herdstead`, or `herdstead-<uid>` in the system temporary folder.
[Configuration](configuration.md) describes directory overrides.

For development or session maintenance, the bundled runtime supports
`herdstead ls`, `herdstead kill <name>` and `herdstead kill <name> --forget`.
These commands are available in a desktop terminal. Killing stops that session's
processes; `--forget` also removes its saved layout. They are helpers rather than
a separate installed front-end. A daemon started before an app update continues
running its original runtime until it is stopped.

Agent status is inferred from process/output activity and quiet time. It is a
hint rather than an authoritative agent API. Multiple clients share one layout;
the most recent window size controls the PTY geometry.
