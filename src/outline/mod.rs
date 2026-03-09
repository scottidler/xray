use eyre::Result;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

use crate::classify;
use crate::config::Config;

mod rust_parser;

pub use rust_parser::RustParser;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)] // Class, Interface used by Python/TS parsers in Phase 4
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Constant,
    TypeAlias,
    Field,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub signature: String,
    pub line: usize,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Symbol>>,
}

#[derive(Debug, Serialize)]
pub struct OutlineOutput {
    pub files: BTreeMap<String, Vec<Symbol>>,
    pub lines: usize,
}

pub trait LanguageParser: Send + Sync {
    fn extract_symbols(&self, source: &str) -> Result<Vec<Symbol>>;
    fn language(&self) -> &str;
}

/// Filter to apply to symbol visibility
#[derive(Debug, Clone, Copy)]
pub enum VisibilityFilter {
    All,
    Public,
    Private,
}

/// Build the outline for files under root.
pub fn build_outline(
    root: &Path,
    config: &Config,
    detected_languages: &[String],
    kind_filter: &[String],
    pattern_filter: &[String],
    exclude_filter: &[String],
    vis_filter: VisibilityFilter,
) -> Result<OutlineOutput> {
    let parsers = build_parsers(detected_languages);

    if parsers.is_empty() {
        eprintln!("warning: no parseable languages detected; outline requires language support");
        return Ok(OutlineOutput {
            files: BTreeMap::new(),
            lines: 0,
        });
    }

    // Collect files to parse
    let files = collect_source_files(
        root,
        root,
        config,
        detected_languages,
        kind_filter,
        pattern_filter,
        exclude_filter,
    )?;

    // Parse in parallel
    let results: Vec<(String, Vec<Symbol>)> = files
        .par_iter()
        .filter_map(|(rel_path, abs_path, ext)| {
            let parser = parsers.iter().find(|p| parser_handles_ext(p.as_ref(), ext))?;
            let source = std::fs::read_to_string(abs_path).ok()?;
            let symbols = parser.extract_symbols(&source).ok()?;
            if symbols.is_empty() {
                return None;
            }
            Some((rel_path.clone(), symbols))
        })
        .collect();

    let mut files_map = BTreeMap::new();
    for (path, symbols) in results {
        let filtered = filter_visibility(symbols, vis_filter);
        if !filtered.is_empty() {
            files_map.insert(path, filtered);
        }
    }

    let lines: usize = files_map.values().map(|syms| count_symbol_lines(syms)).sum();

    Ok(OutlineOutput {
        files: files_map,
        lines,
    })
}

fn build_parsers(detected_languages: &[String]) -> Vec<Box<dyn LanguageParser>> {
    let mut parsers: Vec<Box<dyn LanguageParser>> = Vec::new();
    for lang in detected_languages {
        if lang == "rust" {
            parsers.push(Box::new(RustParser));
        }
    }
    parsers
}

fn parser_handles_ext(parser: &dyn LanguageParser, ext: &str) -> bool {
    match parser.language() {
        "rust" => ext == "rs",
        "python" => ext == "py",
        "typescript" => matches!(ext, "ts" | "tsx" | "js" | "jsx"),
        _ => false,
    }
}

fn collect_source_files(
    root: &Path,
    dir: &Path,
    config: &Config,
    detected_languages: &[String],
    kind_filter: &[String],
    pattern_filter: &[String],
    exclude_filter: &[String],
) -> Result<Vec<(String, std::path::PathBuf, String)>> {
    let hidden = config.effective_hidden(detected_languages);
    let mut files = Vec::new();
    collect_files_recursive(
        root,
        dir,
        &hidden,
        config,
        detected_languages,
        kind_filter,
        pattern_filter,
        exclude_filter,
        &mut files,
    )?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

#[allow(clippy::too_many_arguments)]
fn collect_files_recursive(
    root: &Path,
    dir: &Path,
    hidden: &[String],
    config: &Config,
    detected_languages: &[String],
    kind_filter: &[String],
    pattern_filter: &[String],
    exclude_filter: &[String],
    out: &mut Vec<(String, std::path::PathBuf, String)>,
) -> Result<()> {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };

    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if hidden.iter().any(|h| h == &name) {
            continue;
        }

        if metadata.is_dir() {
            collect_files_recursive(
                root,
                &path,
                hidden,
                config,
                detected_languages,
                kind_filter,
                pattern_filter,
                exclude_filter,
                out,
            )?;
        } else if metadata.is_file() {
            let rel_path = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();

            // Check exclude
            if !exclude_filter.is_empty()
                && exclude_filter.iter().any(|ex| {
                    crate::skeleton::simple_glob_match(ex, &rel_path) || crate::skeleton::simple_glob_match(ex, &name)
                })
            {
                continue;
            }

            // Check pattern
            if !pattern_filter.is_empty()
                && !pattern_filter.iter().any(|p| {
                    crate::skeleton::simple_glob_match(p, &rel_path) || crate::skeleton::simple_glob_match(p, &name)
                })
            {
                continue;
            }

            // Classify the file
            let kind = classify::classify_file(&rel_path, config, detected_languages);

            // Kind filter - if kinds specified, only include matching; otherwise default to source
            if !kind_filter.is_empty() {
                if !kind_filter.iter().any(|k| k == kind.as_str()) {
                    continue;
                }
            } else {
                // Default: only parse source files
                if kind != classify::FileKind::Source && kind != classify::FileKind::Test {
                    continue;
                }
            }

            // Get extension
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();

            if !ext.is_empty() {
                out.push((rel_path, path.clone(), ext));
            }
        }
    }
    Ok(())
}

fn filter_visibility(symbols: Vec<Symbol>, filter: VisibilityFilter) -> Vec<Symbol> {
    match filter {
        VisibilityFilter::All => symbols,
        VisibilityFilter::Public => symbols
            .into_iter()
            .filter_map(|mut s| {
                if s.visibility == Visibility::Public {
                    s.children = s.children.map(|children| filter_visibility(children, filter));
                    Some(s)
                } else {
                    None
                }
            })
            .collect(),
        VisibilityFilter::Private => symbols
            .into_iter()
            .filter_map(|mut s| {
                if s.visibility == Visibility::Private {
                    s.children = s.children.map(|children| filter_visibility(children, filter));
                    Some(s)
                } else {
                    None
                }
            })
            .collect(),
    }
}

fn count_symbol_lines(symbols: &[Symbol]) -> usize {
    symbols
        .iter()
        .map(|s| 1 + s.children.as_ref().map(|c| count_symbol_lines(c)).unwrap_or(0))
        .sum()
}

pub fn count_output_lines(output: &OutlineOutput) -> usize {
    // Each file header + its symbols
    let file_lines: usize = output.files.values().map(|syms| 1 + count_symbol_lines(syms)).sum();
    // files header: 1, lines footer: 1 = 2 overhead
    2 + file_lines
}
