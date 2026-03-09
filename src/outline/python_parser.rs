use eyre::Result;
use rustpython_parser::{self as parser, ast};

use super::{LanguageParser, Symbol, SymbolKind, Visibility};

pub struct PythonParser;

impl LanguageParser for PythonParser {
    fn extract_symbols(&self, source: &str) -> Result<Vec<Symbol>> {
        let parsed = parser::parse(source, parser::Mode::Module, "<input>")
            .map_err(|e| eyre::eyre!("python parse error: {e}"))?;

        let module = match parsed {
            ast::Mod::Module(m) => m,
            _ => return Ok(Vec::new()),
        };

        let line_index = LineIndex::new(source);
        let symbols = extract_from_body(&module.body, &line_index);
        Ok(symbols)
    }

    fn handles_extension(&self, ext: &str) -> bool {
        ext == "py"
    }
}

/// Simple offset-to-line mapper
struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, c) in source.char_indices() {
            if c == '\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    fn offset_to_line(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line + 1,
            Err(line) => line,
        }
    }
}

fn range_to_line(range: &rustpython_parser::text_size::TextRange, idx: &LineIndex) -> usize {
    let offset: usize = range.start().into();
    idx.offset_to_line(offset)
}

fn extract_from_body(body: &[ast::Stmt], idx: &LineIndex) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    for stmt in body {
        match stmt {
            ast::Stmt::FunctionDef(func) => {
                let vis = python_visibility(func.name.as_str());
                let params = format_args(&func.args);
                let ret = func
                    .returns
                    .as_ref()
                    .map(|r| format!(" -> {}", format_expr(r)))
                    .unwrap_or_default();
                let line = range_to_line(&func.range, idx);

                symbols.push(Symbol {
                    signature: format!("def {}{params}{ret}", func.name),
                    line,
                    kind: SymbolKind::Function,
                    visibility: vis,
                    children: None,
                });
            }
            ast::Stmt::AsyncFunctionDef(func) => {
                let vis = python_visibility(func.name.as_str());
                let params = format_args(&func.args);
                let ret = func
                    .returns
                    .as_ref()
                    .map(|r| format!(" -> {}", format_expr(r)))
                    .unwrap_or_default();
                let line = range_to_line(&func.range, idx);

                symbols.push(Symbol {
                    signature: format!("async def {}{params}{ret}", func.name),
                    line,
                    kind: SymbolKind::Function,
                    visibility: vis,
                    children: None,
                });
            }
            ast::Stmt::ClassDef(class) => {
                let vis = python_visibility(class.name.as_str());
                let bases = if class.bases.is_empty() {
                    String::new()
                } else {
                    let base_strs: Vec<String> = class.bases.iter().map(format_expr).collect();
                    format!("({})", base_strs.join(", "))
                };
                let line = range_to_line(&class.range, idx);

                let methods: Vec<Symbol> = class
                    .body
                    .iter()
                    .filter_map(|s| match s {
                        ast::Stmt::FunctionDef(method) => {
                            let mvis = python_visibility(method.name.as_str());
                            let params = format_args(&method.args);
                            let ret = method
                                .returns
                                .as_ref()
                                .map(|r| format!(" -> {}", format_expr(r)))
                                .unwrap_or_default();
                            Some(Symbol {
                                signature: format!("def {}{params}{ret}", method.name),
                                line: range_to_line(&method.range, idx),
                                kind: SymbolKind::Method,
                                visibility: mvis,
                                children: None,
                            })
                        }
                        ast::Stmt::AsyncFunctionDef(method) => {
                            let mvis = python_visibility(method.name.as_str());
                            let params = format_args(&method.args);
                            let ret = method
                                .returns
                                .as_ref()
                                .map(|r| format!(" -> {}", format_expr(r)))
                                .unwrap_or_default();
                            Some(Symbol {
                                signature: format!("async def {}{params}{ret}", method.name),
                                line: range_to_line(&method.range, idx),
                                kind: SymbolKind::Method,
                                visibility: mvis,
                                children: None,
                            })
                        }
                        _ => None,
                    })
                    .collect();

                symbols.push(Symbol {
                    signature: format!("class {}{bases}", class.name),
                    line,
                    kind: SymbolKind::Class,
                    visibility: vis,
                    children: if methods.is_empty() { None } else { Some(methods) },
                });
            }
            ast::Stmt::Assign(assign) => {
                if let Some(ast::Expr::Name(name)) = assign.targets.first() {
                    let var_name = name.id.as_str();
                    if is_constant_name(var_name) {
                        symbols.push(Symbol {
                            signature: var_name.to_string(),
                            line: range_to_line(&assign.range, idx),
                            kind: SymbolKind::Constant,
                            visibility: Visibility::Public,
                            children: None,
                        });
                    }
                }
            }
            ast::Stmt::AnnAssign(assign) => {
                if let ast::Expr::Name(name) = assign.target.as_ref() {
                    let var_name = name.id.as_str();
                    let type_ann = format_expr(&assign.annotation);
                    if is_constant_name(var_name) {
                        symbols.push(Symbol {
                            signature: format!("{var_name}: {type_ann}"),
                            line: range_to_line(&assign.range, idx),
                            kind: SymbolKind::Constant,
                            visibility: Visibility::Public,
                            children: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    symbols
}

fn python_visibility(name: &str) -> Visibility {
    if name.starts_with('_') && !name.starts_with("__") {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

fn is_constant_name(name: &str) -> bool {
    name.len() > 1 && name.chars().all(|c| c.is_uppercase() || c == '_')
}

fn format_args(args: &ast::Arguments) -> String {
    let mut params = Vec::new();
    for arg in &args.args {
        let name = arg.def.arg.as_str();
        let ann = arg
            .def
            .annotation
            .as_ref()
            .map(|a| format!(": {}", format_expr(a)))
            .unwrap_or_default();
        params.push(format!("{name}{ann}"));
    }
    format!("({})", params.join(", "))
}

fn format_expr(expr: &ast::Expr) -> String {
    match expr {
        ast::Expr::Name(n) => n.id.to_string(),
        ast::Expr::Attribute(a) => {
            format!("{}.{}", format_expr(&a.value), a.attr)
        }
        ast::Expr::Subscript(s) => {
            format!("{}[{}]", format_expr(&s.value), format_expr(&s.slice))
        }
        ast::Expr::Tuple(t) => {
            let items: Vec<String> = t.elts.iter().map(format_expr).collect();
            items.join(", ")
        }
        ast::Expr::Constant(c) => format!("{:?}", c.value),
        ast::Expr::BinOp(b) => {
            format!("{} | {}", format_expr(&b.left), format_expr(&b.right))
        }
        _ => "...".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Vec<Symbol> {
        PythonParser.extract_symbols(src).expect("should parse")
    }

    #[test]
    fn test_parse_function() {
        let syms = parse("def hello(name: str) -> str:\n    return name\n");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Function);
        assert!(syms[0].signature.contains("def hello"));
        assert!(syms[0].signature.contains("-> str"));
    }

    #[test]
    fn test_parse_private_function() {
        let syms = parse("def _internal():\n    pass\n");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].visibility, Visibility::Private);
    }

    #[test]
    fn test_parse_dunder_is_public() {
        let syms = parse("def __init__(self):\n    pass\n");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_class_with_methods() {
        let src =
            "class MyClass:\n    def __init__(self):\n        pass\n    def method(self) -> int:\n        return 1\n";
        let syms = parse(src);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Class);
        let methods = syms[0].children.as_ref().expect("should have methods");
        assert_eq!(methods.len(), 2);
    }

    #[test]
    fn test_parse_class_with_base() {
        let syms = parse("class Child(Parent):\n    pass\n");
        assert_eq!(syms.len(), 1);
        assert!(syms[0].signature.contains("(Parent)"));
    }

    #[test]
    fn test_parse_constant() {
        let syms = parse("MAX_SIZE = 100\n");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Constant);
        assert_eq!(syms[0].signature, "MAX_SIZE");
    }

    #[test]
    fn test_parse_async_function() {
        let syms = parse("async def fetch(url: str) -> Response:\n    pass\n");
        assert_eq!(syms.len(), 1);
        assert!(syms[0].signature.contains("async def"));
    }

    #[test]
    fn test_parse_annotated_constant() {
        let syms = parse("MAX_RETRIES: int = 3\n");
        assert_eq!(syms.len(), 1);
        assert!(syms[0].signature.contains("MAX_RETRIES: int"));
    }
}
