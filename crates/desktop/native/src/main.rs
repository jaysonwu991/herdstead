#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tauri::menu::{AboutMetadata, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::{Update, UpdaterExt};

use serde_json::{json, Value};

const MAX_FRAME: usize = 32 * 1024 * 1024;
#[cfg(target_os = "macos")]
const MACOS_RELAUNCH_SCRIPT: &str =
    r#"while /bin/kill -0 "$1" 2>/dev/null; do sleep 0.1; done; exec /usr/bin/open "$2""#;
type Reply = Result<Value, String>;

#[derive(Default)]
struct Inner {
    child: Option<Child>,
    stdin: Option<Arc<Mutex<ChildStdin>>>,
    pending: HashMap<u64, mpsc::SyncSender<Reply>>,
    latest: BTreeMap<String, Value>,
    next_id: u64,
    generation: u64,
}

struct Host {
    inner: Arc<Mutex<Inner>>,
    binary: PathBuf,
    cwd: PathBuf,
    session: String,
}

impl Host {
    fn stop(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.generation += 1;
        inner.stdin.take();
        if let Some(mut child) = inner.child.take() {
            // Only the bridge is stopped. The daemon owns the live shells.
            let _ = child.kill();
            let _ = child.wait();
        }
        for (_, pending) in inner.pending.drain() {
            let _ = pending.send(Err(
                "Desktop connection closed. Reconnect to continue.".into()
            ));
        }
    }

    fn connect(&self, app: tauri::AppHandle) -> Result<(), String> {
        self.stop();
        let mut inner = self.inner.lock().unwrap();
        inner.latest.clear();
        let mut command = Command::new(&self.binary);
        command.env("HERDSTEAD_SHELL_HELPERS", "1");
        // A GUI launch may have a minimal PATH. Rich terminal commands must
        // still find the same bundled runtime as the desktop bridge.
        if let Some(parent) = self.binary.parent() {
            let mut paths = vec![parent.to_path_buf()];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ));
            if let Ok(path) = std::env::join_paths(paths) {
                command.env("PATH", path);
            }
        }
        if let Some(zsh) = ["/bin/zsh", "/usr/bin/zsh"]
            .iter()
            .find(|path| std::path::Path::new(path).is_file())
        {
            command.env("SHELL", zsh);
        }
        let mut child = command
            .args(["--session", &self.session, "desktop-bridge"])
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("Cannot start {}: {err}", self.binary.display()))?;
        inner.stdin = child.stdin.take().map(|stdin| Arc::new(Mutex::new(stdin)));
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        inner.child = Some(child);
        let generation = inner.generation;
        drop(inner);
        let errors = Arc::new(Mutex::new(String::new()));
        let error_tail = Arc::clone(&errors);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut buffer = [0; 4096];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                *error_tail.lock().unwrap() =
                    String::from_utf8_lossy(&buffer[..count]).into_owned();
            }
        });
        let state = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                match reader
                    .by_ref()
                    .take((MAX_FRAME + 1) as u64)
                    .read_until(b'\n', &mut bytes)
                {
                    Ok(0) | Err(_) => break,
                    Ok(count) if count > MAX_FRAME => break,
                    _ => {}
                }
                let Ok(packet) = serde_json::from_slice::<Value>(&bytes) else {
                    continue;
                };
                let mut inner = state.lock().unwrap();
                if inner.generation != generation {
                    return;
                }
                if packet["type"] == "reply" {
                    if let Some(id) = packet["id"].as_u64() {
                        if let Some(sender) = inner.pending.remove(&id) {
                            let result = match packet["error"].as_str() {
                                Some(error) => Err(error.to_string()),
                                None => Ok(packet["value"].clone()),
                            };
                            let _ = sender.send(result);
                        }
                    }
                } else {
                    if let Some(kind @ ("ready" | "snapshot" | "disconnected" | "detached")) =
                        packet["type"].as_str()
                    {
                        inner.latest.insert(kind.to_string(), packet.clone());
                    }
                    let _ = app.emit("herdstead:event", &packet);
                }
            }
            let mut inner = state.lock().unwrap();
            if inner.generation != generation {
                return;
            }
            inner.stdin.take();
            if let Some(mut child) = inner.child.take() {
                let _ = child.wait();
            }
            for (_, pending) in inner.pending.drain() {
                let _ = pending.send(Err("The runtime connection closed.".into()));
            }
            let error = errors.lock().unwrap();
            let message = if error.is_empty() {
                "Disconnected. Your session may still be running."
            } else {
                &error
            };
            let packet = json!({"type": "disconnected", "message": message});
            inner.latest.insert("disconnected".into(), packet.clone());
            let _ = app.emit("herdstead:event", packet);
        });
        Ok(())
    }
}

