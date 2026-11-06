//! Drawing the client: tab strip, sidebar, panes, composer, overlays.

use herdr_plus_core::config::{format_binding, ACTIONS};
use herdr_plus_core::detect::PaneStatus;
use herdr_plus_core::geom::Rect as GeoRect;
use herdr_plus_core::model::PaneKind;
use herdr_plus_core::protocol::{PaneFrame, Snapshot};
use herdr_plus_core::text;
use herdr_plus_core::theme::Rgb;
use herdr_plus_core::Theme;
use herdr_plus_render::style::Style as RStyle;
use herdr_plus_render::Line as RLine;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding};
use ratatui::Frame;

use super::app::{App, Focus, Overlay};
use super::layout::{self as ui_layout, SidebarRow};

/// Chrome rects in ratatui's coordinates.
#[derive(Debug, Clone, Copy)]
struct ChromeRects {
    tab_bar: Rect,
    sidebar: Option<Rect>,
    panes: Rect,
    composer: Rect,
    status: Rect,
}

/// Draws one frame.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = app.theme.clone();
    fill(frame.buffer_mut(), area, RStyle::plain().bg(theme.bg));
    let chrome = ui_layout::chrome(
        GeoRect::new(area.x, area.y, area.width, area.height),
        &app.chrome_state(),
    );
    let rects = ChromeRects {
        tab_bar: rat_rect(chrome.tab_bar),
        sidebar: chrome.sidebar.map(rat_rect),
        panes: rat_rect(chrome.panes),
        composer: rat_rect(chrome.composer),
        status: rat_rect(chrome.status),
    };
    let snapshot = app.snapshot.clone();

    draw_tab_bar(frame, app, &rects, snapshot.as_ref());
    if let Some(sidebar) = rects.sidebar {
        draw_sidebar(frame, app, sidebar, snapshot.as_ref());
    }
    draw_panes(frame, app, rects.panes, snapshot.as_ref());
    draw_composer(frame, app, &rects);
    draw_status(frame, app, &rects);
    if let Some(overlay) = app.overlay.clone() {
        draw_overlay(frame, app, &overlay);
    }
    place_cursor(frame, app, &rects);
}

fn fill(buf: &mut Buffer, area: Rect, style: RStyle) {
    buf.set_style(area, rat_style(style));
}

fn rat_rect(rect: GeoRect) -> Rect {
    Rect::new(rect.x, rect.y, rect.width, rect.height)
}

fn rat_style(style: RStyle) -> Style {
    let mut out = Style::default();
    if let Some(fg) = style.fg {
        out = out.fg(Color::Rgb(fg.r, fg.g, fg.b));
    }
    if let Some(bg) = style.bg {
        out = out.bg(Color::Rgb(bg.r, bg.g, bg.b));
    }
    let mut modifier = Modifier::empty();
    if style.bold {
        modifier |= Modifier::BOLD;
    }
    if style.dim {
        modifier |= Modifier::DIM;
    }
    if style.italic {
        modifier |= Modifier::ITALIC;
    }
    if style.underline {
        modifier |= Modifier::UNDERLINED;
    }
    if style.strikethrough {
        modifier |= Modifier::CROSSED_OUT;
    }
    if style.reversed {
        modifier |= Modifier::REVERSED;
    }
    out.add_modifier(modifier)
}

fn to_rat_line(line: &RLine) -> Line<'static> {
    let spans: Vec<Span> = line
        .spans
        .iter()
        .map(|span| Span::styled(span.text.clone(), rat_style(span.style)))
        .collect();
    Line::from(spans)
}

fn put(buf: &mut Buffer, x: u16, y: u16, line: &RLine, width: u16) {
    if width == 0 || x >= buf.area.right() || y >= buf.area.bottom() {
        return;
    }
    buf.set_line(x, y, &to_rat_line(line), width);
}

fn styled(text: impl Into<String>, style: RStyle) -> RLine {
    RLine::styled(text, style)
}

fn status_style(status: PaneStatus, theme: &Theme) -> RStyle {
    RStyle::plain().fg(status_color(status, theme))
}

