use serde::Deserialize;

use crate::wlines::{self, Settings};

/// A menu color scheme: the six colors a `[themes.<name>]` theme defines, and
/// the same keys usable as top-level overrides. Config keys are short
/// (`bg`/`fg`/…); `bg_input`/`fg_input` map to the renderer's input-box fields
/// (`bg_edit`/`fg_edit`).
#[derive(Debug, Default, Deserialize)]
pub struct Palette {
    pub bg: Option<String>,        // Window background
    pub fg: Option<String>,        // Window text
    pub bg_select: Option<String>, // Selected item background
    pub fg_select: Option<String>, // Selected item text
    pub bg_input: Option<String>,  // Input box background
    pub fg_input: Option<String>,  // Input box text
}

impl Palette {
    /// Overlay the set colors onto `settings`, leaving unset fields alone.
    pub fn apply(&self, settings: &mut Settings) {
        apply_color(&mut settings.bg, &self.bg, "bg");
        apply_color(&mut settings.fg, &self.fg, "fg");
        apply_color(&mut settings.bg_select, &self.bg_select, "bg_select");
        apply_color(&mut settings.fg_select, &self.fg_select, "fg_select");
        apply_color(&mut settings.bg_edit, &self.bg_input, "bg_input");
        apply_color(&mut settings.fg_edit, &self.fg_input, "fg_input");
    }
}

fn apply_color(target: &mut u32, color: &Option<String>, name: &str) {
    if let Some(value) = color {
        match wlines::parse_color(value) {
            Some(parsed) => *target = parsed,
            None => eprintln!("Warning: invalid {} '{}', using default", name, value),
        }
    }
}

/// Parse a rofi-style font spec ("Family Size") into name and size. A trailing
/// integer is taken as the point size; otherwise the whole string is the family.
pub fn parse_font(spec: &str) -> (Option<String>, Option<i32>) {
    let spec = spec.trim();
    if let Some((name, last)) = spec.rsplit_once(char::is_whitespace) {
        if let Ok(size) = last.parse::<i32>() {
            let name = name.trim();
            let name = (!name.is_empty()).then(|| name.to_string());
            return (name, Some(size));
        }
    }
    ((!spec.is_empty()).then(|| spec.to_string()), None)
}

/// Apply a font spec ("Family Size") onto `settings`, updating only the parts
/// present in the spec.
pub fn apply_font(settings: &mut Settings, spec: &str) {
    let (name, size) = parse_font(spec);
    if let Some(name) = name {
        settings.font_name = name;
    }
    if let Some(size) = size {
        settings.font_size = size;
    }
}

/// windmenu's built-in color scheme (the "default" theme, a Windows-blue look).
/// Kept in sync with `[themes.default]` in the shipped windmenu.toml so that a
/// fresh `config init` reproduces the no-config appearance exactly.
pub fn default_palette() -> Palette {
    Palette {
        bg: Some("#1e1e1e".to_string()),
        fg: Some("#ffffff".to_string()),
        bg_select: Some("#0078d4".to_string()),
        fg_select: Some("#ffffff".to_string()),
        bg_input: Some("#2d2d2d".to_string()),
        fg_input: Some("#ffffff".to_string()),
    }
}

/// The renderer settings windmenu starts from before any config is applied:
/// wlines' bare defaults overlaid with windmenu's default window geometry and
/// the built-in color scheme.
pub fn default_settings() -> Settings {
    let mut settings = Settings {
        line_count: 12,
        padding: 8,
        width: 1000,
        center_window: true,
        ..Settings::default()
    };
    apply_font(&mut settings, "Consolas 20");
    default_palette().apply(&mut settings);
    settings
}

#[cfg(test)]
mod tests {
    use super::parse_font;

    #[test]
    fn font_family_and_size() {
        assert_eq!(parse_font("Consolas 18"), (Some("Consolas".to_string()), Some(18)));
    }

    #[test]
    fn font_multiword_family() {
        assert_eq!(parse_font("Cascadia Code 14"), (Some("Cascadia Code".to_string()), Some(14)));
    }

    #[test]
    fn font_no_size() {
        assert_eq!(parse_font("Consolas"), (Some("Consolas".to_string()), None));
    }

    #[test]
    fn font_trailing_nonnumeric() {
        // "Book" isn't a size, so the whole spec is the family.
        assert_eq!(parse_font("Sans Book"), (Some("Sans Book".to_string()), None));
    }
}
