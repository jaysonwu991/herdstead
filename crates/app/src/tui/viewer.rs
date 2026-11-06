//! Rendering viewer panes: highlighted files, rendered Markdown, folder trees.

use std::path::PathBuf;

use herdr_plus_core::config::Config;
use herdr_plus_core::model::ViewerKind;
use herdr_plus_core::protocol::PaneFrame;
use herdr_plus_core::Theme;
use herdr_plus_render::{code, markdown, tree, Line};

/// Renders the content of a viewer pane at `width` columns.
pub fn render(
    pane: &PaneFrame,
    width: u16,
    theme: &Theme,
    config: &Config,
) -> Result<Vec<Line>, String> {
    let Some(spec) = pane.viewer.as_ref() else {
        return Err("this viewer has no file".to_string());
    };
    let width = usize::from(width.max(4));
    let path: PathBuf = spec.path.clone();
    match spec.kind {
        ViewerKind::Tree => {
            let options = tree::TreeOptions {
                ignores: config.tree_ignores.clone(),
                max_entries: config.tree_max_entries,
                ..tree::TreeOptions::default()
            };
            let scan = tree::scan(&path, &options)?;
            Ok(tree::render(&scan, &options, theme, width))
        }
        ViewerKind::Markdown => {
            let source = std::fs::read_to_string(&path)
                .map_err(|err| format!("{}: {err}", path.display()))?;
            Ok(markdown::render_width(&source, width, theme))
        }
        ViewerKind::Code => code::highlight_file(
            &path,
            &code::CodeOptions {
                width,
                line_numbers: true,
                rail: true,
                background: true,
                ..code::CodeOptions::default()
            },
            theme,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdr_plus_core::model::{PaneKind, ViewerSpec};
    use herdr_plus_core::protocol::PaneFrame;

    fn pane(path: &str, kind: ViewerKind) -> PaneFrame {
        PaneFrame {
            id: herdr_plus_core::ids::PaneId(1),
            rect: herdr_plus_core::geom::Rect::new(0, 0, 40, 20),
            title: "viewer".into(),
            cwd: "/tmp".into(),
            command: String::new(),
            kind: PaneKind::Viewer,
            viewer: Some(ViewerSpec {
                path: path.into(),
                kind,
            }),
            agent: None,
            status: herdr_plus_core::detect::PaneStatus::Idle,
            bell: false,
            exit_code: None,
            scroll: 0,
            grid: None,
        }
    }

    #[test]
    fn renders_each_viewer_kind() {
        let root = std::env::temp_dir().join(format!("herdr-plus-viewer-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.md"), "# Title\n\nbody").unwrap();
        std::fs::write(root.join("a.rs"), "fn main() {}\n").unwrap();
        let theme = Theme::midnight();
        let config = Config::default();

        let lines = render(
            &pane(
                &root.join("a.md").display().to_string(),
                ViewerKind::Markdown,
            ),
            40,
            &theme,
            &config,
        )
        .unwrap();
        assert!(lines.iter().any(|line| line.text().contains("Title")));

        let lines = render(
            &pane(&root.join("a.rs").display().to_string(), ViewerKind::Code),
            40,
            &theme,
            &config,
        )
        .unwrap();
        assert!(lines.iter().any(|line| line.text().contains("fn main")));

        let lines = render(
            &pane(&root.display().to_string(), ViewerKind::Tree),
            40,
            &theme,
            &config,
        )
        .unwrap();
        assert!(lines.iter().any(|line| line.text().contains("a.rs")));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_files_report_an_error() {
        let config = Config::default();
        let error = render(
            &pane("/nope/missing.md", ViewerKind::Markdown),
            40,
            &Theme::midnight(),
            &config,
        )
        .unwrap_err();
        assert!(error.contains("missing.md"));
    }
}
