//! Rich text rendering for herdstead: Markdown, syntax highlighting, folder
//! trees, link decoration, the composer and ANSI output.
//!
//! Drawing functions take text plus a [`Theme`] and return styled [`Line`]s.
//! File-loading helpers and tree scanning perform IO. The desktop maps rows
//! into styled spans and the bundled shell helpers into ANSI output.

pub mod ansi;
pub mod code;
pub mod composer;
pub mod inline;
pub mod input;
pub mod link;
pub mod markdown;
pub mod style;
pub mod term;
pub mod tree;
pub mod wrap;

pub use style::{Line, Link, Span, Style};

use herdstead_core::Theme;

/// Renders a file the way a viewer pane shows it: Markdown is rendered, other
/// text is syntax-highlighted. Files must be UTF-8; code reads also reject NULs.
pub fn render_file(
    path: &std::path::Path,
    width: usize,
    theme: &Theme,
    code_options: &code::CodeOptions,
) -> Result<Vec<Line>, String> {
    let kind = herdstead_core::model::ViewerKind::for_path(path);
    match kind {
        herdstead_core::model::ViewerKind::Tree => Err(format!(
            "{}: is a folder, open it as a tree",
            path.display()
        )),
        herdstead_core::model::ViewerKind::Markdown => {
            let source = std::fs::read_to_string(path)
                .map_err(|err| format!("{}: {err}", path.display()))?;
            Ok(markdown::render_width(&source, width, theme))
        }
        herdstead_core::model::ViewerKind::Code => code::highlight_file(
            path,
            &code::CodeOptions {
                width,
                ..code_options.clone()
            },
            theme,
        ),
    }
}

/// Renders whatever a path is: a folder tree, Markdown, or a highlighted file.
pub fn render_path(
    path: &std::path::Path,
    width: usize,
    theme: &Theme,
    tree_options: &tree::TreeOptions,
) -> Result<Vec<Line>, String> {
    if path.is_dir() {
        let scan = tree::scan(path, tree_options)?;
        Ok(tree::render(&scan, tree_options, theme, width))
    } else {
        render_file(path, width, theme, &code::CodeOptions::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_markdown_and_code_files() {
        let root = std::env::temp_dir().join(format!("herdstead-render-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let md = root.join("README.md");
        std::fs::write(&md, "# Title\n\nbody **bold**").unwrap();
        let rs = root.join("main.rs");
        std::fs::write(&rs, "fn main() {}\n").unwrap();
        let theme = Theme::midnight();

        let lines = render_file(&md, 40, &theme, &code::CodeOptions::default()).unwrap();
        assert!(lines.iter().any(|line| line.text().contains("Title")));

        let lines = render_file(&rs, 40, &theme, &code::CodeOptions::default()).unwrap();
        assert!(lines.iter().any(|line| line.text().contains("fn main")));

        let lines = render_path(&root, 100, &theme, &tree::TreeOptions::default()).unwrap();
        assert!(lines[0].text().contains("files"), "{:?}", lines[0].text());
        assert!(lines.iter().any(|line| line.text().contains("README.md")));

        std::fs::remove_dir_all(&root).ok();
    }
}
