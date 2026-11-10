# Panes, tabs and terminals

The desktop uses a real PTY for every shell pane. Split right with Cmd/Ctrl+D,
split down with Cmd/Ctrl+Shift+D, and open a tab with Cmd/Ctrl+T. Pane headers
provide split, zoom and close buttons. Drag a divider to resize, click a pane to
focus it, and use the sidebar to select workspaces. Use **+** beside Spaces
to pick a project folder.

The cursor is drawn on its own terminal cell, never as an overlay: the bridge
marks the cell the shell reports, and the client draws a block that swaps that
cell's colours so the character underneath stays readable. Rich output and
scrolling cannot move it off that cell. It blinks while the terminal has keyboard
focus and becomes an outline when focus moves elsewhere; reduced-motion
preferences keep the active cursor solid. It is hidden while you scroll back and
whenever the program hides the cursor. Terminal controls return keyboard focus to
the shell; search fields and dialogs keep focus while you use them.

Scroll over a pane for daemon-owned history. Click the scroll offset to return
to live output. Viewers scroll locally. Find searches the visible output, not
the complete scrollback. Closing a pane stops its process; closing the app
leaves the session daemon running.

The terminal supports truecolour, wide characters, alternate screens, semantic
control/function keys, bracketed paste, native selection/copy and IME input.
Mouse reporting is reserved for the desktop controls and selection rather than
forwarded to terminal programs.

Cmd/Ctrl-click links to open them. URLs open externally; existing file paths
open viewers. Link logos are local assets and do not fetch favicons. **Rich
output** interprets Markdown and fenced code without changing terminal rows;
turn it off to inspect literal output. Alternate-screen programs retain their
own rendering.

Panes receive `TERM=xterm-256color`, `COLORTERM=truecolor`, `HERDSTEAD=1`,
`HERDSTEAD_PANE` and `HERDSTEAD_SESSION_PID`. The shell reports its folder through
OSC 7 for new panes and relative paths. See [Desktop](desktop.md) for zsh and
rich command behaviour, and [Keys](keys.md) for shortcuts.
