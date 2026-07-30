use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::json;

use crate::address::{Address, Selector};
use crate::budget::{DEFAULT_BUDGET, Detail, estimate_tokens, fit};
use crate::error::Result;
use crate::index::Index;
use crate::ops::Report;
use crate::ops::callers::{self, Caller, Confidence};
use crate::render::{Lines, pad, plural, slash_path};
use crate::source::SourceFile;
use crate::symbols::SymbolKind;

#[derive(Debug, Clone)]
pub struct PacketOptions {
    pub root: PathBuf,
    pub include_tests: bool,
    pub cache: bool,
    pub budget: usize,
    pub quiet: bool,
}

impl Default for PacketOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            include_tests: true,
            cache: true,
            budget: DEFAULT_BUDGET,
            quiet: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Related {
    pub address: String,
    pub signature: String,
    pub note: String,
}

/// Assemble a review packet for `address`: its body plus callers, callees,
/// and the types it references.
///
/// # Errors
///
/// Propagates index construction failures, which in turn propagate
/// [`Error::Io`], [`Error::NotUtf8`], and [`Error::Parse`].
// `callers` and `callees` are the domain terms this command prints. Renaming
// the bindings to satisfy similar_names would make the code diverge from its
// own output vocabulary.
#[allow(clippy::similar_names)]
pub fn run(address: &Address, options: &PacketOptions) -> Result<Report> {
    let (index, stats) = Index::build(&options.root, options.cache)?;
    let key = slash_path(&address.path);
    let Selector::Symbol(parts) = &address.selector else {
        return Ok(not_a_symbol(address));
    };
    let dotted = parts.join(".");
    let targets = index.lookup(&key, &dotted);
    let Some(target) = targets.first().copied() else {
        return Ok(missing(&index, address, &key, &dotted).indexed(stats));
    };

    let file = SourceFile::load(&address.path)?;
    let body = file
        .slice(target.span_start, target.span_end)
        .trim_end()
        .to_string();

    let callers = callers::collect(&index, &key, &dotted, options.include_tests);
    let caller_lines: Vec<Related> = callers
        .iter()
        .filter(|caller| caller.confidence != Confidence::Possible)
        .filter_map(|caller| caller_signature(&index, caller))
        .collect();

    let callee_lines = callees(&index, &key, target.start_line, target.end_line);
    let type_lines = types(&index, &target.type_names);

    let (text, detail, degraded) = fit(options.budget, |detail| {
        render(
            address,
            target.start_line,
            target.end_line,
            &body,
            &target.signature,
            &caller_lines,
            &callee_lines,
            &type_lines,
            detail,
            options,
        )
    });
    let json = json!({
        "command": "packet",
        "address": address.to_string(),
        "found": true,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "target": {
            "address": format!("{key}#{dotted}"),
            "signature": target.signature,
            "start_line": target.start_line,
            "end_line": target.end_line,
            "body": body,
        },
        "callers": caller_lines,
        "callees": callee_lines,
        "types": type_lines,
    });
    Ok(Report::new(text, json, true).indexed(stats))
}

fn caller_signature(index: &Index, caller: &Caller) -> Option<Related> {
    let (path, dotted) = caller.address.split_once('#')?;
    let symbol = index.lookup(path, dotted).first().copied()?;
    Some(Related {
        address: caller.address.clone(),
        signature: symbol.signature.clone(),
        note: format!("{} L{}", caller.confidence.as_str(), caller.line),
    })
}

