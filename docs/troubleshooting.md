# Troubleshooting

## First launch on macOS

The current DMGs are not Apple Developer ID signed or notarised. macOS may
block first launch with “Herdstead.app is damaged and can’t be opened. You should
move it to the Trash.” This can be a Gatekeeper quarantine block on a downloaded
app, rather than a damaged download. The message alone does not confirm the cause.

1. Choose **Cancel**. Download the installer and `SHA256SUMS` from the same
   [official release](https://github.com/jaysonwu991/herdstead/releases/latest).
   Check the DMG before overriding the block. For Apple Silicon:

   ```sh
   cd ~/Downloads
   shasum -a 256 Herdstead-macos-arm64.dmg
   ```

   Compare the result with that filename's entry in `SHA256SUMS`; use
   `Herdstead-macos-x64.dmg` on Intel. If the checksum differs, download again
   and do not continue with that copy.
2. Open the DMG and drag **Herdstead.app** into `/Applications`. Run the installed
   copy, rather than the copy inside the mounted DMG.
3. Try opening it, then use **System Settings → Privacy & Security → Open Anyway**
   if that option appears.
4. If the same damaged-app message remains and you trust the verified download,
   open Terminal and remove the quarantine attribute from this app only:

   ```sh
   xattr -dr com.apple.quarantine "/Applications/Herdstead.app"
   open "/Applications/Herdstead.app"
   ```

   This removes the downloaded-app quarantine check for Herdstead and its bundled
   runtime; it does not sign or notarise the app. If installed in
   `~/Applications`, substitute `"$HOME/Applications/Herdstead.app"` in both
   commands. On a managed Mac, contact your administrator if permissions or
   security policy prevent this step.

If a fresh, checksum-verified copy still fails after these steps, report the
release version, Mac architecture, macOS version and exact error in
[an issue](https://github.com/jaysonwu991/herdstead/issues). Do not disable
Gatekeeper globally or bypass a warning that the app will damage your computer.
Updater signatures verify future updates; they do not replace Apple notarisation.

See [Apple's app security guidance](https://support.apple.com/en-gb/102445) and
[Tauri's macOS code signing guidance](https://v2.tauri.app/distribute/sign/macos/).

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
