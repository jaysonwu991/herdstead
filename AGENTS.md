# AGENTS.md

Guidance for coding agents working in this repository. The user-facing docs are
in [README.md](README.md) and [docs/](docs/README.md); read the page that matches
your task before changing behaviour.

## What this is

herdstead is a Tauri desktop workspace for coding agents, with a bundled Rust
runtime and shell helpers. macOS and Linux only, over a Unix socket.

Three crates:

- `crates/core` — layout, session model, protocol, themes, config, detection,
  `@path`, text scanning. No terminal dependency.
- `crates/render` — Markdown, syntax highlighting, trees, links, composer, ANSI.
  Drawing is pure; file helpers and tree scanning perform IO.
- `crates/app` — daemon, desktop JSON bridge, viewers and bundled shell helpers.
- `desktop/` — Tauri native host and webview UI (separate Cargo workspace).

## Invariants

1. **Do not leak presentation into the runtime.** Theme, composer text, overlays,
   viewer scroll and the pointer stay in the client. Panes, PTYs, layout, focus,
   scrollback offsets, folders and agent status stay in the daemon. A new field
   on `Snapshot` or `Action` is a protocol change; name it neutrally and document
   it in `docs/architecture.md`.
2. **Renderers stay pure.** `markdown`, `inline`, `wrap`, `link`, `style`,
   `ansi` and `composer` take text and a theme; no terminal, no IO. File reading
   lives in `code::highlight_file`, `render_file`/`render_path`, `tree::scan`
   and the client's viewer.
3. **One palette.** A new colour is a slot in `Theme` (plus `slots!`, a default in
   `from_recipe`, and a row in `docs/themes.md`), never a literal in the UI.
4. **The two front-ends agree.** Anything drawn in a viewer must be drawable by
   the CLI from the same function, and vice versa.
5. **No HTML in Markdown.** herdstead renders its own docs; raw HTML shows as
   text.

## Verification

Run all three before claiming a change works:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`crates/app/tests/runtime.rs` starts real daemons over a Unix socket in a
temporary state directory; `desktop/tests` drives the bridge, real zsh PTYs
and browser UI.
They need no display and clean up after
themselves, but an interrupted run can leave a session behind. To inspect or
stop it, set `HERDSTEAD_CONFIG_DIR`, `HERDSTEAD_STATE_DIR` and
`HERDSTEAD_RUNTIME_DIR` to that test fixture's directories before running
`cargo run -- ls` or `cargo run -- kill <name>`. Normal CLI commands use your
own session directories and will not list isolated test sessions. If a change
touches keys, the composer, layout, viewers or the socket, the end-to-end tests are the
ones that prove it — unit tests are not enough.

Desktop host and browser checks are in `docs/desktop.md`. Release packaging checks:

```sh
python3 -m unittest discover -s scripts -p 'test_*.py'
```

## Conventions

- British English in user-facing text (`colour`, `recognise`).
- Comments say why. Keep them short.
- Conventional Commits in pull request titles; the release drafter reads them.
- A new setting is not done until `Config`, `DEFAULT_CONFIG`,
  `docs/configuration.md` and the README sample agree.
- A new action is not done until the keymap defaults, the server handler, the
  client dispatch and `docs/keys.md` agree.
- Keep the docs' stated test commands and file paths true.
- A release is not done until the drafter's install block, `release.yml`,
  Tauri config, the README and `docs/getting-started.md` name the same platforms
  and desktop installers.
- Do not commit `target/`, editor state, or a user's real config/state folders.

## Things not to do

- Do not add a runtime dependency on a terminal library to `core` or `render`.
- Do not read the user's `~/.config/herdstead` or `~/.local/state/herdstead` from
  tests; use `HERDSTEAD_CONFIG_DIR`, `HERDSTEAD_STATE_DIR` and
  `HERDSTEAD_RUNTIME_DIR` with a temporary folder, as the existing tests do.
- Do not lower the honesty of the docs to match a limitation; fix the limitation
  or document it in `docs/troubleshooting.md`.
