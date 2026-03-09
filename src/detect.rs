use crate::config::Config;
use std::path::Path;

/// Detect which languages are present in the target directory by checking for marker files.
pub fn detect_languages(root: &Path, config: &Config) -> Vec<String> {
    let mut detected = Vec::new();
    for (lang_name, lang_config) in &config.languages {
        for marker in &lang_config.detect {
            if root.join(marker).exists() {
                detected.push(lang_name.clone());
                break;
            }
        }
    }
    detected.sort();
    detected
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn test_config() -> Config {
        Config::load(None).expect("should load defaults")
    }

    #[test]
    fn test_detect_rust_project() {
        let dir = TempDir::new().expect("temp dir");
        fs::write(dir.path().join("Cargo.toml"), "[package]").expect("write");
        let langs = detect_languages(dir.path(), &test_config());
        assert!(langs.contains(&"rust".to_string()));
    }

    #[test]
    fn test_detect_python_project() {
        let dir = TempDir::new().expect("temp dir");
        fs::write(dir.path().join("pyproject.toml"), "[project]").expect("write");
        let langs = detect_languages(dir.path(), &test_config());
        assert!(langs.contains(&"python".to_string()));
    }

    #[test]
    fn test_detect_multi_language() {
        let dir = TempDir::new().expect("temp dir");
        fs::write(dir.path().join("Cargo.toml"), "").expect("write");
        fs::write(dir.path().join("pyproject.toml"), "").expect("write");
        let langs = detect_languages(dir.path(), &test_config());
        assert!(langs.contains(&"rust".to_string()));
        assert!(langs.contains(&"python".to_string()));
    }

    #[test]
    fn test_detect_no_language() {
        let dir = TempDir::new().expect("temp dir");
        let langs = detect_languages(dir.path(), &test_config());
        assert!(langs.is_empty());
    }
}
