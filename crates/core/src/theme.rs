//! Semantic themes for the UI and every rich renderer.
//!
//! A theme is a flat set of named colour slots. Built-ins are built from a
//! nine-colour recipe so the quieter slots stay related to the base palette;
//! custom themes (JSON files under the config dir) override any slot by name.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::protocol::TermColor;

/// Generates the by-name slot accessors used by theme files.
macro_rules! slots {
    ($($slot:ident),* $(,)?) => {
        /// Sets a colour slot by name. Returns `false` for an unknown slot.
        pub fn set_slot(&mut self, name: &str, value: Rgb) -> bool {
            match name {
                $(stringify!($slot) => { self.$slot = value; true })*
                _ => false,
            }
        }

        /// Reads a colour slot by name.
        pub fn slot(&self, name: &str) -> Option<Rgb> {
            match name {
                $(stringify!($slot) => Some(self.$slot),)*
                _ => None,
            }
        }

        /// The slot names accepted by a theme file, in display order.
        pub const SLOTS: &'static [&'static str] = &[$(stringify!($slot),)*];
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// `#rrggbb`, `#rgb`, or a named ANSI colour.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(hex) = value.strip_prefix('#') {
            return match hex.len() {
                6 => {
                    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                    Some(Self::new(r, g, b))
                }
                3 => {
                    let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
                    let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
                    let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
                    Some(Self::new(r * 17, g * 17, b * 17))
                }
                _ => None,
            };
        }
        Some(match value.to_ascii_lowercase().as_str() {
            "black" => Self::new(0, 0, 0),
            "red" => Self::new(205, 49, 49),
            "green" => Self::new(13, 188, 121),
            "yellow" => Self::new(229, 229, 16),
            "blue" => Self::new(36, 114, 200),
            "magenta" => Self::new(188, 63, 188),
            "cyan" => Self::new(17, 168, 205),
            "white" => Self::new(229, 229, 229),
            "gray" | "grey" => Self::new(120, 120, 120),
            _ => return None,
        })
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Blends toward `other` by `t` (0.0 keeps `self`).
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let blend = |a: u8, b: u8| {
            (f32::from(a) + (f32::from(b) - f32::from(a)) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Rgb::new(
            blend(self.r, other.r),
            blend(self.g, other.g),
            blend(self.b, other.b),
        )
    }

    pub fn luminance(self) -> f32 {
        (0.299 * f32::from(self.r) + 0.587 * f32::from(self.g) + 0.114 * f32::from(self.b)) / 255.0
    }

    pub fn ansi_fg(self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }

    pub fn ansi_bg(self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }
}

/// The 256-colour xterm palette, for terminal output that uses indexed colours.
pub fn xterm256(index: u8) -> Rgb {
    const BASE: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 0, 0),
        (0, 205, 0),
        (205, 205, 0),
        (0, 0, 238),
        (205, 0, 205),
        (0, 205, 205),
        (229, 229, 229),
        (127, 127, 127),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (92, 92, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    if index < 16 {
        let (r, g, b) = BASE[usize::from(index)];
        return Rgb::new(r, g, b);
    }
    if index < 232 {
        const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
        let i = u16::from(index) - 16;
        return Rgb::new(
            STEPS[(i / 36) as usize % 6],
            STEPS[(i / 6) as usize % 6],
            STEPS[i as usize % 6],
        );
    }
    let v = 8 + (u16::from(index) - 232) * 10;
    let v = v as u8;
    Rgb::new(v, v, v)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    Dark,
    Light,
}

