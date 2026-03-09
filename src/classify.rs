use crate::config::Config;
use crate::skeleton::simple_glob_match;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    Test,
    Ci,
    Config,
    Build,
    Docs,
    Source,
    Unknown,
}

impl FileKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileKind::Test => "test",
            FileKind::Ci => "ci",
            FileKind::Config => "config",
            FileKind::Build => "build",
            FileKind::Docs => "docs",
            FileKind::Source => "source",
            FileKind::Unknown => "unknown",
        }
    }
}

/// Priority order: test > ci > config > build > docs > source
const KIND_PRIORITY: &[&str] = &["test", "ci", "config", "build", "docs", "source"];

/// Classify a file path into a kind based on language configs.
pub fn classify_file(rel_path: &str, config: &Config, detected_languages: &[String]) -> FileKind {
    // Check each language's kind patterns in priority order
    for kind_name in KIND_PRIORITY {
        for lang in detected_languages {
            if let Some(lang_config) = config.languages.get(lang)
                && let Some(patterns) = lang_config.kinds.get(*kind_name)
            {
                for pattern in patterns {
                    if simple_glob_match(pattern, rel_path) {
                        return match *kind_name {
                            "test" => FileKind::Test,
                            "ci" => FileKind::Ci,
                            "config" => FileKind::Config,
                            "build" => FileKind::Build,
                            "docs" => FileKind::Docs,
                            "source" => FileKind::Source,
                            _ => FileKind::Unknown,
                        };
                    }
                }
            }
        }
    }

    // If no language matched, try basic extension matching
    if rel_path.ends_with(".md") || rel_path.ends_with(".rst") || rel_path.ends_with(".txt") {
        return FileKind::Docs;
    }

    FileKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config::load(None).expect("should load defaults")
    }

    #[test]
    fn test_classify_rust_source() {
        let config = test_config();
        let kind = classify_file("src/main.rs", &config, &["rust".to_string()]);
        assert_eq!(kind, FileKind::Source);
    }

    #[test]
    fn test_classify_rust_test() {
        let config = test_config();
        let kind = classify_file("tests/integration.rs", &config, &["rust".to_string()]);
        assert_eq!(kind, FileKind::Test);
    }

    #[test]
    fn test_classify_cargo_toml_as_config() {
        let config = test_config();
        let kind = classify_file("Cargo.toml", &config, &["rust".to_string()]);
        assert_eq!(kind, FileKind::Config);
    }

    #[test]
    fn test_classify_build_rs() {
        let config = test_config();
        let kind = classify_file("build.rs", &config, &["rust".to_string()]);
        assert_eq!(kind, FileKind::Build);
    }

    #[test]
    fn test_classify_markdown_as_docs() {
        let config = test_config();
        let kind = classify_file("README.md", &config, &["rust".to_string()]);
        assert_eq!(kind, FileKind::Docs);
    }

    #[test]
    fn test_test_priority_over_source() {
        let config = test_config();
        let kind = classify_file("tests/foo.rs", &config, &["rust".to_string()]);
        assert_eq!(kind, FileKind::Test);
    }

    #[test]
    fn test_classify_python_source() {
        let config = test_config();
        let kind = classify_file("myapp/core.py", &config, &["python".to_string()]);
        assert_eq!(kind, FileKind::Source);
    }

    #[test]
    fn test_classify_python_test() {
        let config = test_config();
        let kind = classify_file("tests/test_core.py", &config, &["python".to_string()]);
        assert_eq!(kind, FileKind::Test);
    }

    #[test]
    fn test_classify_typescript_source() {
        let config = test_config();
        let kind = classify_file("src/app.ts", &config, &["typescript".to_string()]);
        assert_eq!(kind, FileKind::Source);
    }

    #[test]
    fn test_classify_typescript_test() {
        let config = test_config();
        let kind = classify_file("src/app.test.ts", &config, &["typescript".to_string()]);
        assert_eq!(kind, FileKind::Test);
    }

    #[test]
    fn test_classify_ci_github_actions() {
        let config = test_config();
        let kind = classify_file(".github/workflows/ci.yml", &config, &["rust".to_string()]);
        assert_eq!(kind, FileKind::Ci);
    }

    #[test]
    fn test_classify_no_language_fallback() {
        let config = test_config();
        let kind = classify_file("README.md", &config, &[]);
        assert_eq!(kind, FileKind::Docs);
    }

    #[test]
    fn test_classify_unknown_file() {
        let config = test_config();
        let kind = classify_file("random.xyz", &config, &["rust".to_string()]);
        assert_eq!(kind, FileKind::Unknown);
    }
}
