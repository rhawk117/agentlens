use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use regex::Regex;
use serde::Serialize;
use serde_json::json;

use crate::budget::{DEFAULT_BUDGET, Detail, estimate_tokens, fit};
use crate::error::{Error, Result};
use crate::ops::Report;
use crate::render::{Lines, collapse_ws, indent, line_span, pad, plural, slash_path};
use crate::source::SourceFile;
use crate::symbols::{self, KindFilter, Symbol, SymbolKind};
use crate::walk;

const ROUTE_DECORATOR: &str = r"^@[\w.]+\.(route|get|post|put|patch|delete|head|options|websocket|command|task|on_event|middleware)\b";
const MAIN_GUARD: &str = "__name__ ==";

/// Width past which a module-level value is shown as a shape rather than
/// inlined. One `LANGUAGES` literal cost 3,741 tokens in the benchmark.
const MAX_VALUE_CHARS: usize = 72;

#[derive(Debug, Clone)]
pub struct MapOptions {
    pub depth: usize,
    pub kind: KindFilter,
    pub budget: usize,
    pub quiet: bool,
    /// Dotted symbol to root the outline at, from a `file.py#Class` target.
    pub root: Option<String>,
    /// Show whole values instead of collapsing large ones to a shape.
    pub expand: bool,
    /// Keep only symbols whose name matches this regex.
    pub match_pattern: Option<String>,
}