#[tauri::command]
async fn bridge_request(state: State<'_, Host>, mut message: Value) -> Reply {
    if !matches!(
        message["type"].as_str(),
        Some("action" | "directory" | "viewer" | "preview" | "theme" | "rich_output")
    ) {
        return Err("Unknown desktop request".into());
    }
    let shared = Arc::clone(&state.inner);
    let (sender, receiver) = mpsc::sync_channel(1);
    let id;
    let stdin;
    let mut bytes;
    {
        let mut inner = shared.lock().unwrap();
        if inner.pending.len() >= 128 {
            return Err("Too many pending desktop requests".into());
        }
        inner.next_id += 1;
        id = inner.next_id;
        message["id"] = json!(id);
        bytes = serde_json::to_vec(&message).map_err(|err| err.to_string())?;
        if bytes.len() >= MAX_FRAME {
            return Err("Desktop message exceeds the frame limit".into());
        }
        bytes.push(b'\n');
        stdin = inner
            .stdin
            .clone()
            .ok_or("The desktop connection is closed. Reconnect to continue.")?;
        inner.pending.insert(id, sender);
    }
    // A large paste must not hold the event-state lock while filling the pipe:
    // the reader needs that lock to drain replies and keep the bridge moving.
    let result = tauri::async_runtime::spawn_blocking(move || -> Reply {
        stdin
            .lock()
            .unwrap()
            .write_all(&bytes)
            .map_err(|err| err.to_string())?;
        receiver
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| "The desktop request timed out".to_string())?
    })
    .await
    .map_err(|err| err.to_string())?;
    shared.lock().unwrap().pending.remove(&id);
    result
}

#[tauri::command]
fn bridge_state(state: State<'_, Host>) -> Vec<Value> {
    let inner = state.inner.lock().unwrap();
    ["ready", "snapshot", "detached", "disconnected"]
        .iter()
        .filter_map(|key| inner.latest.get(*key).cloned())
        .collect()
}

#[tauri::command]
fn bridge_reconnect(app: tauri::AppHandle, state: State<'_, Host>) -> Result<(), String> {
    state.connect(app)
}

#[tauri::command]
async fn choose_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Add a workspace")
            .blocking_pick_folder()
            .map(|path| path.to_string())
    })
    .await
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn open_external(app: tauri::AppHandle, target: String) -> Result<(), String> {
    let url = tauri::Url::parse(&target).map_err(|err| err.to_string())?;
    if !matches!(url.scheme(), "https" | "http" | "mailto") {
        return Err("This URL scheme is not supported".into());
    }
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|err| err.to_string())
}

#[derive(Default)]
struct PendingUpdate(Mutex<Option<Update>>);

#[tauri::command]
async fn check_update(app: tauri::AppHandle, state: State<'_, PendingUpdate>) -> Reply {
    let update = app
        .updater_builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?
        .check()
        .await
        .map_err(|err| err.to_string())?;
    let metadata = update
        .as_ref()
        .map(|update| json!({"version": update.version}));
    *state.0.lock().unwrap() = update;
    Ok(json!(metadata))
}

