use eyre::Result;
use syn::visit::Visit;
use syn::{self, ImplItem, ItemConst, ItemEnum, ItemFn, ItemStruct, ItemTrait, ItemType};

use super::{LanguageParser, Symbol, SymbolKind, Visibility};

pub struct RustParser;

impl LanguageParser for RustParser {
    fn extract_symbols(&self, source: &str) -> Result<Vec<Symbol>> {
        let file = syn::parse_file(source).map_err(|e| eyre::eyre!("parse error: {e}"))?;
        let mut visitor = RustVisitor::new();
        visitor.visit_file(&file);
        Ok(visitor.symbols)
    }

    fn handles_extension(&self, ext: &str) -> bool {
        ext == "rs"
    }
}

struct RustVisitor {
    symbols: Vec<Symbol>,
}

impl RustVisitor {
    fn new() -> Self {
        Self { symbols: Vec::new() }
    }

    fn span_to_line(&self, span: proc_macro2::Span) -> usize {
        span.start().line
    }

    fn vis_to_visibility(&self, vis: &syn::Visibility) -> Visibility {
        match vis {
            syn::Visibility::Public(_) => Visibility::Public,
            _ => Visibility::Private,
        }
    }

    fn extract_fn_signature(&self, sig: &syn::Signature) -> String {
        let async_prefix = if sig.asyncness.is_some() { "async " } else { "" };
        let unsafe_prefix = if sig.unsafety.is_some() { "unsafe " } else { "" };
        let name = &sig.ident;
        let generics = if sig.generics.params.is_empty() {
            String::new()
        } else {
            format!("{}", sig.generics.params.to_token_stream())
        };
        let generics = if generics.is_empty() { String::new() } else { format!("<{generics}>") };

        let params = self.format_params(&sig.inputs);
        let ret = self.format_return_type(&sig.output);

        format!("{async_prefix}{unsafe_prefix}fn {name}{generics}({params}){ret}")
    }