impl Default for MapOptions {
    fn default() -> Self {
        Self {
            depth: 2,
            kind: KindFilter::All,
            budget: DEFAULT_BUDGET,
            quiet: false,
            root: None,
            expand: false,
            match_pattern: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OutlineEntry {
    pub address: String,
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub start_line: usize,
    pub end_line: usize,
    pub lines: usize,
    pub depth: usize,
    pub hidden_children: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryPoint {
    pub address: String,
    pub reason: String,
}

/// Produce a structural map of a file or directory.
///
/// # Errors
///
/// Propagates [`Error::Io`], [`Error::NotUtf8`], and [`Error::Parse`] from the
/// files it visits.
pub fn run(path: &Path, options: &MapOptions) -> Result<Report> {
    if path.is_dir() {
        return map_directory(path, options);
    }
    if !path.exists() {
        return Err(Error::Io(
            path.to_path_buf(),
            std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
        ));
    }
    map_file(path, options)
}

pub fn file_outline(file: &SourceFile, options: &MapOptions) -> Vec<OutlineEntry> {
    let symbols = symbols::extract(file);
    let mut out = Vec::new();
    push_entries(file, &symbols, 1, options, &mut out);
    out
}

// A rooted outline shows `--depth` levels *below* the named symbol, so build
// deep enough to reach them before filtering. Building at the requested depth
// and then filtering would silently return nothing for any nested target.
fn rooted_outline(file: &SourceFile, root: &str, options: &MapOptions) -> Vec<OutlineEntry> {
    let deep = MapOptions {
        depth: root.split('.').count() + options.depth,
        ..options.clone()
    };
    let prefix = format!("{root}.");
    let mut base = None;
    let mut out = Vec::new();
    for entry in file_outline(file, &deep) {
        let dotted = entry.address.split_once('#').map_or("", |(_, rest)| rest);
        if dotted != root && !dotted.starts_with(&prefix) {
            continue;
        }
        let start = *base.get_or_insert(entry.depth);
        out.push(OutlineEntry {
            depth: entry.depth - start + 1,
            ..entry
        });
    }
    out
}

fn push_entries(
    file: &SourceFile,
    symbols: &[Symbol],
    depth: usize,
    options: &MapOptions,
    out: &mut Vec<OutlineEntry>,
) {
    for symbol in symbols {
        let keep = options.kind.allows(symbol.kind);
        let deeper = depth < options.depth;
        if keep {
            out.push(OutlineEntry {
                address: format!("{}#{}", slash_path(&file.path), symbol.dotted),
                name: symbol.name.clone(),
                kind: symbol.kind.as_str().to_string(),
                signature: signature_line(file, symbol, options.expand),
                start_line: symbol.start_line,
                end_line: symbol.end_line,
                lines: symbol.line_count(),
                depth,
                hidden_children: if deeper { 0 } else { symbol.descendants() },
            });
        }
        if deeper {
            push_entries(file, &symbol.children, depth + 1, options, out);
        }
    }
}

fn signature_line(file: &SourceFile, symbol: &Symbol, expand: bool) -> String {
    match symbol.kind {
        SymbolKind::Variable => value_line(file, symbol, expand),
        _ => symbols::signature(file, symbol, false),
    }
}

fn value_line(file: &SourceFile, symbol: &Symbol, expand: bool) -> String {
    let value = collapse_ws(file.slice(symbol.span_start, symbol.span_end));
    if expand || value.chars().count() <= MAX_VALUE_CHARS {
        return value;
    }
    if let Some(shape) = literal_shape(file, symbol) {
        return format!("{} = {shape}", symbol.name);
    }
    let head: String = value.chars().take(MAX_VALUE_CHARS).collect();
    format!("{head}...")
}

/// Render a container literal as its shape: `[... 187 items]`.
///
/// The count comes from the parse tree rather than from counting commas, so
/// nested literals and trailing commas cannot skew it.
fn literal_shape(file: &SourceFile, symbol: &Symbol) -> Option<String> {
    let mut node = file
        .root()
        .descendant_for_byte_range(symbol.span_start, symbol.span_end)?;
    while node.kind() != "assignment" {
        node = node.named_child(0)?;
    }
    let value = node.child_by_field_name("right")?;
    let (open, close, noun) = match value.kind() {
        "list" => ("[", "]", "item"),
        "dictionary" => ("{", "}", "key"),
        "set" => ("{", "}", "item"),
        "tuple" => ("(", ")", "item"),
        _ => return None,
    };
    let mut cursor = value.walk();
    let count = value
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .count();
    Some(format!(
        "{open}... {}{close}",
        plural(count, noun, &format!("{noun}s"))
    ))
}

fn map_file(path: &Path, options: &MapOptions) -> Result<Report> {
    let file = SourceFile::load(path)?;
    let all = match &options.root {
        Some(root) => rooted_outline(&file, root, options),
        None => file_outline(&file, options),
    };
    let total = all.len();
    let entries = retain_matching(all, options.match_pattern.as_deref())?;
    let display = match &options.root {
        Some(root) => format!("{}#{root}", slash_path(path)),
        None => slash_path(path),
    };
    let found = !entries.is_empty();

    let (text, detail, degraded) = fit(options.budget, |detail| {
        render_file(&display, &file, &entries, total, detail, options)
    });
    let json = json!({
        "command": "map",
        "target": display,
        "kind": "file",
        "found": found,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "language": file.lang.name(),
        "summary": {
            "lines": file.line_count(),
            "symbols": entries.len(),
            "symbols_total": total,
        },
        "symbols": entries,
    });
    Ok(Report::new(text, json, found))
}

/// Keep only the entries whose symbol name matches `pattern`.
///
/// # Errors
///
/// Returns [`Error::BadRegex`] when `pattern` does not compile.
fn retain_matching(entries: Vec<OutlineEntry>, pattern: Option<&str>) -> Result<Vec<OutlineEntry>> {
    let Some(pattern) = pattern else {
        return Ok(entries);
    };
    let matcher = Regex::new(pattern).map_err(|_| Error::BadRegex(pattern.to_string()))?;
    Ok(entries
        .into_iter()
        .filter(|entry| matcher.is_match(&entry.name))
        .collect())
}

fn render_file(
    display: &str,
    file: &SourceFile,
    entries: &[OutlineEntry],
    total: usize,
    detail: Detail,
    options: &MapOptions,
) -> String {
    let mut out = Lines::new();
    out.push(format!(
        "{display}  {}  {}",
        plural(file.line_count(), "line", "lines"),
        plural(entries.len(), "symbol", "symbols")
    ));

    if let Some(pattern) = &options.match_pattern {
        out.push(format!(
            "  matching `{pattern}`: {} of {}",
            entries.len(),
            plural(total, "symbol", "symbols")
        ));
    }

    if entries.is_empty() {
        out.push("  no definitions".to_string());
        return out.finish();
    }

    match detail {
        Detail::Counts => {
            let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
            for entry in entries {
                *by_kind.entry(entry.kind.as_str()).or_default() += 1;
            }
            for (kind, count) in by_kind {
                out.push(format!("  {}", plural(count, kind, &format!("{kind}s"))));
            }
            out.push("raise --budget or use --depth 1 for the outline".to_string());
        }
        Detail::Summary => {
            for entry in entries.iter().filter(|entry| entry.depth == 1) {
                out.push(outline_row(entry, 1));
            }
            out.push("budget reached: top-level symbols only, raise --budget for the rest");
        }
        Detail::Full => {
            let width = entries
                .iter()
                .map(|entry| entry.signature.chars().count() + entry.depth * 2)
                .max()
                .unwrap_or(0)
                .min(72);
            for entry in entries {
                out.push(outline_row(entry, width));
            }
        }
    }

    // Name a symbol that is actually in this outline. A placeholder like
    // `#<Symbol>` is both a shell redirection error and a guaranteed miss.
    if !options.quiet
        && let Some(first) = entries.first()
    {
        out.blank();
        let collapsed = entries
            .iter()
            .find(|entry| entry.signature.contains("... "));
        if collapsed.is_none_or(|entry| entry.address != first.address) {
            out.push(format!("slice {} for a body", first.address));
        }
        if let Some(entry) = collapsed {
            out.push(format!(
                "slice {} for the whole value, or --expand for every one",
                entry.address
            ));
        }
    }
    out.finish()
}

fn outline_row(entry: &OutlineEntry, width: usize) -> String {
    let head = format!("{}{}", indent(entry.depth), entry.signature);
    let span = line_span(entry.start_line, entry.end_line);
    let mut row = format!("{}  {span}", pad(&head, width + 2));
    if entry.hidden_children > 0 {
        let _ = write!(row, "  +{} nested", entry.hidden_children);
    }
    row.trim_end().to_string()
}

#[derive(Debug, Default)]
struct DirNode {
    files: usize,
    lines: usize,
    children: BTreeMap<String, DirNode>,
}

impl DirNode {
    fn insert(&mut self, parts: &[String], lines: usize) {
        self.files += 1;
        self.lines += lines;
        let Some((head, rest)) = parts.split_first() else {
            return;
        };
        self.children
            .entry(head.clone())
            .or_default()
            .insert(rest, lines);
    }

    fn render(&self, name: &str, depth: usize, max_depth: usize, out: &mut Lines) {
        if depth > 0 {
            out.push(format!(
                "{}{name}/  {}  {}",
                indent(depth),
                plural(self.files, "file", "files"),
                plural(self.lines, "line", "lines")
            ));
        }
        if depth >= max_depth {
            return;
        }
        for (child_name, child) in &self.children {
            if child.children.is_empty() && child.files == 0 {
                continue;
            }
            child.render(child_name, depth + 1, max_depth, out);
        }
    }
}

fn map_directory(root: &Path, options: &MapOptions) -> Result<Report> {
    let files = walk::source_files(root);
    let route = Regex::new(ROUTE_DECORATOR).map_err(|_| Error::BadRegex(ROUTE_DECORATOR.into()))?;

    let mut by_lang: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    let mut tree = DirNode::default();
    let mut entry_points: Vec<EntryPoint> = Vec::new();
    let mut total_lines = 0usize;

    for path in &files {
        let Ok(file) = SourceFile::load(path) else {
            continue;
        };
        let lang = file.lang.name();
        let lines = file.line_count();
        total_lines += lines;
        let slot = by_lang.entry(lang).or_default();
        slot.0 += 1;
        slot.1 += lines;

        let rel = walk::relative(root, path);
        let parts: Vec<String> = rel
            .parent()
            .map(|parent| {
                parent
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy().to_string())
                    .filter(|part| !part.is_empty() && part != ".")
                    .collect()
            })
            .unwrap_or_default();
        tree.insert(&parts, lines);

        collect_entry_points(&file, &slash_path(&rel), &route, &mut entry_points);
    }

    for manifest in walk::manifests(root) {
        entry_points.extend(console_scripts(root, &manifest));
    }
    entry_points.sort_by(|a, b| a.address.cmp(&b.address).then(a.reason.cmp(&b.reason)));
    entry_points.dedup_by(|a, b| a.address == b.address && a.reason == b.reason);

    let manifests: Vec<String> = walk::manifests(root)
        .iter()
        .map(|path| slash_path(&walk::relative(root, path)))
        .collect();

    let display = slash_path(root);
    let found = !files.is_empty();
    let example = files
        .first()
        .map(|path| slash_path(&walk::relative(root, path)));
    let languages: Vec<(&'static str, usize, usize)> = by_lang
        .iter()
        .map(|(name, (count, lines))| (*name, *count, *lines))
        .collect();

    let (text, detail, degraded) = fit(options.budget, |detail| {
        render_directory(
            &display,
            files.len(),
            total_lines,
            &languages,
            &tree,
            &entry_points,
            &manifests,
            example.as_deref(),
            detail,
            options,
        )
    });

    let json = json!({
        "command": "map",
        "target": display,
        "kind": "directory",
        "found": found,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "summary": {
            "files": files.len(),
            "lines": total_lines,
            "entry_points": entry_points.len(),
        },
        "languages": languages
            .iter()
            .map(|(name, count, lines)| json!({"language": name, "files": count, "lines": lines}))
            .collect::<Vec<_>>(),
        "entry_points": entry_points,
        "manifests": manifests,
    });
    Ok(Report::new(text, json, found))
}

#[allow(clippy::too_many_arguments)]
fn render_directory(
    display: &str,
    file_count: usize,
    total_lines: usize,
    languages: &[(&'static str, usize, usize)],
    tree: &DirNode,
    entry_points: &[EntryPoint],
    manifests: &[String],
    example: Option<&str>,
    detail: Detail,
    options: &MapOptions,
) -> String {
    let mut out = Lines::new();
    out.push(format!(
        "{display}  {}  {}",
        plural(file_count, "source file", "source files"),
        plural(total_lines, "line", "lines")
    ));

    if file_count == 0 {
        out.push("  no supported source files".to_string());
        return out.finish();
    }

    if detail == Detail::Counts {
        out.push(format!(
            "  {}  {}",
            plural(entry_points.len(), "entry point", "entry points"),
            plural(manifests.len(), "manifest", "manifests")
        ));
        out.push("raise --budget for the tree and entry points".to_string());
        return out.finish();
    }

    out.blank();
    out.push("languages".to_string());
    for (name, count, lines) in languages {
        out.push(format!(
            "  {}  {}  {}",
            pad(name, 8),
            plural(*count, "file", "files"),
            plural(*lines, "line", "lines")
        ));
    }

    if detail == Detail::Full {
        out.blank();
        out.push(format!("tree (depth {})", options.depth));
        let mut tree_lines = Lines::new();
        tree.render("", 0, options.depth, &mut tree_lines);
        let rendered = tree_lines.finish();
        if rendered.trim().is_empty() {
            out.push("  (flat)".to_string());
        } else {
            out.extend_block(rendered.trim_end());
        }
    }

    out.blank();
    out.push("entry points".to_string());
    if entry_points.is_empty() {
        out.push("  none detected".to_string());
    } else {
        let width = entry_points
            .iter()
            .map(|point| point.address.chars().count())
            .max()
            .unwrap_or(0)
            .min(64);
        for point in entry_points {
            out.push(format!(
                "  {}  {}",
                pad(&point.address, width),
                point.reason
            ));
        }
    }

    if detail == Detail::Full && !manifests.is_empty() {
        out.blank();
        out.push("manifests".to_string());
        for manifest in manifests {
            out.push(format!("  {manifest}"));
        }
    }

    if detail == Detail::Summary {
        out.blank();
        out.push("budget reached: tree and manifests omitted, raise --budget".to_string());
    }

    if !options.quiet
        && let Some(example) = example
    {
        out.blank();
        out.push(format!("map {example} for a file outline"));
    }
    out.finish()
}

fn collect_entry_points(file: &SourceFile, rel: &str, route: &Regex, out: &mut Vec<EntryPoint>) {
    if file.text.contains(MAIN_GUARD) {
        for line in 1..=file.line_count() {
            let text = file.line_text(line);
            if text.trim_start().starts_with("if __name__") {
                out.push(EntryPoint {
                    address: format!("{rel}#{}", line_span(line, line)),
                    reason: "__main__ guard".to_string(),
                });
                break;
            }
        }
    }
    let symbols = symbols::extract(file);
    for symbol in symbols::flatten(&symbols) {
        if symbol.kind == SymbolKind::Function && symbol.name == "main" {
            out.push(EntryPoint {
                address: format!("{rel}#{}", symbol.dotted),
                reason: "main function".to_string(),
            });
        }
        for decorator in &symbol.decorators {
            if route.is_match(decorator) {
                out.push(EntryPoint {
                    address: format!("{rel}#{}", symbol.dotted),
                    reason: decorator.clone(),
                });
            }
        }
    }
}

fn resolve_console_target(root: &Path, target: &str) -> Option<String> {
    let (module, function) = target.split_once(':')?;
    let relative = format!("{}.py", module.replace('.', "/"));
    let candidate = root.join(&relative);
    if candidate.is_file() {
        return Some(format!("{relative}#{function}"));
    }
    let package = root.join(module.replace('.', "/")).join("__main__.py");
    if package.is_file() {
        return Some(format!(
            "{}/__main__.py#{function}",
            module.replace('.', "/")
        ));
    }
    None
}

fn console_scripts(root: &Path, manifest: &Path) -> Vec<EntryPoint> {
    if manifest.file_name().and_then(|name| name.to_str()) != Some("pyproject.toml") {
        return Vec::new();
    }
    let Ok(text) = std::fs::read_to_string(manifest) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == "[project.scripts]" || trimmed == "[tool.poetry.scripts]";
            continue;
        }
        if !inside || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((name, target)) = trimmed.split_once('=') else {
            continue;
        };
        let target = target.trim().trim_matches('"').trim_matches('\'');
        let address = resolve_console_target(root, target)
            .unwrap_or_else(|| format!("{target} (unresolved)"));
        out.push(EntryPoint {
            address,
            reason: format!("console script `{}`", name.trim()),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("agentlens-map-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let mut body = String::from("LANGUAGES = [\n");
        for index in 0..200 {
            writeln!(body, "    (\"lang{index}\", \"Language {index}\"),").expect("write");
        }
        body.push_str("]\n\nSECURE_HSTS_SECONDS = 3600\nDEBUG = False\n");
        let path = dir.join("settings.py");
        std::fs::write(&path, body).expect("write");
        path
    }

    #[test]
    fn map_literal_budget() {
        let path = settings_file("budget");
        let report = run(&path, &MapOptions::default()).expect("maps");
        let tokens = report.json["tokens"].as_u64().expect("tokens");
        assert!(
            report.text.contains("LANGUAGES = [... 200 items]"),
            "literal was not collapsed to a shape:\n{}",
            report.text
        );
        assert!(
            report.text.contains("SECURE_HSTS_SECONDS = 3600"),
            "collapsing hid the settings the caller wanted:\n{}",
            report.text
        );
        assert!(tokens < 1200, "settings-style map cost {tokens} tokens");
    }

    #[test]
    fn expand_opts_back_into_the_whole_value() {
        let path = settings_file("expand");
        let options = MapOptions {
            expand: true,
            budget: 100_000,
            ..MapOptions::default()
        };
        let report = run(&path, &options).expect("maps");
        assert!(report.text.contains("lang199"), "--expand still collapsed");
    }

    #[test]
    fn match_filters_and_states_what_it_dropped() {
        let path = settings_file("match");
        let options = MapOptions {
            match_pattern: Some("^SECURE".to_string()),
            ..MapOptions::default()
        };
        let report = run(&path, &options).expect("maps");
        assert!(report.text.contains("1 of 3 symbols"), "{}", report.text);
        assert!(!report.text.contains("LANGUAGES"));
    }

    #[test]
    fn a_bad_match_pattern_is_a_regex_error() {
        let path = settings_file("badre");
        let options = MapOptions {
            match_pattern: Some("[unclosed".to_string()),
            ..MapOptions::default()
        };
        assert!(matches!(run(&path, &options), Err(Error::BadRegex(_))));
    }
}
