use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::json;
use tree_sitter::Node;

use crate::budget::{DEFAULT_BUDGET, Detail, estimate_tokens, fit};
use crate::error::Result;
use crate::matcher::Matcher;
use crate::ops::Report;
use crate::render::{Lines, pad, plural, slash_path, truncation_note};
use crate::source::{SourceFile, visit_nodes};
use crate::symbols::{self, Symbol};
use crate::walk;

const HARD_CAP: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Occurrence {
    Definition,
    Call,
    Reference,
    Comment,
    String,
}

impl Occurrence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::Call => "call",
            Self::Reference => "reference",
            Self::Comment => "comment",
            Self::String => "string",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceFilter {
    Any,
    Only(Occurrence),
}

impl OccurrenceFilter {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "any" | "all" => Some(Self::Any),
            "definition" | "def" => Some(Self::Only(Occurrence::Definition)),
            "call" => Some(Self::Only(Occurrence::Call)),
            "reference" | "ref" => Some(Self::Only(Occurrence::Reference)),
            _ => None,
        }
    }

    fn allows(self, kind: Occurrence) -> bool {
        match self {
            Self::Any => true,
            Self::Only(wanted) => wanted == kind,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Symbol,
    None,
}

impl Context {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "symbol" => Some(Self::Symbol),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

// Four independent CLI switches. Collapsing them into an enum would imply
// mutual exclusivity that does not exist: any combination is valid.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct FindOptions {
    pub kind: OccurrenceFilter,
    pub exact: bool,
    pub include_comments: bool,
    pub include_strings: bool,
    pub context: Context,
    pub budget: usize,
    pub quiet: bool,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            kind: OccurrenceFilter::Any,
            exact: false,
            include_comments: false,
            include_strings: false,
            context: Context::Symbol,
            budget: DEFAULT_BUDGET,
            quiet: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub path: String,
    pub line: usize,
    pub occurrence: Occurrence,
    pub text: String,
    pub enclosing: Option<String>,
    pub address: String,
}

#[derive(Debug, Clone, Serialize)]
struct FileGroup {
    path: String,
    hits: Vec<Hit>,
}

/// Find occurrences of `pattern` across `paths`.
///
/// # Errors
///
/// Returns [`Error::BadRegex`] if `pattern` is not a valid regex and
/// `options.exact` is unset, and propagates read and parse failures.
pub fn run(pattern: &str, paths: &[PathBuf], options: &FindOptions) -> Result<Report> {
    let matcher = Matcher::new(pattern, options.exact)?;
    let targets = collect_targets(paths);
    let mut groups: Vec<FileGroup> = Vec::new();
    let mut total = 0usize;
    let mut capped = false;

    for path in targets {
        let Ok(file) = SourceFile::load(&path) else {
            continue;
        };
        let hits = scan(&file, &matcher, options, &mut total, &mut capped);
        if hits.is_empty() {
            continue;
        }
        groups.push(FileGroup {
            path: slash_path(&path),
            hits,
        });
        if capped {
            break;
        }
    }

    groups.sort_by(|a, b| a.path.cmp(&b.path));
    let found = !groups.is_empty();
    let hit_count: usize = groups.iter().map(|group| group.hits.len()).sum();

    let (text, detail, degraded) = fit(options.budget, |detail| {
        render(pattern, &groups, hit_count, capped, detail, options)
    });
    let json = json!({
        "command": "find",
        "pattern": pattern,
        "found": found,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "truncated": capped,
        "summary": {
            "matches": hit_count,
            "files": groups.len(),
        },
        "files": groups,
    });
    Ok(Report::new(text, json, found))
}

pub fn collect_targets(paths: &[PathBuf]) -> Vec<PathBuf> {
    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for root in roots {
        out.extend(walk::source_files(&root));
    }
    out.sort();
    out.dedup();
    out
}

fn scan(
    file: &SourceFile,
    matcher: &Matcher,
    options: &FindOptions,
    total: &mut usize,
    capped: &mut bool,
) -> Vec<Hit> {
    let symbols = symbols::extract(file);
    let path = slash_path(&file.path);
    let mut hits: Vec<Hit> = Vec::new();
    let mut seen: BTreeSet<(usize, &'static str)> = BTreeSet::new();

    visit_nodes(file.root(), &mut |node| {
        if *capped {
            return;
        }
        let kind = node.kind();
        if kind == "identifier" {
            let text = file.node_text(node);
            if !matcher.matches(text) {
                return;
            }
            let occurrence = classify(node);
            if !options.kind.allows(occurrence) {
                return;
            }
            if !seen.insert((node.start_byte(), occurrence.as_str())) {
                return;
            }
            push_hit(
                file, &symbols, &path, node, occurrence, text, &mut hits, total, capped,
            );
            return;
        }
        if options.include_comments && file.lang.comment_kinds().contains(&kind) {
            let text = file.node_text(node).trim().to_string();
            if matcher.matches(&text) && options.kind.allows(Occurrence::Comment) {
                push_hit(
                    file,
                    &symbols,
                    &path,
                    node,
                    Occurrence::Comment,
                    &text,
                    &mut hits,
                    total,
                    capped,
                );
            }
            return;
        }
        if options.include_strings && kind == "string_content" {
            let text = file.node_text(node).trim().to_string();
            if matcher.matches(&text) && options.kind.allows(Occurrence::String) {
                push_hit(
                    file,
                    &symbols,
                    &path,
                    node,
                    Occurrence::String,
                    &text,
                    &mut hits,
                    total,
                    capped,
                );
            }
        }
    });

    hits.sort_by(|a, b| a.line.cmp(&b.line).then(a.text.cmp(&b.text)));
    hits
}

#[allow(clippy::too_many_arguments)]
fn push_hit(
    file: &SourceFile,
    symbols: &[Symbol],
    path: &str,
    node: Node<'_>,
    occurrence: Occurrence,
    text: &str,
    hits: &mut Vec<Hit>,
    total: &mut usize,
    capped: &mut bool,
) {
    if *total >= HARD_CAP {
        *capped = true;
        return;
    }
    *total += 1;
    let enclosing =
        symbols::enclosing(symbols, node.start_byte()).map(|symbol| symbol.dotted.clone());
    let address = enclosing.as_ref().map_or_else(
        || format!("{path}#L{}", file.line_of(node.start_byte())),
        |dotted| format!("{path}#{dotted}"),
    );
    hits.push(Hit {
        path: path.to_string(),
        line: file.line_of(node.start_byte()),
        occurrence,
        text: truncate_text(text),
        enclosing,
        address,
    });
}

fn truncate_text(text: &str) -> String {
    let one_line = text.lines().next().unwrap_or_default();
    if one_line.chars().count() <= 60 {
        return one_line.to_string();
    }
    let clipped: String = one_line.chars().take(57).collect();
    format!("{clipped}...")
}

fn classify(node: Node<'_>) -> Occurrence {
    let Some(parent) = node.parent() else {
        return Occurrence::Reference;
    };
    if matches!(parent.kind(), "function_definition" | "class_definition")
        && parent
            .child_by_field_name("name")
            .is_some_and(|name| name.id() == node.id())
    {
        return Occurrence::Definition;
    }
    if parent.kind() == "call"
        && parent
            .child_by_field_name("function")
            .is_some_and(|function| function.id() == node.id())
    {
        return Occurrence::Call;
    }
    if parent.kind() == "attribute"
        && parent
            .child_by_field_name("attribute")
            .is_some_and(|attribute| attribute.id() == node.id())
        && let Some(grand) = parent.parent()
        && grand.kind() == "call"
        && grand
            .child_by_field_name("function")
            .is_some_and(|function| function.id() == parent.id())
    {
        return Occurrence::Call;
    }
    Occurrence::Reference
}

fn render(
    pattern: &str,
    groups: &[FileGroup],
    hit_count: usize,
    capped: bool,
    detail: Detail,
    options: &FindOptions,
) -> String {
    let mut out = Lines::new();
    if groups.is_empty() {
        out.push(format!("no match for `{pattern}`"));
        if !options.include_comments || !options.include_strings {
            out.push(
                "comments and string bodies are excluded: add --include-comments or --include-strings"
                    .to_string(),
            );
        }
        return out.finish();
    }

    match detail {
        Detail::Counts => {
            out.push(format!(
                "`{pattern}`: {} in {}",
                plural(hit_count, "match", "matches"),
                plural(groups.len(), "file", "files")
            ));
            for group in groups {
                out.push(format!(
                    "  {}  {}",
                    group.path,
                    plural(group.hits.len(), "match", "matches")
                ));
            }
            out.push("raise --budget for the locations".to_string());
        }
        Detail::Summary => {
            for group in groups {
                let symbols: BTreeSet<&str> = group
                    .hits
                    .iter()
                    .filter_map(|hit| hit.enclosing.as_deref())
                    .collect();
                out.push(format!(
                    "{}  {}",
                    group.path,
                    plural(group.hits.len(), "match", "matches")
                ));
                if !symbols.is_empty() {
                    out.push(format!(
                        "  in {}",
                        symbols.into_iter().collect::<Vec<_>>().join(", ")
                    ));
                }
            }
            out.push("budget reached: per-file counts only, raise --budget for lines".to_string());
        }
        Detail::Full => {
            let width = groups
                .iter()
                .flat_map(|group| group.hits.iter())
                .map(|hit| hit.text.chars().count())
                .max()
                .unwrap_or(0)
                .min(48);
            for group in groups {
                out.blank();
                out.push(group.path.clone());
                for hit in &group.hits {
                    out.push(hit_row(hit, width, options.context));
                }
            }
        }
    }

    if capped
        && let Some(note) = truncation_note(hit_count, HARD_CAP + 1, "narrow the pattern or paths")
    {
        out.push(note);
    }

    if !options.quiet {
        out.blank();
        out.push(format!(
            "{} in {}",
            plural(hit_count, "match", "matches"),
            plural(groups.len(), "file", "files")
        ));
    }
    out.finish()
}

fn hit_row(hit: &Hit, width: usize, context: Context) -> String {
    let head = format!(
        "  {}  {}",
        pad(&format!("L{}", hit.line), 6),
        pad(hit.occurrence.as_str(), 12)
    );
    match context {
        Context::None => format!("{head}{}", hit.text).trim_end().to_string(),
        Context::Symbol => match &hit.enclosing {
            Some(dotted) => format!("{head}{}  in #{dotted}", pad(&hit.text, width)),
            None => format!("{head}{}  at module level", pad(&hit.text, width)),
        },
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
    fn classifies_definition_call_and_reference() {
        let file = parse("def run(x):\n    return run(x) + run\n");
        let matcher = Matcher::new("run", true).expect("built");
        let mut total = 0;
        let mut capped = false;
        let hits = scan(
            &file,
            &matcher,
            &FindOptions::default(),
            &mut total,
            &mut capped,
        );
        let kinds: Vec<Occurrence> = hits.iter().map(|hit| hit.occurrence).collect();
        assert!(kinds.contains(&Occurrence::Definition));
        assert!(kinds.contains(&Occurrence::Call));
        assert!(kinds.contains(&Occurrence::Reference));
    }

    #[test]
    fn comments_are_excluded_by_default() {
        let file = parse("# run me\ndef other():\n    pass\n");
        let matcher = Matcher::new("run", false).expect("built");
        let mut total = 0;
        let mut capped = false;
        let hits = scan(
            &file,
            &matcher,
            &FindOptions::default(),
            &mut total,
            &mut capped,
        );
        assert!(hits.is_empty());

        let options = FindOptions {
            include_comments: true,
            ..FindOptions::default()
        };
        let mut total = 0;
        let mut capped = false;
        let hits = scan(&file, &matcher, &options, &mut total, &mut capped);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].occurrence, Occurrence::Comment);
    }
}
