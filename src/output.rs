use eyre::Result;
use std::fmt::Write;
use std::io::IsTerminal;

use crate::config::FormatMode;
use crate::outline::{OutlineOutput, Symbol, Visibility};

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

#[derive(Debug, PartialEq, Eq)]
pub struct TruncationInfo {
    pub truncated: bool,
    pub shown: usize,
    pub total: usize,
}

/// Truncate output to budget lines. Returns the (possibly truncated) output and info.
pub fn truncate_to_budget(output: &str, budget: usize) -> (String, TruncationInfo) {
    if budget == 0 {
        let total = output.lines().count();
        return (
            output.to_string(),
            TruncationInfo {
                truncated: false,
                shown: total,
                total,
            },
        );
    }

    let total = output.lines().count();
    if total <= budget {
        return (
            output.to_string(),
            TruncationInfo {
                truncated: false,
                shown: total,
                total,
            },
        );
    }

    let truncated: String = output.lines().take(budget).collect::<Vec<_>>().join("\n");

    (
        truncated + "\n",
        TruncationInfo {
            truncated: true,
            shown: budget,
            total,
        },
    )
}

/// Format truncation footer based on output format.
pub fn format_truncation_footer(info: &TruncationInfo, format: OutputFormat) -> String {
    match format {
        OutputFormat::Yaml => {
            format!("truncated: true\nshown: {}\ntotal: {}\n", info.shown, info.total)
        }
        OutputFormat::Json => {
            format!(
                "{{\"truncated\": true, \"shown\": {}, \"total\": {}}}\n",
                info.shown, info.total
            )
        }
    }
}

/// Serialize outline output in compact one-line-per-symbol format.
pub fn serialize_compact(outline: &OutlineOutput) -> String {
    let mut out = String::new();
    let mut line_count = 0usize;

    for (file_path, symbols) in &outline.files {
        write_symbols_compact(&mut out, file_path, symbols, 0, &mut line_count);
    }

    let _ = writeln!(out, "lines: {line_count}");
    out
}

fn write_symbols_compact(out: &mut String, file_path: &str, symbols: &[Symbol], indent: usize, line_count: &mut usize) {
    for symbol in symbols {
        let indent_str = " ".repeat(indent);
        let vis_prefix = match symbol.visibility {
            Visibility::Public => "pub ",
            Visibility::Private => "",
        };
        let _ = writeln!(
            out,
            "{file_path}:{} {indent_str}{vis_prefix}{}",
            symbol.line, symbol.signature
        );
        *line_count += 1;

        if let Some(children) = &symbol.children {
            write_symbols_compact(out, file_path, children, indent + 2, line_count);
        }
    }
}

/// Format truncation footer for compact (plain text) mode.
pub fn format_truncation_footer_compact(info: &TruncationInfo) -> String {
    format!("...truncated {}/{} lines shown\n", info.shown, info.total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_unlimited() {
        let input = "line1\nline2\nline3\n";
        let (output, info) = truncate_to_budget(input, 0);
        assert_eq!(output, input);
        assert!(!info.truncated);
        assert_eq!(info.shown, 3);
        assert_eq!(info.total, 3);
    }

    #[test]
    fn test_truncate_within_budget() {
        let input = "line1\nline2\nline3\n";
        let (output, info) = truncate_to_budget(input, 10);
        assert_eq!(output, input);
        assert!(!info.truncated);
    }

    #[test]
    fn test_truncate_exact_budget() {
        let input = "line1\nline2\nline3";
        let (output, info) = truncate_to_budget(input, 3);
        assert_eq!(output, input);
        assert!(!info.truncated);
        assert_eq!(info.shown, 3);
        assert_eq!(info.total, 3);
    }

    #[test]
    fn test_truncate_exceeds_budget() {
        let input = "line1\nline2\nline3\nline4\nline5";
        let (output, info) = truncate_to_budget(input, 2);
        assert_eq!(output, "line1\nline2\n");
        assert!(info.truncated);
        assert_eq!(info.shown, 2);
        assert_eq!(info.total, 5);
    }

    #[test]
    fn test_truncation_footer_yaml() {
        let info = TruncationInfo {
            truncated: true,
            shown: 50,
            total: 200,
        };
        let footer = format_truncation_footer(&info, OutputFormat::Yaml);
        assert!(footer.contains("truncated: true"));
        assert!(footer.contains("shown: 50"));
        assert!(footer.contains("total: 200"));
    }

    #[test]
    fn test_truncation_footer_json() {
        let info = TruncationInfo {
            truncated: true,
            shown: 50,
            total: 200,
        };
        let footer = format_truncation_footer(&info, OutputFormat::Json);
        assert!(footer.contains("\"truncated\": true"));
        assert!(footer.contains("\"shown\": 50"));
        assert!(footer.contains("\"total\": 200"));
    }

    #[test]
    fn test_truncation_footer_compact() {
        let info = TruncationInfo {
            truncated: true,
            shown: 5,
            total: 42,
        };
        let footer = format_truncation_footer_compact(&info);
        assert_eq!(footer, "...truncated 5/42 lines shown\n");
    }

    #[test]
    fn test_serialize_compact_basic() {
        use crate::outline::{SymbolKind, Visibility};
        use std::collections::BTreeMap;

        let mut files = BTreeMap::new();
        files.insert(
            "src/main.rs".to_string(),
            vec![Symbol {
                signature: "fn main() -> Result < () >".to_string(),
                line: 22,
                kind: SymbolKind::Function,
                visibility: Visibility::Public,
                children: None,
            }],
        );
        let outline = OutlineOutput { files, lines: 1 };
        let output = serialize_compact(&outline);
        assert!(output.contains("src/main.rs:22 pub fn main() -> Result < () >"));
        assert!(output.contains("lines: 1"));
    }

    #[test]
    fn test_serialize_compact_with_children() {
        use crate::outline::{SymbolKind, Visibility};
        use std::collections::BTreeMap;

        let mut files = BTreeMap::new();
        files.insert(
            "src/config.rs".to_string(),
            vec![Symbol {
                signature: "struct Config".to_string(),
                line: 8,
                kind: SymbolKind::Struct,
                visibility: Visibility::Public,
                children: Some(vec![Symbol {
                    signature: "budget: usize".to_string(),
                    line: 9,
                    kind: SymbolKind::Field,
                    visibility: Visibility::Public,
                    children: None,
                }]),
            }],
        );
        let outline = OutlineOutput { files, lines: 2 };
        let output = serialize_compact(&outline);
        assert!(output.contains("src/config.rs:8 pub struct Config"));
        assert!(output.contains("src/config.rs:9   pub budget: usize"));
        assert!(output.contains("lines: 2"));
    }

    #[test]
    fn test_serialize_compact_private() {
        use crate::outline::{SymbolKind, Visibility};
        use std::collections::BTreeMap;

        let mut files = BTreeMap::new();
        files.insert(
            "src/lib.rs".to_string(),
            vec![Symbol {
                signature: "fn helper()".to_string(),
                line: 10,
                kind: SymbolKind::Function,
                visibility: Visibility::Private,
                children: None,
            }],
        );
        let outline = OutlineOutput { files, lines: 1 };
        let output = serialize_compact(&outline);
        assert!(output.contains("src/lib.rs:10 fn helper()"));
        assert!(!output.contains("pub"));
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
