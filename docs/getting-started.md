# Getting started

## Install

Straight from the repository (macOS or Linux, Rust 1.82 or newer):

```bash
cargo install --git https://github.com/jaysonwu991/herdstead.git herdstead
```

That puts `herdstead` in `~/.cargo/bin`. To build locally instead:

```bash
git clone https://github.com/jaysonwu991/herdstead.git
cd herdstead
cargo build --release          # target/release/herdstead
# or: cargo install --path crates/app
```

`cargo build --release` takes about 15 seconds and produces a 5 MB binary with no
runtime dependencies.

## The first session

Run it from the project you are working in:

```bash
cd ~/projects/my-app
herdstead
```

The session is named after the folder (`my-app`). You get a shell pane with your
`$SHELL`, a tab strip, a sidebar, a composer hint and a status bar:

```
 ▦ my-app  1 shell ○                                              ctrl+b
 my-app            │ ┌ my-app ○ ────────────────────────────────┐
 WORKSPACES        │ │ sh-4.4$                                  │
  ○ my-app       1 │ │                                          │
 TABS              │ │                                          │
    ○ shell      1 │ │                                          │
 PANES             │ │                                          │
    ○ my-app      │ │                                          │
                   │ └──────────────────────────────────────────┘
 ❯ ctrl+b : prompt the agent · @path opens a file or folder · / commands
 my-app ~/projects/my-app  idle            midnight · ctrl+b prompt · prefix+? help
```

Type in the pane exactly as you would in a terminal. Everything that is not a
pane is reached through the **prefix**: `ctrl+b` by default. Press it once, then
the next key is a command.

| try this | what happens |
|---|---|
| `ctrl+b ?` | the keybinding list |
| `ctrl+b v` | split the pane side by side |
| `ctrl+b s` | split it top and bottom |
| `ctrl+b h/j/k/l` | move focus between panes |
| `ctrl+b c` | a new tab |
| `ctrl+b t` | the theme menu, with live preview |
| `ctrl+b :` | the composer — type a prompt for the agent |
| `ctrl+b q` | detach |

Detaching leaves the panes running. Run `herdstead` in the folder again and you
are back, with the same output, the same folders and the same split layout.

## Talk to an agent

Start your agent in a pane (`claude`, `codex`, `gemini`, …) and herdstead will
label it in the sidebar with a status: `●` working, `◆` blocked on you, `○` idle.
Then use the composer instead of typing into the pane:

```
ctrl+b :  explain @src/render/markdown.rs and add a test for tables
```

`@` completes paths from the pane's folder; `Tab` accepts, `Enter` sends. The
composer renders inline Markdown, validates references as you type (a missing one
turns red) and can preview itself as rendered Markdown with `ctrl+p`.

## Look at something

Click a path in the output while holding the link modifier (Cmd on macOS), or use
the composer:

```
/tree src
/md README.md
/cat Cargo.toml
```

Each opens a viewer pane beside the shell: a folder tree, rendered Markdown, or a
syntax-highlighted file. Viewers scroll with `j`/`k`, `g`/`G`, and close with `q`.

## Without a TUI

The same renderers are available as ordinary commands, so they work in scripts
and pipes:

```bash
herdstead cat src/main.rs        # syntax highlighting, line numbers
herdstead tree crates            # folder tree with sizes
herdstead md README.md           # rendered Markdown
herdstead preview assets/        # whatever the path is, as a viewer would
herdstead themes                 # palettes, with swatches
herdstead keys                   # keybindings
herdstead config                 # resolved configuration and paths
```

`herdstead` refuses to attach when stdin or stdout is not a terminal, before it
starts anything, and points at these commands instead.

## Where things live

| what | where |
|---|---|
| config | `~/.config/herdstead/config.toml` |
| themes | `~/.config/herdstead/themes/*.json` |
| saved layouts | `~/.local/state/herdstead/sessions/<name>.json` |
| daemon logs | `~/.local/state/herdstead/logs/<name>.log` |
| sockets | `/tmp/herdstead-<uid>/<name>.sock` |

Write a starter config with `herdstead init`; see
[configuration](configuration.md) for every key. To stop a session, keep its
layout, and remove its socket:

```bash
herdstead ls
herdstead kill my-app
```

Next: [sessions](sessions.md) explains what the daemon does and how restore
works; [the composer](composer.md) is the page to read if you mainly want to
prompt agents.
