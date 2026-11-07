//! Folder trees: scanning a directory and drawing it with box-drawing rules.

use std::path::{Path, PathBuf};

use herdstead_core::text;
use herdstead_core::Theme;

use crate::code::bytes;
use crate::style::{Line, Span, Style};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeOptions {
    /// Folder names never descended into.
    pub ignores: Vec<String>,
    pub max_entries: usize,
    pub max_depth: usize,
    pub show_sizes: bool,
    pub follow_links: bool,
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self {
            ignores: vec![
                "target".into(),
                "node_modules".into(),
                ".git".into(),
                "dist".into(),
                "build".into(),
                ".venv".into(),
            ],
            max_entries: 4_000,
            max_depth: 8,
            show_sizes: true,
            follow_links: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
    Exec,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub kind: EntryKind,
    pub size: Option<u64>,
    /// `true` when this is the last child of its parent, per ancestor.
    pub ancestors_last: Vec<bool>,
    pub last: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TreeScan {
    pub root: PathBuf,
    /// The path as the user gave it, shown in the header.
    pub display_root: PathBuf,
    pub entries: Vec<TreeEntry>,
    pub files: usize,
    pub dirs: usize,
    /// Skipped children at depth/entry limits, unreadable directories and cycles.
    /// Ignored names and unvisited descendants are not counted.
    pub skipped: usize,
}

impl TreeScan {
    pub fn truncated(&self) -> bool {
        self.skipped > 0
    }
}

/// Walks `root`, folders first, alphabetically, skipping ignored folders.
pub fn scan(root: &Path, options: &TreeOptions) -> Result<TreeScan, String> {
    let display_root = root.to_path_buf();
    let root = root
        .canonicalize()
        .map_err(|err| format!("{}: {err}", root.display()))?;
    if !root.is_dir() {
        return Err(format!("{}: not a folder", root.display()));
    }
    let mut out = TreeScan {
        root: root.clone(),
        display_root,
        ..TreeScan::default()
    };
    let mut visited = Vec::new();
    visited.push(root.clone());
    walk(&root, 0, &mut Vec::new(), options, &mut out, &mut visited);
    Ok(out)
}

fn walk(
    dir: &Path,
    depth: usize,
    ancestors_last: &mut Vec<bool>,
    options: &TreeOptions,
    out: &mut TreeScan,
    visited: &mut Vec<PathBuf>,
) {
    let mut children = match read_children(dir, options) {
        Ok(children) => children,
        Err(_) => {
            out.skipped += 1;
            return;
        }
    };
    if depth >= options.max_depth && !children.is_empty() {
        out.skipped += children.len();
        return;
    }
    if out.entries.len() + children.len() > options.max_entries {
        let room = options.max_entries.saturating_sub(out.entries.len());
        out.skipped += children.len().saturating_sub(room);
        children.truncate(room);
    }
    let count = children.len();
    for (index, (path, kind, size, is_dir)) in children.into_iter().enumerate() {
        if out.entries.len() >= options.max_entries {
            out.skipped += count - index;
            break;
        }
        let last = index + 1 == count;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let entry = TreeEntry {
            path: path.clone(),
            name,
            depth,
            kind,
            size,
            ancestors_last: ancestors_last.clone(),
            last,
        };
        if is_dir {
            out.dirs += 1;
        } else {
            out.files += 1;
        }
        out.entries.push(entry);
        if is_dir {
            let canonical = if options.follow_links {
                path.canonicalize().ok()
            } else {
                Some(path.clone())
            };
            if let Some(canonical) = canonical {
                if visited.contains(&canonical) {
                    out.skipped += 1;
                    continue;
                }
                visited.push(canonical);
                ancestors_last.push(last);
                walk(&path, depth + 1, ancestors_last, options, out, visited);
                ancestors_last.pop();
                visited.pop();
            }
        }
    }
}

type Child = (PathBuf, EntryKind, Option<u64>, bool);

fn read_children(dir: &Path, options: &TreeOptions) -> std::io::Result<Vec<Child>> {
    let mut children: Vec<Child> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let symlink = metadata.file_type().is_symlink();
        let is_dir = if symlink {
            options.follow_links && path.is_dir()
        } else {
            metadata.is_dir()
        };
        if is_dir && options.ignores.iter().any(|ignore| ignore == &name) {
            continue;
        }
        if name == ".DS_Store" {
            continue;
        }
        let kind = if symlink {
            EntryKind::Symlink
        } else if is_dir {
            EntryKind::Dir
        } else if is_executable(&metadata) {
            EntryKind::Exec
        } else {
            EntryKind::File
        };
        let size = if metadata.is_file() {
            Some(metadata.len())
        } else {
            None
        };
        children.push((path, kind, size, is_dir));
    }
    children.sort_by(|a, b| {
        let a_dir = a.1 == EntryKind::Dir || a.3;
        let b_dir = b.1 == EntryKind::Dir || b.3;
        b_dir
            .cmp(&a_dir)
            .then_with(|| a.0.file_name().cmp(&b.0.file_name()))
    });
    Ok(children)
}

fn is_executable(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}

/// Draws a scanned tree: a header, then one row per entry.
pub fn render(scan: &TreeScan, options: &TreeOptions, theme: &Theme, width: usize) -> Vec<Line> {
    let width = width.max(20);
    let mut out = Vec::new();
    let mut header = Line::new();
    header.push(Span::new(
        text::abbreviate_home(&scan.display_root.display().to_string(), home().as_deref()),
        Style::plain().fg(theme.tree_dir).bold(),
    ));
    header.push(Span::new(
        format!(
            "  {} files · {} folders{}",
            scan.files,
            scan.dirs,
            if scan.truncated() {
                format!(" · {} not shown", scan.skipped)
            } else {
                String::new()
            }
        ),
        Style::plain().fg(theme.muted),
    ));
    header.truncate(width, true);
    out.push(header);
    out.push(Line::new());

    let size_width = if options.show_sizes {
        scan.entries
            .iter()
            .filter_map(|entry| entry.size)
            .map(|size| bytes(size).len())
            .max()
            .unwrap_or(0)
    } else {
        0
    };
    for entry in &scan.entries {
        let mut row = Line::new();
        for ancestor_last in &entry.ancestors_last {
            let glyph = if *ancestor_last { "   " } else { "│  " };
            row.push(Span::new(glyph, Style::plain().fg(theme.border)));
        }
        let branch = if entry.last { "└─ " } else { "├─ " };
        row.push(Span::new(branch, Style::plain().fg(theme.border)));
        let (style, suffix) = match entry.kind {
            EntryKind::Dir => (Style::plain().fg(theme.tree_dir).bold(), "/"),
            EntryKind::Exec => (Style::plain().fg(theme.tree_exec), "*"),
            EntryKind::Symlink => (Style::plain().fg(theme.tree_link).italic(), ""),
            EntryKind::File => (Style::plain().fg(theme.tree_file), ""),
        };
        row.push(Span::new(format!("{}{suffix}", entry.name), style));
        if entry.kind == EntryKind::Symlink {
            if let Ok(target) = std::fs::read_link(&entry.path) {
                row.push(Span::new(
                    format!(" → {}", target.display()),
                    Style::plain().fg(theme.faint),
                ));
            }
        }
        if size_width > 0 {
            if let Some(size) = entry.size {
                let used = row.width();
                let label = bytes(size);
                let pad = width
                    .saturating_sub(used)
                    .saturating_sub(label.len())
                    .max(1);
                row.push(Span::new(" ".repeat(pad), Style::plain()));
                row.push(Span::new(label, Style::plain().fg(theme.gutter)));
            }
        }
        row.truncate(width, true);
        out.push(row);
    }
    if scan.entries.is_empty() {
        out.push(Line::styled(
            "(empty folder)",
            Style::plain().fg(theme.muted),
        ));
    }
    out
}

fn home() -> Option<String> {
    std::env::var("HOME").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::midnight()
    }

    fn fixture(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("herdstead-tree-{}-{tag}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn x() {}").unwrap();
        std::fs::write(root.join("README.md"), "# hi").unwrap();
        std::fs::write(root.join("target/debug/junk"), "junk").unwrap();
        root
    }

    #[test]
    fn scans_folders_first_and_ignores_build_output() {
        let root = fixture("scan");
        let scan = scan(&root, &TreeOptions::default()).unwrap();
        let names: Vec<&str> = scan
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, vec!["src", "lib.rs", "main.rs", "README.md"]);
        assert_eq!(scan.dirs, 1);
        assert_eq!(scan.files, 3);
        // `target` is ignored entirely.
        assert!(!names.contains(&"target"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn renders_branches_and_sizes() {
        let root = fixture("render");
        let scan = scan(&root, &TreeOptions::default()).unwrap();
        let lines = render(&scan, &TreeOptions::default(), &theme(), 100);
        let all: Vec<String> = lines.iter().map(Line::text).collect();
        assert!(all[0].contains("3 files"), "{all:?}");
        assert!(all.iter().any(|line| line.contains("└─ main.rs")));
        assert!(all.iter().any(|line| line.contains("└─ README.md")));
        assert!(all.iter().any(|line| line.contains("├─ src/")), "{all:?}");
        assert!(
            all.iter().any(|line| line.contains("│  ├─ lib.rs")),
            "{all:?}"
        );
        assert!(all.iter().any(|line| line.contains(" B")));
        for line in &lines {
            assert!(line.width() <= 100, "{:?}", line.text());
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn depth_and_entry_caps_stop_walking() {
        let root = fixture("caps");
        let shallow = TreeOptions {
            max_depth: 1,
            ..TreeOptions::default()
        };
        let shallow_scan = scan(&root, &shallow).unwrap();
        assert!(shallow_scan.entries.iter().all(|entry| entry.depth == 0));
        assert!(shallow_scan.truncated());

        let capped = TreeOptions {
            max_entries: 2,
            ..TreeOptions::default()
        };
        let capped_scan = scan(&root, &capped).unwrap();
        assert_eq!(capped_scan.entries.len(), 2);
        assert!(capped_scan.truncated());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_files_and_missing_paths() {
        let root = fixture("rejects");
        assert!(scan(&root.join("README.md"), &TreeOptions::default()).is_err());
        assert!(scan(&root.join("nope"), &TreeOptions::default()).is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}
