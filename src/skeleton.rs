use eyre::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

use crate::classify::{self, FileKind};
use crate::config::Config;

#[derive(Debug, Serialize)]
pub struct SkeletonOutput {
    pub path: String,
    pub languages: Vec<String>,
    pub tree: Vec<TreeEntry>,
    pub lines: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum TreeEntry {
    File {
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },
    Directory {
        dir: String,
        children: Vec<TreeEntry>,
    },
    Collapsed {
        collapsed: String,
        file_count: usize,
    },
}

impl TreeEntry {
    fn line_count(&self) -> usize {
        match self {
            TreeEntry::File { .. } | TreeEntry::Collapsed { .. } => 1,
            TreeEntry::Directory { children, .. } => 1 + children.iter().map(|c| c.line_count()).sum::<usize>(),
        }
    }
}

struct TreeContext<'a> {
    root: &'a Path,
    hidden: &'a [String],
    collapse: &'a [String],
    expand: &'a [String],
    config: &'a Config,
    detected_languages: &'a [String],
    kind_filter: &'a [String],
    pattern_filter: &'a [String],
    exclude_filter: &'a [String],
    show_hidden: bool,
}

/// Build the skeleton tree for a given root directory.
pub fn build_skeleton(
    root: &Path,
    config: &Config,
    detected_languages: &[String],
    kind_filter: &[String],
    pattern_filter: &[String],
    exclude_filter: &[String],
    show_hidden: bool,
) -> Result<SkeletonOutput> {
    let hidden = config.effective_hidden(detected_languages);
    let collapse = config.effective_collapse(detected_languages);
    let expand = &config.directories.expand;

    let ctx = TreeContext {
        root,
        hidden: &hidden,
        collapse: &collapse,
        expand,
        config,
        detected_languages,
        kind_filter,
        pattern_filter,
        exclude_filter,
        show_hidden,
    };

    let tree = build_tree(&ctx, root)?;

    let lines: usize = tree.iter().map(|e| e.line_count()).sum();

    let display_path = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string();

    Ok(SkeletonOutput {
        path: display_path,
        languages: detected_languages.to_vec(),
        tree,
        lines,
    })
}

fn matches_any_pattern(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if pattern.contains('*') {
            if let Ok(true) = glob_match(pattern, name) {
                return true;
            }
        } else if name == pattern {
            return true;
        }
    }
    false
}

fn glob_match(pattern: &str, name: &str) -> Result<bool, ()> {
    if let Some(suffix) = pattern.strip_prefix('*') {
        return Ok(name.ends_with(suffix));
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return Ok(name.starts_with(prefix));
    }
    Ok(pattern == name)
}

fn build_tree(ctx: &TreeContext<'_>, dir: &Path) -> Result<Vec<TreeEntry>> {
    let mut entries_map: BTreeMap<String, TreeEntry> = BTreeMap::new();

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(Vec::new()),
    };

    let mut dir_entries: Vec<std::fs::DirEntry> = read_dir.filter_map(|e| e.ok()).collect();
    dir_entries.sort_by_key(|e| e.file_name());

    for entry in dir_entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.is_symlink() {
            continue;
        }

        // Skip dotfiles unless --hidden is passed
        if !ctx.show_hidden && name.starts_with('.') {
            continue;
        }

        if matches_any_pattern(&name, ctx.hidden) {
            continue;
        }

        if !ctx.exclude_filter.is_empty() {
            let rel = path
                .strip_prefix(ctx.root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            if ctx
                .exclude_filter
                .iter()
                .any(|ex| simple_glob_match(ex, &rel) || simple_glob_match(ex, &name))
            {
                continue;
            }
        }

        if metadata.is_dir() {
            if matches_any_pattern(&name, ctx.collapse) && !matches_any_pattern(&name, ctx.expand) {
                let count = count_files_recursive(&path);
                if count > 0 {
                    entries_map.insert(
                        name.clone(),
                        TreeEntry::Collapsed {
                            collapsed: format!("{name}/"),
                            file_count: count,
                        },
                    );
                }
                continue;
            }

            let children = build_tree(ctx, &path)?;

            if !children.is_empty() {
                entries_map.insert(
                    name.clone(),
                    TreeEntry::Directory {
                        dir: format!("{name}/"),
                        children,
                    },
                );
            }
        } else if metadata.is_file() {
            let rel_path = path
                .strip_prefix(ctx.root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            if !ctx.pattern_filter.is_empty()
                && !ctx
                    .pattern_filter
                    .iter()
                    .any(|p| simple_glob_match(p, &rel_path) || simple_glob_match(p, &name))
            {
                continue;
            }

            let kind = classify::classify_file(&rel_path, ctx.config, ctx.detected_languages);

            if !ctx.kind_filter.is_empty() {
                let kind_str = kind.as_str();
                if !ctx.kind_filter.iter().any(|k| k == kind_str) {
                    continue;
                }
            }

            let kind_label = match kind {
                FileKind::Source => None,
                other => Some(other.as_str().to_string()),
            };

            entries_map.insert(
                name.clone(),
                TreeEntry::File {
                    file: name.clone(),
                    kind: kind_label,
                },
            );
        }
    }

    Ok(entries_map.into_values().collect())
}

fn count_files_recursive(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(m) = entry.metadata() {
                if m.is_file() {
                    count += 1;
                } else if m.is_dir() {
                    count += count_files_recursive(&entry.path());
                }
            }
        }
    }
    count
}

