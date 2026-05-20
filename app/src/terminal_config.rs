//! Terminal configuration and theme system.
//!
//! Loads configuration from a local TOML file at
//! `~/.config/purewarp/config.toml`. Falls back to sensible defaults
//! when no config file is present.

use anyhow::{Context, Result};
use pathfinder_color::ColorU;
use serde::{Deserialize, Serialize};
use terminal_backend::CursorStyle;

/// Top-level PureWarp configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PureWarpConfig {
    /// Shell configuration.
    #[serde(default)]
    pub shell: ShellConfig,
    /// Terminal display configuration.
    #[serde(default)]
    pub terminal: TerminalConfig,
    /// Color theme.
    #[serde(default)]
    pub theme: ThemeConfig,
}

impl Default for PureWarpConfig {
    fn default() -> Self {
        Self {
            shell: ShellConfig::default(),
            terminal: TerminalConfig::default(),
            theme: ThemeConfig::default(),
        }
    }
}

/// Shell-related configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    /// Path to the shell program. Defaults to `$SHELL` or `/bin/bash`.
    #[serde(default = "default_shell")]
    pub program: String,
    /// Additional arguments to pass to the shell.
    #[serde(default)]
    pub args: Vec<String>,
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            program: default_shell(),
            args: Vec::new(),
        }
    }
}

/// Terminal display configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Font size in points.
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    /// Font family name.
    #[serde(default = "default_font_family")]
    pub font_family: String,
    /// Cursor style.
    #[serde(default)]
    pub cursor_style: CursorStyleConfig,
    /// Initial number of columns.
    #[serde(default = "default_cols")]
    pub cols: usize,
    /// Initial number of rows.
    #[serde(default = "default_rows")]
    pub rows: usize,
    /// Maximum scrollback lines.
    #[serde(default = "default_scrollback")]
    pub max_scrollback: usize,
}

fn default_font_size() -> f32 {
    14.0
}

fn default_font_family() -> String {
    "Menlo".to_string()
}

fn default_cols() -> usize {
    80
}

fn default_rows() -> usize {
    24
}

fn default_scrollback() -> usize {
    10000
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            font_size: default_font_size(),
            font_family: default_font_family(),
            cursor_style: CursorStyleConfig::default(),
            cols: default_cols(),
            rows: default_rows(),
            max_scrollback: default_scrollback(),
        }
    }
}

/// Cursor style configuration (serializable).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorStyleConfig {
    Block,
    Underline,
    Beam,
}

impl Default for CursorStyleConfig {
    fn default() -> Self {
        CursorStyleConfig::Block
    }
}

impl From<CursorStyleConfig> for CursorStyle {
    fn from(config: CursorStyleConfig) -> Self {
        match config {
            CursorStyleConfig::Block => CursorStyle::Block,
            CursorStyleConfig::Underline => CursorStyle::Underline,
            CursorStyleConfig::Beam => CursorStyle::Beam,
        }
    }
}

/// Color theme configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Background color (hex string like "#000000").
    #[serde(default = "default_background")]
    pub background: String,
    /// Foreground (text) color.
    #[serde(default = "default_foreground")]
    pub foreground: String,
    /// Cursor color.
    #[serde(default = "default_cursor")]
    pub cursor: String,
    /// ANSI 16-color palette (indices 0-15).
    #[serde(default = "default_palette")]
    pub palette: [String; 16],
}

fn default_background() -> String {
    "#f6edda".to_string()
}

fn default_foreground() -> String {
    "#005661".to_string()
}

fn default_cursor() -> String {
    "#00c6e0".to_string()
}

fn default_palette() -> [String; 16] {
    [
        // Normal
        "#003b42".to_string(), "#e34e1c".to_string(), "#00b368".to_string(),
        "#f49725".to_string(), "#0094f0".to_string(), "#ff5792".to_string(),
        "#00bdd6".to_string(), "#8ca6a6".to_string(),
        // Bright
        "#004d57".to_string(), "#ff4000".to_string(), "#00d17a".to_string(),
        "#ff8c00".to_string(), "#0fa3ff".to_string(), "#ff6b9f".to_string(),
        "#00cbe6".to_string(), "#bbc3c4".to_string(),
    ]
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            background: default_background(),
            foreground: default_foreground(),
            cursor: default_cursor(),
            palette: default_palette(),
        }
    }
}

impl ThemeConfig {
    /// Parse a hex color string like "#RRGGBB" into a `ColorU`.
    pub fn parse_color(hex: &str) -> Result<ColorU> {
        let hex = hex.trim().trim_start_matches('#');
        if hex.len() != 6 {
            anyhow::bail!("Invalid hex color: '{}'", hex);
        }
        let r = u8::from_str_radix(&hex[0..2], 16)
            .context("Invalid red component")?;
        let g = u8::from_str_radix(&hex[2..4], 16)
            .context("Invalid green component")?;
        let b = u8::from_str_radix(&hex[4..6], 16)
            .context("Invalid blue component")?;
        Ok(ColorU::new(r, g, b, 0xFF))
    }

    /// Apply this theme to a `ColorPalette`.
    pub fn apply_to_palette(&self, palette: &mut terminal_backend::ColorPalette) {
        for (i, hex) in self.palette.iter().enumerate() {
            if let Ok(color) = Self::parse_color(hex) {
                if i < palette.colors.len() {
                    palette.colors[i] = color;
                }
            }
        }
        if let Ok(color) = Self::parse_color(&self.foreground) {
            palette.foreground = color;
        }
        if let Ok(color) = Self::parse_color(&self.background) {
            palette.background = color;
        }
        if let Ok(color) = Self::parse_color(&self.cursor) {
            palette.cursor = color;
        }
    }
}

/// Load the configuration from the default path.
///
/// Searches `~/.config/purewarp/config.toml`. Returns defaults if the file
/// does not exist or cannot be parsed.
pub fn load_config() -> PureWarpConfig {
    let config_path = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("purewarp")
        .join("config.toml");

    match std::fs::read_to_string(&config_path) {
        Ok(contents) => {
            match toml::from_str::<PureWarpConfig>(&contents) {
                Ok(config) => {
                    log::info!("Loaded config from {}", config_path.display());
                    config
                }
                Err(e) => {
                    log::warn!(
                        "Failed to parse config at {}: {}. Using defaults.",
                        config_path.display(),
                        e
                    );
                    PureWarpConfig::default()
                }
            }
        }
        Err(_) => {
            log::info!(
                "No config file found at {}. Using defaults.",
                config_path.display()
            );
            PureWarpConfig::default()
        }
    }
}

/// Create the default config directory and write a default config file.
#[allow(dead_code)]
pub fn ensure_default_config() -> Result<()> {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("purewarp");

    std::fs::create_dir_all(&config_dir)
        .context("Failed to create config directory")?;

    let config_path = config_dir.join("config.toml");
    if !config_path.exists() {
        let default_config = PureWarpConfig::default();
        let toml_str = toml::to_string_pretty(&default_config)
            .context("Failed to serialize default config")?;
        std::fs::write(&config_path, toml_str)
            .context("Failed to write default config file")?;
        log::info!("Created default config at {}", config_path.display());
    }
    Ok(())
}
