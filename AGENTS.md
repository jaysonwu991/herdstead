# AGENTS.md

Guidance for coding agents working in this repository. The user-facing docs are
in [README.md](README.md) and [docs/](docs/README.md); read the page that matches
your task before changing behaviour.

## What this is

herdstead is a terminal workspace manager for coding agents, in Rust. One binary
with two roles: a TUI client and a session daemon that owns the panes. macOS and
Linux only, over a Unix socket.

Three crates:

- `crates/core` — layout, session model, protocol, themes, config, detection,
  `@path`, text scanning. No terminal dependency.
- `crates/render` — Markdown, syntax highlighting, trees, links, composer, ANSI.
  Pure: text plus a theme in, styled rows out.
- `crates/app` — the daemon (`src/server`), the client (`src/tui`) and the CLI.

## Invariants

1. **Do not leak presentation into the runtime.** Theme, composer text, overlays,
   viewer scroll and the pointer stay in the client. Panes, PTYs, layout, focus,
   scrollback offsets, folders and agent status stay in the daemon. A new field
   on `Snapshot` or `Action` is a protocol change; name it neutrally and document
   it in `docs/architecture.md`.
2. **Renderers stay pure.** `markdown`, `inline`, `wrap`, `link`, `style`,
   `ansi` and `composer` take text and a theme; no terminal, no IO. File reading
   lives in `code::highlight_file`, `tree::scan` and the client's viewer.
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

`tests/runtime.rs` starts real daemons (over a Unix socket, in a temp state dir);
`tests/client.rs` runs the TUI in a PTY. They need no display and clean up after
themselves, but they do spawn processes: a failure leaves a session behind, which
`cargo run -- ls` shows and `cargo run -- kill <name>` clears. If a change touches
keys, the composer, layout, viewers or the socket, the end-to-end tests are the
ones that prove it — unit tests are not enough.

`install.sh` has its own check, offline against fixture archives:

```sh
sh scripts/test-install.sh
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
  `install.sh`, the README and `docs/getting-started.md` name the same platforms
  and the same install path.
- Do not commit `target/`, editor state, or a user's real config/state folders.

## Things not to do

- Do not add a runtime dependency on a terminal library to `core` or `render`.
- Do not read the user's `~/.config/herdstead` or `~/.local/state/herdstead` from
  tests; use `HERDSTEAD_CONFIG_DIR`, `HERDSTEAD_STATE_DIR` and
  `HERDSTEAD_RUNTIME_DIR` with a temporary folder, as the existing tests do.
- Do not lower the honesty of the docs to match a limitation; fix the limitation
  or document it in `docs/troubleshooting.md`.
