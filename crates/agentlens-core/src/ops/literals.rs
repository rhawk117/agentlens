use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::json;
use tree_sitter::Node;

use crate::address::{Address, Selector};
use crate::budget::{DEFAULT_BUDGET, Detail, estimate_tokens, fit};
use crate::error::Result;
use crate::matcher::Matcher;
use crate::ops::Report;
use crate::ops::find::collect_targets;
use crate::render::{Lines, pad, plural, slash_path};
use crate::source::{SourceFile, visit_nodes};
use crate::symbols;

const REGEX_META: &[char] = &[
    '^', '$', '[', ']', '(', ')', '{', '}', '\\', '|', '+', '*', '?',
];
const REGEX_MODULES: &[&str] = &["re.", "regex."];
const HARD_CAP: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LiteralKind {
    Number,
    Regex,
    String,
}

impl LiteralKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Regex => "regex",
            Self::String => "string",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralFilter {
    All,
    Only(LiteralKind),
}

impl LiteralFilter {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "all" | "any" => Some(Self::All),
            "string" | "str" => Some(Self::Only(LiteralKind::String)),
            "number" | "num" => Some(Self::Only(LiteralKind::Number)),
            "regex" | "re" => Some(Self::Only(LiteralKind::Regex)),
            _ => None,
        }
    }

    fn allows(self, kind: LiteralKind) -> bool {
        match self {
            Self::All => true,
            Self::Only(wanted) => wanted == kind,
        }
    }
}

// Four independent CLI switches; see the note on FindOptions.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct LiteralsOptions {
    pub kind: LiteralFilter,
    pub match_pattern: Option<String>,
    pub min_len: usize,
    pub scope: Option<Address>,
    pub group: bool,
    pub skeleton: bool,
    pub include_docstrings: bool,
    pub budget: usize,
    pub quiet: bool,
}

