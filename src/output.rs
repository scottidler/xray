use eyre::Result;
use std::io::IsTerminal;

use crate::config::FormatMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Yaml,
    Json,
}

/// Resolve the effective output format based on config, CLI override, and TTY detection.
pub fn resolve_format(format_flag: Option<&str>, config_default: &FormatMode) -> OutputFormat {
    if let Some(flag) = format_flag {
        return match flag.to_lowercase().as_str() {
            "json" => OutputFormat::Json,
            "yaml" | "yml" => OutputFormat::Yaml,
            _ => detect_tty(),
        };
    }

    match config_default {
        FormatMode::Json => OutputFormat::Json,
        FormatMode::Yaml => OutputFormat::Yaml,
        FormatMode::Auto => detect_tty(),
    }
}

fn detect_tty() -> OutputFormat {
    if std::io::stdout().is_terminal() {
        OutputFormat::Yaml
    } else {
        OutputFormat::Json
    }
}

/// Serialize any serde::Serialize value to the appropriate format.
pub fn serialize<T: serde::Serialize>(value: &T, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Yaml => {
            let yaml = serde_yaml::to_string(value)?;
            Ok(yaml)
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(value)?;
            Ok(json)
        }
    }
}

/// Check budget and return error if exceeded.
pub fn check_budget(line_count: usize, budget: usize) -> Result<(), BudgetExceeded> {
    if budget == 0 {
        return Ok(());
    }
    if line_count > budget {
        return Err(BudgetExceeded {
            actual: line_count,
            budget,
            overage: line_count - budget,
        });
    }
    Ok(())
}

#[derive(Debug)]
pub struct BudgetExceeded {
    pub actual: usize,
    pub budget: usize,
    pub overage: usize,
}

impl std::fmt::Display for BudgetExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "budget exceeded: output would be {} lines (budget: {}, overage: {})\nhint: use --kind or --pattern to narrow scope, or increase --budget",
            self.actual, self.budget, self.overage
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_unlimited() {
        assert!(check_budget(1000, 0).is_ok());
    }

    #[test]
    fn test_budget_within() {
        assert!(check_budget(50, 100).is_ok());
    }

    #[test]
    fn test_budget_exact() {
        assert!(check_budget(100, 100).is_ok());
    }

    #[test]
    fn test_budget_exceeded() {
        let err = check_budget(150, 100).expect_err("should exceed budget");
        assert_eq!(err.actual, 150);
        assert_eq!(err.budget, 100);
        assert_eq!(err.overage, 50);
    }

    #[test]
    fn test_resolve_format_json_flag() {
        assert_eq!(resolve_format(Some("json"), &FormatMode::Auto), OutputFormat::Json);
    }

    #[test]
    fn test_resolve_format_yaml_flag() {
        assert_eq!(resolve_format(Some("yaml"), &FormatMode::Auto), OutputFormat::Yaml);
    }
}