fn status_color(status: PaneStatus, theme: &Theme) -> Rgb {
    match status {
        PaneStatus::Working => theme.accent,
        PaneStatus::Blocked => theme.warning,
        PaneStatus::Exited => theme.error,
        PaneStatus::Idle => theme.muted,
    }
}

fn draw_tab_bar(frame: &mut Frame, app: &App, chrome: &ChromeRects, snapshot: Option<&Snapshot>) {
    let theme = &app.theme;
    let bar = chrome.tab_bar;
    fill(frame.buffer_mut(), bar, RStyle::plain().bg(theme.panel));
    let Some(snapshot) = snapshot else {
        put(
            frame.buffer_mut(),
            bar.x,
            bar.y,
            &styled(
                format!(" herdr-plus · {}", app.session),
                RStyle::plain().fg(theme.fg).bold(),
            ),
            bar.width,
        );
        return;
    };
    let workspace = snapshot.workspaces.get(snapshot.active_workspace);
    let mut line = RLine::new();
    if let Some(workspace) = workspace {
        line.push(herdr_plus_render::Span::new(
            ui_layout::chip_label(&workspace.name),
            RStyle::plain()
                .fg(theme.tab_active_fg)
                .bg(theme.tab_active_bg)
                .bold(),
        ));
        for (index, tab) in workspace.tabs.iter().enumerate() {
            let active = index == workspace.active_tab;
            let style = if active {
                RStyle::plain()
                    .fg(theme.tab_active_fg)
                    .bg(theme.tab_active_bg)
                    .bold()
            } else {
                RStyle::plain().fg(theme.tab_fg)
            };
            line.push(herdr_plus_render::Span::new(
                format!(" {} {} ", index + 1, text::truncate(&tab.name, 18)),
                style,
            ));
            line.push(herdr_plus_render::Span::new(
                tab.status.glyph(),
                status_style(tab.status, theme),
            ));
            line.push(herdr_plus_render::Span::new(" ", RStyle::plain()));
        }
    }
    // The prefix hint travels at the far right of the strip.
    let hint = format!(" {} ", format_binding(&app.prefix));
    let used = line.width();
    let width = usize::from(bar.width);
    if used + hint.len() + 2 < width {
        let pad = width.saturating_sub(used + hint.len() + 1);
        line.push(herdr_plus_render::Span::new(
            " ".repeat(pad),
            RStyle::plain(),
        ));
        line.push(herdr_plus_render::Span::new(
            hint,
            RStyle::plain().fg(theme.faint),
        ));
    }
    put(frame.buffer_mut(), bar.x, bar.y, &line, bar.width);
}