impl Default for LiteralsOptions {
    fn default() -> Self {
        Self {
            kind: LiteralFilter::All,
            match_pattern: None,
            min_len: 2,
            scope: None,
            group: true,
            skeleton: true,
            include_docstrings: false,
            budget: DEFAULT_BUDGET,
            quiet: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LiteralHit {
    pub path: String,
    pub line: usize,
    pub kind: LiteralKind,
    pub value: String,
    pub enclosing: Option<String>,
    pub address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiteralGroup {
    pub value: String,
    pub kind: LiteralKind,
    pub count: usize,
    pub sites: Vec<Site>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Site {
    pub address: String,
    pub line: usize,
}

/// Extract string and numeric literals from `paths`.
///
/// # Errors
///
/// Returns [`Error::BadRegex`] if `options.match_pattern` is not a valid
/// regex, and propagates [`Error::Io`], [`Error::NotUtf8`], and
/// [`Error::Parse`] from the files it visits.
pub fn run(paths: &[PathBuf], options: &LiteralsOptions) -> Result<Report> {
    let value_matcher = match &options.match_pattern {
        Some(pattern) => Some(Matcher::new(pattern, false)?),
        None => None,
    };

    let targets = match &options.scope {
        Some(address) => vec![address.path.clone()],
        None => collect_targets(paths),
    };

    let mut hits: Vec<LiteralHit> = Vec::new();
    let mut capped = false;
    for path in targets {
        let Ok(file) = SourceFile::load(&path) else {
            continue;
        };
        let ranges = scope_ranges(&file, options);
        if ranges.is_empty() && options.scope.is_some() {
            continue;
        }
        collect(
            &file,
            &ranges,
            options,
            value_matcher.as_ref(),
            &mut hits,
            &mut capped,
        );
        if capped {
            break;
        }
    }

    hits.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.line.cmp(&b.line))
            .then(a.value.cmp(&b.value))
    });

    let groups = group(&hits);
    let found = !hits.is_empty();
    let (text, detail, degraded) = fit(options.budget, |detail| {
        render(&hits, &groups, capped, detail, options)
    });
    let json = json!({
        "command": "literals",
        "found": found,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "truncated": capped,
        "grouped": options.group,
        "skeleton": options.skeleton,
        "summary": {
            "literals": hits.len(),
            "distinct": groups.len(),
        },
        "groups": if options.group { Some(&groups) } else { None },
        "literals": if options.group { None } else { Some(&hits) },
    });
    Ok(Report::new(text, json, found))
}

fn scope_ranges(file: &SourceFile, options: &LiteralsOptions) -> Vec<(usize, usize)> {
    let Some(address) = &options.scope else {
        return vec![(0, file.text.len())];
    };
    match &address.selector {
        Selector::Outline => vec![(0, file.text.len())],
        Selector::Lines(start, end) => vec![file.lines_span(*start, *end)],
        Selector::Symbol(parts) => {
            let symbols = symbols::extract(file);
            symbols::resolve(&symbols, parts)
                .into_iter()
                .map(|symbol| (symbol.span_start, symbol.span_end))
                .collect()
        }
    }
}

fn collect(
    file: &SourceFile,
    ranges: &[(usize, usize)],
    options: &LiteralsOptions,
    value_matcher: Option<&Matcher>,
    hits: &mut Vec<LiteralHit>,
    capped: &mut bool,
) {
    let symbols = symbols::extract(file);
    let path = slash_path(&file.path);
    visit_nodes(file.root(), &mut |node| {
        if *capped {
            return;
        }
        let kind = match node.kind() {
            "string" => classify_string(file, node),
            "integer" | "float" => LiteralKind::Number,
            _ => return,
        };
        let start = node.start_byte();
        if !ranges
            .iter()
            .any(|(from, to)| start >= *from && start < *to)
        {
            return;
        }
        if !options.kind.allows(kind) {
            return;
        }
        if !options.include_docstrings && is_docstring(node) {
            return;
        }
        let value = match kind {
            LiteralKind::Number => file.node_text(node).to_string(),
            _ => string_value(file, node, options.skeleton),
        };
        if kind != LiteralKind::Number && value.chars().count() < options.min_len {
            return;
        }
        if value_matcher.is_some_and(|matcher| !matcher.matches(&value)) {
            return;
        }
        if hits.len() >= HARD_CAP {
            *capped = true;
            return;
        }
        let enclosing = symbols::enclosing(&symbols, start).map(|symbol| symbol.dotted.clone());
        let line = file.line_of(start);
        let address = enclosing.as_ref().map_or_else(
            || format!("{path}#L{line}"),
            |dotted| format!("{path}#{dotted}"),
        );
        hits.push(LiteralHit {
            path: path.clone(),
            line,
            kind,
            value,
            enclosing,
            address,
        });
    });
}

fn is_docstring(node: Node<'_>) -> bool {
    let Some(statement) = node.parent() else {
        return false;
    };
    if statement.kind() != "expression_statement" {
        return false;
    }
    let Some(container) = statement.parent() else {
        return false;
    };
    if !matches!(container.kind(), "module" | "block") {
        return false;
    }
    container
        .named_child(0)
        .is_some_and(|first| first.id() == statement.id())
}

fn classify_string(file: &SourceFile, node: Node<'_>) -> LiteralKind {
    if in_regex_call(file, node) {
        return LiteralKind::Regex;
    }
    let prefix = string_prefix(file, node);
    if prefix.contains('r') {
        let body = string_value(file, node, false);
        if body.chars().any(|ch| REGEX_META.contains(&ch)) {
            return LiteralKind::Regex;
        }
    }
    LiteralKind::String
}

fn in_regex_call(file: &SourceFile, node: Node<'_>) -> bool {
    let Some(arguments) = node.parent() else {
        return false;
    };
    if arguments.kind() != "argument_list" {
        return false;
    }
    let Some(call) = arguments.parent() else {
        return false;
    };
    if call.kind() != "call" {
        return false;
    }
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    let name = file.node_text(function);
    REGEX_MODULES.iter().any(|module| name.starts_with(module))
}

fn string_prefix(file: &SourceFile, node: Node<'_>) -> String {
    let text = file.node_text(node);
    text.chars()
        .take_while(|ch| *ch != '"' && *ch != '\'')
        .flat_map(char::to_lowercase)
        .collect()
}

fn string_value(file: &SourceFile, node: Node<'_>, skeleton: bool) -> String {
    if !skeleton {
        return file.node_text(node).to_string();
    }
    let mut cursor = node.walk();
    let mut out = String::new();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "string_content" => out.push_str(file.node_text(child)),
            "interpolation" => out.push_str("<>"),
            _ => {}
        }
    }
    out
}

fn group(hits: &[LiteralHit]) -> Vec<LiteralGroup> {
    let mut index: BTreeMap<(LiteralKind, String), Vec<Site>> = BTreeMap::new();
    for hit in hits {
        index
            .entry((hit.kind, hit.value.clone()))
            .or_default()
            .push(Site {
                address: hit.address.clone(),
                line: hit.line,
            });
    }
    let mut groups: Vec<LiteralGroup> = index
        .into_iter()
        .map(|((kind, value), sites)| LiteralGroup {
            value,
            kind,
            count: sites.len(),
            sites,
        })
        .collect();
    groups.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.kind.cmp(&b.kind))
            .then(a.value.cmp(&b.value))
    });
    groups
}

