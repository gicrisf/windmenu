use serde::Deserialize;

use crate::wlines::{self, Settings};

#[derive(Debug, Deserialize)]
pub struct WlinesTheme {
    pub lines: Option<usize>,                    // Lines to show
    pub prompt: Option<String>,                  // Prompt text
    pub selected_index: Option<usize>,           // Initial selected index
    pub padding_x: Option<usize>,                // Window padding
    pub width_x: Option<usize>,                  // Window width (centers the window)
    pub background_color: Option<String>,        // Background color
    pub foreground_color: Option<String>,        // Foreground color
    pub selected_background_color: Option<String>, // Selected bg color
    pub selected_foreground_color: Option<String>, // Selected fg color
    pub text_background_color: Option<String>,   // Text input bg
    pub text_foreground_color: Option<String>,   // Text input fg
    pub font_name: Option<String>,               // Font name
    pub font_size: Option<usize>,                // Font size
}

fn apply_color(target: &mut u32, color: &Option<String>, name: &str) {
    if let Some(value) = color {
        match wlines::parse_color(value) {
            Some(parsed) => *target = parsed,
            None => eprintln!("Warning: invalid {} '{}', using default", name, value),
        }
    }
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
        if let Some(selected_index) = self.selected_index {
            settings.initial_index = selected_index;
        }
        if let Some(padding_x) = self.padding_x {
            settings.padding = padding_x as i32;
        }
        if let Some(width_x) = self.width_x {
            settings.width = width_x as i32;
            settings.center_window = true;
        }
        apply_color(&mut settings.bg, &self.background_color, "background_color");
        apply_color(&mut settings.fg, &self.foreground_color, "foreground_color");
        apply_color(&mut settings.bg_select, &self.selected_background_color, "selected_background_color");
        apply_color(&mut settings.fg_select, &self.selected_foreground_color, "selected_foreground_color");
        apply_color(&mut settings.bg_edit, &self.text_background_color, "text_background_color");
        apply_color(&mut settings.fg_edit, &self.text_foreground_color, "text_foreground_color");
        if let Some(ref font_name) = self.font_name {
            settings.font_name = font_name.clone();
        }
        if let Some(font_size) = self.font_size {
            settings.font_size = font_size as i32;
        }

        settings
    }

    pub fn default() -> Self {
        WlinesTheme {
            lines: Some(12),
            prompt: None,
            selected_index: None,
            padding_x: Some(8),
            width_x: Some(1000),
            background_color: Some("#1e1e1e".to_string()),
            foreground_color: Some("#ffffff".to_string()),
            selected_background_color: Some("#0078d4".to_string()),
            selected_foreground_color: Some("#ffffff".to_string()),
            text_background_color: Some("#2d2d2d".to_string()),
            text_foreground_color: Some("#ffffff".to_string()),
            font_name: Some("Consolas".to_string()),
            font_size: Some(18),
        }
    }
}
