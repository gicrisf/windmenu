use serde::Deserialize;

use crate::wlines::{self, Settings};

/// Appearance of the menu. Field names follow rofi's theme vocabulary:
/// `text_color` (not "foreground"), `entry_*` for the input box, a single
/// `font` string ("Family Size"), and `width`/`padding`/`selected_row`.
#[derive(Debug, Deserialize)]
pub struct WlinesTheme {
    pub lines: Option<usize>,                      // Lines to show
    pub prompt: Option<String>,                    // Prompt text
    pub selected_row: Option<usize>,               // Initial selected row
    pub padding: Option<usize>,                    // Window padding
    pub width: Option<usize>,                      // Window width (centers the window)
    pub background_color: Option<String>,          // Window background
    pub text_color: Option<String>,                // Window text
    pub selected_background_color: Option<String>, // Selected item background
    pub selected_text_color: Option<String>,       // Selected item text
    pub entry_background_color: Option<String>,    // Input box background
    pub entry_text_color: Option<String>,          // Input box text
    pub font: Option<String>,                      // Font as "Family Size", e.g. "Consolas 18"
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
fn parse_font(spec: &str) -> (Option<String>, Option<i32>) {
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

impl WlinesTheme {
    /// Convert the theme into renderer settings, keeping renderer defaults
    /// for any unset field.
    pub fn to_settings(&self) -> Settings {
        let mut settings = Settings::default();

        if let Some(lines) = self.lines {
            settings.line_count = lines;
        }
        settings.prompt = self.prompt.clone();
        if let Some(selected_row) = self.selected_row {
            settings.initial_index = selected_row;
        }
        if let Some(padding) = self.padding {
            settings.padding = padding as i32;
        }
        if let Some(width) = self.width {
            settings.width = width as i32;
            settings.center_window = true;
        }
        apply_color(&mut settings.bg, &self.background_color, "background_color");
        apply_color(&mut settings.fg, &self.text_color, "text_color");
        apply_color(&mut settings.bg_select, &self.selected_background_color, "selected_background_color");
        apply_color(&mut settings.fg_select, &self.selected_text_color, "selected_text_color");
        apply_color(&mut settings.bg_edit, &self.entry_background_color, "entry_background_color");
        apply_color(&mut settings.fg_edit, &self.entry_text_color, "entry_text_color");
        if let Some(ref font) = self.font {
            let (name, size) = parse_font(font);
            if let Some(name) = name {
                settings.font_name = name;
            }
            if let Some(size) = size {
                settings.font_size = size;
            }
        }

        settings
    }

    pub fn default() -> Self {
        WlinesTheme {
            lines: Some(12),
            prompt: None,
            selected_row: None,
            padding: Some(8),
            width: Some(1000),
            background_color: Some("#1e1e1e".to_string()),
            text_color: Some("#ffffff".to_string()),
            selected_background_color: Some("#0078d4".to_string()),
            selected_text_color: Some("#ffffff".to_string()),
            entry_background_color: Some("#2d2d2d".to_string()),
            entry_text_color: Some("#ffffff".to_string()),
            font: Some("Consolas 18".to_string()),
        }
    }
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