/// Every colour the interface and the renderers draw with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub appearance: Appearance,
    /// syntect theme used for code; falls back to the appearance default.
    #[serde(default)]
    pub syntax: String,

    // Surfaces and text.
    pub bg: Rgb,
    pub panel: Rgb,
    pub fg: Rgb,
    pub muted: Rgb,
    pub faint: Rgb,
    pub accent: Rgb,
    pub accent_alt: Rgb,
    pub border: Rgb,
    pub border_focus: Rgb,
    pub selection: Rgb,
    pub instruction: Rgb,

    // States.
    pub success: Rgb,
    pub warning: Rgb,
    pub error: Rgb,
    pub info: Rgb,

    // Rich content.
    pub heading: Rgb,
    pub link: Rgb,
    pub mention: Rgb,
    pub path: Rgb,
    pub inline_code_fg: Rgb,
    pub code_bg: Rgb,
    pub code_border: Rgb,
    pub quote: Rgb,
    pub bullet: Rgb,
    pub number: Rgb,
    pub table_border: Rgb,
    pub gutter: Rgb,

    // Trees.
    pub tree_dir: Rgb,
    pub tree_file: Rgb,
    pub tree_exec: Rgb,
    pub tree_link: Rgb,

    // Chrome.
    pub tab_active_bg: Rgb,
    pub tab_active_fg: Rgb,
    pub tab_fg: Rgb,
    pub chip_bg: Rgb,
    pub chip_fg: Rgb,
    pub spinner: Rgb,
}

/// The nine colours each built-in theme starts from.
pub struct Recipe {
    bg: Rgb,
    panel: Rgb,
    fg: Rgb,
    accent: Rgb,
    accent_alt: Rgb,
    success: Rgb,
    warning: Rgb,
    error: Rgb,
    info: Rgb,
}

impl Theme {
    /// Names accepted by `--theme` and `theme = "..."`.
    pub const NAMES: [&'static str; 6] = [
        "midnight",
        "daylight",
        "nord",
        "gruvbox",
        "mono",
        "solarized",
    ];

