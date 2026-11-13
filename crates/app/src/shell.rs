//! zsh startup integration for output helpers and OSC 7 working-directory updates.

use std::path::PathBuf;

use anyhow::Result;
use herdstead_core::paths::Paths;

pub const ZSH: &str = include_str!("../../../shell/herdstead.zsh");

pub fn prepare(paths: &Paths) -> Result<PathBuf> {
    paths.ensure_runtime()?;
    let dir = paths.runtime.join(format!("zsh-{}", std::process::id()));
    if dir.join(".ready").is_file() {
        return Ok(dir);
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("herdstead.zsh"), ZSH)?;
    // Source each user startup file once, restoring ZDOTDIR before their zshrc
    // so frameworks and nested shells see the user's normal paths.
    std::fs::write(
        dir.join(".zshenv"),
        r#"ZDOTDIR=$HERDSTEAD_USER_ZDOTDIR
[[ -f "$ZDOTDIR/.zshenv" ]] && source "$ZDOTDIR/.zshenv"
HERDSTEAD_USER_ZDOTDIR=${ZDOTDIR:-$HOME}
ZDOTDIR=$HERDSTEAD_INTEGRATION_DIR
"#,
    )?;
    std::fs::write(
        dir.join(".zprofile"),
        r#"ZDOTDIR=$HERDSTEAD_USER_ZDOTDIR
[[ -f "$ZDOTDIR/.zprofile" ]] && source "$ZDOTDIR/.zprofile"
HERDSTEAD_USER_ZDOTDIR=${ZDOTDIR:-$HOME}
ZDOTDIR=$HERDSTEAD_INTEGRATION_DIR
"#,
    )?;
    std::fs::write(
        dir.join(".zshrc"),
        r#"ZDOTDIR=$HERDSTEAD_USER_ZDOTDIR
[[ -f "$ZDOTDIR/.zshrc" ]] && source "$ZDOTDIR/.zshrc"
source "$HERDSTEAD_INTEGRATION_DIR/herdstead.zsh"
"#,
    )?;
    std::fs::write(dir.join(".ready"), "1")?;
    Ok(dir)
}
