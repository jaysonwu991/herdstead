# Themes

A theme is one set of named colours used by everything: the interface, pane
output defaults, Markdown, syntax highlighting, trees and the CLI's ANSI output.
Change the theme and the whole surface changes with it.

## Built-in palettes

| name | appearance | syntax theme | notes |
|---|---|---|---|
| `midnight` (default) | dark | base16-ocean.dark | ink-blue surfaces, blue-violet accent |
| `daylight` | light | base16-ocean.light | paper surfaces, GitHub-ish accent |
| `nord` | dark | Nord | cool blue-grey |
| `gruvbox` | dark | gruvbox-dark | warm retro |
| `mono` | dark | ansi | single-hue; token colours are mapped to grey |
| `solarized` | light | Solarized (light) | classic Solarized light |

Aliases: `dark` and `default` mean `midnight`, `light` means `daylight`,
`solarized-light` means `solarized`.

Choose a theme in **Appearance**. The desktop saves that choice in webview
storage and reapplies it when reconnecting. The bridge initially loads the
runtime helper's `--theme`, `HERDSTEAD_THEME` or configured `theme`, in that order.
An unknown startup theme falls back to `midnight` with a warning.

## Theme files

Drop a JSON file in `~/.config/herdstead/themes/` and it appears in
`herdstead themes` and the Appearance selector:

```json
{
  "name": "plum",
  "extends": "nord",
  "appearance": "dark",
  "syntax": "Nord",
  "colors": {
    "accent": "#ff00aa",
    "link": "#7de3ff",
    "selection": "#3b2a4a",
    "tree_dir": "#ff00aa"
  }
}
```

| key | required | meaning |
|---|---|---|
| `name` | no | recommended: the selection name; when omitted, inherits the base name and replaces that catalogue entry |
| `extends` | no | a built-in palette to start from (default `midnight`); custom-theme inheritance is not supported |
| `appearance` | no | `dark` or `light`; decides the syntax fallback without recomputing inherited slots |
| `syntax` | no | syntax theme name; inherited from the base when omitted |
| `colors` | no | slot → colour; `#rrggbb`, `#rgb` or a basic colour name |

The file is read by file name (`themes/<name>.json` when selected by name) and
then by its `name`; built-in names take precedence. An unknown slot or an
unparsable colour rejects the custom file. When no valid theme matches, startup
reports the problem and uses `midnight`; invalid files are omitted from the menu.
See [troubleshooting](troubleshooting.md#the-theme-did-not-change).

You can also override slots without a file, applying to whichever theme is
active:

```toml
[theme-colors]
accent = "#8b5cf6"
link = "#7de3ff"
```

`herdstead themes` lists what is available; `herdstead config` shows the resolved
`theme` and `theme-colors`.

## The colour slots

Thirty-five slots, all optional in a theme file because every one has a derived
default.

### Surfaces and text

| slot | used for |
|---|---|
| `bg` | the application background and the terminal's default background |
| `panel` | the tab strip, sidebar, status bar and composer |
| `fg` | default text |
| `muted` | secondary text, unfocused pane titles, list counts |
| `faint` | hints, section headers, dim details |

### Chrome

| slot | used for |
|---|---|
| `accent` | focus, the active pane's border, working status, bullets |
| `accent_alt` | transient status messages and hover |
| `border` | pane borders, dividers, rules, sidebar divider |
| `border_focus` | the focused pane, completion and overlay borders |
| `selection` | the selected row in the completion popup and the Appearance selector |
| `tab_active_bg`, `tab_active_fg` | the active tab |
| `tab_fg` | inactive tabs |
| `chip_bg`, `chip_fg` | the workspace chip at the left of the tab strip |

### States

| slot | used for |
|---|---|
| `success` | completed tasks, executable files, "done" |
| `warning` | blocked panes, scroll indicators |
| `error` | exited panes, missing `@path` references, failures |
| `info` | agent glyphs and agent names |

### Content

| slot | used for |
|---|---|
| `heading` | Markdown headings and their bars |
| `link` | URLs |
| `mention` | `@path` references that exist |
| `path` | file paths in output |
| `inline_code_fg`, `code_bg` | inline code and fenced blocks |
| `code_border` | the rail beside a code block or file |
| `quote` | block quotes and their bar |
| `bullet`, `number` | unordered and ordered list markers |
| `table_border` | table rules |
| `gutter` | line numbers |
| `tree_dir`, `tree_file`, `tree_exec`, `tree_link` | tree entries by kind |

### The syntax theme

Code colours come from a [syntect](https://github.com/trishume/syntect) theme,
not from the slots. The bundled set includes `base16-ocean.dark`,
`base16-ocean.light`, `Nord`, `gruvbox-dark`, `gruvbox-light`, `Solarized
(dark)`, `Solarized (light)`, `GitHub`, `Dracula`, `Monokai Extended`,
`OneHalfDark`, `Catppuccin Mocha`, `TwoDark`, `zenburn` and more. Use the exact
name from the bundled set. An empty or unrecognised
`syntax` falls back to `base16-ocean.dark` for dark appearance and
`base16-ocean.light` for light; an omitted `syntax` inherits the base theme.

`mono` is special: it keeps the syntax theme's structure but maps each token
colour to grey by luminance, so structure survives without hue.

## Reloading

Selecting a theme in Appearance re-renders immediately. Restart the desktop
bridge to reload configuration changes; existing daemons retain their settings.
