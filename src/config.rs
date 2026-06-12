use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const EMBEDDED_DEFAULTS: &str = include_str!("../xray.yml");

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FormatMode {
    #[default]
    Auto,
    Json,
    Yaml,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Defaults {
    pub budget: usize,
    pub format: FormatMode,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            budget: 0,
            format: FormatMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DirectoryRules {
    pub hidden: Vec<String>,
    pub collapse: Vec<String>,
    pub expand: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct LanguageConfig {
    pub detect: Vec<String>,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub collapse: Vec<String>,
    #[serde(default)]
    pub kinds: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub defaults: Defaults,
    pub directories: DirectoryRules,
    pub languages: HashMap<String, LanguageConfig>,
}

/// XDG config dir, honoring `$XDG_CONFIG_HOME` and falling back to `$HOME/.config`.
///
/// We deliberately do NOT use `dirs::config_dir()`: it honors `$XDG_CONFIG_HOME`
/// only on Linux. On macOS it resolves via system APIs and returns `~/Library/...`,
/// ignoring the env var. This helper resolves to the same XDG layout on every platform.
fn xdg_config_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        let path = PathBuf::from(dir);
        if path.is_absolute() {
            return Some(path);
        }
    }
    dirs::home_dir().map(|h| h.join(".config"))
}

impl Config {
    /// Load configuration with fallback chain:
    /// embedded defaults → ~/.config/xray/xray.yml → --config override
    pub fn load(config_path: Option<&PathBuf>) -> Result<Self> {
        // Start with embedded defaults
        let base: Config = serde_yaml::from_str(EMBEDDED_DEFAULTS).context("Failed to parse embedded defaults")?;

        // If explicit config path provided, load and merge
        if let Some(path) = config_path {
            let user = Self::load_from_file(path).context(format!("Failed to load config from {}", path.display()))?;
            return Ok(Self::merge(base, user));
        }

        // Try ~/.config/xray/xray.yml
        if let Some(config_dir) = xdg_config_dir() {
            let global_config = config_dir.join("xray").join("xray.yml");
            if global_config.exists() {
                match Self::load_from_file(&global_config) {
                    Ok(user) => return Ok(Self::merge(base, user)),
                    Err(e) => {
                        eprintln!("warning: failed to load {}: {}", global_config.display(), e);
                    }
                }
            }
        }

        Ok(base)
    }

    fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path).context("Failed to read config file")?;
        let config: Self = serde_yaml::from_str(&content).context("Failed to parse config file")?;
        Ok(config)
    }

    /// Merge user config on top of base. Language-specific hidden/collapse
    /// lists are unioned with the global lists (additive).
    fn merge(base: Self, user: Self) -> Self {
        let mut directories = base.directories;
        for item in &user.directories.hidden {
            if !directories.hidden.contains(item) {
                directories.hidden.push(item.clone());
            }
        }
        for item in &user.directories.collapse {
            if !directories.collapse.contains(item) {
                directories.collapse.push(item.clone());
            }
        }
        for item in &user.directories.expand {
            if !directories.expand.contains(item) {
                directories.expand.push(item.clone());
            }
        }

        let mut languages = base.languages;
        for (lang, user_lang) in user.languages {
            languages
                .entry(lang)
                .and_modify(|base_lang| {
                    if !user_lang.detect.is_empty() {
                        base_lang.detect = user_lang.detect.clone();
                    }
                    if !user_lang.extensions.is_empty() {
                        base_lang.extensions = user_lang.extensions.clone();
                    }
                    for item in &user_lang.hidden {
                        if !base_lang.hidden.contains(item) {
                            base_lang.hidden.push(item.clone());
                        }
                    }
                    for item in &user_lang.collapse {
                        if !base_lang.collapse.contains(item) {
                            base_lang.collapse.push(item.clone());
                        }
                    }
                    for (kind, patterns) in &user_lang.kinds {
                        base_lang
                            .kinds
                            .entry(kind.clone())
                            .or_default()
                            .extend(patterns.clone());
                    }
                })
                .or_insert(user_lang);
        }

        Self {
            defaults: Defaults {
                budget: if user.defaults.budget != 0 {
                    user.defaults.budget
                } else {
                    base.defaults.budget
                },
                format: user.defaults.format,
            },
            directories,
            languages,
        }
    }

    /// Get effective hidden directories: global + language-specific
    pub fn effective_hidden(&self, detected_languages: &[String]) -> Vec<String> {
        let mut hidden = self.directories.hidden.clone();
        for lang in detected_languages {
            if let Some(lang_config) = self.languages.get(lang) {
                for item in &lang_config.hidden {
                    if !hidden.contains(item) {
                        hidden.push(item.clone());
                    }
                }
            }
        }
        hidden
    }

    /// Get effective collapse directories: global + language-specific
    pub fn effective_collapse(&self, detected_languages: &[String]) -> Vec<String> {
        let mut collapse = self.directories.collapse.clone();
        for lang in detected_languages {
            if let Some(lang_config) = self.languages.get(lang) {
                for item in &lang_config.collapse {
                    if !collapse.contains(item) {
                        collapse.push(item.clone());
                    }
                }
            }
        }
        collapse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_defaults_parse() {
        let config: Config = serde_yaml::from_str(EMBEDDED_DEFAULTS).expect("embedded defaults should parse");
        assert!(!config.directories.hidden.is_empty());
        assert!(config.languages.contains_key("rust"));
        assert!(config.languages.contains_key("python"));
        assert!(config.languages.contains_key("typescript"));
    }

    #[test]
    fn test_default_budget_is_unlimited() {
        let config: Config = serde_yaml::from_str(EMBEDDED_DEFAULTS).expect("embedded defaults should parse");
        assert_eq!(config.defaults.budget, 0);
    }

    #[test]
    fn test_effective_hidden_merges_language() {
        let config: Config = serde_yaml::from_str(EMBEDDED_DEFAULTS).expect("embedded defaults should parse");
        let hidden = config.effective_hidden(&["rust".to_string()]);
        assert!(hidden.contains(&"target".to_string()));
        assert!(hidden.contains(&".git".to_string()));
    }

    #[test]
    fn test_load_with_no_config_uses_defaults() {
        let config = Config::load(None).expect("should load defaults");
        assert!(!config.directories.hidden.is_empty());
    }
}