fn draw_sidebar(frame: &mut Frame, app: &App, sidebar: Rect, snapshot: Option<&Snapshot>) {
    if sidebar.width == 0 || sidebar.height == 0 {
        return;
    }
    let theme = &app.theme;
    fill(frame.buffer_mut(), sidebar, RStyle::plain().bg(theme.panel));
    // A single divider line keeps the panes visually separate.
    for row in 0..sidebar.height {
        if let Some(cell) = frame
            .buffer_mut()
            .cell_mut((sidebar.right().saturating_sub(1), sidebar.y + row))
        {
            cell.set_char('│');
            cell.set_style(rat_style(RStyle::plain().fg(theme.border)));
        }
    }
    put(
        frame.buffer_mut(),
        sidebar.x,
        sidebar.y,
        &styled(
            format!(" {}", text::truncate(&app.session, 24)),
            RStyle::plain().fg(theme.accent).bold(),
        ),
        sidebar.width.saturating_sub(1),
    );
    let Some(snapshot) = snapshot else {
        return;
    };
    for (offset, row) in ui_layout::sidebar_rows(snapshot).into_iter().enumerate() {
        let y = sidebar.y + offset as u16;
        if y >= sidebar.bottom() {
            break;
        }
        let width = sidebar.width.saturating_sub(1);
        match row {
            SidebarRow::Blank => {}
            SidebarRow::Header(label) => {
                put(
                    frame.buffer_mut(),
                    sidebar.x,
                    y,
                    &styled(
                        format!(" {}", label.to_uppercase()),
                        RStyle::plain().fg(theme.faint).dim(),
                    ),
                    width,
                );
            }
            SidebarRow::Workspace {
                name,
                status,
                panes,
                active,
                ..
            } => {
                let bg = if active {
                    RStyle::plain().bg(theme.tab_active_bg)
                } else {
                    RStyle::plain()
                };
                let mut line = RLine::new();
                line.push(herdr_plus_render::Span::new(
                    format!(" {} ", status.glyph()),
                    status_style(status, theme),
                ));
                let mut style = RStyle::plain()
                    .fg(theme.fg)
                    .bg(bg.bg.unwrap_or(theme.panel));
                if active {
                    style = style.bold();
                }
                line.push(herdr_plus_render::Span::new(
                    text::truncate(&name, usize::from(width).saturating_sub(8)),
                    style,
                ));
                let label = panes.to_string();
                let pad = usize::from(width).saturating_sub(line.width() + label.len() + 1);
                line.push(herdr_plus_render::Span::new(
                    format!("{}{} ", " ".repeat(pad), label),
                    RStyle::plain()
                        .fg(theme.muted)
                        .bg(style.bg.unwrap_or(theme.panel)),
                ));
                put(frame.buffer_mut(), sidebar.x, y, &line, width);
            }
            SidebarRow::Tab {
                name,
                status,
                panes,
                active,
                agent,
                ..
            } => {
                let mut line = RLine::new();
                line.push(herdr_plus_render::Span::new("   ", RStyle::plain()));
                line.push(herdr_plus_render::Span::new(
                    format!("{} ", status.glyph()),
                    status_style(status, theme),
                ));
                if let Some(agent) = &agent {
                    line.push(herdr_plus_render::Span::new(
                        format!("{} ", agent.glyph()),
                        RStyle::plain().fg(theme.info),
                    ));
                }
                let style = if active {
                    RStyle::plain().fg(theme.accent).bold()
                } else {
                    RStyle::plain().fg(theme.muted)
                };
                line.push(herdr_plus_render::Span::new(
                    text::truncate(&name, usize::from(width).saturating_sub(10)),
                    style,
                ));
                let label = panes.to_string();
                let pad = usize::from(width).saturating_sub(line.width() + label.len());
                line.push(herdr_plus_render::Span::new(
                    format!("{}{}", " ".repeat(pad), label),
                    RStyle::plain().fg(theme.faint),
                ));
                put(frame.buffer_mut(), sidebar.x, y, &line, width);
            }
            SidebarRow::Pane {
                title,
                status,
                agent,
                active,
                ..
            } => {
                let mut line = RLine::new();
                line.push(herdr_plus_render::Span::new("   ", RStyle::plain()));
                line.push(herdr_plus_render::Span::new(
                    format!("{} ", status.glyph()),
                    status_style(status, theme),
                ));
                if let Some(agent) = &agent {
                    line.push(herdr_plus_render::Span::new(
                        format!("{} ", agent.glyph()),
                        RStyle::plain().fg(theme.info),
                    ));
                }
                let style = if active {
                    RStyle::plain().fg(theme.fg).bold()
                } else {
                    RStyle::plain().fg(theme.muted)
                };
                line.push(herdr_plus_render::Span::new(
                    text::truncate(&title, usize::from(width).saturating_sub(6)),
                    style,
                ));
                put(frame.buffer_mut(), sidebar.x, y, &line, width);
            }
        }
    }
}

