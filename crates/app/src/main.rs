//! Bundled desktop runtime and rich shell helpers.

mod cli;
mod client;
mod desktop;
mod server;
mod shell;
mod viewer;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use herdstead_core::config::Config;
use herdstead_core::paths::{default_session_name, sanitize_session, Paths};
use herdstead_core::theme::Theme;

#[derive(Debug, Parser)]
#[command(
    name = "herdstead",
    version,
    about = "Herdstead desktop runtime and shell helpers",
    long_about = "herdstead keeps coding agents running in panes you can split, tab and detach from. \
The desktop app bundles this runtime for sessions, JSON transport and rich shell commands."
)]
struct Args {
    /// Session to use (defaults to the current folder's name).
    #[arg(long, short = 's', global = true, env = "HERDSTEAD_SESSION")]
    session: Option<String>,

    /// Theme for this run: midnight, daylight, nord, gruvbox, mono, solarized, or a theme file.
    #[arg(long, global = true, env = "HERDSTEAD_THEME")]
    theme: Option<String>,

    /// Run the session daemon in the foreground (used internally by the desktop bridge).
    #[arg(long, hide = true)]
    server: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print zsh integration for `eval "$(herdstead shell-init)"`.
    ShellInit,
    /// Render Markdown from a file or piped command output.
    Render {
        #[arg(default_value = "-")]
        path: PathBuf,
    },
    /// JSON bridge used by the desktop application (no terminal required).
    #[command(hide = true)]
    DesktopBridge,
    /// List live sessions.
    Ls,
    /// Stop a session's daemon. The layout stays on disk unless --forget is given.
    Kill {
        #[arg(default_value = "")]
        session: String,
        /// Delete the saved layout as well.
        #[arg(long)]
        forget: bool,
    },
    /// Print a file with syntax highlighting, like an editor's buffer.
    Cat {
        path: PathBuf,
        #[arg(long)]
        no_line_numbers: bool,
        /// Allow OSC 8 output for any attached links (source highlighting adds none).
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        links: bool,
    },
    /// Print a folder tree.
    Tree {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Depth to descend.
        #[arg(long, default_value_t = 8)]
        depth: usize,
        /// Do not skip build folders.
        #[arg(long)]
        all: bool,
    },
    /// Render Markdown to the terminal.
    Md {
        path: PathBuf,
        /// Read stdin instead of the required path argument; `md -` also reads stdin.
        #[arg(long)]
        stdin: bool,
    },
    /// Render any file or folder the way a viewer pane would.
    Preview { path: PathBuf },
    /// List the available themes.
    Themes,
    /// List languages the highlighter knows.
    Languages,
    /// Print the resolved configuration.
    Config,
    /// Write the starter configuration if it is missing.
    Init,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let paths = Paths::discover();
    let cwd = std::env::current_dir().context("reading the current folder")?;
    let session = args
        .session
        .clone()
        .map(|name| sanitize_session(&name))
        .unwrap_or_else(|| default_session_name(&cwd));

    if args.server {
        return server::run(session);
    }

    let (config, warnings) = Config::load(&cwd, &paths);
    for warning in &warnings {
        eprintln!("herdstead: {warning}");
    }
    let theme = load_theme(&args.theme, &config, &paths);

    match args.command {
        Some(Command::ShellInit) => {
            print!("{}", shell::ZSH);
            Ok(())
        }
        Some(Command::Render { path }) => cli::markdown(&path, &theme, path.as_os_str() == "-"),
        Some(Command::DesktopBridge) => desktop::run(session, cwd, paths, config, theme),
        None => anyhow::bail!(
            "Open the Herdstead desktop app to use a workspace. This bundled helper has no TUI."
        ),
        Some(Command::Ls) => cli::list_sessions(&paths),
        Some(Command::Kill {
            session: name,
            forget,
        }) => {
            let name = if name.is_empty() {
                session
            } else {
                sanitize_session(&name)
            };
            cli::kill_session(&paths, &name, forget)
        }
        Some(Command::Cat {
            path,
            no_line_numbers,
            links,
        }) => cli::cat(&path, &theme, !no_line_numbers, links),
        Some(Command::Tree { path, depth, all }) => cli::tree(&path, &theme, depth, all),
        Some(Command::Md { path, stdin }) => cli::markdown(&path, &theme, stdin),
        Some(Command::Preview { path }) => cli::preview(&path, &theme, &config),
        Some(Command::Themes) => {
            cli::themes(&paths, &theme);
            Ok(())
        }
        Some(Command::Languages) => {
            cli::languages();
            Ok(())
        }
        Some(Command::Config) => {
            cli::show_config(&config, &paths, &session);
            Ok(())
        }
        Some(Command::Init) => cli::init(&paths),
    }
}

/// Resolves a theme name, applies the configured colour overrides and warns
/// about a name that does not exist instead of silently drawing another one.
pub fn load_theme(name: &Option<String>, config: &Config, paths: &Paths) -> Theme {
    let requested = name.clone().unwrap_or_else(|| config.theme.clone());
    let mut theme = Theme::load(&requested, &paths.themes_dir());
    if !requested.eq_ignore_ascii_case(&theme.name) {
        // Say why the requested theme did not win: a missing name and a broken
        // file produce the same fallback otherwise.
        let file = paths.themes_dir().join(format!("{requested}.json"));
        if file.exists() {
            if let Err(err) = Theme::from_file(&file) {
                eprintln!("herdstead: {err}");
            }
        } else {
            eprintln!(
                "herdstead: unknown theme `{requested}`, using `{}` (try `herdstead themes`)",
                theme.name
            );
        }
    }
    if let Err(err) = theme.apply_overrides(&config.theme_colors) {
        eprintln!("herdstead: theme-colors: {err}");
    }
    theme
}
