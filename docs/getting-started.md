# Getting started

## Download and install

Open [Releases](https://github.com/jaysonwu991/herdstead/releases/latest) and
choose your platform:

| computer | download |
|---|---|
| macOS Apple Silicon | `Herdstead-macos-arm64.dmg` |
| macOS Intel | `Herdstead-macos-x64.dmg` |
| Linux x64 | `Herdstead-linux-x64.AppImage` or `Herdstead-linux-x64.deb` |
| Linux arm64 | `Herdstead-linux-arm64.AppImage` or `Herdstead-linux-arm64.deb` |

On macOS, open the DMG and drag Herdstead to `/Applications`, then launch it.
The current packages are not Apple Developer ID signed or notarised. If macOS
says “Herdstead.app is damaged and can’t be opened”, follow the download
verification and app-specific quarantine steps in
[first-launch troubleshooting](troubleshooting.md#first-launch-on-macos).

On Linux, download the AppImage, make it executable and run it:

```sh
chmod +x Herdstead-linux-x64.AppImage
./Herdstead-linux-x64.AppImage
```

Substitute `arm64` for an ARM computer. AppImages need FUSE 2 support; on
Ubuntu 22.04 install `libfuse2`. Alternatively use `--appimage-extract-and-run`.
For Debian/Ubuntu packages:

```sh
sudo apt install ./Herdstead-linux-x64.deb
```

The package manager resolves WebKitGTK 4.1 and other desktop dependencies.
Install zsh for rich shell integration (`sudo apt install zsh`). Prebuilt Linux
packages require glibc; Alpine/musl and Windows are not supported.

For download verification, fetch `SHA256SUMS` from the same release and compare
the installer's `shasum -a 256` (macOS) or `sha256sum` (Linux) with its entry.
The runtime is bundled inside the app; no shell installer or CLI is needed.

## Your first workspace

Launch Herdstead. Use **+** beside Spaces to select a project folder and
start `claude`, `codex`, `gemini` or another agent in a terminal pane.
The sidebar shows detected agents and their status.

| try this | result |
|---|---|
| Cmd/Ctrl+D | split right |
| Cmd/Ctrl+Shift+D | split down |
| Cmd/Ctrl+T | new terminal tab |
| Cmd/Ctrl+Shift+P | command palette |
| Cmd/Ctrl+, | settings and updates |
| click a file in Files | open a rich viewer |

Your zsh startup files load normally. Type `cat @REA` and press Tab to complete
a file reference; Enter expands the reference to a safely quoted path and runs
the command. `cat README.md` renders Markdown, `cat src/main.rs` highlights code,
and `tree .` draws a folder tree. Existing aliases are preserved; `hcat` and
`hmd` explicitly select the rich readers.

Close the window to leave the session running. Reopen with the same launch
folder/session to reconnect. Saved layouts survive a daemon restart, but shells
restart as fresh processes. See [sessions](sessions.md).

## Updates

Open **Settings → Check for updates**. If a newer version is available, choose
**Install update and restart**. The app downloads and verifies the signed update
before replacing the installed app and restarting. Daemons and panes stay running.
Existing daemons continue using their original runtime until you stop the session.
Install to a writable location, such as your user's Applications folder if a
system-wide Applications folder is not writable.

Linux in-app updates apply to AppImages. For `.deb` installs, download the newer
package and run `sudo apt install ./Herdstead-linux-x64.deb` again.
The update check requires an internet connection; failures appear in Settings
and can be retried. Before the first desktop release, the update endpoint is
unavailable. For source builds see [Desktop](desktop.md).