pub fn simple_glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "**" {
        return true;
    }

    // Handle **/ prefix patterns (matches any path prefix)
    if let Some(rest) = pattern.strip_prefix("**/") {
        if simple_glob_match(rest, text) {
            return true;
        }
        for (i, c) in text.char_indices() {
            if c == '/' && simple_glob_match(rest, &text[i + 1..]) {
                return true;
            }
        }
        return false;
    }

    // Handle /** suffix patterns (matches any path suffix)
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return text.starts_with(prefix)
            && (text.len() == prefix.len() || text.as_bytes().get(prefix.len()) == Some(&b'/'));
    }

    // Handle middle ** patterns like "tests/**/*.rs"
    if let Some(pos) = pattern.find("/**/") {
        let prefix = &pattern[..pos];
        let suffix = &pattern[pos + 4..];
        if text.starts_with(prefix) && text.len() > prefix.len() && text.as_bytes()[prefix.len()] == b'/' {
            let rest = &text[prefix.len() + 1..];
            // Try matching suffix against every sub-path
            if simple_glob_match(suffix, rest) {
                return true;
            }
            for (i, c) in rest.char_indices() {
                if c == '/' && simple_glob_match(suffix, &rest[i + 1..]) {
                    return true;
                }
            }
        }
        return false;
    }

    // Handle *.ext
    if let Some(ext) = pattern.strip_prefix('*') {
        return text.ends_with(ext);
    }

    // Handle prefix*
    if let Some(prefix) = pattern.strip_suffix('*') {
        return text.starts_with(prefix);
    }

    pattern == text
}

pub fn count_output_lines(output: &SkeletonOutput) -> usize {
    4 + output.lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_glob_match_exact() {
        assert!(simple_glob_match("Cargo.toml", "Cargo.toml"));
        assert!(!simple_glob_match("Cargo.toml", "cargo.toml"));
    }

    #[test]
    fn test_simple_glob_match_star_ext() {
        assert!(simple_glob_match("*.rs", "main.rs"));
        assert!(!simple_glob_match("*.rs", "main.py"));
    }

    #[test]
    fn test_simple_glob_match_double_star() {
        assert!(simple_glob_match("**/*.rs", "src/main.rs"));
        assert!(simple_glob_match("**/*.rs", "src/nested/deep/main.rs"));
    }

    #[test]
    fn test_simple_glob_match_dir_prefix() {
        assert!(simple_glob_match("src/**", "src/main.rs"));
        assert!(!simple_glob_match("src/**", "tests/main.rs"));
    }

    #[test]
    fn test_count_files_in_temp_dir() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        std::fs::write(dir.path().join("a.txt"), "").expect("write");
        std::fs::write(dir.path().join("b.txt"), "").expect("write");
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).expect("mkdir");
        std::fs::write(sub.join("c.txt"), "").expect("write");
        assert_eq!(count_files_recursive(dir.path()), 3);
    }

    #[test]
    fn test_tree_entry_line_count() {
        let file = TreeEntry::File {
            file: "main.rs".to_string(),
            kind: None,
        };
        assert_eq!(file.line_count(), 1);

        let collapsed = TreeEntry::Collapsed {
            collapsed: "data/".to_string(),
            file_count: 50,
        };
        assert_eq!(collapsed.line_count(), 1);

        let dir = TreeEntry::Directory {
            dir: "src/".to_string(),
            children: vec![
                TreeEntry::File {
                    file: "main.rs".to_string(),
                    kind: None,
                },
                TreeEntry::File {
                    file: "lib.rs".to_string(),
                    kind: None,
                },
            ],
        };
        assert_eq!(dir.line_count(), 3);
    }

    #[test]
    fn test_dotfiles_hidden_by_default() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        std::fs::write(dir.path().join("visible.rs"), "").expect("write");
        std::fs::write(dir.path().join(".hidden"), "").expect("write");
        let dotdir = dir.path().join(".dotdir");
        std::fs::create_dir(&dotdir).expect("mkdir");
        std::fs::write(dotdir.join("inside.rs"), "").expect("write");

        let config = Config::default();
        let langs: Vec<String> = vec![];
        let empty: Vec<String> = vec![];

        let result = build_skeleton(dir.path(), &config, &langs, &empty, &empty, &empty, false).expect("build");

        let names: Vec<&str> = result
            .tree
            .iter()
            .filter_map(|e| match e {
                TreeEntry::File { file, .. } => Some(file.as_str()),
                TreeEntry::Directory { dir, .. } => Some(dir.as_str()),
                _ => None,
            })
            .collect();

        assert!(names.contains(&"visible.rs"));
        assert!(!names.contains(&".hidden"));
        assert!(!names.contains(&".dotdir/"));
    }

    #[test]
    fn test_dotfiles_shown_with_flag() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        std::fs::write(dir.path().join("visible.rs"), "").expect("write");
        std::fs::write(dir.path().join(".hidden"), "").expect("write");

        let config = Config::default();
        let langs: Vec<String> = vec![];
        let empty: Vec<String> = vec![];

        let result = build_skeleton(dir.path(), &config, &langs, &empty, &empty, &empty, true).expect("build");

        let names: Vec<&str> = result
            .tree
            .iter()
            .filter_map(|e| match e {
                TreeEntry::File { file, .. } => Some(file.as_str()),
                _ => None,
            })
            .collect();

        assert!(names.contains(&"visible.rs"));
        assert!(names.contains(&".hidden"));
    }
}
