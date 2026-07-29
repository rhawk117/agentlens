use serde::Serialize;
use serde_json::json;

use crate::address::{Address, Selector};
use crate::budget::{DEFAULT_BUDGET, Detail, estimate_tokens, fit};
use crate::error::Result;
use crate::ops::Report;
use crate::ops::map;
use crate::render::{Lines, collapse_ws, commas, line_span, plural, slash_path, truncation_note};
use crate::source::SourceFile;
use crate::symbols::{self, Symbol};

const MAX_LISTED_SYMBOLS: usize = 50;

#[derive(Debug, Clone)]
pub struct SliceOptions {
    pub signature_only: bool,
    pub no_decorators: bool,
    pub budget: usize,
    pub quiet: bool,
}

impl Default for SliceOptions {
    fn default() -> Self {
        Self {
            signature_only: false,
            no_decorators: false,
            budget: DEFAULT_BUDGET,
            quiet: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SliceMatch {
    pub address: String,
    pub path: String,
    pub kind: String,
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub lines: usize,
    pub signature: String,
    pub text: String,
}

/// Extract the source slice a resolved address points at.
///
/// # Errors
///
/// Propagates [`Error::Io`], [`Error::NotUtf8`], and [`Error::Parse`] from the
/// addressed file.
pub fn run(address: &Address, options: &SliceOptions) -> Result<Report> {
    if matches!(address.selector, Selector::Outline) {
        let map_options = map::MapOptions {
            budget: options.budget,
            quiet: options.quiet,
            ..map::MapOptions::default()
        };
        return map::run(&address.path, &map_options);
    }

    let file = SourceFile::load(&address.path)?;
    let path = slash_path(&address.path);

    let matches = match &address.selector {
        Selector::Lines(start, end) => vec![line_match(&file, &path, *start, *end)],
        Selector::Symbol(parts) => {
            let symbols = symbols::extract(&file);
            symbols::resolve(&symbols, parts)
                .into_iter()
                .map(|symbol| symbol_match(&file, &path, symbol, options))
                .collect()
        }
        Selector::Outline => Vec::new(),
    };

    if matches.is_empty() {
        return Ok(not_found(&file, address, &path));
    }

    let (text, detail, degraded) = fit(options.budget, |detail| {
        render(&matches, detail, options, address)
    });
    let total_lines: usize = matches.iter().map(|item| item.lines).sum();
    let json = json!({
        "command": "slice",
        "address": address.to_string(),
        "found": true,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "summary": {
            "matches": matches.len(),
            "lines": total_lines,
        },
        "matches": matches,
    });
    Ok(Report::new(text, json, true))
}

fn line_match(file: &SourceFile, path: &str, start: usize, end: usize) -> SliceMatch {
    let end = end.min(file.line_count().max(1));
    let start = start.min(end);
    let (start_byte, end_byte) = file.lines_span(start, end);
    let text = file.slice(start_byte, end_byte).trim_end().to_string();
    SliceMatch {
        address: format!("{path}#{}", line_span(start, end)),
        path: path.to_string(),
        kind: "lines".to_string(),
        name: line_span(start, end),
        start_line: start,
        end_line: end,
        lines: end - start + 1,
        signature: collapse_ws(file.line_text(start)),
        text,
    }
}

fn symbol_match(
    file: &SourceFile,
    path: &str,
    symbol: &Symbol,
    options: &SliceOptions,
) -> SliceMatch {
    let with_decorators = !options.no_decorators;
    let start_byte = if with_decorators {
        symbol.span_start
    } else {
        file.snap_to_line_start(symbol.def_start)
    };
    let end_byte = if options.signature_only {
        symbol.sig_end
    } else {
        symbol.span_end
    };
    let text = file.slice(start_byte, end_byte).trim_end().to_string();
    let start_line = file.line_of(start_byte);
    let end_line = file.line_of(end_byte.saturating_sub(1).max(start_byte));
    SliceMatch {
        address: format!("{path}#{}", symbol.dotted),
        path: path.to_string(),
        kind: symbol.kind.as_str().to_string(),
        name: symbol.dotted.clone(),
        start_line,
        end_line,
        lines: end_line.saturating_sub(start_line) + 1,
        signature: symbols::signature(file, symbol, with_decorators),
        text,
    }
}

fn render(
    matches: &[SliceMatch],
    detail: Detail,
    options: &SliceOptions,
    address: &Address,
) -> String {
    let mut out = Lines::new();
    match detail {
        Detail::Counts => {
            let total: usize = matches.iter().map(|item| item.lines).sum();
            out.push(format!(
                "{} matching {} at {}, {}",
                commas(matches.len()),
                if matches.len() == 1 {
                    "definition"
                } else {
                    "definitions"
                },
                address,
                plural(total, "line", "lines")
            ));
            for item in matches {
                out.push(format!(
                    "  {}  {}  {}",
                    item.address,
                    line_span(item.start_line, item.end_line),
                    plural(item.lines, "line", "lines")
                ));
            }
            out.push("raise --budget to see the bodies".to_string());
        }
        Detail::Summary | Detail::Full => {
            for (index, item) in matches.iter().enumerate() {
                out.blank();
                out.push(header(item, index, matches.len()));
                if options.signature_only || detail == Detail::Full {
                    out.extend_block(&item.text);
                } else {
                    out.push(item.signature.clone());
                }
            }
            if detail == Detail::Summary && !options.signature_only {
                out.blank();
                out.push("budget reached: signatures only, raise --budget for bodies".to_string());
            }
        }
    }
    if !options.quiet {
        out.blank();
        out.push(summary_line(matches));
    }
    out.finish()
}

fn header(item: &SliceMatch, index: usize, total: usize) -> String {
    let span = line_span(item.start_line, item.end_line);
    let base = format!(
        "{}  {span}  {}",
        item.address,
        plural(item.lines, "line", "lines")
    );
    if total == 1 {
        base
    } else {
        format!("{base}  ({} of {})", index + 1, total)
    }
}

fn summary_line(matches: &[SliceMatch]) -> String {
    let total: usize = matches.iter().map(|item| item.lines).sum();
    format!(
        "{}, {}",
        plural(matches.len(), "match", "matches"),
        plural(total, "line", "lines")
    )
}

fn not_found(file: &SourceFile, address: &Address, path: &str) -> Report {
    let symbols = symbols::extract(file);
    let flat = symbols::flatten(&symbols);
    let names: Vec<String> = flat.iter().map(|symbol| symbol.dotted.clone()).collect();
    let listed = dedupe_with_counts(&names);
    let mut out = Lines::new();
    out.push(format!("no symbol `{}` in {path}", address.dotted()));
    if listed.is_empty() {
        out.push("this file defines no symbols".to_string());
    } else {
        out.push("available:".to_string());
        for (name, count) in listed.iter().take(MAX_LISTED_SYMBOLS) {
            if *count == 1 {
                out.push(format!("  {path}#{name}"));
            } else {
                out.push(format!("  {path}#{name}  ({count} definitions)"));
            }
        }
        if let Some(note) = truncation_note(
            MAX_LISTED_SYMBOLS.min(listed.len()),
            listed.len(),
            "run `agentlens map` on this file for the whole outline",
        ) {
            out.push(note);
        }
    }
    let json = json!({
        "command": "slice",
        "address": address.to_string(),
        "found": false,
        "matches": [],
        "available": listed
            .iter()
            .map(|(name, count)| json!({"address": format!("{path}#{name}"), "definitions": count}))
            .collect::<Vec<_>>(),
    });
    Report::new(out.finish(), json, false)
}

fn dedupe_with_counts(names: &[String]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for name in names {
        match out.iter_mut().find(|(seen, _)| seen == name) {
            Some(entry) => entry.1 += 1,
            None => out.push((name.clone(), 1)),
        }
    }
    out
}