fn render(
    hits: &[LiteralHit],
    groups: &[LiteralGroup],
    capped: bool,
    detail: Detail,
    options: &LiteralsOptions,
) -> String {
    let mut out = Lines::new();
    if hits.is_empty() {
        out.push("no literals matched".to_string());
        if !options.include_docstrings {
            out.push("docstrings are excluded: add --include-docstrings".to_string());
        }
        return out.finish();
    }

    if detail == Detail::Counts {
        let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
        for hit in hits {
            *by_kind.entry(hit.kind.as_str()).or_default() += 1;
        }
        out.push(format!(
            "{}, {} distinct",
            plural(hits.len(), "literal", "literals"),
            groups.len()
        ));
        for (kind, count) in by_kind {
            out.push(format!("  {}", plural(count, kind, &format!("{kind}s"))));
        }
        out.push("raise --budget for the values".to_string());
        return out.finish();
    }

    if options.group {
        let width = groups
            .iter()
            .map(|group| display_value(&group.value, group.kind).chars().count())
            .max()
            .unwrap_or(0)
            .min(56);
        for group in groups {
            out.push(format!(
                "{}  {}  x{}",
                pad(&display_value(&group.value, group.kind), width),
                pad(group.kind.as_str(), 6),
                group.count
            ));
            if detail == Detail::Full {
                let site_width = group
                    .sites
                    .iter()
                    .map(|site| site.address.chars().count())
                    .max()
                    .unwrap_or(0)
                    .min(64);
                for site in &group.sites {
                    out.push(format!(
                        "  {}  L{}",
                        pad(&site.address, site_width),
                        site.line
                    ));
                }
            }
        }
    } else {
        let mut current = String::new();
        for hit in hits {
            if hit.path != current {
                out.blank();
                out.push(hit.path.clone());
                current.clone_from(&hit.path);
            }
            out.push(format!(
                "  {}  {}  {}",
                pad(&format!("L{}", hit.line), 6),
                pad(hit.kind.as_str(), 6),
                display_value(&hit.value, hit.kind)
            ));
        }
    }

    if detail == Detail::Summary {
        out.push("budget reached: values without call sites, raise --budget".to_string());
    }
    if capped {
        out.push(format!(
            "showing the first {} literals — narrow with --in or --match",
            HARD_CAP
        ));
    }
    if !options.quiet {
        out.blank();
        out.push(format!(
            "{}, {} distinct",
            plural(hits.len(), "literal", "literals"),
            groups.len()
        ));
    }
    out.finish()
}

fn display_value(value: &str, kind: LiteralKind) -> String {
    let one_line: String = value.replace('\n', "\\n");
    let clipped = if one_line.chars().count() <= 56 {
        one_line
    } else {
        let head: String = one_line.chars().take(53).collect();
        format!("{head}...")
    };
    if kind == LiteralKind::Number {
        clipped
    } else {
        format!("{clipped:?}")
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

    fn gather(file: &SourceFile, options: &LiteralsOptions) -> Vec<LiteralHit> {
        let mut hits = Vec::new();
        let mut capped = false;
        collect(
            file,
            &[(0, file.text.len())],
            options,
            None,
            &mut hits,
            &mut capped,
        );
        hits
    }

    #[test]
    fn f_strings_become_skeletons() {
        let file = parse("def f(host):\n    return f\"refused: {host}\"\n");
        let hits = gather(&file, &LiteralsOptions::default());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].value, "refused: <>");
    }

    #[test]
    fn docstrings_are_excluded_by_default() {
        let file = parse("\"\"\"module doc\"\"\"\nX = \"kept\"\n");
        let hits = gather(&file, &LiteralsOptions::default());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].value, "kept");
    }

    #[test]
    fn regex_calls_are_tagged() {
        let file = parse("import re\nP = re.compile(\"^a+$\")\n");
        let hits = gather(&file, &LiteralsOptions::default());
        assert_eq!(hits[0].kind, LiteralKind::Regex);
    }

    #[test]
    fn min_len_skips_trivial_strings() {
        let file = parse("A = \"x\"\nB = \"xyz\"\n");
        let hits = gather(&file, &LiteralsOptions::default());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].value, "xyz");
    }

    #[test]
    fn identical_values_group_with_counts() {
        let file = parse("A = \"same\"\nB = \"same\"\n");
        let hits = gather(&file, &LiteralsOptions::default());
        let groups = group(&hits);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].count, 2);
    }
}