fn callees(index: &Index, key: &str, start_line: usize, end_line: usize) -> Vec<Related> {
    let Some(entry) = index.entry(key) else {
        return Vec::new();
    };
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    for call in &entry.calls {
        if call.line < start_line || call.line > end_line {
            continue;
        }
        let matches = index.symbols_named(&call.name);
        if matches.is_empty() {
            continue;
        }
        let note = if matches.len() == 1 {
            "certain".to_string()
        } else {
            format!("{} definitions share this name", matches.len())
        };
        for (path, symbol) in matches.iter().take(3) {
            let address = format!("{path}#{}", symbol.dotted);
            if !seen.insert(address.clone()) {
                continue;
            }
            out.push(Related {
                address,
                signature: symbol.signature.clone(),
                note: note.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.address.cmp(&b.address));
    out
}

fn types(index: &Index, type_names: &[String]) -> Vec<Related> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    for name in type_names {
        for (path, symbol) in index.symbols_named(name) {
            if symbol.kind != SymbolKind::Class {
                continue;
            }
            let address = format!("{path}#{}", symbol.dotted);
            if !seen.insert(address.clone()) {
                continue;
            }
            out.push(Related {
                address,
                signature: symbol.signature.clone(),
                note: format!("named in the signature as `{name}`"),
            });
        }
    }
    out.sort_by(|a, b| a.address.cmp(&b.address));
    out
}

#[allow(clippy::too_many_arguments)]
// See the note on `run`: callers/callees are domain terms, not accidental
// near-duplicates.
#[allow(clippy::similar_names)]
fn render(
    address: &Address,
    start_line: usize,
    end_line: usize,
    body: &str,
    signature: &str,
    callers: &[Related],
    callees: &[Related],
    types: &[Related],
    detail: Detail,
    options: &PacketOptions,
) -> String {
    let mut out = Lines::new();
    out.push(format!("packet {address}"));

    if detail == Detail::Counts {
        out.push(format!("  {signature}"));
        out.push(format!(
            "  {}, {}, {}",
            plural(callers.len(), "caller", "callers"),
            plural(callees.len(), "callee", "callees"),
            plural(types.len(), "type", "types")
        ));
        out.push("raise --budget for the body".to_string());
        return out.finish();
    }

    out.blank();
    out.push(format!("target  L{start_line}-L{end_line}"));
    out.extend_block(body);

    if detail == Detail::Summary {
        out.blank();
        out.push(format!(
            "{}, {}, {} — raise --budget for their signatures",
            plural(callers.len(), "caller", "callers"),
            plural(callees.len(), "callee", "callees"),
            plural(types.len(), "type", "types")
        ));
        return out.finish();
    }

    section(&mut out, "callers", callers);
    section(&mut out, "callees", callees);
    section(&mut out, "types in the signature", types);

    if !options.quiet {
        out.blank();
        out.push(format!(
            "{}, {}, {}",
            plural(callers.len(), "caller", "callers"),
            plural(callees.len(), "callee", "callees"),
            plural(types.len(), "type", "types")
        ));
    }
    out.finish()
}

fn section(out: &mut Lines, title: &str, items: &[Related]) {
    out.blank();
    if items.is_empty() {
        out.push(format!("{title}  none"));
        return;
    }
    out.push(format!("{title}  {}", items.len()));
    let width = items
        .iter()
        .map(|item| item.address.chars().count())
        .max()
        .unwrap_or(0)
        .min(56);
    for item in items {
        out.push(format!(
            "  {}  {}  ({})",
            pad(&item.address, width),
            item.signature,
            item.note
        ));
    }
}

fn missing(index: &Index, address: &Address, key: &str, dotted: &str) -> Report {
    let mut out = Lines::new();
    out.push(format!("no symbol `{dotted}` in {key}"));
    let names: Vec<&str> = index
        .entry(key)
        .map(|entry| {
            entry
                .symbols
                .iter()
                .map(|symbol| symbol.dotted.as_str())
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() {
        out.push("this file is not indexed: is the path right?".to_string());
    } else {
        out.push("available:".to_string());
        for name in names.iter().take(50) {
            out.push(format!("  {key}#{name}"));
        }
    }
    let json = json!({
        "command": "packet",
        "address": address.to_string(),
        "found": false,
        "available": names,
    });
    Report::new(out.finish(), json, false)
}

fn not_a_symbol(address: &Address) -> Report {
    let message = "packet needs a symbol address, not an outline or a line span";
    let json = json!({
        "command": "packet",
        "address": address.to_string(),
        "found": false,
        "error": message,
    });
    Report::new(format!("{message}\n"), json, false)
}