    fn format_params(&self, inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::Token![,]>) -> String {
        inputs
            .iter()
            .map(|arg| match arg {
                syn::FnArg::Receiver(r) => {
                    let mut s = String::new();
                    if r.reference.is_some() {
                        s.push('&');
                        if r.mutability.is_some() {
                            s.push_str("mut ");
                        }
                    }
                    s.push_str("self");
                    s
                }
                syn::FnArg::Typed(t) => {
                    let pat = t.pat.to_token_stream().to_string();
                    let ty = t.ty.to_token_stream().to_string();
                    format!("{pat}: {ty}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn format_return_type(&self, output: &syn::ReturnType) -> String {
        match output {
            syn::ReturnType::Default => String::new(),
            syn::ReturnType::Type(_, ty) => {
                format!(" -> {}", ty.to_token_stream())
            }
        }
    }
}

use quote::ToTokens;

impl<'ast> Visit<'ast> for RustVisitor {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let vis = self.vis_to_visibility(&node.vis);
        let sig = self.extract_fn_signature(&node.sig);
        let line = self.span_to_line(node.sig.fn_token.span);

        self.symbols.push(Symbol {
            signature: sig,
            line,
            kind: SymbolKind::Function,
            visibility: vis,
            children: None,
        });
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        let vis = self.vis_to_visibility(&node.vis);
        let name = &node.ident;
        let generics = format_generics(&node.generics);
        let line = self.span_to_line(node.ident.span());

        let fields: Vec<Symbol> = match &node.fields {
            syn::Fields::Named(fields) => fields
                .named
                .iter()
                .map(|f| {
                    let fname = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
                    let ftype = f.ty.to_token_stream().to_string();
                    let fvis = self.vis_to_visibility(&f.vis);
                    Symbol {
                        signature: format!("{fname}: {ftype}"),
                        line: self.span_to_line(
                            f.ident
                                .as_ref()
                                .map(|i| i.span())
                                .unwrap_or(proc_macro2::Span::call_site()),
                        ),
                        kind: SymbolKind::Field,
                        visibility: fvis,
                        children: None,
                    }
                })
                .collect(),
            _ => vec![],
        };

        self.symbols.push(Symbol {
            signature: format!("struct {name}{generics}"),
            line,
            kind: SymbolKind::Struct,
            visibility: vis,
            children: if fields.is_empty() { None } else { Some(fields) },
        });
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        let vis = self.vis_to_visibility(&node.vis);
        let name = &node.ident;
        let generics = format_generics(&node.generics);
        let line = self.span_to_line(node.ident.span());

        let variants: Vec<Symbol> = node
            .variants
            .iter()
            .map(|v| Symbol {
                signature: v.ident.to_string(),
                line: self.span_to_line(v.ident.span()),
                kind: SymbolKind::Field,
                visibility: Visibility::Public,
                children: None,
            })
            .collect();

        self.symbols.push(Symbol {
            signature: format!("enum {name}{generics}"),
            line,
            kind: SymbolKind::Enum,
            visibility: vis,
            children: if variants.is_empty() { None } else { Some(variants) },
        });
    }

    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        let vis = self.vis_to_visibility(&node.vis);
        let name = &node.ident;
        let generics = format_generics(&node.generics);
        let line = self.span_to_line(node.ident.span());

        let methods: Vec<Symbol> = node
            .items
            .iter()
            .filter_map(|item| {
                if let syn::TraitItem::Fn(method) = item {
                    let sig = self.extract_fn_signature(&method.sig);
                    Some(Symbol {
                        signature: sig,
                        line: self.span_to_line(method.sig.fn_token.span),
                        kind: SymbolKind::Method,
                        visibility: Visibility::Public,
                        children: None,
                    })
                } else {
                    None
                }
            })
            .collect();

        self.symbols.push(Symbol {
            signature: format!("trait {name}{generics}"),
            line,
            kind: SymbolKind::Trait,
            visibility: vis,
            children: if methods.is_empty() { None } else { Some(methods) },
        });
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let self_ty = node.self_ty.to_token_stream().to_string();
        let trait_prefix = node
            .trait_
            .as_ref()
            .map(|(_, path, _)| format!("{} for ", path.to_token_stream()))
            .unwrap_or_default();
        let generics = format_generics(&node.generics);
        let line = self.span_to_line(node.impl_token.span);

        let methods: Vec<Symbol> = node
            .items
            .iter()
            .filter_map(|item| {
                if let ImplItem::Fn(method) = item {
                    let vis = self.vis_to_visibility(&method.vis);
                    let sig = self.extract_fn_signature(&method.sig);
                    Some(Symbol {
                        signature: sig,
                        line: self.span_to_line(method.sig.fn_token.span),
                        kind: SymbolKind::Method,
                        visibility: vis,
                        children: None,
                    })
                } else {
                    None
                }
            })
            .collect();

        if !methods.is_empty() {
            self.symbols.push(Symbol {
                signature: format!("impl {trait_prefix}{self_ty}{generics}"),
                line,
                kind: SymbolKind::Struct, // impl block displayed under struct kind
                visibility: Visibility::Public,
                children: Some(methods),
            });
        }
    }

    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        let vis = self.vis_to_visibility(&node.vis);
        let name = &node.ident;
        let ty = node.ty.to_token_stream().to_string();
        let line = self.span_to_line(node.ident.span());

        self.symbols.push(Symbol {
            signature: format!("const {name}: {ty}"),
            line,
            kind: SymbolKind::Constant,
            visibility: vis,
            children: None,
        });
    }

    fn visit_item_type(&mut self, node: &'ast ItemType) {
        let vis = self.vis_to_visibility(&node.vis);
        let name = &node.ident;
        let generics = format_generics(&node.generics);
        let ty = node.ty.to_token_stream().to_string();
        let line = self.span_to_line(node.ident.span());

        self.symbols.push(Symbol {
            signature: format!("type {name}{generics} = {ty}"),
            line,
            kind: SymbolKind::TypeAlias,
            visibility: vis,
            children: None,
        });
    }
}

fn format_generics(generics: &syn::Generics) -> String {
    if generics.params.is_empty() {
        String::new()
    } else {
        let params = generics.params.to_token_stream().to_string();
        format!("<{params}>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Vec<Symbol> {
        RustParser.extract_symbols(src).expect("should parse")
    }

    #[test]
    fn test_parse_function() {
        let syms = parse("pub fn hello(name: &str) -> String { name.to_string() }");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Function);
        assert_eq!(syms[0].visibility, Visibility::Public);
        assert!(syms[0].signature.contains("fn hello"));
        assert!(syms[0].signature.contains("name: & str"));
        assert!(syms[0].signature.contains("-> String"));
    }

    #[test]
    fn test_parse_private_function() {
        let syms = parse("fn internal() {}");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].visibility, Visibility::Private);
    }

    #[test]
    fn test_parse_struct_with_fields() {
        let syms = parse("pub struct Config { pub name: String, age: u32 }");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Struct);
        let children = syms[0].children.as_ref().expect("should have fields");
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].kind, SymbolKind::Field);
        assert!(children[0].signature.contains("name: String"));
        assert_eq!(children[0].visibility, Visibility::Public);
        assert_eq!(children[1].visibility, Visibility::Private);
    }

    #[test]
    fn test_parse_enum_with_variants() {
        let syms = parse("pub enum Color { Red, Green, Blue }");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Enum);
        let children = syms[0].children.as_ref().expect("should have variants");
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn test_parse_trait_with_methods() {
        let syms = parse("pub trait Parser { fn parse(&self, input: &str) -> Result<()>; }");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Trait);
        let children = syms[0].children.as_ref().expect("should have methods");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].kind, SymbolKind::Method);
    }

    #[test]
    fn test_parse_impl_block() {
        let src = r#"
            struct Foo;
            impl Foo {
                pub fn new() -> Self { Foo }
                fn internal(&self) {}
            }
        "#;
        let syms = parse(src);
        // Should find struct + impl
        assert!(
            syms.iter()
                .any(|s| s.kind == SymbolKind::Struct && s.signature.contains("Foo"))
        );
        let impl_sym = syms
            .iter()
            .find(|s| s.signature.contains("impl Foo"))
            .expect("should find impl");
        let methods = impl_sym.children.as_ref().expect("should have methods");
        assert_eq!(methods.len(), 2);
    }

    #[test]
    fn test_parse_const() {
        let syms = parse("pub const MAX_SIZE: usize = 100;");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::Constant);
        assert!(syms[0].signature.contains("MAX_SIZE"));
    }

    #[test]
    fn test_parse_type_alias() {
        let syms = parse("pub type Result<T> = std::result::Result<T, Error>;");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::TypeAlias);
        assert!(syms[0].signature.contains("type Result"));
    }

    #[test]
    fn test_parse_async_function() {
        let syms = parse("pub async fn fetch(url: &str) -> Response { todo!() }");
        assert_eq!(syms.len(), 1);
        assert!(syms[0].signature.starts_with("async fn"));
    }

    #[test]
    fn test_parse_generic_struct() {
        let syms = parse("pub struct Container<T> { items: Vec<T> }");
        assert_eq!(syms.len(), 1);
        assert!(syms[0].signature.contains("<T>") || syms[0].signature.contains("< T >"));
    }
}
