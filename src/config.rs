use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to zig binary
    pub zig_path: Option<PathBuf>,

    /// Enable format on save
    pub format_on_save: bool,

    /// Enable diagnostics on save
    pub diagnostics_on_save: bool,

    /// Enable diagnostics on change
    pub diagnostics_on_change: bool,

    /// Debounce delay for diagnostics in milliseconds
    pub diagnostics_debounce_ms: u64,

    /// Log level
    pub log_level: String,

    /// Enable semantic tokens
    pub enable_semantic_tokens: bool,

    /// Enable inlay hints
    pub enable_inlay_hints: bool,

    /// Inlay hints trigger characters
    pub inlay_hints_trigger: Vec<String>,

    /// Build arguments
    pub build_args: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            zig_path: None,
            format_on_save: true,
            diagnostics_on_save: true,
            diagnostics_on_change: false,
            diagnostics_debounce_ms: 200,
            log_level: "info".to_string(),
            enable_semantic_tokens: true,
            enable_inlay_hints: false,
            inlay_hints_trigger: vec!["(".to_string(), ",".to_string()],
            build_args: vec![],
        }
    }
}

#[allow(dead_code)]
impl Config {
    /// Merge with another config (for workspace settings)
    pub fn merge(&mut self, other: Config) {
        if other.zig_path.is_some() {
            self.zig_path = other.zig_path;
        }
        // Only override if explicitly set
        if other.diagnostics_debounce_ms != 200 {
            self.diagnostics_debounce_ms = other.diagnostics_debounce_ms;
        }
    }

    /// Load config from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Save config to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.format_on_save);
        assert!(config.diagnostics_on_save);
        assert_eq!(config.diagnostics_debounce_ms, 200);
    }

    #[test]
    fn test_config_from_json() {
        let json = r#"{
            "zig_path": "/usr/bin/zig",
            "format_on_save": false,
            "log_level": "debug"
        }"#;

        let config = Config::from_json(json).unwrap();
        assert_eq!(config.zig_path, Some(PathBuf::from("/usr/bin/zig")));
        assert!(!config.format_on_save);
        assert_eq!(config.log_level, "debug");
    }
}
