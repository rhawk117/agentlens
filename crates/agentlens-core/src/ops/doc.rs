use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;
use walkdir::WalkDir;

use crate::budget::{DEFAULT_BUDGET, Detail, estimate_tokens, fit};
use crate::doc::{self, DocAddress, DocFormat, DocKind, DocNode};
use crate::error::Result;
use crate::matcher::Matcher;
use crate::ops::Report;
use crate::render::{Lines, indent, line_span, pad, plural, slash_path, truncation_note};
use crate::walk;

const MAX_LISTED: usize = 50;

// Each flag is an independent CLI switch on the doclens commands; grouping them
// into enums would not match how they are set or read.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct DocOptions {
    pub depth: usize,
    pub with_key: bool,
    pub exact: bool,
    pub keys_only: bool,
    pub values_only: bool,
    pub budget: usize,
    pub quiet: bool,
}

impl Default for DocOptions {
    fn default() -> Self {
        Self {
            depth: 2,
            with_key: false,
            exact: false,
            keys_only: false,
            values_only: false,
            budget: DEFAULT_BUDGET,
            quiet: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DocSlice {
    pub address: String,
    pub path: String,
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub lines: usize,
    pub summary: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutlineRow {
    pub address: String,
    pub key: String,
    pub kind: String,
    pub summary: String,
    pub start_line: usize,
    pub end_line: usize,
    pub depth: usize,
    pub hidden_children: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocHit {
    pub address: String,
    pub path: String,
    pub line: usize,
    pub where_: &'static str,
    pub text: String,
}

/// Render the document subtree a `path#selector` address points at.
///
/// # Errors
///
/// Returns any error from [`doc::parse_file`] — unsupported format, unreadable
/// file, non-UTF-8 contents, or a parse failure.
pub fn slice(address: &DocAddress, options: &DocOptions) -> Result<Report> {
    let (format, text, nodes) = doc::parse_file(&address.path)?;
    let display = slash_path(&address.path);
    if address.is_outline() {
        return Ok(render_outline(&display, format, &nodes, options));
    }

    let found = doc::resolve(&nodes, &address.steps);
    if found.is_empty() {
        return Ok(missing(&display, address, &nodes));
    }

    let starts = doc::line_starts(&text);
    let matches: Vec<DocSlice> = found
        .iter()
        .map(|node| {
            let raw_start = if options.with_key {
                node.entry_start
            } else {
                node.span_start
            };
            let start = snap_to_line_start(&text, &starts, raw_start);
            DocSlice {
                address: format!("{display}#{}", node.path),
                path: display.clone(),
                kind: node.kind.as_str().to_string(),
                start_line: doc::line_of(&starts, start),
                end_line: node.end_line,
                lines: node.end_line.saturating_sub(node.start_line) + 1,
                summary: node.summary.clone(),
                text: text[start..node.span_end].trim_end().to_string(),
            }
        })
        .collect();

    let (rendered, detail, degraded) = fit(options.budget, |detail| {
        render_slice(&matches, detail, options)
    });
    let json = json!({
        "command": "slice",
        "address": address.display(),
        "format": format.name(),
        "found": true,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&rendered),
        "summary": {"matches": matches.len()},
        "matches": matches,
    });
    Ok(Report::new(rendered, json, true))
}

fn snap_to_line_start(text: &str, starts: &[usize], byte: usize) -> usize {
    let line = doc::line_of(starts, byte);
    let start = starts.get(line.saturating_sub(1)).copied().unwrap_or(byte);
    if text[start..byte].trim().is_empty() {
        start
    } else {
        byte
    }
}

fn render_slice(matches: &[DocSlice], detail: Detail, options: &DocOptions) -> String {
    let mut out = Lines::new();
    match detail {
        Detail::Counts => {
            for item in matches {
                out.push(format!(
                    "{}  {}  {}  {}",
                    item.address,
                    line_span(item.start_line, item.end_line),
                    item.kind,
                    item.summary
                ));
            }
            out.push("raise --budget to see the value".to_string());
        }
        Detail::Summary | Detail::Full => {
            for (index, item) in matches.iter().enumerate() {
                out.blank();
                let mut header = format!(
                    "{}  {}  {}  {}",
                    item.address,
                    line_span(item.start_line, item.end_line),
                    plural(item.lines, "line", "lines"),
                    item.kind
                );
                if matches.len() > 1 {
                    let _ = write!(header, "  ({} of {})", index + 1, matches.len());
                }
                out.push(header);
                if detail == Detail::Full {
                    out.extend_block(&item.text);
                } else {
                    out.push(item.summary.clone());
                }
            }
            if detail == Detail::Summary {
                out.blank();
                out.push("budget reached: summaries only, raise --budget for values".to_string());
            }
        }
    }
    if !options.quiet {
        out.blank();
        out.push(plural(matches.len(), "match", "matches"));
    }
    out.finish()
}

/// Outline a document file, or every document under a directory.
///
/// # Errors
///
/// Returns any error from [`doc::parse_file`] when `path` is a single file.
/// Directory walks skip files that fail to parse and so do not error.
pub fn map(path: &Path, options: &DocOptions) -> Result<Report> {
    if path.is_dir() {
        return Ok(map_directory(path, options));
    }
    let (format, _, nodes) = doc::parse_file(path)?;
    Ok(render_outline(&slash_path(path), format, &nodes, options))
}

fn render_outline(
    display: &str,
    format: DocFormat,
    nodes: &[DocNode],
    options: &DocOptions,
) -> Report {
    let mut rows = Vec::new();
    push_rows(display, nodes, 1, options.depth, &mut rows);
    let found = !rows.is_empty();
    let (text, detail, degraded) = fit(options.budget, |detail| {
        render_rows(display, format, &rows, detail, options)
    });
    let json = json!({
        "command": "map",
        "target": display,
        "format": format.name(),
        "found": found,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "summary": {"entries": rows.len()},
        "entries": rows,
    });
    Report::new(text, json, found)
}

fn push_rows(
    display: &str,
    nodes: &[DocNode],
    depth: usize,
    max_depth: usize,
    out: &mut Vec<OutlineRow>,
) {
    for node in nodes {
        let deeper = depth < max_depth;
        out.push(OutlineRow {
            address: format!("{display}#{}", node.path),
            key: node.key.clone(),
            kind: node.kind.as_str().to_string(),
            summary: node.summary.clone(),
            start_line: node.start_line,
            end_line: node.end_line,
            depth,
            hidden_children: if deeper { 0 } else { node.descendants() },
        });
        if deeper {
            push_rows(display, &node.children, depth + 1, max_depth, out);
        }
    }
}

fn render_rows(
    display: &str,
    format: DocFormat,
    rows: &[OutlineRow],
    detail: Detail,
    options: &DocOptions,
) -> String {
    let mut out = Lines::new();
    out.push(format!(
        "{display}  {}  {}",
        format.name(),
        plural(rows.len(), "entry", "entries")
    ));
    if rows.is_empty() {
        out.push("  empty document".to_string());
        return out.finish();
    }

    match detail {
        Detail::Counts => {
            out.push(format!(
                "  {} top-level {}",
                rows.iter().filter(|row| row.depth == 1).count(),
                if format == DocFormat::Markdown {
                    "sections"
                } else {
                    "keys"
                }
            ));
            out.push("raise --budget for the outline".to_string());
        }
        Detail::Summary | Detail::Full => {
            let shown: Vec<&OutlineRow> = if detail == Detail::Full {
                rows.iter().collect()
            } else {
                rows.iter().filter(|row| row.depth == 1).collect()
            };
            let width = shown
                .iter()
                .map(|row| row.key.chars().count() + row.depth * 2)
                .max()
                .unwrap_or(0)
                .min(48);
            for row in &shown {
                let head = format!("{}{}", indent(row.depth), row.key);
                let mut line = format!(
                    "{}  {}  {}",
                    pad(&head, width + 2),
                    pad(&line_span(row.start_line, row.end_line), 12),
                    row.summary
                );
                if row.hidden_children > 0 {
                    let _ = write!(line, "  +{} nested", row.hidden_children);
                }
                out.push(line.trim_end().to_string());
            }
            if detail == Detail::Summary {
                out.push("budget reached: top level only, raise --budget".to_string());
            }
        }
    }

    if !options.quiet {
        out.blank();
        let example = rows
            .first()
            .map_or_else(String::new, |row| row.address.clone());
        out.push(format!("slice {example} for the value"));
    }
    out.finish()
}

fn map_directory(root: &Path, options: &DocOptions) -> Report {
    let files = doc_files(root);
    let mut rows: Vec<(String, &'static str, usize, usize)> = Vec::new();
    for path in &files {
        let Ok((format, text, nodes)) = doc::parse_file(path) else {
            continue;
        };
        rows.push((
            slash_path(&walk::relative(root, path)),
            format.name(),
            doc::flatten(&nodes).len(),
            doc::line_starts(&text).len(),
        ));
    }
    rows.sort();
    let found = !rows.is_empty();
    let display = slash_path(root);
    let (text, detail, degraded) = fit(options.budget, |detail| {
        let mut out = Lines::new();
        out.push(format!(
            "{display}  {}",
            plural(rows.len(), "document", "documents")
        ));
        if rows.is_empty() {
            out.push("  no json, yaml or markdown files".to_string());
            return out.finish();
        }
        if detail == Detail::Counts {
            out.push("raise --budget for the file list".to_string());
            return out.finish();
        }
        let width = rows
            .iter()
            .map(|(path, _, _, _)| path.chars().count())
            .max()
            .unwrap_or(0)
            .min(56);
        let limit = if detail == Detail::Summary {
            20
        } else {
            rows.len()
        };
        for (path, format, entries, lines) in rows.iter().take(limit) {
            out.push(format!(
                "  {}  {}  {}  {}",
                pad(path, width),
                pad(format, 9),
                pad(&plural(*entries, "entry", "entries"), 12),
                plural(*lines, "line", "lines")
            ));
        }
        if let Some(note) = truncation_note(limit.min(rows.len()), rows.len(), "raise --budget") {
            out.push(note);
        }
        if !options.quiet {
            out.blank();
            out.push(format!("map {display}/<file> for a document outline"));
        }
        out.finish()
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
        "documents": rows
            .iter()
            .map(|(path, format, entries, lines)| json!({
                "path": path,
                "format": format,
                "entries": entries,
                "lines": lines,
            }))
            .collect::<Vec<_>>(),
    });
    Report::new(text, json, found)
}

/// Search keys and values across documents for `pattern`.
///
/// # Errors
///
/// Returns [`crate::error::Error::BadRegex`] if `pattern` is not a valid
/// regular expression.
/// Files that fail to parse are skipped rather than erroring.
pub fn find(pattern: &str, paths: &[PathBuf], options: &DocOptions) -> Result<Report> {
    let matcher = Matcher::new(pattern, options.exact)?;
    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };
    let mut targets: Vec<PathBuf> = Vec::new();
    for root in &roots {
        targets.extend(doc_files(root));
    }
    targets.sort();
    targets.dedup();

    let mut hits: Vec<DocHit> = Vec::new();
    for path in targets {
        let Ok((_, text, nodes)) = doc::parse_file(&path) else {
            continue;
        };
        let display = slash_path(&path);
        for node in doc::flatten(&nodes) {
            let key_hit = !options.values_only && matcher.matches(&node.key);
            let body = &text[node.span_start..node.span_end];
            let value_hit =
                !options.keys_only && node.kind == DocKind::Scalar && matcher.matches(body.trim());
            if !key_hit && !value_hit {
                continue;
            }
            hits.push(DocHit {
                address: format!("{display}#{}", node.path),
                path: display.clone(),
                line: node.start_line,
                where_: if key_hit { "key" } else { "value" },
                text: doc::preview(if key_hit { &node.key } else { body }, 48),
            });
        }
    }
    hits.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));

    let found = !hits.is_empty();
    let (text, detail, degraded) = fit(options.budget, |detail| {
        render_hits(pattern, &hits, detail, options)
    });
    let json = json!({
        "command": "find",
        "pattern": pattern,
        "found": found,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "summary": {"matches": hits.len()},
        "matches": hits,
    });
    Ok(Report::new(text, json, found))
}

fn render_hits(pattern: &str, hits: &[DocHit], detail: Detail, options: &DocOptions) -> String {
    let mut out = Lines::new();
    if hits.is_empty() {
        out.push(format!("no match for `{pattern}`"));
        return out.finish();
    }
    if detail == Detail::Counts {
        out.push(format!(
            "`{pattern}`: {}",
            plural(hits.len(), "match", "matches")
        ));
        out.push("raise --budget for the addresses".to_string());
        return out.finish();
    }
    let width = hits
        .iter()
        .map(|hit| hit.address.chars().count())
        .max()
        .unwrap_or(0)
        .min(56);
    let mut current = String::new();
    for hit in hits {
        if hit.path != current {
            out.blank();
            out.push(hit.path.clone());
            current.clone_from(&hit.path);
        }
        if detail == Detail::Summary {
            out.push(format!("  {}", hit.address));
        } else {
            out.push(format!(
                "  {}  {}  {}  {}",
                pad(&hit.address, width),
                pad(&format!("L{}", hit.line), 6),
                pad(hit.where_, 5),
                hit.text
            ));
        }
    }
    if !options.quiet {
        out.blank();
        out.push(plural(hits.len(), "match", "matches"));
    }
    out.finish()
}

fn doc_files(root: &Path) -> Vec<PathBuf> {
    if root.is_file() {
        return if DocFormat::from_path(root).is_some() {
            vec![root.to_path_buf()]
        } else {
            Vec::new()
        };
    }
    let mut out: Vec<PathBuf> = WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 || !entry.file_type().is_dir() {
                return true;
            }
            !walk::is_skipped_dir(&entry.file_name().to_string_lossy())
        })
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| DocFormat::from_path(path).is_some())
        .collect();
    out.sort();
    out
}

fn missing(display: &str, address: &DocAddress, nodes: &[DocNode]) -> Report {
    let flat = doc::flatten(nodes);
    let paths: Vec<&str> = flat.iter().map(|node| node.path.as_str()).collect();
    let mut out = Lines::new();
    out.push(format!("no entry `{}` in {display}", address.raw_selector));
    if paths.is_empty() {
        out.push("this document is empty".to_string());
    } else {
        out.push("available:".to_string());
        for path in paths.iter().take(MAX_LISTED) {
            out.push(format!("  {display}#{path}"));
        }
        if let Some(note) = truncation_note(
            MAX_LISTED.min(paths.len()),
            paths.len(),
            "run `doclens map` on this file for the whole outline",
        ) {
            out.push(note);
        }
    }
    let json = json!({
        "command": "slice",
        "address": address.display(),
        "found": false,
        "matches": [],
        "available": paths,
    });
    Report::new(out.finish(), json, false)
}
