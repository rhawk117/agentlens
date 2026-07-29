use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::render::collapse_ws;
use crate::source::{SourceFile, visit_nodes};
use crate::symbols::{self, Symbol};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSite {
    pub name: String,
    pub receiver: Option<String>,
    pub full: String,
    pub arity: usize,
    pub line: usize,
    pub enclosing: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub alias: Option<String>,
}

pub fn call_sites(file: &SourceFile, symbols: &[Symbol]) -> Vec<CallSite> {
    let mut out = Vec::new();
    visit_nodes(file.root(), &mut |node| {
        if node.kind() != "call" {
            return;
        }
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let full = collapse_ws(file.node_text(function));
        let (name, receiver) = split_callee(file, function);
        if name.is_empty() {
            return;
        }
        let arity = node
            .child_by_field_name("arguments")
            .map_or(0, |arguments| count_arguments(arguments));
        out.push(CallSite {
            name,
            receiver,
            full,
            arity,
            line: file.line_of(node.start_byte()),
            enclosing: symbols::enclosing(symbols, node.start_byte())
                .map(|symbol| symbol.dotted.clone()),
        });
    });
    out.sort_by(|a, b| a.line.cmp(&b.line).then(a.full.cmp(&b.full)));
    out
}

fn split_callee(file: &SourceFile, function: Node<'_>) -> (String, Option<String>) {
    match function.kind() {
        "identifier" => (file.node_text(function).to_string(), None),
        "attribute" => {
            let name = function
                .child_by_field_name("attribute")
                .map(|node| file.node_text(node).to_string())
                .unwrap_or_default();
            let receiver = function
                .child_by_field_name("object")
                .map(|node| collapse_ws(file.node_text(node)));
            (name, receiver)
        }
        _ => (String::new(), None),
    }
}

fn count_arguments(arguments: Node<'_>) -> usize {
    let mut cursor = arguments.walk();
    arguments
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .count()
}

pub fn imports(file: &SourceFile) -> Vec<Import> {
    let mut out = Vec::new();
    visit_nodes(file.root(), &mut |node| match node.kind() {
        "import_from_statement" => push_from_import(file, node, &mut out),
        "import_statement" => push_plain_import(file, node, &mut out),
        _ => {}
    });
    out.sort_by(|a, b| a.module.cmp(&b.module).then(a.name.cmp(&b.name)));
    out.dedup_by(|a, b| a.module == b.module && a.name == b.name && a.alias == b.alias);
    out
}

fn push_from_import(file: &SourceFile, node: Node<'_>, out: &mut Vec<Import>) {
    let module = node
        .child_by_field_name("module_name")
        .map(|inner| collapse_ws(file.node_text(inner)))
        .unwrap_or_default();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "dotted_name" | "identifier" => {
                if node
                    .child_by_field_name("module_name")
                    .is_some_and(|module_node| module_node.id() == child.id())
                {
                    continue;
                }
                out.push(Import {
                    module: module.clone(),
                    name: collapse_ws(file.node_text(child)),
                    alias: None,
                });
            }
            "aliased_import" => {
                let name = child
                    .child_by_field_name("name")
                    .map(|inner| collapse_ws(file.node_text(inner)))
                    .unwrap_or_default();
                let alias = child
                    .child_by_field_name("alias")
                    .map(|inner| file.node_text(inner).to_string());
                out.push(Import {
                    module: module.clone(),
                    name,
                    alias,
                });
            }
            _ => {}
        }
    }
}

fn push_plain_import(file: &SourceFile, node: Node<'_>, out: &mut Vec<Import>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "dotted_name" => out.push(Import {
                module: collapse_ws(file.node_text(child)),
                name: collapse_ws(file.node_text(child)),
                alias: None,
            }),
            "aliased_import" => {
                let name = child
                    .child_by_field_name("name")
                    .map(|inner| collapse_ws(file.node_text(inner)))
                    .unwrap_or_default();
                out.push(Import {
                    module: name.clone(),
                    name,
                    alias: child
                        .child_by_field_name("alias")
                        .map(|inner| file.node_text(inner).to_string()),
                });
            }
            _ => {}
        }
    }
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
    fn records_receiver_and_arity() {
        let file = parse("def go():\n    service.create_user(a, b)\n    plain(1)\n");
        let symbols = symbols::extract(&file);
        let calls = call_sites(&file, &symbols);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "create_user");
        assert_eq!(calls[0].receiver.as_deref(), Some("service"));
        assert_eq!(calls[0].arity, 2);
        assert_eq!(calls[0].enclosing.as_deref(), Some("go"));
        assert_eq!(calls[1].name, "plain");
        assert_eq!(calls[1].receiver, None);
    }

    #[test]
    fn reads_from_imports() {
        let file = parse("from .api.users import UserService, User as U\nimport os\n");
        let found = imports(&file);
        assert!(
            found
                .iter()
                .any(|item| item.name == "UserService" && item.module == ".api.users")
        );
        assert!(
            found
                .iter()
                .any(|item| item.name == "User" && item.alias.as_deref() == Some("U"))
        );
        assert!(found.iter().any(|item| item.name == "os"));
    }
}
