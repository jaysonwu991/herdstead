//! The desktop client transport. Presentation stays here, never in the daemon.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use herdstead_core::config::Config;
use herdstead_core::geom::Rect;
use herdstead_core::model::Size;
use herdstead_core::paths::Paths;
use herdstead_core::protocol::{encode_line, Action, Event, PaneFrame, Request, MAX_FRAME_BYTES};
use herdstead_core::Theme;
use herdstead_render::{link, markdown, term, Line};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{client, tui::viewer};

#[derive(Deserialize)]
struct Message {
    id: u64,
    #[serde(flatten)]
    command: Command,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Command {
    Action { action: Action },
    Directory { path: PathBuf },
    Viewer { pane: Box<PaneFrame>, width: u16 },
    Preview { text: String, width: u16 },
    Theme { name: String },
    RichOutput { enabled: bool },
}

fn emit(value: &Value) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn palette(theme: &Theme) -> Value {
    Theme::SLOTS
        .iter()
        .filter_map(|name| {
            theme
                .slot(name)
                .map(|rgb| (name.to_string(), json!(rgb.to_hex())))
        })
        .collect()
}

// Serialise the shared renderer's output, rather than implementing highlighting
// or Markdown a second time in JavaScript.
fn rows(lines: Vec<Line>) -> Value {
    json!(lines.into_iter().map(|line| {
        let mut column = 0;
        let mut space = None;
        let mut previous_link = None;
        let mut badges = std::collections::BTreeSet::new();
        line.spans.into_iter().map(|span| {
            let style = span.style;
            let link = span.link.as_ref().map(|target| {
                let cell = space.unwrap_or(usize::MAX);
                let first = previous_link.as_ref() != Some(target);
                let offset = (first && badges.insert(cell)).then_some(space.map_or(column + 1, |space| column - space));
                json!({"target": target.target(), "url": target.is_url(), "badge_offset": offset})
            });
            previous_link = span.link.clone();
            for ch in span.text.chars() {
                if ch.is_whitespace() { space = Some(column); }
                column += herdstead_core::text::char_width(ch);
            }
            json!({
                "text": span.text,
                "fg": style.fg.map(|c| c.to_hex()), "bg": style.bg.map(|c| c.to_hex()),
                "bold": style.bold, "dim": style.dim, "italic": style.italic,
                "underline": style.underline, "strike": style.strikethrough,
                "inverse": style.reversed, "link": link
            })
        }).collect::<Vec<_>>()
    }).collect::<Vec<_>>())
}

fn directory(path: &Path, config: &Config) -> Result<Value> {
    let root = path
        .canonicalize()
        .with_context(|| format!("opening {}", path.display()))?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&root)?.take(config.tree_max_entries + 1) {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if config.tree_ignores.contains(&name) {
            continue;
        }
        // A symlink is navigable, but never followed recursively by this scan.
        entries
            .push(json!({"name": name, "path": entry.path(), "directory": entry.path().is_dir()}));
    }
    entries.sort_by(|a, b| {
        b["directory"]
            .as_bool()
            .cmp(&a["directory"].as_bool())
            .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
    });
    let truncated = entries.len() > config.tree_max_entries;
    entries.truncate(config.tree_max_entries);
    Ok(json!({"path": root, "entries": entries, "truncated": truncated}))
}

