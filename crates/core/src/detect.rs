//! Which coding agent owns a pane, and what state that pane is in.
//!
//! Agent identity uses command metadata; activity uses visible text and quiet
//! time. These classifiers stay pure so the runtime owns all process queries.

use serde::{Deserialize, Serialize};

/// A coding agent the sidebar knows how to label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    Codex,
    Gemini,
    Aider,
    OpenCode,
    Oxide,
    Cursor,
    Goose,
    Amp,
    Qwen,
    Other(String),
}

impl AgentKind {
    /// Recognises an agent from the foreground command line or a pane title.
    pub fn detect(command: &str, title: &str) -> Option<Self> {
        let mut words = command.split_whitespace();
        let executable = words.next().map(basename).unwrap_or_default();
        Self::from_name(executable)
            .or_else(|| {
                // Interpreters run agent scripts; arbitrary command arguments do not.
                matches!(
                    executable,
                    "node" | "bun" | "python" | "python3" | "sh" | "bash" | "zsh"
                )
                .then(|| {
                    words
                        .next()
                        .and_then(|word| Self::from_name(basename(word)))
                })
                .flatten()
            })
            .or_else(|| {
                title
                    .split_whitespace()
                    .find_map(|word| Self::from_name(basename(word)))
            })
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().trim_end_matches([':', ',', ';']);
        let name = name.strip_prefix('.').unwrap_or(name).to_ascii_lowercase();
        let name = name
            .strip_suffix(".js")
            .or_else(|| name.strip_suffix(".py"))
            .unwrap_or(&name);
        Some(match name {
            "claude" | "claude-code" => Self::Claude,
            "codex" => Self::Codex,
            "gemini" => Self::Gemini,
            "aider" => Self::Aider,
            "opencode" => Self::OpenCode,
            "oxide" => Self::Oxide,
            "cursor" | "cursor-agent" => Self::Cursor,
            "goose" => Self::Goose,
            "amp" => Self::Amp,
            "qwen" => Self::Qwen,
            _ => return None,
        })
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Aider => "aider",
            Self::OpenCode => "opencode",
            Self::Oxide => "oxide",
            Self::Cursor => "cursor",
            Self::Goose => "goose",
            Self::Amp => "amp",
            Self::Qwen => "qwen",
            Self::Other(name) => name,
        }
    }

    /// Glyph drawn before the pane title in the sidebar and tab bar.
    pub fn glyph(&self) -> &str {
        match self {
            Self::Claude => "✳",
            Self::Codex => "◆",
            Self::Gemini => "✦",
            Self::Aider => "▲",
            Self::OpenCode => "◇",
            Self::Oxide => "⬢",
            Self::Cursor => "➤",
            Self::Goose => "●",
            Self::Amp => "⚡",
            Self::Qwen => "◎",
            Self::Other(_) => "•",
        }
    }
}

fn basename(value: &str) -> &str {
    let value = value.trim_matches(|c: char| c == '"' || c == '\'');
    value
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(value)
}

/// Recognises an agent from what its interface draws on screen, which is what
/// a shell pane shows once a full-screen agent takes over.
pub fn agent_from_text(text: &str) -> Option<AgentKind> {
    let lower = text.to_ascii_lowercase();
    const SIGNATURES: [(&str, AgentKind); 8] = [
        ("welcome to claude code", AgentKind::Claude),
        ("claude code", AgentKind::Claude),
        ("openai codex", AgentKind::Codex),
        ("codex cli", AgentKind::Codex),
        ("gemini cli", AgentKind::Gemini),
        ("aider v", AgentKind::Aider),
        ("opencode", AgentKind::OpenCode),
        ("oxide", AgentKind::Oxide),
    ];
    // When multiple banners remain visible, the last matching signature wins.
    let mut found = None;
    for (needle, agent) in SIGNATURES {
        if lower.contains(needle) {
            found = Some(agent);
        }
    }
    found
}

/// What a pane is doing right now. `Blocked` is the one that needs a human.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaneStatus {
    #[default]
    Idle,
    Working,
    Blocked,
    Exited,
}

impl PaneStatus {
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Idle => "○",
            Self::Working => "●",
            Self::Blocked => "◆",
            Self::Exited => "✕",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Exited => "exited",
        }
    }

    /// Sort key for the sidebar: blocked panes first, then working, then idle.
    pub const fn priority(self) -> u8 {
        match self {
            Self::Blocked => 0,
            Self::Working => 1,
            Self::Idle => 2,
            Self::Exited => 3,
        }
    }
}

