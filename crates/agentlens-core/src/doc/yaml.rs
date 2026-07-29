use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::doc::json::summarise;
use crate::doc::{DocFormat, DocKind, DocNode, child_path, index_path, line_of, line_starts};
use crate::error::{Error, Result};

const TRANSPARENT: &[&str] = &["stream", "document", "block_node", "flow_node"];

/// Parse YAML source text into document nodes.
///
/// # Errors
///
/// Returns [`Error::Parse`] if the tree-sitter parser cannot produce a tree.
pub fn parse(path: &Path, text: &str) -> Result<Vec<DocNode>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_yaml::LANGUAGE.into())
        .map_err(|_| Error::Parse(path.to_path_buf()))?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| Error::Parse(path.to_path_buf()))?;
    let starts = line_starts(text);
    let mut out = Vec::new();
    children_of(unwrap(tree.root_node()), "", text, &starts, &mut out);
    Ok(out)
}

fn unwrap(node: Node<'_>) -> Node<'_> {
    let mut current = node;
    loop {
        if !TRANSPARENT.contains(&current.kind()) {
            return current;
        }
        let Some(next) = first_named(current) else {
            return current;
        };
        current = next;
    }
}

fn children_of(
    node: Node<'_>,
    parent_path: &str,
    text: &str,
    starts: &[usize],
    out: &mut Vec<DocNode>,
) {
    match node.kind() {
        "block_mapping" | "flow_mapping" => {
            let mut cursor = node.walk();
            for pair in node.named_children(&mut cursor) {
                if !matches!(pair.kind(), "block_mapping_pair" | "flow_pair") {
                    continue;
                }
                let Some(key_node) = pair.child_by_field_name("key") else {
                    continue;
                };
                let key = scalar_text(text, unwrap(key_node));
                let path = child_path(parent_path, &key, DocFormat::Yaml);
                let value = pair.child_by_field_name("value");
                out.push(build(key, path, value, pair, text, starts));
            }
        }
        "block_sequence" | "flow_sequence" => {
            let mut cursor = node.walk();
            for (index, item) in node
                .named_children(&mut cursor)
                .filter(|child| {
                    matches!(
                        child.kind(),
                        "block_sequence_item" | "flow_node" | "block_node"
                    )
                })
                .enumerate()
            {
                let path = index_path(parent_path, index);
                let value = if item.kind() == "block_sequence_item" {
                    first_named(item)
                } else {
                    Some(item)
                };
                out.push(build(index.to_string(), path, value, item, text, starts));
            }
        }
        _ => {}
    }
}

fn first_named(node: Node<'_>) -> Option<Node<'_>> {
    let count = u32::try_from(node.named_child_count()).unwrap_or(u32::MAX);
    (0..count)
        .filter_map(|index| node.named_child(index))
        .find(|child| child.kind() != "comment")
}

fn build(
    key: String,
    path: String,
    value: Option<Node<'_>>,
    entry: Node<'_>,
    text: &str,
    starts: &[usize],
) -> DocNode {
    let Some(value) = value else {
        return DocNode {
            key,
            path,
            kind: DocKind::Scalar,
            span_start: entry.start_byte(),
            span_end: entry.end_byte(),
            entry_start: entry.start_byte(),
            start_line: line_of(starts, entry.start_byte()),
            end_line: line_of(starts, entry.end_byte().saturating_sub(1)),
            summary: "~".to_string(),
            children: Vec::new(),
        };
    };
    let inner = unwrap(value);
    let kind = match inner.kind() {
        "block_mapping" | "flow_mapping" => DocKind::Mapping,
        "block_sequence" | "flow_sequence" => DocKind::Sequence,
        _ => DocKind::Scalar,
    };
    let mut children = Vec::new();
    children_of(inner, &path, text, starts, &mut children);
    let body = &text[value.start_byte()..value.end_byte()];
    DocNode {
        key,
        path,
        kind,
        span_start: value.start_byte(),
        span_end: value.end_byte(),
        entry_start: entry.start_byte(),
        start_line: line_of(starts, value.start_byte()),
        end_line: line_of(starts, value.end_byte().saturating_sub(1)),
        summary: summarise(kind, body, children.len()),
        children,
    }
}

fn scalar_text(text: &str, node: Node<'_>) -> String {
    text[node.start_byte()..node.end_byte()]
        .trim()
        .trim_matches(['"', '\''])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Step, resolve};

    const COMPOSE: &str = "services:\n  web:\n    image: nginx  # pinned later\n    ports:\n      - 8080:80\n      - 9090:90\n  db:\n    image: postgres\n";

    #[test]
    fn walks_nested_mappings() {
        let nodes = parse(Path::new("t.yaml"), COMPOSE).expect("parses");
        let found = resolve(
            &nodes,
            &[
                Step::Key("services".into()),
                Step::Key("web".into()),
                Step::Key("image".into()),
            ],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(&COMPOSE[found[0].span_start..found[0].span_end], "nginx");
    }

    #[test]
    fn indexes_sequence_items() {
        let nodes = parse(Path::new("t.yaml"), COMPOSE).expect("parses");
        let found = resolve(
            &nodes,
            &[
                Step::Key("services".into()),
                Step::Key("web".into()),
                Step::Key("ports".into()),
                Step::Index(1),
            ],
        );
        assert_eq!(&COMPOSE[found[0].span_start..found[0].span_end], "9090:90");
    }

    #[test]
    fn slicing_a_block_keeps_the_comment() {
        let nodes = parse(Path::new("t.yaml"), COMPOSE).expect("parses");
        let found = resolve(
            &nodes,
            &[Step::Key("services".into()), Step::Key("web".into())],
        );
        let body = &COMPOSE[found[0].span_start..found[0].span_end];
        assert!(body.contains("# pinned later"));
    }
}