pub fn run(
    session: String,
    cwd: PathBuf,
    paths: Paths,
    config: Config,
    theme: Theme,
) -> Result<()> {
    let mut connection = client::connect_or_spawn(&paths, &session, &cwd)?;
    connection.send(&Request::Hello {
        session,
        size: Size {
            rows: 40,
            cols: 120,
        },
        area: Rect::new(0, 0, 120, 40),
    })?;
    let mut writer = connection.stream.try_clone()?;
    let theme = Arc::new(Mutex::new(theme));
    let initial_theme = theme.lock().unwrap();
    emit(&json!({"type": "ready", "theme": initial_theme.name,
        "palette": palette(&initial_theme),
        "themes": Theme::catalog(&paths.themes_dir()).iter().map(|t| &t.name).collect::<Vec<_>>(),
        "cwd": cwd}))?;
    drop(initial_theme);
    let render_theme = Arc::clone(&theme);
    let rich = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let rich_reader = Arc::clone(&rich);
    let links = config.links;
    let reader = std::thread::spawn(move || loop {
        let result = match connection.next_event() {
            Ok(Event::Snapshot { snapshot }) => {
                let theme = render_theme.lock().unwrap();
                let screens: serde_json::Map<String, Value> = snapshot
                    .panes
                    .iter()
                    .filter_map(|pane| {
                        pane.grid.as_ref().map(|grid| {
                            let mut lines =
                                if rich_reader.load(std::sync::atomic::Ordering::Relaxed) {
                                    term::rich_lines(grid, &theme)
                                } else {
                                    term::term_lines(grid, &theme)
                                };
                            let base = Path::new(&pane.cwd);
                            term::decorate_input(grid, &mut lines, &theme, links, &|path| {
                                base.join(path).exists()
                            });
                            if links {
                                lines = lines
                                    .iter()
                                    .map(|line| {
                                        link::decorate(line, &theme, &|path| {
                                            base.join(path).exists()
                                        })
                                    })
                                    .collect();
                            }
                            (pane.id.0.to_string(), rows(lines))
                        })
                    })
                    .collect();
                emit(&json!({"type": "snapshot", "snapshot": snapshot, "screens": screens}))
            }
            Ok(event) => emit(&serde_json::to_value(event).unwrap()),
            Err(err) => {
                let _ = emit(&json!({"type": "disconnected", "message": err.to_string()}));
                break;
            }
        };
        if result.is_err() {
            break;
        }
    });

    let result = (|| -> Result<()> {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        loop {
            let mut bytes = Vec::new();
            // Limit before allocating the whole frame, including malformed input.
            let read = io::Read::take(&mut input, (MAX_FRAME_BYTES + 1) as u64)
                .read_until(b'\n', &mut bytes)?;
            if read == 0 {
                break;
            }
            if read > MAX_FRAME_BYTES {
                bail!("desktop message exceeds the frame limit");
            }
            let message: Message = match serde_json::from_slice(&bytes) {
                Ok(message) => message,
                Err(err) => {
                    let error = format!("invalid desktop message: {err}");
                    let id = serde_json::from_slice::<Value>(&bytes)
                        .ok()
                        .and_then(|value| value["id"].as_u64());
                    if let Some(id) = id {
                        emit(&json!({"type": "reply", "id": id, "error": error}))?;
                    } else {
                        emit(&json!({"type": "error", "message": error}))?;
                    }
                    continue;
                }
            };
            let result: Result<Value> = (|| match message.command {
                Command::Action { action } => {
                    writer.write_all(&encode_line(&Request::Action { action })?)?;
                    Ok(Value::Null)
                }
                Command::RichOutput { enabled } => {
                    rich.store(enabled, std::sync::atomic::Ordering::Relaxed);
                    writer.write_all(&encode_line(&Request::Refresh)?)?;
                    Ok(Value::Null)
                }
                Command::Directory { path } => directory(&path, &config),
                Command::Viewer { pane, width } => {
                    if let Some(spec) = &pane.viewer {
                        if spec.path.is_file()
                            && std::fs::metadata(&spec.path)?.len() > 4 * 1024 * 1024
                        {
                            bail!("viewer limit is 4 MiB; use a terminal command for this file");
                        }
                    }
                    let lines = viewer::render(
                        &pane,
                        width.clamp(4, 1000),
                        &theme.lock().unwrap(),
                        &config,
                    )
                    .map_err(anyhow::Error::msg)?;
                    Ok(rows(lines))
                }
                Command::Preview { text, width } => Ok(rows(markdown::render_width(
                    &text,
                    usize::from(width.clamp(4, 1000)),
                    &theme.lock().unwrap(),
                ))),
                Command::Theme { name } => {
                    let mut selected = Theme::load(&name, &paths.themes_dir());
                    selected
                        .apply_overrides(&config.theme_colors)
                        .map_err(anyhow::Error::msg)?;
                    let response = json!({"name": selected.name, "palette": palette(&selected)});
                    *theme.lock().unwrap() = selected;
                    writer.write_all(&encode_line(&Request::Refresh)?)?;
                    Ok(response)
                }
            })();
            let reply = match result {
                Ok(value) => json!({"type": "reply", "id": message.id, "value": value}),
                Err(err) => json!({"type": "reply", "id": message.id, "error": err.to_string()}),
            };
            emit(&reply)?;
        }
        Ok(())
    })();
    // Wake the reader and finish its current JSON frame before exiting. EOF
    // detaches just this client; shells remain owned by the daemon.
    let _ = writer.shutdown(std::net::Shutdown::Both);
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("desktop event reader panicked"))?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdstead_render::{Link, Span, Style};

    #[test]
    fn link_badges_use_existing_cells_and_never_change_source() {
        let target = Link::Url("https://github.com".into());
        let mut line = Line::plain("日本 ");
        line.push(Span::plain("https://git").linked(target.clone()));
        line.push(Span::new("hub.com", Style::plain().bold()).linked(target));
        let data = rows(vec![line]);
        let spans = data[0].as_array().unwrap();
        assert_eq!(
            spans
                .iter()
                .map(|span| span["text"].as_str().unwrap())
                .collect::<String>(),
            "日本 https://github.com"
        );
        assert_eq!(spans[1]["link"]["badge_offset"], 1);
        assert!(spans[2]["link"]["badge_offset"].is_null());
    }
}
