use serde::Serialize;
use tree_sitter::Node;

use crate::render::collapse_ws;
use crate::source::SourceFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Class,
    Function,
    Variable,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Function => "function",
            Self::Variable => "variable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindFilter {
    All,
    Only(SymbolKind),
}

impl KindFilter {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "all" | "any" => Some(Self::All),
            "class" => Some(Self::Only(SymbolKind::Class)),
            "function" | "func" | "fn" => Some(Self::Only(SymbolKind::Function)),
            "variable" | "var" => Some(Self::Only(SymbolKind::Variable)),
            _ => None,
        }
    }

    pub fn allows(self, kind: SymbolKind) -> bool {
        match self {
            Self::All => true,
            Self::Only(wanted) => wanted == kind,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub dotted: String,
    pub start_line: usize,
    pub end_line: usize,
    pub span_start: usize,
    pub span_end: usize,
    pub def_start: usize,
    pub sig_end: usize,
    pub decorators: Vec<String>,
    pub children: Vec<Symbol>,
}

impl Symbol {
    pub fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line) + 1
    }

    pub fn descendants(&self) -> usize {
        self.children
            .iter()
            .map(|child| 1 + child.descendants())
            .sum()
    }
}

const TRANSPARENT: &[&str] = &[
    "block",
    "if_statement",
    "else_clause",
    "elif_clause",
    "try_statement",
    "except_clause",
    "finally_clause",
    "with_statement",
    "for_statement",
    "while_statement",
    "match_statement",
    "case_clause",
    "decorated_definition",
];

pub fn extract(file: &SourceFile) -> Vec<Symbol> {
    let mut out = Vec::new();
    collect(file, file.root(), "", true, &mut out);
    out
}

fn collect(
    file: &SourceFile,
    node: Node<'_>,
    prefix: &str,
    bind_names: bool,
    out: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "decorated_definition" => {
                if let Some(inner) = child.child_by_field_name("definition") {
                    push_definition(file, inner, Some(child), prefix, out);
                }
            }
            "function_definition" | "class_definition" => {
                push_definition(file, child, None, prefix, out);
            }
            "expression_statement" if bind_names => {
                push_assignments(file, child, prefix, out);
            }
            kind if TRANSPARENT.contains(&kind) => {
                collect(file, child, prefix, bind_names, out);
            }
            _ => {}
        }
    }
}

fn push_definition(
    file: &SourceFile,
    def: Node<'_>,
    decorated: Option<Node<'_>>,
    prefix: &str,
    out: &mut Vec<Symbol>,
) {
    let Some(name_node) = def.child_by_field_name("name") else {
        return;
    };
    let name = file.node_text(name_node).to_string();
    let dotted = join_dotted(prefix, &name);
    let kind = if def.kind() == "class_definition" {
        SymbolKind::Class
    } else {
        SymbolKind::Function
    };
    let outer = decorated.unwrap_or(def);
    let span_start = file.snap_to_line_start(outer.start_byte());
    let span_end = def.end_byte();
    let sig_end = def
        .child_by_field_name("body")
        .map_or(def.end_byte(), |body| {
            trim_back(file, def.start_byte(), body.start_byte())
        });
    let decorators = decorated.map_or_else(Vec::new, |node| decorator_texts(file, node));

    let mut children = Vec::new();
    if let Some(body) = def.child_by_field_name("body") {
        collect(
            file,
            body,
            &dotted,
            kind == SymbolKind::Class,
            &mut children,
        );
    }

    out.push(Symbol {
        name,
        kind,
        dotted,
        start_line: file.line_of(span_start),
        end_line: file.line_of(span_end.saturating_sub(1).max(span_start)),
        span_start,
        span_end,
        def_start: def.start_byte(),
        sig_end,
        decorators,
        children,
    });
}

fn push_assignments(file: &SourceFile, statement: Node<'_>, prefix: &str, out: &mut Vec<Symbol>) {
    let mut cursor = statement.walk();
    for child in statement.named_children(&mut cursor) {
        if child.kind() != "assignment" {
            continue;
        }
        let Some(left) = child.child_by_field_name("left") else {
            continue;
        };
        if left.kind() != "identifier" {
            continue;
        }
        let name = file.node_text(left).to_string();
        if !is_constant_name(&name) {
            continue;
        }
        let span_start = file.snap_to_line_start(statement.start_byte());
        let span_end = statement.end_byte();
        out.push(Symbol {
            dotted: join_dotted(prefix, &name),
            name,
            kind: SymbolKind::Variable,
            start_line: file.line_of(span_start),
            end_line: file.line_of(span_end.saturating_sub(1).max(span_start)),
            span_start,
            span_end,
            def_start: span_start,
            sig_end: span_end,
            decorators: Vec::new(),
            children: Vec::new(),
        });
    }
}

