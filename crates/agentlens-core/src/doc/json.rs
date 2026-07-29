use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::doc::{
    DocFormat, DocKind, DocNode, child_path, index_path, line_of, line_starts, preview,
};
use crate::error::{Error, Result};

/// Parse JSON source text into document nodes.
///
/// # Errors
///
/// Returns [`Error::Parse`] if the tree-sitter parser cannot produce a tree.
pub fn parse(path: &Path, text: &str) -> Result<Vec<DocNode>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_json::LANGUAGE.into())
        .map_err(|_| Error::Parse(path.to_path_buf()))?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| Error::Parse(path.to_path_buf()))?;
    let starts = line_starts(text);
    let mut out = Vec::new();
    if let Some(root) = value_node(tree.root_node()) {
        children_of(root, "", text, &starts, &mut out);
    }
    Ok(out)
}

fn value_node(node: Node<'_>) -> Option<Node<'_>> {
    if node.kind() == "document" {
        let mut cursor = node.walk();
        return node
            .named_children(&mut cursor)
            .find(|child| child.kind() != "comment");
    }
    Some(node)
}

fn children_of(
    node: Node<'_>,
    parent_path: &str,
    text: &str,
    starts: &[usize],
    out: &mut Vec<DocNode>,
) {
    match node.kind() {
        "object" => {
            let mut cursor = node.walk();
            for pair in node.named_children(&mut cursor) {
                if pair.kind() != "pair" {
                    continue;
                }
                let Some(key_node) = pair.child_by_field_name("key") else {
                    continue;
                };
                let Some(value) = pair.child_by_field_name("value") else {
                    continue;
                };
                let key = unquote(&text[key_node.start_byte()..key_node.end_byte()]);
                let path = child_path(parent_path, &key, DocFormat::Json);
                out.push(build(key, path, value, pair.start_byte(), text, starts));
            }
        }
        "array" => {
            let mut cursor = node.walk();
            for (index, value) in node
                .named_children(&mut cursor)
                .filter(|child| child.kind() != "comment")
                .enumerate()
            {
                let path = index_path(parent_path, index);
                out.push(build(
                    index.to_string(),
                    path,
                    value,
                    value.start_byte(),
                    text,
                    starts,
                ));
            }
        }
        _ => {}
    }
}

fn build(
    key: String,
    path: String,
    value: Node<'_>,
    entry_start: usize,
    text: &str,
    starts: &[usize],
) -> DocNode {
    let kind = match value.kind() {
        "object" => DocKind::Mapping,
        "array" => DocKind::Sequence,
        _ => DocKind::Scalar,
    };
    let mut children = Vec::new();
    children_of(value, &path, text, starts, &mut children);
    let body = &text[value.start_byte()..value.end_byte()];
    DocNode {
        key,
        path,
        kind,
        span_start: value.start_byte(),
        span_end: value.end_byte(),
        entry_start,
        start_line: line_of(starts, value.start_byte()),
        end_line: line_of(starts, value.end_byte().saturating_sub(1)),
        summary: summarise(kind, body, children.len()),
        children,
    }
}

pub fn summarise(kind: DocKind, body: &str, count: usize) -> String {
    match kind {
        DocKind::Mapping => format!("{{{}}}", crate::render::plural(count, "key", "keys")),
        DocKind::Sequence => format!("[{}]", crate::render::plural(count, "item", "items")),
        _ => preview(body, 48),
    }
}

fn unquote(text: &str) -> String {
    text.trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Step, resolve};

    #[test]
    fn builds_a_key_tree() {
        let text = "{\n  \"a\": {\"b\": [1, 2]},\n  \"c\": \"x\"\n}\n";
        let nodes = parse(Path::new("t.json"), text).expect("parses");
        assert_eq!(nodes.len(), 2);
        let found = resolve(&nodes, &[Step::Key("a".into()), Step::Key("b".into())]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, DocKind::Sequence);
        assert_eq!(&text[found[0].span_start..found[0].span_end], "[1, 2]");
    }

    #[test]
    fn indexes_array_members() {
        let text = "{\"ports\": [8080, 9090]}";
        let nodes = parse(Path::new("t.json"), text).expect("parses");
        let found = resolve(&nodes, &[Step::Key("ports".into()), Step::Index(1)]);
        assert_eq!(&text[found[0].span_start..found[0].span_end], "9090");
    }
}
