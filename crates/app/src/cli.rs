//! Subcommands that render into the caller's terminal instead of a session.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use herdstead_core::paths::Paths;
use herdstead_core::theme::Theme;
use herdstead_core::Config;
use herdstead_render::ansi::{self, AnsiOptions};
use herdstead_render::{code, markdown, tree, Line};

use crate::client::Connection;

/// Renderers report plain strings; lift them into `anyhow`.
fn lift<T>(result: Result<T, String>) -> Result<T> {
    result.map_err(anyhow::Error::msg)
}

fn width() -> usize {
    crossterm::terminal::size()
        .map(|(cols, _)| usize::from(cols).clamp(40, 200))
        .unwrap_or(100)
}

fn print_lines(lines: &[Line], theme: &Theme, links: bool) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    ansi::write(
        &mut stdout,
        lines,
        theme,
        &AnsiOptions {
            hyperlinks: links,
            reset_lines: true,
        },
    )
    .context("writing to the terminal")
}

pub fn cat(path: &Path, theme: &Theme, line_numbers: bool, links: bool) -> Result<()> {
    let options = code::CodeOptions {
        width: 500,
        line_numbers,
        background: false,
        rail: false,
        ..code::CodeOptions::default()
    };
    let lines = lift(code::highlight_file(path, &options, theme))?;
    print_lines(&lines, theme, links)
}

pub fn tree(path: &Path, theme: &Theme, depth: usize, all: bool) -> Result<()> {
    let mut options = tree::TreeOptions {
        max_depth: depth,
        ..tree::TreeOptions::default()
    };
    if all {
        options.ignores.clear();
    }
    let scan = lift(tree::scan(path, &options))?;
    let lines = tree::render(&scan, &options, theme, width());
    print_lines(&lines, theme, true)
}

pub fn markdown(path: &Path, theme: &Theme, stdin: bool) -> Result<()> {
    let source = if stdin || path.as_os_str() == "-" {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("reading stdin")?;
        buffer
    } else {
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
    };
    let lines = markdown::render_width(&source, width(), theme);
    print_lines(&lines, theme, true)
}

pub fn preview(path: &Path, theme: &Theme, config: &Config) -> Result<()> {
    let options = tree::TreeOptions {
        ignores: config.tree_ignores.clone(),
        max_entries: config.tree_max_entries,
        ..tree::TreeOptions::default()
    };
    if path.is_dir() {
        let scan = lift(tree::scan(path, &options))?;
        let lines = tree::render(&scan, &options, theme, width());
        return print_lines(&lines, theme, true);
    }
    let lines = lift(herdstead_render::render_file(
        path,
        width(),
        theme,
        &code::CodeOptions::default(),
    ))?;
    print_lines(&lines, theme, true)
}

pub fn themes(paths: &Paths, active: &Theme) {
    for theme in Theme::catalog(&paths.themes_dir()) {
        let marker = if theme.name == active.name {
            "●"
        } else {
            "○"
        };
        let swatch = format!(
            "{}{}▇▇▇{} {}{}▇▇▇{}",
            theme.bg.ansi_bg(),
            theme.bg.ansi_fg(),
            "\x1b[0m",
            theme.accent.ansi_bg(),
            theme.accent.ansi_fg(),
            "\x1b[0m"
        );
        println!(
            "{marker} {:<10} {:<6} {swatch}  {}",
            theme.name,
            theme.appearance_label(),
            theme.syntax
        );
    }
    println!();
    println!("themes live in {}", paths.themes_dir().display());
}

pub fn languages() {
    let mut names = code::supported_languages();
    names.sort();
    println!("{} languages", names.len());
    for chunk in names.chunks(4) {
        println!("  {}", chunk.join(", "));
    }
}

pub fn show_config(config: &Config, paths: &Paths, session: &str) {
    println!("session: {}", session);
    println!("config:  {}", paths.config_file().display());
    println!("themes:  {}", paths.themes_dir().display());
    println!("state:   {}", paths.state.display());
    println!("runtime: {}", paths.runtime.display());
    println!();
    match serde_json::to_string_pretty(config) {
        Ok(json) => println!("{json}"),
        Err(err) => eprintln!("config: {err}"),
    }
}

pub fn init(paths: &Paths) -> Result<()> {
    paths.ensure()?;
    match paths.ensure_default_config() {
        Ok(true) => println!("wrote {}", paths.config_file().display()),
        Ok(false) => println!("{} already exists", paths.config_file().display()),
        Err(err) => return Err(err).context("writing the starter config"),
    }
    Ok(())
}

pub fn list_sessions(paths: &Paths) -> Result<()> {
    let sessions = paths.sessions();
    if sessions.is_empty() {
        println!("no live sessions");
        return Ok(());
    }
    for session in sessions {
        match Connection::snapshot_once(paths, &session) {
            Ok(snapshot) => {
                let panes: usize = snapshot
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.panes)
                    .sum();
                let focused = snapshot
                    .focused
                    .and_then(|id| snapshot.panes.iter().find(|pane| pane.id == id))
                    .map(|pane| pane.title.clone())
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "{:<20} {:>2} workspaces  {:>3} panes  focused: {}",
                    session,
                    snapshot.workspaces.len(),
                    panes,
                    focused
                );
            }
            Err(err) => println!("{session:<20} (not responding: {err})"),
        }
    }
    Ok(())
}

pub fn kill_session(paths: &Paths, session: &str, forget: bool) -> Result<()> {
    match Connection::kill(paths, session) {
        Ok(()) => println!("stopped {session}"),
        Err(err) => {
            println!("{session} is not running ({err})");
            std::fs::remove_file(paths.socket(session)).ok();
        }
    }
    if forget {
        paths.remove_session_files(session);
        println!("forgot {session}");
    }
    Ok(())
}
