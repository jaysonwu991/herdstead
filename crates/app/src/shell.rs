//! zsh startup integration and safe, literal expansion of shell path references.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
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
    // so Oh My Zsh, Starship and nested shells see the user's normal paths.
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

fn quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

/// Expand standalone, unquoted @references into one literal shell argument.
/// Quoted strings, comments, escapes and here-documents remain shell text.
pub fn expand(input: &str, cwd: &Path) -> Result<String> {
    if input.contains("<<") {
        return Ok(input.to_string());
    }
    let mut out = String::new();
    let mut index = 0;
    let mut boundary = true;
    while index < input.len() {
        let ch = input[index..].chars().next().unwrap();
        if matches!(ch, '\'' | '"' | '`') {
            let end = quoted_end(input, index, ch);
            out.push_str(&input[index..end]);
            index = end;
            boundary = false;
        } else if ch == '\\' {
            let end = index + ch.len_utf8();
            let end = end + input[end..].chars().next().map_or(0, char::len_utf8);
            out.push_str(&input[index..end]);
            index = end;
            boundary = false;
        } else if ch == '#' && boundary {
            let end = input[index..].find('\n').map_or(input.len(), |n| index + n);
            out.push_str(&input[index..end]);
            index = end;
        } else if ch == '@' && boundary {
            let start = index;
            index += 1;
            let mut path = String::new();
            while index < input.len() {
                let next = input[index..].chars().next().unwrap();
                if next.is_whitespace() || ";|&<>()".contains(next) {
                    break;
                }
                if matches!(next, '\'' | '"') {
                    let end = quoted_end(input, index, next);
                    if !input[index + 1..end].ends_with(next) {
                        bail!("unfinished @path quote");
                    }
                    path.push_str(&input[index + 1..end - 1]);
                    index = end;
                } else if next == '\\' {
                    index += 1;
                    if let Some(escaped) = input[index..].chars().next() {
                        path.push(escaped);
                        index += escaped.len_utf8();
                    }
                } else {
                    path.push(next);
                    index += next.len_utf8();
                }
            }
            if path.is_empty() {
                out.push_str(&input[start..index]);
            } else {
                let (path, _, _) = herdstead_core::text::split_location(&path);
                let resolved = if let Some(rest) = path.strip_prefix("~/") {
                    PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(rest)
                } else {
                    cwd.join(path)
                };
                if !resolved.exists() {
                    bail!("@path does not exist: {path}");
                }
                out.push_str(&quote(&resolved.to_string_lossy()));
            }
            boundary = false;
        } else {
            out.push(ch);
            index += ch.len_utf8();
            boundary = ch.is_whitespace() || ";|&<>()".contains(ch);
        }
    }
    Ok(out)
}

fn quoted_end(input: &str, start: usize, quote: char) -> usize {
    let mut escaped = false;
    for (offset, ch) in input[start + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != '\'' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return start + 1 + offset + ch.len_utf8();
        }
    }
    input.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_literal_arguments_and_shell_syntax_is_preserved() {
        let dir = std::env::temp_dir().join(format!("herdstead-shell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for file in ["a.rs", "a b.rs", "$(touch PWNED)", "a'b"] {
            std::fs::write(dir.join(file), "test").unwrap();
        }
        let result = expand("cat @a.rs | wc -c", &dir).unwrap();
        assert_eq!(
            result,
            format!("cat '{}' | wc -c", dir.join("a.rs").display())
        );
        assert!(expand("cat @\"a b.rs\"", &dir).unwrap().contains("a b.rs'"));
        assert!(expand("cat @a\\ b.rs", &dir).unwrap().contains("a b.rs'"));
        assert!(expand("cat @\"$(touch PWNED)\"", &dir)
            .unwrap()
            .contains("'"));
        assert!(!dir.join("PWNED").exists());
        assert!(expand("cat @\"a'b\"", &dir).unwrap().contains("'\\''"));
        for source in [
            "echo 'hello @a.rs'",
            "echo \"@a.rs\"",
            "echo a@b",
            "echo \\@a.rs",
            "# @missing\necho ok",
            "cat <<EOF\n@missing\nEOF",
        ] {
            assert_eq!(expand(source, &dir).unwrap(), source);
        }
        assert!(expand("cat @missing", &dir).is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
