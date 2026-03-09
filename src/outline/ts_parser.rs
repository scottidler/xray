use eyre::Result;
use tree_sitter::{Parser, Tree};

use super::{LanguageParser, Symbol, SymbolKind, Visibility};

pub struct TypeScriptParser;

impl LanguageParser for TypeScriptParser {
    fn extract_symbols(&self, source: &str) -> Result<Vec<Symbol>> {
        let mut parser = Parser::new();
        let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser
            .set_language(&language)
            .map_err(|e| eyre::eyre!("ts language error: {e}"))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| eyre::eyre!("ts parse failed"))?;
        let symbols = extract_top_level(source, &tree);
        Ok(symbols)
    }

    fn handles_extension(&self, ext: &str) -> bool {
        matches!(ext, "ts" | "tsx")
    }
}

pub struct JavaScriptParser;

impl LanguageParser for JavaScriptParser {
    fn extract_symbols(&self, source: &str) -> Result<Vec<Symbol>> {
        let mut parser = Parser::new();
        let language = tree_sitter_javascript::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|e| eyre::eyre!("js language error: {e}"))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| eyre::eyre!("js parse failed"))?;
        let symbols = extract_top_level(source, &tree);
        Ok(symbols)
    }

    fn handles_extension(&self, ext: &str) -> bool {
        matches!(ext, "js" | "jsx")
    }
}

fn extract_top_level(source: &str, tree: &Tree) -> Vec<Symbol> {
    let root = tree.root_node();
    let mut symbols = Vec::new();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(sym) = extract_function(source, &child, false) {
                    symbols.push(sym);
                }
            }
            "export_statement" => {
                // Check what's being exported
                let mut inner_cursor = child.walk();
                for export_child in child.children(&mut inner_cursor) {
                    match export_child.kind() {
                        "function_declaration" => {
                            if let Some(mut sym) = extract_function(source, &export_child, false) {
                                sym.visibility = Visibility::Public;
                                symbols.push(sym);
                            }
                        }
                        "class_declaration" => {
                            if let Some(mut sym) = extract_class(source, &export_child) {
                                sym.visibility = Visibility::Public;
                                symbols.push(sym);
                            }
                        }
                        "interface_declaration" => {
                            if let Some(mut sym) = extract_interface(source, &export_child) {
                                sym.visibility = Visibility::Public;
                                symbols.push(sym);
                            }
                        }
                        "type_alias_declaration" => {
                            if let Some(mut sym) = extract_type_alias(source, &export_child) {
                                sym.visibility = Visibility::Public;
                                symbols.push(sym);
                            }
                        }
                        "lexical_declaration" => {
                            let mut decl_symbols = extract_lexical(source, &export_child);
                            for sym in &mut decl_symbols {
                                sym.visibility = Visibility::Public;
                            }
                            symbols.extend(decl_symbols);
                        }
                        _ => {}
                    }
                }
            }
            "class_declaration" => {
                if let Some(sym) = extract_class(source, &child) {
                    symbols.push(sym);
                }
            }
            "interface_declaration" => {
                if let Some(sym) = extract_interface(source, &child) {
                    symbols.push(sym);
                }
            }
            "type_alias_declaration" => {
                if let Some(sym) = extract_type_alias(source, &child) {
                    symbols.push(sym);
                }
            }
            "lexical_declaration" => {
                symbols.extend(extract_lexical(source, &child));
            }
            _ => {}
        }
    }

    symbols
}

fn node_text<'a>(source: &'a str, node: &tree_sitter::Node<'_>) -> &'a str {
    &source[node.byte_range()]
}

fn extract_function(source: &str, node: &tree_sitter::Node<'_>, is_async: bool) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(source, &name);
    let params = node.child_by_field_name("parameters")?;
    let params_text = node_text(source, &params);
    let ret = node
        .child_by_field_name("return_type")
        .map(|r| format!(": {}", node_text(source, &r).trim_start_matches(':')))
        .unwrap_or_default();

    let async_prefix = if is_async { "async " } else { "" };
    let type_params = node
        .child_by_field_name("type_parameters")
        .map(|tp| node_text(source, &tp).to_string())
        .unwrap_or_default();

    Some(Symbol {
        signature: format!("{async_prefix}function {name_text}{type_params}{params_text}{ret}"),
        line: node.start_position().row + 1,
        kind: SymbolKind::Function,
        visibility: Visibility::Private,
        children: None,
    })
}