    pub fn builtin(name: &str) -> Option<Self> {
        let recipe = |bg, panel, fg, accent, accent_alt, success, warning, error, info| Recipe {
            bg,
            panel,
            fg,
            accent,
            accent_alt,
            success,
            warning,
            error,
            info,
        };
        Some(match name.to_ascii_lowercase().as_str() {
            "midnight" | "dark" | "default" => Self::from_recipe(
                "midnight",
                Appearance::Dark,
                "base16-ocean.dark",
                recipe(
                    Rgb::new(0x0e, 0x11, 0x17),
                    Rgb::new(0x15, 0x1a, 0x24),
                    Rgb::new(0xd7, 0xdc, 0xe5),
                    Rgb::new(0x7a, 0xa2, 0xf7),
                    Rgb::new(0x7d, 0xcf, 0xff),
                    Rgb::new(0x9e, 0xce, 0x6a),
                    Rgb::new(0xe0, 0xaf, 0x68),
                    Rgb::new(0xf7, 0x76, 0x8e),
                    Rgb::new(0xbb, 0x9a, 0xf7),
                ),
            ),
            "daylight" | "light" => Self::from_recipe(
                "daylight",
                Appearance::Light,
                "base16-ocean.light",
                recipe(
                    Rgb::new(0xfb, 0xfb, 0xfd),
                    Rgb::new(0xf0, 0xf2, 0xf7),
                    Rgb::new(0x24, 0x29, 0x2f),
                    Rgb::new(0x09, 0x69, 0xda),
                    Rgb::new(0x0f, 0x76, 0x6e),
                    Rgb::new(0x1a, 0x7f, 0x37),
                    Rgb::new(0x9a, 0x67, 0x00),
                    Rgb::new(0xcf, 0x22, 0x2e),
                    Rgb::new(0x82, 0x50, 0xdf),
                ),
            ),
            "nord" => Self::from_recipe(
                "nord",
                Appearance::Dark,
                "Nord",
                recipe(
                    Rgb::new(0x2e, 0x34, 0x40),
                    Rgb::new(0x33, 0x3b, 0x4a),
                    Rgb::new(0xd8, 0xde, 0xe9),
                    Rgb::new(0x88, 0xc0, 0xd0),
                    Rgb::new(0x81, 0xa1, 0xc1),
                    Rgb::new(0xa3, 0xbe, 0x8c),
                    Rgb::new(0xeb, 0xcb, 0x8b),
                    Rgb::new(0xbf, 0x61, 0x6a),
                    Rgb::new(0xb4, 0x8e, 0xad),
                ),
            ),
            "gruvbox" => Self::from_recipe(
                "gruvbox",
                Appearance::Dark,
                "gruvbox-dark",
                recipe(
                    Rgb::new(0x1d, 0x20, 0x21),
                    Rgb::new(0x28, 0x28, 0x28),
                    Rgb::new(0xeb, 0xdb, 0xb2),
                    Rgb::new(0xfa, 0xbd, 0x2f),
                    Rgb::new(0x83, 0xa5, 0x98),
                    Rgb::new(0xb8, 0xbb, 0x26),
                    Rgb::new(0xfe, 0x80, 0x19),
                    Rgb::new(0xfb, 0x49, 0x34),
                    Rgb::new(0xd3, 0x86, 0x9b),
                ),
            ),
            "mono" => Self::from_recipe(
                "mono",
                Appearance::Dark,
                "ansi",
                recipe(
                    Rgb::new(0x10, 0x10, 0x10),
                    Rgb::new(0x1a, 0x1a, 0x1a),
                    Rgb::new(0xe4, 0xe4, 0xe4),
                    Rgb::new(0xff, 0xff, 0xff),
                    Rgb::new(0xb0, 0xb0, 0xb0),
                    Rgb::new(0xd0, 0xd0, 0xd0),
                    Rgb::new(0xc8, 0xc8, 0xc8),
                    Rgb::new(0xff, 0x6b, 0x6b),
                    Rgb::new(0xa8, 0xa8, 0xa8),
                ),
            ),
            "solarized" | "solarized-light" => Self::from_recipe(
                "solarized",
                Appearance::Light,
                "Solarized (light)",
                recipe(
                    Rgb::new(0xfd, 0xf6, 0xe3),
                    Rgb::new(0xee, 0xe8, 0xd5),
                    Rgb::new(0x58, 0x6e, 0x75),
                    Rgb::new(0x26, 0x8b, 0xd2),
                    Rgb::new(0x2a, 0xa1, 0x98),
                    Rgb::new(0x85, 0x99, 0x00),
                    Rgb::new(0xb5, 0x89, 0x00),
                    Rgb::new(0xdc, 0x32, 0x2f),
                    Rgb::new(0x6c, 0x71, 0xc4),
                ),
            ),
            _ => return None,
        })
    }

    /// The default theme, also the base every custom theme extends.
    pub fn midnight() -> Self {
        Self::builtin("midnight").expect("midnight is built in")
    }

    pub fn from_recipe(name: &str, appearance: Appearance, syntax: &str, r: Recipe) -> Self {
        let dark = appearance == Appearance::Dark;
        let shade = |t: f32| {
            r.bg.mix(
                if dark {
                    Rgb::new(255, 255, 255)
                } else {
                    Rgb::new(0, 0, 0)
                },
                t,
            )
        };
        let recessive = |t: f32| r.fg.mix(r.bg, t);
        let surface = r.panel;
        Self {
            name: name.to_string(),
            appearance,
            syntax: syntax.to_string(),
            bg: r.bg,
            panel: surface,
            fg: r.fg,
            muted: recessive(0.42),
            faint: recessive(0.62),
            accent: r.accent,
            accent_alt: r.accent_alt,
            border: shade(0.14),
            border_focus: r.accent,
            selection: r.bg.mix(r.accent, if dark { 0.30 } else { 0.22 }),
            instruction: r.accent_alt.mix(r.fg, 0.25),
            success: r.success,
            warning: r.warning,
            error: r.error,
            info: r.info,
            heading: r.accent,
            link: r.accent_alt,
            mention: r.accent_alt.mix(r.accent, 0.35),
            path: r.accent_alt,
            inline_code_fg: r.accent_alt.mix(r.fg, 0.30),
            code_bg: shade(0.05),
            code_border: shade(0.18),
            quote: r.accent_alt.mix(r.fg.mix(r.bg, 0.55), 0.35),
            bullet: r.accent,
            number: r.accent_alt,
            table_border: shade(0.20),
            gutter: recessive(0.68),
            tree_dir: r.accent,
            tree_file: r.fg,
            tree_exec: r.success,
            tree_link: r.accent_alt,
            tab_active_bg: r.bg.mix(r.accent, if dark { 0.18 } else { 0.12 }),
            tab_active_fg: r.fg,
            tab_fg: recessive(0.35),
            chip_bg: shade(0.10),
            chip_fg: r.fg.mix(r.bg, 0.15),
            spinner: r.accent,
        }
    }

