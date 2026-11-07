# Viewers

A pane is either a terminal or a **viewer**. A viewer renders content with the
client's theme and the same code the CLI uses, which is why a file looks the same
in a pane and in CLI output, with different widths and framing options.

## Opening one

| from | how |
|---|---|
| the composer | `/open <path>`, `/cat <file>`, `/md <file>`, `/tree [dir]` |
| a pane's output | modifier-click a path that exists |
| the keys | `prefix+e` (file) or `prefix+shift+e` (folder tree) fill the composer |
| the CLI, without a TUI | `herdstead cat`, `herdstead md`, `herdstead tree`, `herdstead preview` |

`/open` (and a click) chooses by the path: a folder is a tree, `.md`/`.markdown`/
`.mdx` is rendered Markdown, anything else is highlighted source. The explicit
commands override that: `/cat README.md` shows the Markdown *source*,
highlighted.

A viewer opens in a new split beside the focused pane. If the focused pane is
already a viewer, it is reused, so clicking through files does not multiply panes.

## The code viewer

`syntect` highlighter with [bat](https://github.com/sharkdp/bat)'s syntax set:
213 languages, including TOML, TypeScript and Dockerfiles, matched by file name or
extension.

- a line-number gutter, a coloured rail, and tabs expanded to four spaces
- the theme's syntax theme decides token colours; `mono` maps them to grey
- long lines are truncated at the pane width, and files are capped at 5000 lines
  with a `… n more lines` row
- the whole file is read as UTF-8; invalid UTF-8 or a NUL byte produces an error
  before highlighting. SVG and XML files containing valid UTF-8 open normally
- `herdstead languages` lists every supported language

## The Markdown viewer

The renderer covers headings, paragraphs, emphasis, inline code, links, bare URLs,
`@mentions`, bullet and ordered lists, task lists, nested lists, block quotes,
thematic rules, tables with alignment, and fenced code blocks (highlighted with
the theme's syntax theme). Images become `▧ alt` placeholders linked to their
target — a terminal cannot draw the bitmap, but the reference stays useful.

Like every renderer here it wraps to the pane width, including inside list items
and table cells, and it keeps link targets on the characters they belong to.

The same renderer draws the composer preview and `herdstead md`.

## The tree viewer

```bash
herdstead tree                    # the current folder
herdstead tree crates --depth 3
herdstead tree . --all            # include target/, node_modules/, .git/
```

- folders first, then files, alphabetical within each
- box-drawing guides, colour by kind: folder, file, executable (`*`), symlink
  (with its target)
- each file's size, right-aligned
- a header with the folder, shown file and folder counts, and a skipped-entry
  count when depth, entry caps or unreadable directories limit the scan. Ignored
  folders are excluded from the counts
- `tree-ignores` (default `target`, `node_modules`, `.git`, `dist`, `build`,
  `.venv`) are skipped, and symlinked folders are not followed unless you change
  the code's options
- `tree-max-entries` (default 4000) and a depth of 8 in the viewer stop a huge
  tree from producing unbounded output

The standalone `herdstead tree` command uses the built-in ignore list and entry
cap, with `--all` and `--depth` overrides. `herdstead preview` and TUI tree viewers
use the configured `tree-ignores` and `tree-max-entries`.

## Reading and scrolling

| key | effect |
|---|---|
| `j` / `↓` | scroll down a line |
| `k` / `↑` | scroll up a line |
| `space` / `PageDown` | down a screen |
| `b` / `PageUp` | up a screen |
| `g` / `Home` | the top |
| `G` / `End` | the end |
| `q` / `Esc` | close the viewer pane |
| wheel | scroll three lines |

The pane title shows `↑n` when a viewer is scrolled. In a shell pane, use
`prefix+pgup`/`pgdn`/`home`/`end` or the wheel instead — the shell's own keys
belong to the shell.

## Freshness

Viewers are rendered on the client from the file on disk, cached for 750 ms per
pane and width. A file that changes (a formatter, another agent, your editor) is
re-read within a second without any action; `prefix+r` reloads the config and
theme, and switching theme re-renders everything.

Because the client reads the file, a viewer works for any file this machine can
read. A future remote client would need the file locally.

## What a viewer is not

A viewer has no process and no PTY: it cannot run anything, and text typed into
it scrolls instead of being sent somewhere. It is a pane in every other way — it
splits, focuses, zooms, renames and closes like a terminal.
