pub mod json;
pub mod markdown;
pub mod yaml;

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::render::{collapse_ws, slash_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DocFormat {
    Json,
    Yaml,
    Markdown,
}

impl DocFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "json" | "jsonc" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "md" | "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Markdown => "markdown",
        }
    }

    pub fn separator(self) -> char {
        match self {
            Self::Markdown => '/',
            _ => '.',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    Mapping,
    Sequence,
    Scalar,
    Section,
}

impl DocKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mapping => "mapping",
            Self::Sequence => "sequence",
            Self::Scalar => "scalar",
            Self::Section => "section",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DocNode {
    pub key: String,
    pub path: String,
    pub kind: DocKind,
    pub span_start: usize,
    pub span_end: usize,
    pub entry_start: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub summary: String,
    pub children: Vec<DocNode>,
}

impl DocNode {
    pub fn descendants(&self) -> usize {
        self.children
            .iter()
            .map(|child| 1 + child.descendants())
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocAddress {
    pub path: PathBuf,
    pub steps: Vec<Step>,
    pub raw_selector: String,
}

impl DocAddress {
    /// Parse a `path#selector` document address.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AddressMissingHash`] if `raw` has no `#`,
    /// [`Error::AddressEmptyPath`] if the file part is empty, and
    /// [`Error::UnsupportedFormat`] if the extension is not json, yaml or
    /// markdown.
    pub fn parse(raw: &str) -> Result<Self> {
        let Some(hash) = raw.find('#') else {
            return Err(Error::AddressMissingHash(raw.to_string()));
        };
        let (path_part, selector) = raw.split_at(hash);
        let selector = &selector[1..];
        if path_part.is_empty() {
            return Err(Error::AddressEmptyPath(raw.to_string()));
        }
        let path = PathBuf::from(path_part);
        let format =
            DocFormat::from_path(&path).ok_or_else(|| Error::UnsupportedFormat(path.clone()))?;
        Ok(Self {
            steps: parse_steps(selector, format),
            raw_selector: selector.to_string(),
            path,
        })
    }

    pub fn is_outline(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn display(&self) -> String {
        format!("{}#{}", slash_path(&self.path), self.raw_selector)
    }
}

fn parse_steps(selector: &str, format: DocFormat) -> Vec<Step> {
    if selector.is_empty() {
        return Vec::new();
    }
    if format == DocFormat::Markdown {
        return selector
            .split('/')
            .filter(|part| !part.is_empty())
            .map(|part| Step::Key(part.trim().to_string()))
            .collect();
    }
    let mut steps = Vec::new();
    let mut current = String::new();
    let mut chars = selector.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '.' => flush(&mut current, &mut steps),
            '[' => {
                flush(&mut current, &mut steps);
                let mut digits = String::new();
                for inner in chars.by_ref() {
                    if inner == ']' {
                        break;
                    }
                    digits.push(inner);
                }
                let trimmed = digits.trim().trim_matches(['"', '\'']);
                match trimmed.parse::<usize>() {
                    Ok(index) => steps.push(Step::Index(index)),
                    Err(_) => steps.push(Step::Key(trimmed.to_string())),
                }
            }
            _ => current.push(ch),
        }
    }
    flush(&mut current, &mut steps);
    steps
}

fn flush(current: &mut String, steps: &mut Vec<Step>) {
    if current.is_empty() {
        return;
    }
    steps.push(Step::Key(std::mem::take(current)));
}

/// Read and parse a document file, returning its format, text and nodes.
///
/// # Errors
///
/// Returns [`Error::UnsupportedFormat`] if the extension maps to no known
/// format, [`Error::Io`] if `path` cannot be read, [`Error::NotUtf8`] if its
/// contents are not valid UTF-8, and [`Error::Parse`] if parsing fails.
pub fn parse_file(path: &Path) -> Result<(DocFormat, String, Vec<DocNode>)> {
    let format =
        DocFormat::from_path(path).ok_or_else(|| Error::UnsupportedFormat(path.to_path_buf()))?;
    let bytes = std::fs::read(path).map_err(|err| Error::Io(path.to_path_buf(), err))?;
    let text = String::from_utf8(bytes).map_err(|_| Error::NotUtf8(path.to_path_buf()))?;
    let nodes = match format {
        DocFormat::Json => json::parse(path, &text)?,
        DocFormat::Yaml => yaml::parse(path, &text)?,
        DocFormat::Markdown => markdown::parse(&text),
    };
    Ok((format, text, nodes))
}

pub fn resolve<'a>(nodes: &'a [DocNode], steps: &[Step]) -> Vec<&'a DocNode> {
    let Some((head, rest)) = steps.split_first() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (position, node) in nodes.iter().enumerate() {
        let hit = match head {
            Step::Key(name) => node.key == *name,
            Step::Index(index) => position == *index,
        };
        if !hit {
            continue;
        }
        if rest.is_empty() {
            out.push(node);
        } else {
            out.extend(resolve(&node.children, rest));
        }
    }
    out
}

pub fn flatten(nodes: &[DocNode]) -> Vec<&DocNode> {
    let mut out = Vec::new();
    push_flat(nodes, &mut out);
    out
}

fn push_flat<'a>(nodes: &'a [DocNode], out: &mut Vec<&'a DocNode>) {
    for node in nodes {
        out.push(node);
        push_flat(&node.children, out);
    }
}

pub fn child_path(parent: &str, key: &str, format: DocFormat) -> String {
    if parent.is_empty() {
        return key.to_string();
    }
    format!("{parent}{}{key}", format.separator())
}

pub fn index_path(parent: &str, index: usize) -> String {
    format!("{parent}[{index}]")
}

pub fn preview(text: &str, limit: usize) -> String {
    let one_line = collapse_ws(text);
    if one_line.chars().count() <= limit {
        return one_line;
    }
    let clipped: String = one_line.chars().take(limit.saturating_sub(3)).collect();
    format!("{clipped}...")
}

pub fn line_of(line_starts: &[usize], byte: usize) -> usize {
    line_starts.partition_point(|start| *start <= byte).max(1)
}

pub fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    if starts.len() > 1 && starts.last() == Some(&text.len()) {
        starts.pop();
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dotted_and_indexed_steps() {
        let address = DocAddress::parse("compose.yaml#services.web.ports[0]").expect("parses");
        assert_eq!(
            address.steps,
            vec![
                Step::Key("services".into()),
                Step::Key("web".into()),
                Step::Key("ports".into()),
                Step::Index(0)
            ]
        );
    }

    #[test]
    fn markdown_steps_split_on_slash() {
        let address = DocAddress::parse("README.md#Install/From source").expect("parses");
        assert_eq!(
            address.steps,
            vec![Step::Key("Install".into()), Step::Key("From source".into())]
        );
    }

    #[test]
    fn quoted_keys_survive_dots() {
        let address = DocAddress::parse("a.json#[\"a.b\"].c").expect("parses");
        assert_eq!(
            address.steps,
            vec![Step::Key("a.b".into()), Step::Key("c".into())]
        );
    }

    #[test]
    fn an_empty_selector_is_an_outline() {
        assert!(DocAddress::parse("a.json#").expect("parses").is_outline());
    }
}
