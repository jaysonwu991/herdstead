# Contributing to Herdstead

Use a current stable Rust toolchain with rustfmt/clippy, Node.js 26, pnpm 12, git, and
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) on macOS or Linux.
Linux rich shell tests require zsh. Windows is not supported by the Unix runtime.

## Development

```sh
cargo build
cd desktop
pnpm install --frozen-lockfile
pnpm run dev
```

The desktop bundles the runtime automatically. The root `cargo build` builds
that helper; it does not start a user interface. Follow [Desktop](docs/desktop.md)
for local native builds and environment overrides.

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo build
cd desktop
pnpm install --frozen-lockfile
pnpm run prepare:runtime
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm test
pnpm run test:e2e
```

Runtime tests and desktop bridge tests start real daemons with isolated config,
state and runtime directories. Browser E2E uses Chrome and replaces only native
Tauri IPC. Check native window behaviour separately. To inspect an interrupted
test session, use all three fixture directory overrides described in [AGENTS.md](AGENTS.md).

## Layout and conventions

`crates/core` owns the model, config, themes, layout and protocol. `crates/render`
provides pure styled-row renderers plus explicit file/tree IO helpers.
`crates/app/src/server` owns daemon facts; `desktop.rs` provides JSON transport;
`viewer.rs` reads viewer files; `cli.rs` provides bundled shell commands.
`desktop/src-tauri` is the native host and `desktop/src` the webview UI.

Keep presentation out of the daemon and terminal dependencies out of core/render.
Viewer and shell output must use the same renderer. Protocol changes require
neutral names and [architecture](docs/architecture.md) documentation. Use theme
slots rather than literal UI colours, British English, short comments explaining
why, and Conventional Commits PR titles. Do not commit build output or real user
configuration/state. Preserve the docs' no-raw-HTML convention.

## Desktop releases

Release Drafter prepares notes for merged PRs. Publish a draft, push a `v*` tag,
or manually dispatch **Desktop release** with an existing version tag. The tag
must point to code containing the desktop workflow. GitHub's default workflow
token does not trigger other workflows when it creates a tag/release; use manual
dispatch when publishing through such automation.

`scripts/set-version.sh v1.2.3` stamps the root runtime, native Cargo package,
Tauri config and desktop package manifest. CI refreshes both Cargo locks after stamping.
Native builds produce macOS arm64/x64 DMGs and updater app archives, and Linux
arm64/x64 AppImages and Debian packages. Linux ARM uses a native ARM runner.
The bundled runtime is built for the app's exact target.

Release CI requires `TAURI_SIGNING_PRIVATE_KEY` in repository Actions secrets,
and optionally `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Its public key is committed
in `desktop/src-tauri/tauri.conf.json`. Keep the private key safe: replacing it
breaks updates for existing installations. These signatures verify updater
artifacts; Apple code signing/notarisation requires separate credentials and is
not currently configured.

Packaging checks every platform's installer and signature before assembling
`latest.json` and `SHA256SUMS` and publishing assets. Stable platform names make
latest-download links reusable. Prereleases do not replace the latest stable
update. Reruns overwrite matching release assets. A failed build publishes no
new assets; an already published draft may remain visible without downloads
until the build succeeds.

Apache-2.0 — contributions are under the repository's licence.
