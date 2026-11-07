# Troubleshooting

## First launch on macOS

The current DMGs are not Apple Developer ID signed or notarised. macOS may block
first launch. After checking the download source and checksum, use System
Settings → Privacy & Security → Open Anyway when available. Managed Macs may
require your administrator's approval. Do not disable Gatekeeper globally.
Updater signatures verify future updates; they do not replace Apple notarisation.

## Linux installation

Use a glibc distribution with WebKitGTK 4.1. Debian packages resolve native
libraries through `apt`; the AppImage also needs FUSE 2, or can run with
`--appimage-extract-and-run`. Install zsh for rich shell completion and startup
integration. Alpine/musl and Windows are not supported.

## Update failures

Open Appearance and check again. The release's `latest.json` must be reachable,
contain your platform, and name a newer version. Before the first desktop
release, this endpoint is absent. A signature failure means the update does not
match the public key built into the installed app; install a trusted full
installer if the release key changed. Do not bypass signature verification.

Install the macOS app or Linux AppImage in a writable location. Debian installs
update through the package manager by installing the newer `.deb`. Running an
AppImage extracted outside its original image may require a fresh download.
The old CLI releases have no desktop updater metadata.

## Runtime connection and logs

Use **Reconnect to session** from the command palette or the reconnect banner.
Daemon logs live in `~/.local/state/herdstead/logs/<session>.log`, subject to XDG
and `HERDSTEAD_STATE_DIR` overrides. Saved layouts are in the adjacent `sessions`
folder. Sockets use `$XDG_RUNTIME_DIR/herdstead` or `herdstead-<uid>` in the system
temporary directory.

The bundled helper supports `herdstead ls` and `herdstead kill <name>` in desktop
terminals. Killing a session stops its processes but preserves the saved layout;
`--forget` removes that layout too. For isolated test daemons set all three
`HERDSTEAD_CONFIG_DIR`, `HERDSTEAD_STATE_DIR` and `HERDSTEAD_RUNTIME_DIR` to the
fixture directories before inspection. Normal helper commands use your own
session directories.

An app update leaves existing daemons running their original executable. Stop
a session when you are ready for its panes to restart on the new runtime.
Saved layouts restore folders and splits, not old process memory or output.

## Shell, theme and keys

Existing daemons keep the environment they started with. Start a new session to
enable new shell integration. An explicit `shell` in config takes precedence over
the desktop's zsh preference. Original aliases and plugins are preserved;
`hcat` and `hmd` explicitly select rich readers. Agent-owned input uses the
agent's own completion.

Use Appearance for desktop themes and fonts, and the bundled Nerd Font for
prompt glyphs. Old TUI prefix bindings and mouse/sidebar configuration do not
control the desktop. See [keys](keys.md) and [configuration](configuration.md).

## Viewers and output

Viewer files must be UTF-8 and no larger than 4 MiB. Source files containing
NULs are refused. Use the ordinary shell tools for binary or larger files.
Turn Rich output off for literal terminal rows; use `md` for complete Markdown
layout. Find searches visible output, not all scrollback. Mouse reporting is
reserved for desktop controls rather than forwarded to terminal programs.
