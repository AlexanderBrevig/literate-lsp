use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

// Embedded Helix languages.toml as fallback default
const HELIX_LANGUAGES_TOML: &str = include_str!("../helix-languages.toml");

/// Forbidden documentation formats that cannot be child LSPs
/// This prevents fork bombs - literate-lsp must never be a child of itself
/// Only includes formats that can contain code blocks for language-specific processing
pub const FORBIDDEN_FORMATS: &[&str] = &[
    "md",
    "markdown",
    "typst",
    "rst",
    "restructuredtext",
    "org",
    "asciidoc",
    "latex",
    "tex",
];

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LspConfig {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct LanguageServerRef {
    pub name: String,
    #[serde(default)]
    pub except_features: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum LanguageServerEntry {
    String(String),
    Object(LanguageServerRef),
}

#[derive(Debug, Clone, Deserialize)]
pub struct LanguageConfig {
    pub name: String,
    #[serde(default)]
    #[serde(rename = "language-servers")]
    pub language_servers: Vec<LanguageServerEntry>,
}

impl LanguageConfig {
    /// Extract language server names, handling both string and object variants
    pub fn get_server_names(&self) -> Vec<String> {
        self.language_servers
            .iter()
            .map(|entry| entry.to_server_name())
            .collect()
    }
}

/// Extension trait for extracting server names from LanguageServerEntry
pub trait LanguageServerEntryExt {
    /// Extract the server name regardless of variant (String or Object)
    fn to_server_name(&self) -> String;
}

impl LanguageServerEntryExt for LanguageServerEntry {
    fn to_server_name(&self) -> String {
        match self {
            LanguageServerEntry::String(s) => s.clone(),
            LanguageServerEntry::Object(obj) => obj.name.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub language: Vec<LanguageConfig>,
    #[serde(rename = "language-server")]
    pub language_server: HashMap<String, LspConfig>,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load from default Helix config location (~/.config/helix/languages.toml)
    /// Works cross-platform on macOS, Linux, and Windows
    pub fn load_from_helix_config() -> Result<Self> {
        let home_dir = home::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        let config_path = home_dir.join(".config/helix/languages.toml");
        Self::load(
            config_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
        )
    }

    /// Load with local overrides
    ///
    /// Loads from Helix config first, then merges with local ./.languages.toml if it exists.
    /// Local config takes precedence over Helix config.
    /// Automatically filters out forbidden documentation formats.
    pub fn load_with_local_overrides() -> Self {
        Self::load_with_local_overrides_internal(true)
    }

    /// Internal version with control over logging
    /// Merging order: embedded default → ~/.config/helix/languages.toml → ./.languages.toml
    fn load_with_local_overrides_internal(verbose: bool) -> Self {
        // Start with embedded default
        let mut config = Self::default();
        if verbose {
            info!("Loaded embedded default Helix config");
        }

        // Try to merge ~/.config/helix/languages.toml
        match Self::load_from_helix_config() {
            Ok(helix_config) => {
                if verbose {
                    info!("Merging ~/.config/helix/languages.toml");
                    info!("  Languages: {}", helix_config.language.len());
                }
                config.merge(helix_config);
            }
            Err(e) => {
                // Show error but continue with default
                warn!(
                    "Failed to load ~/.config/helix/languages.toml: {}",
                    e
                );
            }
        }

        // Try to load and merge local ./.languages.toml
        if let Ok(local_config) = Self::load("./.languages.toml") {
            if verbose {
                info!("Merging ./.languages.toml");
                info!("  Languages: {}", local_config.language.len());
            }
            config.merge(local_config);
        }

        // Filter out forbidden formats to prevent fork bombs
        config.filter_forbidden_formats();

        config
    }

    /// Load for health check (less verbose)
    pub fn load_for_health_check() -> Self {
        Self::load_with_local_overrides_internal(false)
    }

    /// Merge another config into this one (other takes precedence)
    /// For language-servers: user's list completely replaces the default
    pub fn merge(&mut self, other: Config) {
        for (key, value) in other.language_server {
            self.language_server.insert(key, value);
        }
        // Merge languages: user's language-servers list replaces the embedded default
        for other_lang in other.language {
            if let Some(existing) = self.language.iter_mut().find(|l| l.name == other_lang.name) {
                // Replace the language-servers list with the user's version
                existing.language_servers = other_lang.language_servers;
            } else {
                // Add new language if it doesn't exist
                self.language.push(other_lang);
            }
        }
    }

    /// Remove forbidden documentation formats to prevent fork bombs
    fn filter_forbidden_formats(&mut self) {
        let forbidden_lsps = self.get_forbidden_lsps();
        self.language_server.retain(|lang, _| {
            let lower = lang.to_lowercase();
            !FORBIDDEN_FORMATS.contains(&lower.as_str()) && !forbidden_lsps.contains(&lower)
        });
    }

    /// Get LSP command for a language
    /// Walks the language-servers list and returns the first one that exists and has a command
    /// First checks if lang is a direct LSP name, then checks if it's a language with LSPs
    pub fn get_command(&self, lang: &str) -> Option<String> {
        // First try direct LSP lookup
        if let Some(lsp) = self.language_server.get(lang) {
            if !lsp.command.is_empty() {
                return Some(lsp.command.clone());
            }
        }

        // If not found, try to find the language and walk its LSPs
        if let Some(language) = self.language.iter().find(|l| l.name == lang) {
            for server_entry in &language.language_servers {
                let server_name = server_entry.to_server_name();

                // Skip forbidden LSPs
                if self.is_format_forbidden(&server_name) {
                    continue;
                }

                // Check if this LSP has a command and it exists in PATH
                if let Some(lsp_cfg) = self.language_server.get(&server_name) {
                    if !lsp_cfg.command.is_empty() {
                        // Check if the binary exists in PATH
                        if Self::command_exists(&lsp_cfg.command) {
                            return Some(lsp_cfg.command.clone());
                        }
                    }
                }
            }
        }

        None
    }

    /// Check if a command exists in PATH
    fn command_exists(cmd: &str) -> bool {
        if let Ok(path_env) = std::env::var("PATH") {
            for path_dir in path_env.split(':') {
                let full_path = std::path::PathBuf::from(path_dir).join(cmd);
                if full_path.exists() {
                    return true;
                }
            }
        }
        false
    }

    /// Get arguments for an LSP command
    pub fn get_args(&self, lang: &str) -> Vec<String> {
        self.language_server
            .get(lang)
            .map(|lsp| lsp.args.clone())
            .unwrap_or_default()
    }

    /// Get both command and args for a language
    /// Returns (command, args) by walking the language-servers list
    pub fn get_command_and_args(&self, lang: &str) -> Option<(String, Vec<String>)> {
        // First try direct LSP lookup
        if let Some(lsp) = self.language_server.get(lang) {
            if !lsp.command.is_empty() {
                return Some((lsp.command.clone(), lsp.args.clone()));
            }
        }

        // If not found, try to find the language and walk its LSPs
        if let Some(language) = self.language.iter().find(|l| l.name == lang) {
            info!("[Config] Found language '{}' with {} servers", lang, language.language_servers.len());
            for server_entry in &language.language_servers {
                let server_name = server_entry.to_server_name();

                info!("[Config] Checking server '{}'", server_name);

                // Skip forbidden LSPs
                if self.is_format_forbidden(&server_name) {
                    info!("[Config] Server '{}' is forbidden", server_name);
                    continue;
                }

                // Check if this LSP has a command and it exists in PATH
                if let Some(lsp_cfg) = self.language_server.get(&server_name) {
                    info!("[Config] Server '{}' command: '{}'", server_name, lsp_cfg.command);
                    if !lsp_cfg.command.is_empty() {
                        // Check if the binary exists in PATH
                        if Self::command_exists(&lsp_cfg.command) {
                            info!("[Config] Server '{}' command found in PATH", server_name);
                            return Some((lsp_cfg.command.clone(), lsp_cfg.args.clone()));
                        } else {
                            info!("[Config] Server '{}' command NOT found in PATH", server_name);
                        }
                    }
                } else {
                    info!("[Config] Server '{}' not found in language_server map", server_name);
                }
            }
        } else {
            info!("[Config] Language '{}' not found in config", lang);
        }

        None
    }

    /// Get initialization options for an LSP
    pub fn get_init_options(&self, lang: &str) -> Option<serde_json::Value> {
        self.language_server.get(lang).and_then(|lsp| {
            if lsp.config.is_null() {
                None
            } else {
                Some(lsp.config.clone())
            }
        })
    }

    /// Get all LSPs that handle forbidden documentation formats
    /// Excludes literate-lsp since it's the parent LSP, not a child
    pub fn get_forbidden_lsps(&self) -> Vec<String> {
        let mut forbidden_lsps = Vec::new();

        // Find all LSPs used by forbidden formats
        for lang in &self.language {
            if FORBIDDEN_FORMATS.contains(&lang.name.as_str())
                || FORBIDDEN_FORMATS.iter().any(|fmt| lang.name.contains(fmt))
            {
                for server_name in lang.get_server_names() {
                    // Never forbid literate-lsp itself - it's the parent LSP
                    if server_name.to_lowercase() == "literate-lsp" {
                        continue;
                    }
                    if !forbidden_lsps.contains(&server_name) {
                        forbidden_lsps.push(server_name);
                    }
                }
            }
        }

        forbidden_lsps
    }

    /// Check if a format/LSP name is forbidden as a child LSP
    pub fn is_format_forbidden(&self, name: &str) -> bool {
        let lower = name.to_lowercase();

        // Check if it's a forbidden format
        if FORBIDDEN_FORMATS.contains(&lower.as_str()) {
            return true;
        }

        // Check if it's an LSP for a forbidden format
        self.get_forbidden_lsps()
            .iter()
            .any(|lsp| lsp.to_lowercase() == lower)
    }

    /// Static method for backward compatibility (checks only format names)
    pub fn is_format_forbidden_static(lang: &str) -> bool {
        let lower = lang.to_lowercase();
        FORBIDDEN_FORMATS.contains(&lower.as_str())
    }
}

impl Default for Config {
    fn default() -> Self {
        // Load the embedded Helix languages.toml as default
        match toml::from_str::<Config>(HELIX_LANGUAGES_TOML) {
            Ok(cfg) => cfg,
            Err(e) => {
                warn!("Failed to parse embedded Helix config: {}", e);
                // Minimal fallback if something goes wrong
                let mut language_server = HashMap::new();
                language_server.insert(
                    "forth".to_string(),
                    LspConfig {
                        command: "forth-lsp".to_string(),
                        args: vec![],
                        config: serde_json::json!({}),
                    },
                );
                Config {
                    language: vec![],
                    language_server,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forbidden_formats_are_detected() {
        let config: Config = toml::from_str("").unwrap_or_default();
        assert!(config.is_format_forbidden("md"));
        assert!(config.is_format_forbidden("markdown"));
        assert!(config.is_format_forbidden("typst"));
        assert!(config.is_format_forbidden("rst"));
        assert!(config.is_format_forbidden("MD")); // Case insensitive
        assert!(config.is_format_forbidden("Markdown"));
    }

    #[test]
    fn test_allowed_formats_are_not_forbidden() {
        let config: Config = toml::from_str(
            r#"
[[language]]
name = "forth"
language-servers = ["forth-lsp"]

[[language]]
name = "python"
language-servers = ["pyright"]

[language-server.forth-lsp]
command = "forth-lsp"

[language-server.pyright]
command = "pyright"
"#,
        )
        .unwrap();

        assert!(!config.is_format_forbidden("forth"));
        assert!(!config.is_format_forbidden("python"));
        assert!(!config.is_format_forbidden("forth-lsp"));
        assert!(!config.is_format_forbidden("pyright"));
    }

    #[test]
    fn test_forbidden_lsps_are_dynamically_determined() {
        let config: Config = toml::from_str(
            r#"
[[language]]
name = "markdown"
language-servers = ["marksman"]

[[language]]
name = "python"
language-servers = ["pyright"]

[[language]]
name = "typst"
language-servers = ["tinymist"]

[language-server.marksman]
command = "marksman"

[language-server.pyright]
command = "pyright"

[language-server.tinymist]
command = "tinymist"
"#,
        )
        .unwrap();

        let forbidden = config.get_forbidden_lsps();
        assert!(forbidden.contains(&"marksman".to_string()));
        assert!(forbidden.contains(&"tinymist".to_string()));
        assert!(!forbidden.contains(&"pyright".to_string()));
    }

    #[test]
    fn test_config_filters_forbidden_formats() {
        let toml_str = r#"
[language-server.markdown]
command = "marksman"

[language-server.forth]
command = "forth-lsp"

[language-server.typst]
command = "tinymist"
"#;

        let mut config: Config = toml::from_str(toml_str).unwrap();
        config.filter_forbidden_formats();

        assert!(config.language_server.contains_key("forth"));
        assert!(!config.language_server.contains_key("markdown"));
        assert!(!config.language_server.contains_key("typst"));
    }

    #[test]
    fn test_lenient_lsp_config_parsing() {
        // Test that LspConfig can deserialize even with unknown fields
        let toml_str = r#"
[language-server.pyright]
command = "pyright"
args = ["--option1"]
unknown_field = "should be ignored"
extra_config = { nested = "value" }
"#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.language_server.contains_key("pyright"));
        let pyright = config.language_server.get("pyright").unwrap();
        assert_eq!(pyright.command, "pyright");
        assert_eq!(pyright.args, vec!["--option1"]);
    }

    #[test]
    fn test_nested_config_only_without_command() {
        // Test what happens when only nested config is defined, no command
        let toml_str = r#"
[language-server.pyright.config.python.analysis]
typeCheckingMode = "basic"
"#;

        let config: Config = toml::from_str(toml_str).unwrap();
        if let Some(pyright) = config.language_server.get("pyright") {
            eprintln!("pyright found: command='{}', config={:?}", pyright.command, pyright.config);
        } else {
            eprintln!("pyright NOT found in language_server");
        }
    }
}
