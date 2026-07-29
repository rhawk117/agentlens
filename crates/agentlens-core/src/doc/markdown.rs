use crate::doc::{DocKind, DocNode, line_starts, preview};

#[derive(Debug, Clone)]
struct Heading {
    level: usize,
    title: String,
    line: usize,
    start: usize,
    body_start: usize,
}

pub fn parse(text: &str) -> Vec<DocNode> {
    let starts = line_starts(text);
    let headings = scan(text, &starts);
    let mut position = 0usize;
    build(&headings, &mut position, 1, "", text, &starts)
}

fn scan(text: &str, starts: &[usize]) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut fenced = false;
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(text.len());
        let raw = text[*start..end].trim_end_matches(['\n', '\r']);
        let trimmed = raw.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced || !trimmed.starts_with('#') {
            continue;
        }
        let level = trimmed.chars().take_while(|ch| *ch == '#').count();
        if level > 6 {
            continue;
        }
        let rest = &trimmed[level..];
        if !rest.is_empty() && !rest.starts_with(' ') {
            continue;
        }
        out.push(Heading {
            level,
            title: rest.trim().trim_end_matches('#').trim().to_string(),
            line: index + 1,
            start: *start,
            body_start: end,
        });
    }
    out
}

fn build(
    headings: &[Heading],
    position: &mut usize,
    level: usize,
    parent_path: &str,
    text: &str,
    starts: &[usize],
) -> Vec<DocNode> {
    let mut out = Vec::new();
    while let Some(heading) = headings.get(*position) {
        if heading.level < level {
            break;
        }
        if heading.level > level {
            let nested = build(headings, position, heading.level, parent_path, text, starts);
            out.extend(nested);
            continue;
        }
        *position += 1;
        let path = if parent_path.is_empty() {
            heading.title.clone()
        } else {
            format!("{parent_path}/{}", heading.title)
        };
        let children = build(headings, position, level + 1, &path, text, starts);
        let span_end = next_boundary(headings, *position, level, text);
        let body = &text[heading.body_start..span_end.max(heading.body_start)];
        out.push(DocNode {
            key: heading.title.clone(),
            path,
            kind: DocKind::Section,
            span_start: heading.start,
            span_end,
            entry_start: heading.start,
            start_line: heading.line,
            end_line: starts.partition_point(|start| *start < span_end).max(1),
            summary: format!("h{} — {}", heading.level, preview(body, 40)),
            children,
        });
    }
    out
}

fn next_boundary(headings: &[Heading], position: usize, level: usize, text: &str) -> usize {
    headings
        .iter()
        .skip(position)
        .find(|heading| heading.level <= level)
        .map_or(text.len(), |heading| heading.start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Step, resolve};

    const DOC: &str = "# Title\n\nIntro text.\n\n## Install\n\nRun it.\n\n### From source\n\ncargo build\n\n```\n# not a heading\n```\n\n## Usage\n\nGo.\n";

    #[test]
    fn nests_headings_by_level() {
        let nodes = parse(DOC);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].key, "Title");
        assert_eq!(nodes[0].children.len(), 2);
    }

    #[test]
    fn resolves_a_heading_path() {
        let nodes = parse(DOC);
        let found = resolve(
            &nodes,
            &[
                Step::Key("Title".into()),
                Step::Key("Install".into()),
                Step::Key("From source".into()),
            ],
        );
        assert_eq!(found.len(), 1);
        let body = &DOC[found[0].span_start..found[0].span_end];
        assert!(body.contains("cargo build"));
        assert!(!body.contains("## Usage"));
    }

    #[test]
    fn fenced_hashes_are_not_headings() {
        let nodes = parse(DOC);
        let flat = crate::doc::flatten(&nodes);
        assert!(!flat.iter().any(|node| node.key == "not a heading"));
    }

    #[test]
    fn a_section_span_covers_its_children() {
        let nodes = parse(DOC);
        let install = &nodes[0].children[0];
        let body = &DOC[install.span_start..install.span_end];
        assert!(body.contains("### From source"));
    }
}