    pub fn is_dark(&self) -> bool {
        self.appearance == Appearance::Dark
    }

    pub fn outline(&self) -> String {
        format!("{} theme · {}", self.appearance_label(), self.name)
    }

    pub fn appearance_label(&self) -> &'static str {
        match self.appearance {
            Appearance::Dark => "dark",
            Appearance::Light => "light",
        }
    }

    /// Resolves a terminal colour for text or background use.
    pub fn resolve(&self, color: TermColor) -> Rgb {
        match color {
            TermColor::Default => self.fg,
            TermColor::Idx(index) => xterm256(index),
            TermColor::Rgb(r, g, b) => Rgb::new(r, g, b),
        }
    }

    pub fn resolve_bg(&self, color: TermColor) -> Rgb {
        match color {
            TermColor::Default => self.bg,
            other => self.resolve(other),
        }
    }

    /// Applies `name = "#rrggbb"` overrides, returning the first unknown slot.
    pub fn apply_overrides(&mut self, overrides: &BTreeMap<String, String>) -> Result<(), String> {
        for (slot, value) in overrides {
            let Some(color) = Rgb::parse(value) else {
                return Err(format!("`{value}` is not a colour (slot `{slot}`)"));
            };
            if !self.set_slot(slot, color) {
                return Err(format!("unknown theme slot `{slot}`"));
            }
        }
        Ok(())
    }

    slots!(
        bg,
        panel,
        fg,
        muted,
        faint,
        accent,
        accent_alt,
        border,
        border_focus,
        selection,
        instruction,
        success,
        warning,
        error,
        info,
        heading,
        link,
        mention,
        path,
        inline_code_fg,
        code_bg,
        code_border,
        quote,
        bullet,
        number,
        table_border,
        gutter,
        tree_dir,
        tree_file,
        tree_exec,
        tree_link,
        tab_active_bg,
        tab_active_fg,
        tab_fg,
        chip_bg,
        chip_fg,
        spinner,
    );

    /// A JSON theme file: `{ "extends": "midnight", "colors": { ... } }`.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let text =
            std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
        let file: ThemeFile =
            serde_json::from_str(&text).map_err(|err| format!("{}: {err}", path.display()))?;
        let base_name = file.extends.as_deref().unwrap_or("midnight");
        let mut theme =
            Self::builtin(base_name).ok_or_else(|| format!("unknown base theme `{base_name}`"))?;
        if let Some(name) = file.name {
            theme.name = name;
        }
        if let Some(appearance) = file.appearance {
            theme.appearance = appearance;
        }
        if let Some(syntax) = file.syntax {
            theme.syntax = syntax;
        }
        theme.apply_overrides(&file.colors)?;
        Ok(theme)
    }

    /// Built-ins plus every `*.json` theme file in `dir`, sorted by name.
    pub fn catalog(dir: &Path) -> Vec<Self> {
        let mut themes: Vec<Self> = Self::NAMES
            .iter()
            .filter_map(|name| Self::builtin(name))
            .collect();
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut custom: Vec<Self> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
                .filter_map(|path| Self::from_file(&path).ok())
                .collect();
            custom.sort_by(|a, b| a.name.cmp(&b.name));
            for theme in custom {
                themes.retain(|existing| existing.name != theme.name);
                themes.push(theme);
            }
        }
        themes
    }

    /// Resolves a name against built-ins and the theme directory, falling back
    /// to the default when it cannot be found.
    pub fn load(name: &str, dir: &Path) -> Self {
        if let Some(theme) = Self::builtin(name) {
            return theme;
        }
        let direct = dir.join(format!("{name}.json"));
        if let Ok(theme) = Self::from_file(&direct) {
            return theme;
        }
        Self::catalog(dir)
            .into_iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(Self::midnight)
    }
}