fn is_constant_name(name: &str) -> bool {
    !name.starts_with('_')
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn decorator_texts(file: &SourceFile, decorated: Node<'_>) -> Vec<String> {
    let mut cursor = decorated.walk();
    decorated
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "decorator")
        .map(|child| collapse_ws(file.node_text(child)))
        .collect()
}

fn trim_back(file: &SourceFile, start: usize, mut end: usize) -> usize {
    while end > start {
        let ch = file.slice(start, end).chars().next_back();
        match ch {
            Some(c) if c.is_whitespace() => end -= c.len_utf8(),
            _ => break,
        }
    }
    end
}

fn join_dotted(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

pub fn resolve<'a>(symbols: &'a [Symbol], parts: &[String]) -> Vec<&'a Symbol> {
    let Some((head, rest)) = parts.split_first() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for symbol in symbols {
        if &symbol.name != head {
            continue;
        }
        if rest.is_empty() {
            out.push(symbol);
        } else {
            out.extend(resolve(&symbol.children, rest));
        }
    }
    out
}

pub fn flatten(symbols: &[Symbol]) -> Vec<&Symbol> {
    let mut out = Vec::new();
    push_flat(symbols, &mut out);
    out
}

fn push_flat<'a>(symbols: &'a [Symbol], out: &mut Vec<&'a Symbol>) {
    for symbol in symbols {
        out.push(symbol);
        push_flat(&symbol.children, out);
    }
}

pub fn enclosing(symbols: &[Symbol], byte: usize) -> Option<&Symbol> {
    for symbol in symbols {
        if byte < symbol.span_start || byte >= symbol.span_end {
            continue;
        }
        return Some(enclosing(&symbol.children, byte).unwrap_or(symbol));
    }
    None
}

pub fn signature(file: &SourceFile, symbol: &Symbol, with_decorators: bool) -> String {
    let body = file.slice(symbol.def_start, symbol.sig_end);
    let core = collapse_ws(body);
    if !with_decorators || symbol.decorators.is_empty() {
        return core;
    }
    let mut parts = symbol.decorators.clone();
    parts.push(core);
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::lang::Lang;

    fn parse(text: &str) -> SourceFile {
        SourceFile::from_text(Path::new("t.py"), Lang::Python, text.to_string()).expect("parses")
    }

    #[test]
    fn finds_nested_symbols() {
        let file = parse("class A:\n    class B:\n        def c(self):\n            pass\n");
        let symbols = extract(&file);
        let found = resolve(&symbols, &["A".into(), "B".into(), "c".into()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, SymbolKind::Function);
    }

    #[test]
    fn returns_every_overload_in_source_order() {
        let file = parse(
            "@overload\ndef f(a: int) -> int: ...\n@overload\ndef f(a: str) -> str: ...\ndef f(a):\n    return a\n",
        );
        let symbols = extract(&file);
        let found = resolve(&symbols, &["f".into()]);
        assert_eq!(found.len(), 3);
        assert!(found[0].start_line < found[1].start_line);
        assert!(found[1].start_line < found[2].start_line);
    }

    #[test]
    fn span_starts_at_the_decorator() {
        let file = parse("# leading comment\n@deco\ndef f():\n    pass\n");
        let symbols = extract(&file);
        assert_eq!(symbols[0].start_line, 2);
        assert_eq!(symbols[0].decorators, vec!["@deco".to_string()]);
    }

    #[test]
    fn conditional_definitions_are_visible() {
        let file = parse("if TYPE_CHECKING:\n    def only_typing():\n        pass\n");
        let symbols = extract(&file);
        assert_eq!(resolve(&symbols, &["only_typing".into()]).len(), 1);
    }

    #[test]
    fn class_constants_are_symbols() {
        let file = parse("class A:\n    RETRY = 2\n    def f(self):\n        LOCAL = 1\n");
        let symbols = extract(&file);
        assert_eq!(resolve(&symbols, &["A".into(), "RETRY".into()]).len(), 1);
        assert!(resolve(&symbols, &["A".into(), "f".into(), "LOCAL".into()]).is_empty());
    }

    #[test]
    fn module_constants_are_symbols() {
        let file = parse("MAX_RETRIES = 3\nlower = 1\n");
        let symbols = extract(&file);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "MAX_RETRIES");
    }

    #[test]
    fn signature_stops_before_the_body() {
        let file = parse("def f(a: int,\n      b: str) -> None:\n    pass\n");
        let symbols = extract(&file);
        assert_eq!(
            signature(&file, &symbols[0], false),
            "def f(a: int, b: str) -> None:"
        );
    }
}