fn extract_class(source: &str, node: &tree_sitter::Node<'_>) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(source, &name);
    let type_params = node
        .child_by_field_name("type_parameters")
        .map(|tp| node_text(source, &tp).to_string())
        .unwrap_or_default();

    let body = node.child_by_field_name("body")?;
    let mut methods = Vec::new();

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "method_definition"
            && let Some(method_name) = child.child_by_field_name("name")
        {
            let method_name_text = node_text(source, &method_name);
            let params = child
                .child_by_field_name("parameters")
                .map(|p| node_text(source, &p).to_string())
                .unwrap_or_else(|| "()".to_string());
            let ret = child
                .child_by_field_name("return_type")
                .map(|r| format!(": {}", node_text(source, &r).trim_start_matches(':')))
                .unwrap_or_default();

            let vis = if has_modifier(&child, "private") || method_name_text.starts_with('#') {
                Visibility::Private
            } else {
                Visibility::Public
            };

            methods.push(Symbol {
                signature: format!("{method_name_text}{params}{ret}"),
                line: child.start_position().row + 1,
                kind: SymbolKind::Method,
                visibility: vis,
                children: None,
            });
        }
    }

    Some(Symbol {
        signature: format!("class {name_text}{type_params}"),
        line: node.start_position().row + 1,
        kind: SymbolKind::Class,
        visibility: Visibility::Private,
        children: if methods.is_empty() { None } else { Some(methods) },
    })
}

fn extract_interface(source: &str, node: &tree_sitter::Node<'_>) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(source, &name);
    let type_params = node
        .child_by_field_name("type_parameters")
        .map(|tp| node_text(source, &tp).to_string())
        .unwrap_or_default();

    Some(Symbol {
        signature: format!("interface {name_text}{type_params}"),
        line: node.start_position().row + 1,
        kind: SymbolKind::Interface,
        visibility: Visibility::Private,
        children: None,
    })
}

fn extract_type_alias(source: &str, node: &tree_sitter::Node<'_>) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(source, &name);
    let type_params = node
        .child_by_field_name("type_parameters")
        .map(|tp| node_text(source, &tp).to_string())
        .unwrap_or_default();
    let value = node
        .child_by_field_name("value")
        .map(|v| format!(" = {}", node_text(source, &v)))
        .unwrap_or_default();

    Some(Symbol {
        signature: format!("type {name_text}{type_params}{value}"),
        line: node.start_position().row + 1,
        kind: SymbolKind::TypeAlias,
        visibility: Visibility::Private,
        children: None,
    })
}

fn extract_lexical(source: &str, node: &tree_sitter::Node<'_>) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator"
            && let Some(name) = child.child_by_field_name("name")
        {
            let name_text = node_text(source, &name);
            let type_ann = child
                .child_by_field_name("type")
                .map(|t| format!(": {}", node_text(source, &t).trim_start_matches(':')))
                .unwrap_or_default();

            let is_const = node_text(source, node).starts_with("const");

            if is_const {
                symbols.push(Symbol {
                    signature: format!("const {name_text}{type_ann}"),
                    line: node.start_position().row + 1,
                    kind: SymbolKind::Constant,
                    visibility: Visibility::Private,
                    children: None,
                });
            }
        }
    }

    symbols
}

fn has_modifier(node: &tree_sitter::Node<'_>, modifier: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| child.kind() == "accessibility_modifier" && child.utf8_text(&[]).unwrap_or("") == modifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ts(src: &str) -> Vec<Symbol> {
        TypeScriptParser.extract_symbols(src).expect("should parse")
    }

    fn parse_js(src: &str) -> Vec<Symbol> {
        JavaScriptParser.extract_symbols(src).expect("should parse")
    }

    #[test]
    fn test_ts_function() {
        let syms = parse_ts("function hello(name: string): string { return name; }");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Function);
        assert!(syms[0].signature.contains("function hello"));
    }

    #[test]
    fn test_ts_exported_function() {
        let syms = parse_ts("export function greet(name: string): void {}");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].visibility, Visibility::Public);
    }

    #[test]
    fn test_ts_class_with_methods() {
        let src = "class MyClass {\n  constructor() {}\n  getName(): string { return ''; }\n}";
        let syms = parse_ts(src);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Class);
        let methods = syms[0].children.as_ref().expect("should have methods");
        assert_eq!(methods.len(), 2);
    }

    #[test]
    fn test_ts_interface() {
        let syms = parse_ts("interface Config { name: string; }");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Interface);
    }

    #[test]
    fn test_ts_type_alias() {
        let syms = parse_ts("type Result<T> = Success<T> | Error;");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::TypeAlias);
    }

    #[test]
    fn test_ts_const() {
        let syms = parse_ts("const MAX_SIZE: number = 100;");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Constant);
    }

    #[test]
    fn test_js_function() {
        let syms = parse_js("function hello(name) { return name; }");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_js_class() {
        let src = "class Animal {\n  constructor(name) { this.name = name; }\n  speak() { return this.name; }\n}";
        let syms = parse_js(src);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Class);
        let methods = syms[0].children.as_ref().expect("should have methods");
        assert_eq!(methods.len(), 2);
    }
}