#[derive(Debug, Deserialize)]
struct ThemeFile {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    extends: Option<String>,
    #[serde(default)]
    appearance: Option<Appearance>,
    #[serde(default)]
    syntax: Option<String>,
    #[serde(default)]
    colors: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_named_colours() {
        assert_eq!(Rgb::parse("#0e1117"), Some(Rgb::new(14, 17, 23)));
        assert_eq!(Rgb::parse("#f80"), Some(Rgb::new(255, 136, 0)));
        assert_eq!(Rgb::parse("red"), Some(Rgb::new(205, 49, 49)));
        assert_eq!(Rgb::parse("not-a-colour"), None);
        assert_eq!(Rgb::new(14, 17, 23).to_hex(), "#0e1117");
    }

    #[test]
    fn xterm_palette_matches_the_standard() {
        assert_eq!(xterm256(0), Rgb::new(0, 0, 0));
        assert_eq!(xterm256(15), Rgb::new(255, 255, 255));
        assert_eq!(xterm256(16), Rgb::new(0, 0, 0));
        assert_eq!(xterm256(196), Rgb::new(255, 0, 0));
        assert_eq!(xterm256(231), Rgb::new(255, 255, 255));
        assert_eq!(xterm256(232), Rgb::new(8, 8, 8));
        assert_eq!(xterm256(255), Rgb::new(238, 238, 238));
    }

    #[test]
    fn every_builtin_has_a_name_and_contrasting_text() {
        for name in Theme::NAMES {
            let theme = Theme::builtin(name).unwrap();
            assert_eq!(theme.name, name);
            let contrast = (theme.fg.luminance() - theme.bg.luminance()).abs();
            assert!(
                contrast > 0.35,
                "{name} has too little contrast: {contrast}"
            );
        }
    }

    #[test]
    fn overrides_apply_and_reject_unknown_slots() {
        let mut theme = Theme::midnight();
        let mut overrides = BTreeMap::new();
        overrides.insert("accent".to_string(), "#ff0000".to_string());
        theme.apply_overrides(&overrides).unwrap();
        assert_eq!(theme.accent, Rgb::new(255, 0, 0));

        overrides.insert("accent".to_string(), "nope".to_string());
        assert!(theme.apply_overrides(&overrides).is_err());
        overrides.insert("nonsense".to_string(), "#ff0000".to_string());
        assert!(theme.apply_overrides(&overrides).is_err());
    }

    #[test]
    fn slots_cover_every_colour_field() {
        let theme = Theme::midnight();
        assert!(Theme::SLOTS.contains(&"accent"));
        assert!(Theme::SLOTS.contains(&"tree_dir"));
        assert_eq!(theme.slot("tree_dir"), Some(theme.tree_dir));
        assert_eq!(theme.slot("missing"), None);
    }

    #[test]
    fn custom_theme_file_extends_a_builtin() {
        let dir = std::env::temp_dir().join(format!("herdr-plus-theme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plum.json");
        std::fs::write(
            &path,
            r##"{ "name": "plum", "extends": "nord", "colors": { "accent": "#ff00ff" } }"##,
        )
        .unwrap();
        let theme = Theme::from_file(&path).unwrap();
        assert_eq!(theme.name, "plum");
        assert_eq!(theme.accent, Rgb::new(255, 0, 255));
        assert_eq!(theme.bg, Theme::builtin("nord").unwrap().bg);
        assert!(Theme::load("plum", &dir).name == "plum");
        std::fs::remove_dir_all(&dir).ok();
    }
}