/// How long a pane may be quiet before it counts as idle rather than working.
pub const WORKING_QUIET_MS: u64 = 900;

/// Classifies a pane from its screen text and how long ago it last produced
/// output. A prompt waiting for an answer stays quiet, so it is checked before
/// the quiet threshold.
pub fn classify(text: &str, quiet_ms: u64, exited: bool) -> PaneStatus {
    if exited {
        return PaneStatus::Exited;
    }
    if looks_blocked(text) {
        return PaneStatus::Blocked;
    }
    if quiet_ms < WORKING_QUIET_MS {
        PaneStatus::Working
    } else {
        PaneStatus::Idle
    }
}

/// Heuristics for "the agent is asking the human something". Only the tail of
/// the screen is considered; an old question scrolled away is not a blocker.
pub fn looks_blocked(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(8)..];
    tail.iter().any(|line| blocked_line(line))
}

fn blocked_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let lower = lower.trim();
    if lower.ends_with("[y/n]") || lower.ends_with("(y/n)") || lower.ends_with("[y/n]:") {
        return true;
    }
    const NEEDLES: [&str; 16] = [
        "do you want to",
        "would you like to",
        "press enter to continue",
        "press enter to confirm",
        "waiting for approval",
        "waiting for your response",
        "enter to submit",
        "esc to interrupt",
        "allow this",
        "allow tool",
        "(y)es",
        "1. yes",
        "❯ 1.",
        "yes, allow",
        "do you trust",
        "confirm to proceed",
    ];
    if NEEDLES.iter().any(|needle| lower.contains(needle)) {
        return true;
    }
    // A numbered choice menu whose selected row is marked with `❯`.
    if lower.starts_with("❯") && lower.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }
    // A bare question at the very end of the prompt.
    line.ends_with('?') && line.len() > 8 && !line.contains("://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_agents_from_commands_and_titles() {
        assert_eq!(
            AgentKind::detect("claude --dangerously-skip-permissions", "zsh"),
            Some(AgentKind::Claude)
        );
        assert_eq!(
            AgentKind::detect("node /usr/local/bin/codex", ""),
            Some(AgentKind::Codex)
        );
        assert_eq!(
            AgentKind::detect("/opt/homebrew/bin/oxide", ""),
            Some(AgentKind::Oxide)
        );
        assert_eq!(
            AgentKind::detect("vim src/main.rs", "claude: working"),
            Some(AgentKind::Claude)
        );
        assert_eq!(AgentKind::detect("zsh", "~/projects"), None);
        assert_eq!(AgentKind::detect("cat /tmp/oxide", ""), None);
        assert_eq!(AgentKind::detect("echo opencode", ""), None);
    }

    #[test]
    fn quiet_time_decides_idle_and_working() {
        assert_eq!(classify("done\n", 10, false), PaneStatus::Working);
        assert_eq!(classify("done\n", 5_000, false), PaneStatus::Idle);
        assert_eq!(classify("done\n", 5_000, true), PaneStatus::Exited);
    }

    #[test]
    fn prompt_questions_block_a_pane() {
        let screen = "I can refactor this module.\n\nDo you want to proceed?\n❯ 1. Yes\n  2. No\n";
        assert_eq!(classify(screen, 30_000, false), PaneStatus::Blocked);
        assert_eq!(
            classify("Overwrite file? [y/N]\n", 30_000, false),
            PaneStatus::Blocked
        );
        assert_eq!(
            classify("Cloning into herdr...\n", 200, false),
            PaneStatus::Working
        );
    }

    #[test]
    fn a_scrolled_away_question_does_not_block() {
        let mut text = String::from("Do you want to proceed?\n");
        for i in 0..40 {
            text.push_str(&format!("line {i}\n"));
        }
        assert_eq!(classify(&text, 5_000, false), PaneStatus::Idle);
    }

    #[test]
    fn recognises_agents_from_screen_text() {
        assert_eq!(
            agent_from_text("  Welcome to Claude Code v2\n  cwd: ~/proj"),
            Some(AgentKind::Claude)
        );
        assert_eq!(agent_from_text("OpenAI Codex\n> "), Some(AgentKind::Codex));
        assert_eq!(agent_from_text("aider v0.86.0\n"), Some(AgentKind::Aider));
        assert_eq!(agent_from_text("~/proj $ ls\n"), None);
        assert!(AgentKind::Claude.glyph().chars().count() == 1);
    }

    #[test]
    fn urls_with_question_marks_are_not_questions() {
        assert_eq!(
            classify("see https://example.com/a?\n", 5_000, false),
            PaneStatus::Idle
        );
    }
}
