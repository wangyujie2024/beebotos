//! Color Theme System for BeeBotOS Gateway Configuration Wizard
//!
//! Provides configurable color themes for terminal output with support for:
//! - Multiple preset themes (Default, Dark, Light, HighContrast, Minimal)
//! - Environment variable configuration (BEE__WIZARD__COLOR_THEME)
//! - Command line arguments (--theme, --no-color)
//! - CI/Log-friendly no-color mode

use std::env;
use std::str::FromStr;

use colored::Colorize;
use serde::{Deserialize, Serialize};

/// Available color themes for the configuration wizard
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorTheme {
    /// Default theme with vibrant colors
    #[default]
    Default,
    /// Dark background optimized theme
    Dark,
    /// Light background optimized theme
    Light,
    /// High contrast for accessibility
    HighContrast,
    /// Minimal colors, mostly white/gray
    Minimal,
    /// No colors (plain text)
    NoColor,
}

impl ColorTheme {
    /// Get theme name
    pub fn name(&self) -> &'static str {
        match self {
            ColorTheme::Default => "default",
            ColorTheme::Dark => "dark",
            ColorTheme::Light => "light",
            ColorTheme::HighContrast => "high_contrast",
            ColorTheme::Minimal => "minimal",
            ColorTheme::NoColor => "no_color",
        }
    }

    /// Get theme display name
    pub fn display_name(&self) -> &'static str {
        match self {
            ColorTheme::Default => "Default",
            ColorTheme::Dark => "Dark",
            ColorTheme::Light => "Light",
            ColorTheme::HighContrast => "High Contrast",
            ColorTheme::Minimal => "Minimal",
            ColorTheme::NoColor => "No Color",
        }
    }

    /// Get theme description
    pub fn description(&self) -> &'static str {
        match self {
            ColorTheme::Default => "Vibrant colors suitable for most terminals",
            ColorTheme::Dark => "Optimized for dark terminal backgrounds",
            ColorTheme::Light => "Optimized for light terminal backgrounds",
            ColorTheme::HighContrast => "High contrast colors for accessibility",
            ColorTheme::Minimal => "Minimal color usage, mostly monochrome",
            ColorTheme::NoColor => "No colors, plain text only (CI/Logs)",
        }
    }

    /// Check if colors are enabled for this theme
    pub fn colors_enabled(&self) -> bool {
        !matches!(self, ColorTheme::NoColor)
    }

    /// Get primary color (used for headers, banners)
    pub fn primary(&self) -> ThemeColor {
        match self {
            ColorTheme::Default => ThemeColor::Cyan,
            ColorTheme::Dark => ThemeColor::BrightCyan,
            ColorTheme::Light => ThemeColor::Blue,
            ColorTheme::HighContrast => ThemeColor::White,
            ColorTheme::Minimal => ThemeColor::White,
            ColorTheme::NoColor => ThemeColor::None,
        }
    }

    /// Get secondary color (used for subheaders)
    pub fn secondary(&self) -> ThemeColor {
        match self {
            ColorTheme::Default => ThemeColor::Yellow,
            ColorTheme::Dark => ThemeColor::BrightYellow,
            ColorTheme::Light => ThemeColor::Magenta,
            ColorTheme::HighContrast => ThemeColor::BrightWhite,
            ColorTheme::Minimal => ThemeColor::BrightBlack,
            ColorTheme::NoColor => ThemeColor::None,
        }
    }

    /// Get success color
    pub fn success(&self) -> ThemeColor {
        match self {
            ColorTheme::Default => ThemeColor::Green,
            ColorTheme::Dark => ThemeColor::BrightGreen,
            ColorTheme::Light => ThemeColor::Green,
            ColorTheme::HighContrast => ThemeColor::BrightGreen,
            ColorTheme::Minimal => ThemeColor::White,
            ColorTheme::NoColor => ThemeColor::None,
        }
    }

    /// Get error color
    pub fn error(&self) -> ThemeColor {
        match self {
            ColorTheme::Default => ThemeColor::Red,
            ColorTheme::Dark => ThemeColor::BrightRed,
            ColorTheme::Light => ThemeColor::Red,
            ColorTheme::HighContrast => ThemeColor::BrightRed,
            ColorTheme::Minimal => ThemeColor::BrightBlack,
            ColorTheme::NoColor => ThemeColor::None,
        }
    }

    /// Get warning color
    pub fn warning(&self) -> ThemeColor {
        match self {
            ColorTheme::Default => ThemeColor::Yellow,
            ColorTheme::Dark => ThemeColor::BrightYellow,
            ColorTheme::Light => ThemeColor::Yellow,
            ColorTheme::HighContrast => ThemeColor::BrightYellow,
            ColorTheme::Minimal => ThemeColor::BrightBlack,
            ColorTheme::NoColor => ThemeColor::None,
        }
    }

    /// Get info color
    pub fn info(&self) -> ThemeColor {
        match self {
            ColorTheme::Default => ThemeColor::Blue,
            ColorTheme::Dark => ThemeColor::BrightBlue,
            ColorTheme::Light => ThemeColor::Cyan,
            ColorTheme::HighContrast => ThemeColor::BrightCyan,
            ColorTheme::Minimal => ThemeColor::BrightBlack,
            ColorTheme::NoColor => ThemeColor::None,
        }
    }

    /// Get muted/dimmed color
    pub fn muted(&self) -> ThemeColor {
        match self {
            ColorTheme::Default => ThemeColor::BrightBlack,
            ColorTheme::Dark => ThemeColor::BrightBlack,
            ColorTheme::Light => ThemeColor::Black,
            ColorTheme::HighContrast => ThemeColor::White,
            ColorTheme::Minimal => ThemeColor::BrightBlack,
            ColorTheme::NoColor => ThemeColor::None,
        }
    }

    /// Get accent color (for highlights)
    pub fn accent(&self) -> ThemeColor {
        match self {
            ColorTheme::Default => ThemeColor::Magenta,
            ColorTheme::Dark => ThemeColor::BrightMagenta,
            ColorTheme::Light => ThemeColor::Blue,
            ColorTheme::HighContrast => ThemeColor::BrightMagenta,
            ColorTheme::Minimal => ThemeColor::White,
            ColorTheme::NoColor => ThemeColor::None,
        }
    }

    /// Get menu item color
    pub fn menu_item(&self) -> ThemeColor {
        match self {
            ColorTheme::Default => ThemeColor::Cyan,
            ColorTheme::Dark => ThemeColor::BrightCyan,
            ColorTheme::Light => ThemeColor::Blue,
            ColorTheme::HighContrast => ThemeColor::BrightWhite,
            ColorTheme::Minimal => ThemeColor::White,
            ColorTheme::NoColor => ThemeColor::None,
        }
    }

    /// Get menu description color
    pub fn menu_desc(&self) -> ThemeColor {
        match self {
            ColorTheme::Default => ThemeColor::BrightBlack,
            ColorTheme::Dark => ThemeColor::BrightBlack,
            ColorTheme::Light => ThemeColor::Black,
            ColorTheme::HighContrast => ThemeColor::White,
            ColorTheme::Minimal => ThemeColor::BrightBlack,
            ColorTheme::NoColor => ThemeColor::None,
        }
    }

    /// Get all available themes
    pub fn all_themes() -> &'static [ColorTheme] {
        &[
            ColorTheme::Default,
            ColorTheme::Dark,
            ColorTheme::Light,
            ColorTheme::HighContrast,
            ColorTheme::Minimal,
            ColorTheme::NoColor,
        ]
    }

    /// Apply theme globally (sets NO_COLOR env var if needed)
    pub fn apply(&self) {
        if matches!(self, ColorTheme::NoColor) {
            env::set_var("NO_COLOR", "1");
            colored::control::set_override(false);
        } else {
            colored::control::set_override(true);
        }
    }

    /// Detect theme from environment
    /// Priority: command line args > env vars > config file > auto-detect
    pub fn detect_from_env() -> Self {
        // Check for NO_COLOR environment variable (standard)
        if env::var("NO_COLOR").is_ok() {
            return ColorTheme::NoColor;
        }

        // Check for BeeBotOS specific theme env var
        if let Ok(theme_str) = env::var("BEE__WIZARD__COLOR_THEME") {
            if let Ok(theme) = ColorTheme::from_str(&theme_str) {
                return theme;
            }
        }

        // Check for FORCE_COLOR
        if let Ok(force) = env::var("FORCE_COLOR") {
            if force == "0" {
                return ColorTheme::NoColor;
            }
        }

        // Check if stdout is a TTY
        if !atty::is(atty::Stream::Stdout) {
            return ColorTheme::NoColor;
        }

        // Check for CI environment (disable colors in CI)
        if env::var("CI").is_ok() {
            return ColorTheme::NoColor;
        }

        // Default theme
        ColorTheme::Default
    }

    /// Parse theme from command line arguments
    pub fn from_args(args: &[String]) -> Option<Self> {
        for (i, arg) in args.iter().enumerate() {
            match arg.as_str() {
                "--no-color" | "--nocolor" => return Some(ColorTheme::NoColor),
                "--theme" => {
                    if let Some(theme_str) = args.get(i + 1) {
                        if let Ok(theme) = ColorTheme::from_str(theme_str) {
                            return Some(theme);
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
}

impl FromStr for ColorTheme {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(ColorTheme::Default),
            "dark" => Ok(ColorTheme::Dark),
            "light" => Ok(ColorTheme::Light),
            "high_contrast" | "high-contrast" | "highcontrast" => Ok(ColorTheme::HighContrast),
            "minimal" => Ok(ColorTheme::Minimal),
            "no_color" | "no-color" | "nocolor" | "none" => Ok(ColorTheme::NoColor),
            _ => Err(format!(
                "Unknown theme: {}. Available themes: default, dark, light, high_contrast, \
                 minimal, no_color",
                s
            )),
        }
    }
}

/// Theme-aware color wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeColor {
    None,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl ThemeColor {
    /// Apply color to a string
    pub fn apply(&self, text: &str) -> String {
        match self {
            ThemeColor::None => text.to_string(),
            ThemeColor::Black => text.black().to_string(),
            ThemeColor::Red => text.red().to_string(),
            ThemeColor::Green => text.green().to_string(),
            ThemeColor::Yellow => text.yellow().to_string(),
            ThemeColor::Blue => text.blue().to_string(),
            ThemeColor::Magenta => text.magenta().to_string(),
            ThemeColor::Cyan => text.cyan().to_string(),
            ThemeColor::White => text.white().to_string(),
            ThemeColor::BrightBlack => text.bright_black().to_string(),
            ThemeColor::BrightRed => text.bright_red().to_string(),
            ThemeColor::BrightGreen => text.bright_green().to_string(),
            ThemeColor::BrightYellow => text.bright_yellow().to_string(),
            ThemeColor::BrightBlue => text.bright_blue().to_string(),
            ThemeColor::BrightMagenta => text.bright_magenta().to_string(),
            ThemeColor::BrightCyan => text.bright_cyan().to_string(),
            ThemeColor::BrightWhite => text.bright_white().to_string(),
        }
    }

    /// Apply bold style
    pub fn bold(&self, text: &str) -> String {
        match self {
            ThemeColor::None => text.to_string(),
            ThemeColor::Black => text.black().bold().to_string(),
            ThemeColor::Red => text.red().bold().to_string(),
            ThemeColor::Green => text.green().bold().to_string(),
            ThemeColor::Yellow => text.yellow().bold().to_string(),
            ThemeColor::Blue => text.blue().bold().to_string(),
            ThemeColor::Magenta => text.magenta().bold().to_string(),
            ThemeColor::Cyan => text.cyan().bold().to_string(),
            ThemeColor::White => text.white().bold().to_string(),
            ThemeColor::BrightBlack => text.bright_black().bold().to_string(),
            ThemeColor::BrightRed => text.bright_red().bold().to_string(),
            ThemeColor::BrightGreen => text.bright_green().bold().to_string(),
            ThemeColor::BrightYellow => text.bright_yellow().bold().to_string(),
            ThemeColor::BrightBlue => text.bright_blue().bold().to_string(),
            ThemeColor::BrightMagenta => text.bright_magenta().bold().to_string(),
            ThemeColor::BrightCyan => text.bright_cyan().bold().to_string(),
            ThemeColor::BrightWhite => text.bright_white().bold().to_string(),
        }
    }

    /// Apply underline style
    pub fn underline(&self, text: &str) -> String {
        match self {
            ThemeColor::None => text.to_string(),
            ThemeColor::Black => text.black().underline().to_string(),
            ThemeColor::Red => text.red().underline().to_string(),
            ThemeColor::Green => text.green().underline().to_string(),
            ThemeColor::Yellow => text.yellow().underline().to_string(),
            ThemeColor::Blue => text.blue().underline().to_string(),
            ThemeColor::Magenta => text.magenta().underline().to_string(),
            ThemeColor::Cyan => text.cyan().underline().to_string(),
            ThemeColor::White => text.white().underline().to_string(),
            ThemeColor::BrightBlack => text.bright_black().underline().to_string(),
            ThemeColor::BrightRed => text.bright_red().underline().to_string(),
            ThemeColor::BrightGreen => text.bright_green().underline().to_string(),
            ThemeColor::BrightYellow => text.bright_yellow().underline().to_string(),
            ThemeColor::BrightBlue => text.bright_blue().underline().to_string(),
            ThemeColor::BrightMagenta => text.bright_magenta().underline().to_string(),
            ThemeColor::BrightCyan => text.bright_cyan().underline().to_string(),
            ThemeColor::BrightWhite => text.bright_white().underline().to_string(),
        }
    }
}

/// Theme-aware text styling utility
pub struct ThemedText {
    theme: ColorTheme,
}

impl ThemedText {
    /// Create new themed text utility
    pub fn new(theme: ColorTheme) -> Self {
        Self { theme }
    }

    /// Get the current theme
    pub fn theme(&self) -> ColorTheme {
        self.theme
    }

    /// Set a new theme
    pub fn set_theme(&mut self, theme: ColorTheme) {
        self.theme = theme;
        theme.apply();
    }

    /// Primary color text
    pub fn primary(&self, text: &str) -> String {
        self.theme.primary().apply(text)
    }

    /// Primary bold text
    pub fn primary_bold(&self, text: &str) -> String {
        self.theme.primary().bold(text)
    }

    /// Secondary color text
    pub fn secondary(&self, text: &str) -> String {
        self.theme.secondary().apply(text)
    }

    /// Secondary bold text
    pub fn secondary_bold(&self, text: &str) -> String {
        self.theme.secondary().bold(text)
    }

    /// Success text
    pub fn success(&self, text: &str) -> String {
        self.theme.success().apply(text)
    }

    /// Success bold text
    pub fn success_bold(&self, text: &str) -> String {
        self.theme.success().bold(text)
    }

    /// Error text
    pub fn error(&self, text: &str) -> String {
        self.theme.error().apply(text)
    }

    /// Error bold text
    pub fn error_bold(&self, text: &str) -> String {
        self.theme.error().bold(text)
    }

    /// Warning text
    pub fn warning(&self, text: &str) -> String {
        self.theme.warning().apply(text)
    }

    /// Warning bold text
    pub fn warning_bold(&self, text: &str) -> String {
        self.theme.warning().bold(text)
    }

    /// Info text
    pub fn info(&self, text: &str) -> String {
        self.theme.info().apply(text)
    }

    /// Info bold text
    pub fn info_bold(&self, text: &str) -> String {
        self.theme.info().bold(text)
    }

    /// Muted/dimmed text
    pub fn muted(&self, text: &str) -> String {
        self.theme.muted().apply(text)
    }

    /// Accent text
    pub fn accent(&self, text: &str) -> String {
        self.theme.accent().apply(text)
    }

    /// Accent bold text
    pub fn accent_bold(&self, text: &str) -> String {
        self.theme.accent().bold(text)
    }

    /// Menu item text
    pub fn menu_item(&self, text: &str) -> String {
        self.theme.menu_item().apply(text)
    }

    /// Menu description text
    pub fn menu_desc(&self, text: &str) -> String {
        self.theme.menu_desc().apply(text)
    }

    /// Primary underline text
    pub fn primary_underline(&self, text: &str) -> String {
        self.theme.primary().underline(text)
    }

    /// Format a success checkmark with text
    pub fn success_icon(&self, text: &str) -> String {
        format!("{} {}", self.success("✓"), text)
    }

    /// Format an error icon with text
    pub fn error_icon(&self, text: &str) -> String {
        format!("{} {}", self.error("✗"), text)
    }

    /// Format a warning icon with text
    pub fn warning_icon(&self, text: &str) -> String {
        format!("{} {}", self.warning("⚠"), text)
    }

    /// Format an info icon with text
    pub fn info_icon(&self, text: &str) -> String {
        format!("{} {}", self.info("ℹ"), text)
    }
}

impl Default for ThemedText {
    fn default() -> Self {
        Self::new(ColorTheme::Default)
    }
}

/// Configuration for wizard appearance
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WizardConfig {
    #[serde(default)]
    pub color_theme: ColorTheme,
    #[serde(default = "default_true")]
    pub show_icons: bool,
    #[serde(default = "default_true")]
    pub show_emoji: bool,
    #[serde(default = "default_true")]
    pub use_unicode: bool,
}

impl Default for WizardConfig {
    fn default() -> Self {
        Self {
            color_theme: ColorTheme::Default,
            show_icons: true,
            show_emoji: true,
            use_unicode: true,
        }
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_theme_from_str() {
        assert_eq!(
            ColorTheme::from_str("default").unwrap(),
            ColorTheme::Default
        );
        assert_eq!(ColorTheme::from_str("dark").unwrap(), ColorTheme::Dark);
        assert_eq!(ColorTheme::from_str("light").unwrap(), ColorTheme::Light);
        assert_eq!(
            ColorTheme::from_str("high_contrast").unwrap(),
            ColorTheme::HighContrast
        );
        assert_eq!(
            ColorTheme::from_str("high-contrast").unwrap(),
            ColorTheme::HighContrast
        );
        assert_eq!(
            ColorTheme::from_str("minimal").unwrap(),
            ColorTheme::Minimal
        );
        assert_eq!(
            ColorTheme::from_str("no_color").unwrap(),
            ColorTheme::NoColor
        );
        assert_eq!(
            ColorTheme::from_str("no-color").unwrap(),
            ColorTheme::NoColor
        );
        assert!(ColorTheme::from_str("unknown").is_err());
    }

    #[test]
    fn test_color_theme_names() {
        assert_eq!(ColorTheme::Default.name(), "default");
        assert_eq!(ColorTheme::Dark.name(), "dark");
        assert_eq!(ColorTheme::NoColor.name(), "no_color");
    }

    #[test]
    fn test_color_theme_display_names() {
        assert_eq!(ColorTheme::Default.display_name(), "Default");
        assert_eq!(ColorTheme::HighContrast.display_name(), "High Contrast");
        assert_eq!(ColorTheme::NoColor.display_name(), "No Color");
    }

    #[test]
    fn test_colors_enabled() {
        assert!(ColorTheme::Default.colors_enabled());
        assert!(ColorTheme::Dark.colors_enabled());
        assert!(!ColorTheme::NoColor.colors_enabled());
    }

    #[test]
    fn test_theme_from_args() {
        let args = vec!["--theme".to_string(), "dark".to_string()];
        assert_eq!(ColorTheme::from_args(&args), Some(ColorTheme::Dark));

        let args = vec!["--no-color".to_string()];
        assert_eq!(ColorTheme::from_args(&args), Some(ColorTheme::NoColor));

        let args = vec!["other".to_string()];
        assert_eq!(ColorTheme::from_args(&args), None);
    }

    #[test]
    fn test_themed_text() {
        let themed = ThemedText::new(ColorTheme::NoColor);
        assert_eq!(themed.primary("test"), "test");
        assert_eq!(themed.success_bold("test"), "test");

        let themed = ThemedText::new(ColorTheme::Default);
        assert_ne!(themed.primary("test"), "test"); // Should have ANSI codes
    }

    #[test]
    fn test_all_themes() {
        let themes = ColorTheme::all_themes();
        assert_eq!(themes.len(), 6);
        assert!(themes.contains(&ColorTheme::Default));
        assert!(themes.contains(&ColorTheme::NoColor));
    }

    #[test]
    fn test_wizard_config_default() {
        let config = WizardConfig::default();
        assert!(config.show_icons);
        assert!(config.show_emoji);
        assert!(config.use_unicode);
        assert_eq!(config.color_theme, ColorTheme::Default);
    }
}
