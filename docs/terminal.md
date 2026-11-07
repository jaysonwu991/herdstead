# Panes, tabs, workspaces

## The three levels

```
workspace  ~/projects/my-app
├── tab  shell
│   ├── pane  zsh            ← focused
│   └── pane  claude
└── tab  tests
    └── pane  npm test
```

- A **workspace** is a project folder. It has tabs, a name and its own panes.
- A **tab** is a pane layout: a tree of splits with one focused pane.
- A **pane** is a terminal (a process on a PTY) or a viewer (a file or folder
  rendered by the client).

The sidebar lists workspaces, the active workspace's tabs, and the active tab's
panes; the tab strip along the top shows the workspace's tabs. Click any row to
go there, or use the keys below.

## Splits, focus, resize

| key | action |
|---|---|
| `prefix+v` | split side by side, new pane on the right |
| `prefix+s` | split top and bottom, new pane below |
| `prefix+h` `j` `k` `l` | focus left / down / up / right |
| `prefix+o` / `prefix+i` | focus the next / previous pane in layout order |
| `prefix+z` | zoom the focused pane to fill the tab (again to unzoom) |
| `prefix+x` | close the focused pane |
| `prefix+pgup` / `pgdn` | scroll the pane a page |
| `prefix+home` / `end` | scroll to the top of scrollback / back to live |

A split divides the space in half; the divider is one cell wide. Divider positions
are stored as ratios, so a resize of the terminal keeps the proportions.

**Resize with the mouse:** press on a divider and drag. **Resize with the
keyboard:** there is no default binding, but any action can be rebound (see
[keys](keys.md)) — the runtime accepts `ResizePane { dir, cells }`.

New panes start in the focused pane's working folder, with the shell from
`shell` (or `$SHELL`, or `/bin/sh`). Closing the last pane of the last tab starts
a fresh shell rather than leaving an empty session.

## Tabs and workspaces

| key | action |
|---|---|
| `prefix+c` | new tab |
| `prefix+n` / `prefix+p` | next / previous tab |
| `prefix+1` … `9` | go to tab 1–9 |
| `prefix+w` | close the tab (and its panes) |
| `prefix+shift+r` | rename the tab |
| `prefix+shift+n` | new workspace in the current folder |
| `prefix+[` / `prefix+]` | previous / next workspace |
| `prefix+shift+1` … `9` | go to workspace 1–9 |
| `prefix+shift+x` | close the workspace |
| `prefix+shift+w` | rename the workspace |

Closing a workspace stops every pane in it. Renaming is cosmetic; the folder and
the session name are unchanged.

## The mouse

| gesture | effect |
|---|---|
| click a pane | focus it |
| drag a divider | resize the two panes it separates |
| wheel over a pane | scroll its output three rows (a viewer scrolls its content) |
| click a tab | select it |
| click a sidebar row | switch workspace, tab or pane |
| modifier-click a link | open a URL, or a path in a viewer |
| click the composer | focus it and place the caret |

The link modifier is the platform's command key by default (`link-modifier =
"super"`); set it to `ctrl` or `alt` in the config. Move the pointer over a link
with the modifier held and the status bar names its target.

Mouse events are used by the interface and are **not forwarded** to programs in
a pane, so an application inside a pane never sees a click.

## Output, scrollback and links

Each pane is a real terminal: a full VT screen with truecolor, wide characters,
cursor shapes, the alternate screen, application cursor keys and bracketed paste,
kept in up to `scrollback` rows (default 5000). The cursor follows the pane's own
when the pane is focused and live. Scrollback offsets show `↑n` in the pane's
title, and writing resumes the live view.

Links are found in the visible text every frame:

- `http://`, `https://`, `file://`, `mailto:` and bare `www.` addresses
- paths that **exist** relative to the pane's folder, with optional `:12:3` or
  `#L12-L14` locations
- `@path` references in Markdown and in the composer

A URL is drawn in the link colour and underlined; a path only becomes a link when
it resolves, so prose that looks like a path stays prose. Link targets travel with
the characters, so a wrapped URL is still clickable on every row.

The CLI does the same for its own output: `herdstead cat` and friends emit OSC 8
hyperlinks, which iTerm2, Ghostty, Kitty, WezTerm and VS Code's terminal make
clickable.

## What a pane's environment gets

| variable | value |
|---|---|
| `TERM` | `xterm-256color` |
| `COLORTERM` | `truecolor` |
| `HERDSTEAD` | `1` |
| `HERDSTEAD_PANE` | the pane's id, e.g. `p3` |
| `HERDSTEAD_SESSION_PID` | the daemon's process id |

The daemon reads OSC 7 and OSC 0/2 from the pane's output; any shell that emits
them — bash, zsh, fish, and most prompts — keeps the folder and title current.

## Rich output and zsh integration

Ordinary plain output can render headings, inline Markdown and fenced code in
place. Row coordinates remain stable, and input, existing ANSI styles, wrapped
rows and alternate-screen programs are preserved. The desktop offers a raw-output
toggle. For full Markdown document layout, pipe to `herdstead render`.

The desktop's zsh integration adds native `@path` completion and safely quoted
arguments, rich interactive `cat`, `md` and `tree`, while loading your existing
prompt framework. See [Desktop](desktop.md) for shell semantics and setup.
