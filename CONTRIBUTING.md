# Contributing to herdstead

Thanks for wanting to improve herdstead. This page covers the local workflow,
the layout of the code, and the conventions that keep the daemon and the client
in agreement. For the user-facing picture, start at [README.md](README.md) and
[docs/](docs/README.md).

## Prerequisites

- A stable Rust toolchain (edition 2021, Rust 1.82 or newer) with `rustfmt` and
  `clippy`.
- `git`.
- macOS or Linux. Windows is not supported: the daemon and client talk over a
  Unix socket, and the integration tests spawn PTYs.

For the terminal binary, no system libraries are needed. Syntax highlighting and PTY handling are pure
Rust.

The optional Tauri desktop app has separate prerequisites and checks in
[Desktop](docs/desktop.md).

## Getting started

```bash
git clone https://github.com/jaysonwu991/herdstead.git
cd herdstead
cargo build
cargo test --workspace
```

Run it from a project folder, in a real terminal:

```bash
cargo run --release            # attaches to (or starts) a session
cargo run -- cat README.md     # the CLI, no TUI needed
```

## Commands

| command | purpose |
|---|---|
| `cargo build --release` | the binary at `target/release/herdstead` |
| `cargo test --workspace` | the whole suite, including the daemon and PTY tests |
| `cargo test -p herdstead-core` | one crate, when iterating on the model or renderers |
| `cargo clippy --workspace --all-targets -- -D warnings` | lint; warnings are errors |
| `cargo fmt --all` | format |
| `cargo run -- ls` | inspect live sessions while developing |
| `cargo run -- kill <name>` | clean up a session a test or a run left behind |
| `scripts/set-version.sh v1.2.3` | stamp the workspace version from a tag, as release CI does |
| `scripts/test-install.sh` | run `install.sh` against fixtures, with no network |

The integration tests start their own daemons in a temporary state folder and
clean up after themselves, so they do not touch your real session.

## Layout

| path | contents |
|---|---|
| `crates/core` | geometry, layout tree, session model, protocol, themes, config, agent detection, `@path`, text scanning |
| `crates/render` | Markdown, syntax highlighting, trees, link decoration, the composer, ANSI output |
| `crates/app/src/server` | the daemon: PTY panes, grid extraction, key encoding, snapshots |
| `crates/app/src/tui` | the client: state, chrome layout, drawing, viewers |
| `desktop/` | Tauri native host, static desktop UI, packaging and desktop integration tests |
| `crates/app/src/desktop.rs` | desktop transport over the existing daemon client |
| `crates/app/src/cli.rs` | the non-interactive commands |
| `crates/app/tests` | the daemon and PTY end-to-end tests |
| `docs/` | user documentation |
| `assets/` | the mark and lockups |
| `install.sh`, `scripts/` | the curl-to-sh installer and the helpers CI and releases call |

`core` and `render` do not depend on a terminal library. Keep it that way: a
renderer takes text and a theme and returns styled rows, so the TUI and the CLI
draw the same thing.

## The boundary rule

The daemon owns **runtime facts**: panes, processes, layout, focus, scrollback
offsets, folders, agent status. The client owns **presentation**: theme, composer
state, overlays, viewer scroll and the pointer.

Do not put a UI concept into the protocol, and do not keep a runtime fact only in
the client. If a change needs both, extend `Action` (client → daemon) or the
`Snapshot` (daemon → client) deliberately, with neutral names, and document it.

## Tests

Three layers, and new behaviour belongs in the right one:

1. **Unit tests** next to the code in `core` and `render` — geometry, parsing,
   wrapping, themes, config, renderers. Most changes need one.
2. **`tests/runtime.rs`** — a real daemon: start it, connect, send actions, assert
   snapshots. Use it for protocol, PTY, scrollback, restore and viewer changes.
3. **`tests/client.rs`** — the real TUI in a PTY: keys, drawing, the composer,
   viewers, detach and resize. Use it when the change is visible.

A change to key handling is not verified by a unit test alone: the end-to-end
tests are what catch a key that never reaches the pane.

## Conventions

- **British English** in user-facing text and docs (`colour`, `recognise`,
  `behaviour`), matching the existing pages.
- **No HTML in Markdown.** herdstead renders its own documentation; raw HTML
  would show up as text. Use tables, lists and fenced blocks.
- **Comments explain why**, not what. Keep them short.
- **Keep the docs true.** A new setting, action, slash command, theme slot or
  viewer kind is not finished until the matching page in `docs/` says so, and the
  README's feature table still reads correctly.
- **Pull request titles** follow [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat: …`, `fix(composer): …`, `docs: …`). The release drafter turns them into
  release notes and applies labels from the title.
- **One idea per commit.** Run `cargo fmt`, clippy and the tests before pushing.
- **No new dependency without a reason.** The renderer and the core are
  deliberately small; a dependency needs to earn its place and its build time.

## Adding a thing

| change | touch |
|---|---|
| an action | `config::ACTIONS`, `protocol::Action`, the server's `apply`, the client's `run_action`/`on_key`, `docs/keys.md` |
| a setting | `Config` + `Default`, `paths::DEFAULT_CONFIG`, `docs/configuration.md`, the README sample |
| a theme slot | the `Theme` field, the `slots!` list, `from_recipe`, `docs/themes.md` |
| a slash command | `tui::app::run_command`, `docs/composer.md` |
| a viewer kind | `core::model::ViewerKind`, `tui::viewer`, `docs/viewers.md` |
| a protocol frame | `core::protocol`, the server's `handle_request`/`apply`, the client's reader/writer, `docs/architecture.md` |

## Releases

Releases are tags. Labels are synced from `.github/workflows/labels.yml`; the
release drafter keeps a draft for the next version, built from merged pull
requests and their labels. To cut a release, publish the draft: that creates the
`v*` tag, and `release.yml` answers the tag — it re-checks the tree, stamps the
workspace version from the tag (`scripts/set-version.sh`), builds `herdstead` on
macOS (arm64, x64) and Linux (x64, arm64), and attaches
`herdstead-<platform>.tar.gz` plus a `.sha256` to the release — the archives that
`install.sh` downloads. There is no changelog file to update by hand.

## License

herdstead is Apache-2.0. By contributing you agree that your work is licensed
under the same terms.