#[tauri::command]
async fn install_update(
    app: tauri::AppHandle,
    state: State<'_, PendingUpdate>,
) -> Result<(), String> {
    let update = state
        .0
        .lock()
        .unwrap()
        .take()
        .ok_or("Check for updates first")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|err| err.to_string())?;
    app.state::<Host>().stop();
    #[cfg(target_os = "macos")]
    {
        relaunch_after_update().map_err(|err| {
            format!("The update was installed but Herdstead could not restart: {err}")
        })?;
        app.exit(0);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        app.restart()
    }
}

#[cfg(target_os = "macos")]
fn relaunch_after_update() -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let binary = std::env::current_exe().map_err(|err| err.to_string())?;
    let bundle =
        macos_bundle_path(&binary).ok_or("Cannot locate the updated application bundle")?;
    macos_relaunch_command(std::process::id(), bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_bundle_path(binary: &std::path::Path) -> Option<&std::path::Path> {
    let macos = binary.parent()?;
    if macos.file_name()? != "MacOS" {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    contents.parent()
}

#[cfg(target_os = "macos")]
fn macos_relaunch_command(pid: u32, bundle: &std::path::Path) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(MACOS_RELAUNCH_SCRIPT)
        .arg("herdstead-updater")
        .arg(pid.to_string())
        .arg(bundle);
    command
}

#[tauri::command]
fn set_zoom(window: tauri::WebviewWindow, scale: f64) -> Result<(), String> {
    if !scale.is_finite() {
        return Err("The zoom level is not a number".into());
    }
    window
        .set_zoom(scale.clamp(0.7, 1.6))
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn toggle_fullscreen(window: tauri::WebviewWindow) -> Result<(), String> {
    let fullscreen = window.is_fullscreen().map_err(|err| err.to_string())?;
    window
        .set_fullscreen(!fullscreen)
        .map_err(|err| err.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(PendingUpdate::default())
        .menu(|app| {
            let menu = Menu::default(app)?;
            let application = Submenu::with_items(
                app,
                "Herdstead",
                true,
                &[
                    &PredefinedMenuItem::about(
                        app,
                        Some("About Herdstead"),
                        Some(AboutMetadata {
                            name: Some("Herdstead".into()),
                            version: Some(app.package_info().version.to_string()),
                            ..Default::default()
                        }),
                    )?,
                    &PredefinedMenuItem::separator(app)?,
                    &MenuItem::with_id(
                        app,
                        "check-for-updates",
                        "Check for Updates…",
                        true,
                        None::<&str>,
                    )?,
                    &MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?,
                    &PredefinedMenuItem::separator(app)?,
                    #[cfg(target_os = "macos")]
                    &PredefinedMenuItem::services(app, None)?,
                    #[cfg(target_os = "macos")]
                    &PredefinedMenuItem::separator(app)?,
                    #[cfg(target_os = "macos")]
                    &PredefinedMenuItem::hide(app, Some("Hide Herdstead"))?,
                    #[cfg(target_os = "macos")]
                    &PredefinedMenuItem::hide_others(app, None)?,
                    #[cfg(target_os = "macos")]
                    &PredefinedMenuItem::show_all(app, None)?,
                    #[cfg(target_os = "macos")]
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::quit(app, Some("Quit Herdstead"))?,
                ],
            )?;
            #[cfg(target_os = "macos")]
            menu.remove_at(0)?;
            menu.insert(&application, 0)?;
            // The default View menu only exists on macOS; Linux gets the same
            // zoom entries plus a full-screen item the toolkit lacks.
            let view = Submenu::with_items(
                app,
                "View",
                true,
                &[
                    &MenuItem::with_id(app, "zoom-in", "Zoom In", true, Some("CmdOrCtrl+="))?,
                    &MenuItem::with_id(app, "zoom-out", "Zoom Out", true, Some("CmdOrCtrl+-"))?,
                    &MenuItem::with_id(
                        app,
                        "zoom-reset",
                        "Actual Size",
                        true,
                        Some("CmdOrCtrl+0"),
                    )?,
                    &PredefinedMenuItem::separator(app)?,
                    #[cfg(target_os = "macos")]
                    &PredefinedMenuItem::fullscreen(app, Some("Enter Full Screen"))?,
                    #[cfg(not(target_os = "macos"))]
                    &MenuItem::with_id(
                        app,
                        "toggle-fullscreen",
                        "Toggle Full Screen",
                        true,
                        Some("F11"),
                    )?,
                ],
            )?;
            let mut view_slot = None;
            let mut window_slot = None;
            for (index, item) in menu.items()?.into_iter().enumerate() {
                if let MenuItemKind::Submenu(submenu) = item {
                    match submenu.text()?.as_str() {
                        "View" => view_slot = Some(index),
                        "Window" => window_slot = Some(index),
                        _ => {}
                    }
                }
            }
            if let Some(index) = view_slot {
                menu.remove_at(index)?;
                menu.insert(&view, index)?;
            } else if let Some(index) = window_slot {
                menu.insert(&view, index)?;
            } else {
                menu.append(&view)?;
            }
            Ok(menu)
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "toggle-fullscreen" {
                if let Some(window) = app.get_webview_window("main") {
                    let fullscreen = window.is_fullscreen().unwrap_or(false);
                    let _ = window.set_fullscreen(!fullscreen);
                }
                return;
            }
            let command = match event.id().as_ref() {
                "settings" => "settings",
                "check-for-updates" => "check-for-updates",
                "zoom-in" => "zoom-in",
                "zoom-out" => "zoom-out",
                "zoom-reset" => "zoom-reset",
                _ => return,
            };
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.emit("herdstead:command", command);
            }
        })
        .invoke_handler(tauri::generate_handler![
            bridge_request,
            bridge_state,
            bridge_reconnect,
            choose_folder,
            open_external,
            set_zoom,
            toggle_fullscreen,
            check_update,
            install_update
        ])
        .setup(|app| {
            let binary = std::env::var_os("HERDSTEAD_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    if cfg!(debug_assertions) {
                        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries/herdstead")
                    } else {
                        app.path()
                            .resource_dir()
                            .expect("application resources")
                            .join("herdstead")
                    }
                });
            let cwd = std::env::var_os("HERDSTEAD_CWD")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::current_dir()
                        .ok()
                        .filter(|path| path != &PathBuf::from("/"))
                })
                .unwrap_or(app.path().home_dir()?);
            let session = std::env::var("HERDSTEAD_SESSION").unwrap_or_else(|_| {
                cwd.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            let host = Host {
                inner: Arc::new(Mutex::new(Inner::default())),
                binary,
                cwd,
                session,
            };
            if let Err(message) = host.connect(app.handle().clone()) {
                host.inner.lock().unwrap().latest.insert(
                    "disconnected".into(),
                    json!({"type": "disconnected", "message": message}),
                );
            }
            app.manage(host);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("building Herdstead desktop")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<Host>().stop();
            }
        });
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn macos_relaunch_waits_before_opening_the_bundle_without_a_new_instance() {
        let bundle = std::path::Path::new("/Applications/Herdstead.app");
        let command = macos_relaunch_command(42, bundle);
        let args: Vec<_> = command.get_args().collect();

        assert_eq!(
            args,
            [
                OsStr::new("-c"),
                OsStr::new(MACOS_RELAUNCH_SCRIPT),
                OsStr::new("herdstead-updater"),
                OsStr::new("42"),
                bundle.as_os_str(),
            ]
        );
        assert!(!args.iter().any(|arg| *arg == OsStr::new("-n")));
    }

    #[test]
    fn macos_bundle_path_only_accepts_app_bundle_executables() {
        assert_eq!(
            macos_bundle_path(std::path::Path::new(
                "/Applications/Herdstead.app/Contents/MacOS/Herdstead"
            )),
            Some(std::path::Path::new("/Applications/Herdstead.app"))
        );
        assert_eq!(
            macos_bundle_path(std::path::Path::new("/tmp/Herdstead")),
            None
        );
    }
}
