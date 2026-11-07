# Keys

`ctrl+b` is the **prefix**. Press it, release it, then press the key for the
action. Panes receive everything else directly, so the prefix is the only thing
standing between you and a shell.

`herdstead keys` prints the live map, including any rebinding.

## Panes and layout

| key | action | what it does |
|---|---|---|
| `prefix+v` | `split-vertical` | split side by side, new pane on the right |
| `prefix+s` | `split-horizontal` | split top and bottom, new pane below |
| `prefix+x` | `close-pane` | close the focused pane |
| `prefix+h` | `focus-left` | focus the pane to the left |
| `prefix+j` | `focus-down` | focus the pane below |
| `prefix+k` | `focus-up` | focus the pane above |
| `prefix+l` | `focus-right` | focus the pane to the right |
| `prefix+o` | `focus-next` | next pane in layout order |
| `prefix+i` | `focus-prev` | previous pane in layout order |
| `prefix+z` | `zoom` | zoom the focused pane to fill the tab |

## Tabs and workspaces

| key | action | what it does |
|---|---|---|
| `prefix+c` | `new-tab` | new tab |
| `prefix+n` | `next-tab` | next tab |
| `prefix+p` | `prev-tab` | previous tab |
| `prefix+w` | `close-tab` | close the tab and its panes |
| `prefix+shift+r` | `rename-tab` | rename the tab |
| `prefix+1` … `9` | — | go to tab 1–9 |
| `prefix+shift+n` | `new-workspace` | new workspace in the current folder |
| `prefix+]` | `next-workspace` | next workspace |
| `prefix+[` | `prev-workspace` | previous workspace |
| `prefix+shift+x` | `close-workspace` | close the workspace and its panes |
| `prefix+shift+w` | `rename-workspace` | rename the workspace |
| `prefix+shift+1` … `9` | — | go to workspace 1–9 |

Digits are handled by the client rather than the keymap, so they cannot be
rebound — `prefix+shift+1` arrives as `prefix+!` and is treated as workspace 1.

## Interface

| key | action | what it does |
|---|---|---|
| `prefix+:` | `composer` | focus the composer |
| `prefix+t` | `theme-menu` | theme menu with live preview |
| `prefix+m` | `markdown-preview` | composer with the Markdown preview on |
| `prefix+e` | `open-file` | fill the composer with `/open ` |
| `prefix+shift+e` | `open-tree` | fill the composer with `/tree ` |
| `prefix+\|` | `sidebar` | show or hide the sidebar |
| `prefix+?` | `help` | the keybinding list |
| `prefix+r` | `reload` | reload config and theme |
| `prefix+q` | `detach` | detach, leaving the panes running |

## Scrolling

| key | action | what it does |
|---|---|---|
| `prefix+pgup` | `scroll-page-up` | scroll the focused pane up a page |
| `prefix+pgdn` | `scroll-page-down` | scroll it down a page |
| `prefix+home` | `scroll-top` | the top of scrollback |
| `prefix+end` | `scroll-bottom` | back to the live view |

The mouse wheel scrolls three rows; see [terminal](terminal.md).

## Composer

While the composer has focus it takes every key — including the prefix — so
`Esc` is how you get back to the pane.

| key | effect |
|---|---|
| `Enter` | accept an open completion; otherwise send or run a slash command |
| `Shift+Enter` | newline |
| `Tab` | accept the completion, else insert two spaces |
| `Esc` | close the composer |
| `←` / `→` | move the caret; with `Ctrl`, by word |
| `Home` / `End` | start / end of the whole message |
| `↑` `↓` | completion, or the previous/next line |
| `Ctrl+A` / `Ctrl+E` | start / end of the whole message |
| `Ctrl+U` / `Ctrl+K` | delete before / after the caret |
| `Ctrl+W` | delete the previous word |
| `Ctrl+P` | toggle the Markdown preview |

The full composer reference is in [the composer](composer.md).

## Viewers

| key | effect |
|---|---|
| `j` / `↓`, `k` / `↑` | scroll a line |
| `space` / `PageDown`, `b` / `PageUp` | scroll a screen |
| `g` / `Home`, `G` / `End` | top, end |
| `q` / `Esc` | close the viewer pane |

## Mouse

| gesture | effect |
|---|---|
| click a pane | focus it |
| drag a divider | resize |
| wheel over a pane | scroll it |
| click a tab or sidebar row | select it |
| modifier-click a link | open the URL or path |
| click the composer | focus it |

`link-modifier` chooses the modifier (`super`/`ctrl`/`alt`). Mouse capture is
disabled entirely when `mouse = false`, which gives the terminal's own selection
back.

## Rebinding

```toml
[keybinds]
split-vertical = "prefix+|"
close-pane = "prefix+shift+q"
focus-left = "prefix+alt+h"
theme-menu = "prefix+y"
```

A spec is modifiers and a key, joined by `+`:

- modifiers: `prefix`, `ctrl` (or `control`), `alt` (or `option`, `opt`),
  `shift`, `super` (or `cmd`, `command`, `meta`)
- keys: a single character, `space`, `tab`, `backtab`, `enter`, `esc`,
  `backspace`, `delete`, `insert`, `left`, `right`, `up`, `down`, `home`, `end`,
  `pageup`/`pgup`, `pagedown`/`pgdn`/`pagedn`, `f1` … `f24`
- `prefix++` binds the `+` key; `prefix+|` and `prefix+?` are written that way
  and pressed with Shift, which punctuation ignores

Matching rules worth knowing:

- For letters, Shift is carried by the character's case: `shift+r` matches `R`,
  and plain `v` does not match `V`.
- Punctuation ignores the Shift modifier when matching the resulting character:
  press Shift+/ for `prefix+?` on a US keyboard, and write `prefix+?` in config.
- Modifiers must match exactly. `prefix+ctrl+v` is not `prefix+v`.
- If two actions share a spec, the alphabetically first action name wins, so
  avoid duplicates (the config reports unknown actions, not conflicts).
- The parser accepts specs without `prefix`, but the current TUI only dispatches
  configured actions after the prefix. Use `prefix+…` bindings; direct global
  shortcuts are not implemented (see [troubleshooting](troubleshooting.md#a-key-does-nothing)).

Unknown action names and unparsable specs are warnings, and the default stays in
place for an unparsable spec. Unknown names have no implemented action and
should be removed: a valid binding for one can still consume that key.
`herdstead keys` lists supported actions; rebinding is picked up by `prefix+r`
or by the next attach, without restarting the daemon.