fn draw_panes(frame: &mut Frame, app: &mut App, panes: Rect, snapshot: Option<&Snapshot>) {
    let theme = app.theme.clone();
    let Some(snapshot) = snapshot else {
        return;
    };
    if snapshot.panes.is_empty() {
        let message = styled(
            "No panes — `prefix :` opens the composer, `/tab` starts a shell",
            RStyle::plain().fg(theme.muted).italic(),
        );
        let x = panes.x + panes.width.saturating_sub(message.width() as u16) / 2;
        put(
            frame.buffer_mut(),
            x,
            panes.y + panes.height / 2,
            &message,
            panes.width,
        );
        return;
    }
    for pane in &snapshot.panes {
        let rect = rat_rect(pane.rect).intersection(panes);
        if rect.width < 3 || rect.height < 2 {
            continue;
        }
        let focused = snapshot.focused == Some(pane.id);
        let border = if focused {
            theme.border_focus
        } else {
            theme.border
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(rat_style(RStyle::plain().fg(border)));
        frame.render_widget(block, rect);
        let title = pane_title(pane, focused, &theme);
        put(
            frame.buffer_mut(),
            rect.x + 1,
            rect.y,
            &title,
            rect.width.saturating_sub(2),
        );
        let inner = Rect::new(
            rect.x + 1,
            rect.y + 1,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(2),
        );
        let Some(lines) = app.pane_lines(pane, inner.width) else {
            continue;
        };
        let offset = if pane.kind == PaneKind::Viewer {
            app.viewer_offset(pane.id)
        } else {
            0
        };
        for row in 0..inner.height {
            let index = offset + usize::from(row);
            if let Some(line) = lines.get(index) {
                put(
                    frame.buffer_mut(),
                    inner.x,
                    inner.y + row,
                    line,
                    inner.width,
                );
            }
        }
        // A scroll indicator sits on the bottom border.
        if pane.kind == PaneKind::Viewer && offset > 0 {
            let indicator = format!(" ⇡ {offset} ");
            let x = rect.right().saturating_sub(indicator.len() as u16 + 1);
            put(
                frame.buffer_mut(),
                x,
                rect.bottom().saturating_sub(1),
                &styled(indicator, RStyle::plain().fg(theme.warning)),
                rect.width.saturating_sub(2),
            );
        }
    }
}

fn pane_title(pane: &PaneFrame, focused: bool, theme: &Theme) -> RLine {
    let mut line = RLine::new();
    let fg = if focused { theme.accent } else { theme.muted };
    if let Some(agent) = &pane.agent {
        line.push(herdr_plus_render::Span::new(
            format!("{} ", agent.glyph()),
            RStyle::plain().fg(theme.info),
        ));
    }
    line.push(herdr_plus_render::Span::new(
        format!(" {} ", text::truncate(&pane.title, 32)),
        RStyle::plain().fg(fg).bold(),
    ));
    line.push(herdr_plus_render::Span::new(
        format!("{} ", pane.status.glyph()),
        status_style(pane.status, theme),
    ));
    if let Some(viewer) = &pane.viewer {
        let kind = match viewer.kind {
            herdr_plus_core::model::ViewerKind::Tree => "tree",
            herdr_plus_core::model::ViewerKind::Markdown => "markdown",
            herdr_plus_core::model::ViewerKind::Code => "code",
        };
        line.push(herdr_plus_render::Span::new(
            format!("{kind} "),
            RStyle::plain().fg(theme.faint).dim(),
        ));
    } else if pane.scroll > 0 {
        line.push(herdr_plus_render::Span::new(
            format!("↑{} ", pane.scroll),
            RStyle::plain().fg(theme.warning),
        ));
    }
    line
}

fn draw_composer(frame: &mut Frame, app: &mut App, chrome: &ChromeRects) {
    let theme = app.theme.clone();
    let area = chrome.composer;
    if area.height == 0 {
        return;
    }
    fill(frame.buffer_mut(), area, RStyle::plain().bg(theme.panel));
    let top = area.y;
    for column in 0..area.width {
        if let Some(cell) = frame.buffer_mut().cell_mut((area.x + column, top)) {
            cell.set_char('─');
            cell.set_style(rat_style(RStyle::plain().fg(theme.border)));
        }
    }
    let inner_width = area.width.saturating_sub(6);
    let preview_rows = if app.composer.preview {
        ui_layout::PREVIEW_ROWS
    } else {
        0
    };
    let input_rows = area.height.saturating_sub(1 + preview_rows);
    let input_y = area.y + 1 + preview_rows;

    if app.composer.preview {
        let preview_area = Rect::new(
            area.x + 2,
            area.y + 1,
            area.width.saturating_sub(4),
            ui_layout::PREVIEW_ROWS,
        );
        let lines = app.preview_lines(usize::from(preview_area.width));
        for (index, line) in lines
            .iter()
            .take(usize::from(preview_area.height))
            .enumerate()
        {
            put(
                frame.buffer_mut(),
                preview_area.x,
                preview_area.y + index as u16,
                line,
                preview_area.width,
            );
        }
        let label = styled(
            " preview ",
            RStyle::plain().fg(theme.accent).bg(theme.panel).dim(),
        );
        put(
            frame.buffer_mut(),
            preview_area.x,
            preview_area.y,
            &label,
            preview_area.width,
        );
    }

    if app.composer.open {
        let (lines, _, _) = app.composer_layout(usize::from(inner_width));
        for (index, line) in lines.iter().take(usize::from(input_rows)).enumerate() {
            let prefix = if index == 0 {
                styled(" ❯ ", RStyle::plain().fg(theme.accent).bold())
            } else {
                styled("   ", RStyle::plain())
            };
            put(
                frame.buffer_mut(),
                area.x,
                input_y + index as u16,
                &prefix,
                area.width,
            );
            put(
                frame.buffer_mut(),
                area.x + 3,
                input_y + index as u16,
                line,
                inner_width,
            );
        }
        if let Some(completion) = &app.composer.completion {
            draw_completion(frame, app, area, completion);
        }
    } else {
        let hint = styled(
            format!(
                " ❯ {} : prompt the agent · @path opens a file or folder · / for commands",
                format_binding(&app.prefix)
            ),
            RStyle::plain().fg(theme.faint),
        );
        put(frame.buffer_mut(), area.x, area.y + 1, &hint, area.width);
    }
    if let Some((message, _)) = &app.status {
        let message = styled(
            format!(" ✦ {message}"),
            RStyle::plain().fg(theme.accent_alt).bold(),
        );
        put(
            frame.buffer_mut(),
            area.x,
            area.bottom().saturating_sub(1),
            &message,
            area.width,
        );
    }
}

fn draw_completion(frame: &mut Frame, app: &App, input: Rect, completion: &super::app::Completion) {
    let theme = &app.theme;
    let rows = completion.items.len().min(8);
    let height = rows as u16 + 2;
    let width = 60.min(input.width.max(13)).max(13);
    let y = input.y.saturating_sub(height);
    let area = Rect::new(input.x + 3, y, width, height);
    fill(frame.buffer_mut(), area, RStyle::plain().bg(theme.bg));
    frame.render_widget(Clear, area);
    fill(frame.buffer_mut(), area, RStyle::plain().bg(theme.bg));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(rat_style(RStyle::plain().fg(theme.border_focus)))
        .title(Span::styled(
            " @path ",
            rat_style(RStyle::plain().fg(theme.mention).bold()),
        ))
        .padding(Padding::horizontal(1));
    frame.render_widget(block, area);
    for (index, item) in completion.items.iter().take(rows).enumerate() {
        let selected = index == completion.index;
        let style = if selected {
            RStyle::plain().fg(theme.bg).bg(theme.accent).bold()
        } else {
            RStyle::plain().fg(theme.fg)
        };
        let label = if item.ends_with('/') {
            format!("▸ {item}")
        } else {
            format!("  {item}")
        };
        put(
            frame.buffer_mut(),
            area.x + 2,
            area.y + 1 + index as u16,
            &styled(
                text::truncate(&label, usize::from(width).saturating_sub(4)),
                style,
            ),
            width.saturating_sub(4),
        );
        if selected {
            let row = area.y + 1 + index as u16;
            for column in 1..area.width.saturating_sub(1) {
                if let Some(cell) = frame.buffer_mut().cell_mut((area.x + column, row)) {
                    cell.set_style(rat_style(style));
                }
            }
        }
    }
}

fn draw_status(frame: &mut Frame, app: &App, chrome: &ChromeRects) {
    let theme = &app.theme;
    let area = chrome.status;
    if area.height == 0 {
        return;
    }
    fill(frame.buffer_mut(), area, RStyle::plain().bg(theme.panel));
    let focused = app.focused_pane();
    let cwd = focused
        .map(|pane| {
            let home = std::env::var("HOME").ok();
            text::abbreviate_home(&pane.cwd, home.as_deref())
        })
        .unwrap_or_default();
    let mut left = RLine::new();
    left.push(herdr_plus_render::Span::new(
        format!(" {} ", app.session),
        RStyle::plain().fg(theme.accent).bold(),
    ));
    left.push(herdr_plus_render::Span::new(
        text::truncate(&cwd, 48),
        RStyle::plain().fg(theme.muted),
    ));
    if let Some(pane) = focused {
        left.push(herdr_plus_render::Span::new(
            format!("  {}", pane.status.label()),
            status_style(pane.status, theme),
        ));
        if let Some(agent) = &pane.agent {
            left.push(herdr_plus_render::Span::new(
                format!(" {}", agent.label()),
                RStyle::plain().fg(theme.info),
            ));
        }
    }
    put(frame.buffer_mut(), area.x, area.y, &left, area.width);

    let hint = format!("{} · {} ", app.theme.name, app.hint());
    let hint_width = text::display_width(&hint) as u16;
    if area.width > hint_width + left.width() as u16 + 2 {
        let x = area.right() - hint_width;
        put(
            frame.buffer_mut(),
            x,
            area.y,
            &styled(hint, RStyle::plain().fg(theme.faint)),
            hint_width,
        );
    }
}

fn draw_overlay(frame: &mut Frame, app: &App, overlay: &Overlay) {
    let theme = &app.theme;
    match overlay {
        Overlay::ThemeMenu { index } => {
            let rows = app.themes.len();
            let area = rat_rect(ui_layout::overlay_rect(app.screen, rows));
            frame.render_widget(Clear, area);
            fill(frame.buffer_mut(), area, RStyle::plain().bg(theme.bg));
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(rat_style(RStyle::plain().fg(theme.border_focus)))
                .title(Span::styled(
                    " themes ",
                    rat_style(RStyle::plain().fg(theme.accent).bold()),
                ));
            frame.render_widget(block, area);
            for (row, theme) in app.themes.iter().enumerate() {
                let y = area.y + 1 + row as u16;
                if y >= area.bottom() {
                    break;
                }
                let selected = row == *index;
                let style = if selected {
                    RStyle::plain().fg(theme.bg).bg(theme.bg).reversed().bold()
                } else {
                    RStyle::plain().fg(theme.fg)
                };
                if selected {
                    for column in 1..area.width.saturating_sub(1) {
                        if let Some(cell) = frame.buffer_mut().cell_mut((area.x + column, y)) {
                            cell.set_style(rat_style(RStyle::plain().bg(theme.border)));
                        }
                    }
                }
                let mut line = RLine::new();
                line.push(herdr_plus_render::Span::new(" ", style));
                line.push(herdr_plus_render::Span::new(
                    "▇▇ ",
                    RStyle::plain().fg(theme.accent).bg(theme.code_bg),
                ));
                line.push(herdr_plus_render::Span::new(
                    format!("{:<10} ", theme.name),
                    style,
                ));
                line.push(herdr_plus_render::Span::new(
                    format!("{:<6}", theme.appearance_label()),
                    style,
                ));
                put(
                    frame.buffer_mut(),
                    area.x,
                    y,
                    &line,
                    area.width.saturating_sub(1),
                );
            }
            let footer = styled(
                " ↑↓ preview · enter apply · esc cancel ",
                RStyle::plain().fg(theme.faint).dim(),
            );
            put(
                frame.buffer_mut(),
                area.x,
                area.bottom().saturating_sub(1),
                &footer,
                area.width,
            );
        }
        Overlay::Help => {
            let rows = ACTIONS.len() + 4;
            let area = rat_rect(ui_layout::overlay_rect(app.screen, rows.min(28)));
            frame.render_widget(Clear, area);
            fill(frame.buffer_mut(), area, RStyle::plain().bg(theme.bg));
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(rat_style(RStyle::plain().fg(theme.border_focus)))
                .title(Span::styled(
                    " keys ",
                    rat_style(RStyle::plain().fg(theme.accent).bold()),
                ))
                .padding(Padding::horizontal(1));
            frame.render_widget(block, area);
            let mut line = RLine::new();
            line.push(herdr_plus_render::Span::new(
                format!(" prefix {} ", format_binding(&app.prefix)),
                RStyle::plain().fg(theme.accent_alt),
            ));
            line.push(herdr_plus_render::Span::new(
                "· esc closes ",
                RStyle::plain().fg(theme.faint),
            ));
            put(
                frame.buffer_mut(),
                area.x + 2,
                area.y + 1,
                &line,
                area.width.saturating_sub(4),
            );
            for (index, action) in ACTIONS.iter().enumerate() {
                let y = area.y + 3 + index as u16;
                if y >= area.bottom().saturating_sub(1) {
                    break;
                }
                let binding = app
                    .keymap
                    .get(action.name)
                    .map(format_binding)
                    .unwrap_or_else(|| "-".to_string());
                let mut line = RLine::new();
                line.push(herdr_plus_render::Span::new(
                    format!("{binding:<20}"),
                    RStyle::plain().fg(theme.warning),
                ));
                line.push(herdr_plus_render::Span::new(
                    action.help,
                    RStyle::plain().fg(theme.fg),
                ));
                put(
                    frame.buffer_mut(),
                    area.x + 2,
                    y,
                    &line,
                    area.width.saturating_sub(4),
                );
            }
        }
        Overlay::Prompt { title, text, .. } => {
            let area = rat_rect(ui_layout::overlay_rect(app.screen, 1));
            let area = Rect::new(area.x, area.y, area.width, 5);
            frame.render_widget(Clear, area);
            fill(frame.buffer_mut(), area, RStyle::plain().bg(theme.bg));
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(rat_style(RStyle::plain().fg(theme.border_focus)))
                .title(Span::styled(
                    format!(" {title} "),
                    rat_style(RStyle::plain().fg(theme.accent).bold()),
                ))
                .padding(Padding::horizontal(1));
            frame.render_widget(block, area);
            put(
                frame.buffer_mut(),
                area.x + 2,
                area.y + 2,
                &styled(format!("❯ {text}"), RStyle::plain().fg(theme.fg)),
                area.width.saturating_sub(4),
            );
            put(
                frame.buffer_mut(),
                area.x + 2,
                area.y + 3,
                &styled(
                    "enter confirm · esc cancel",
                    RStyle::plain().fg(theme.faint).dim(),
                ),
                area.width.saturating_sub(4),
            );
        }
    }
}

fn place_cursor(frame: &mut Frame, app: &mut App, chrome: &ChromeRects) {
    if let Some(Overlay::Prompt { text, .. }) = &app.overlay {
        let area = rat_rect(ui_layout::overlay_rect(app.screen, 1));
        let area = Rect::new(area.x, area.y, area.width, 5);
        frame.set_cursor_position(Position::new(
            area.x + 3 + text::display_width(text) as u16,
            area.y + 2,
        ));
        return;
    }
    if app.composer.open && app.focus == Focus::Composer {
        let width = chrome.composer.width.saturating_sub(6);
        let (_, row, column) = app.composer_layout(usize::from(width));
        let input_y = chrome.composer.y
            + 1
            + if app.composer.preview {
                ui_layout::PREVIEW_ROWS
            } else {
                0
            };
        frame.set_cursor_position(Position::new(
            chrome.composer.x + 3 + column as u16,
            input_y + row as u16,
        ));
        return;
    }
    // Otherwise the terminal cursor follows the focused shell pane.
    let Some(pane) = app.focused_pane().cloned() else {
        return;
    };
    if pane.kind != PaneKind::Shell {
        return;
    }
    let Some(grid) = pane.grid.as_ref() else {
        return;
    };
    if grid.offset > 0 {
        return;
    }
    if let Some(cursor) = grid.cursor {
        let inner = rat_rect(pane.rect).inner(ratatui::layout::Margin::new(1, 1));
        if cursor.row < inner.height && cursor.col < inner.width {
            frame.set_cursor_position(Position::new(inner.x + cursor.col, inner.y + cursor.row));
        }
    }
}
