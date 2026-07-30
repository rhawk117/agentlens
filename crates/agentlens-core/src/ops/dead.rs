use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::json;

use crate::budget::{DEFAULT_BUDGET, Detail, estimate_tokens, fit};
use crate::error::Result;
use crate::index::Index;
use crate::ops::Report;
use crate::render::{Lines, pad, plural};
use crate::symbols::SymbolKind;

const REGISTRATION_HINTS: &[&str] = &[
    "@abstractmethod",
    "@app.",
    "@bp.",
    "@celery",
    "@click",
    "@fixture",
    "@hookimpl",
    "@overload",
    "@property",
    "@pytest",
    "@register",
    "@router.",
    "@setter",
    "@task",
    "@typing.overload",
];

const ALWAYS_LIVE: &[&str] = &["main", "setup", "teardown"];

#[derive(Debug, Clone)]
pub struct DeadOptions {
    pub root: PathBuf,
    pub cache: bool,
    pub budget: usize,
    pub quiet: bool,
}

impl Default for DeadOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            cache: true,
            budget: DEFAULT_BUDGET,
            quiet: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub address: String,
    pub path: String,
    pub kind: String,
    pub signature: String,
    pub start_line: usize,
    pub end_line: usize,
    pub lines: usize,
}

/// Report symbols with no reachable call site.
///
/// # Errors
///
/// Propagates index construction failures, which in turn propagate
/// [`Error::Io`], [`Error::NotUtf8`], and [`Error::Parse`].
pub fn run(options: &DeadOptions) -> Result<Report> {
    let (index, stats) = Index::build(&options.root, options.cache)?;

    let mut called: BTreeSet<&str> = BTreeSet::new();
    let mut referenced: BTreeSet<&str> = BTreeSet::new();
    for entry in index.files.values() {
        for call in &entry.calls {
            called.insert(call.name.as_str());
        }
        for import in &entry.imports {
            referenced.insert(import.name.as_str());
            if let Some(alias) = &import.alias {
                referenced.insert(alias.as_str());
            }
        }
        for export in &entry.exports {
            referenced.insert(export.as_str());
        }
        for symbol in &entry.symbols {
            for type_name in &symbol.type_names {
                referenced.insert(type_name.as_str());
            }
            for decorator in &symbol.decorators {
                referenced.insert(decorator.trim_start_matches('@'));
            }
        }
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    for (path, entry) in &index.files {
        if entry.is_test || entry.is_package_init {
            continue;
        }
        for symbol in &entry.symbols {
            if !matches!(symbol.kind, SymbolKind::Function | SymbolKind::Class) {
                continue;
            }
            if symbol.name.starts_with("__") && symbol.name.ends_with("__") {
                continue;
            }
            if ALWAYS_LIVE.contains(&symbol.name.as_str()) {
                continue;
            }
            if called.contains(symbol.name.as_str()) || referenced.contains(symbol.name.as_str()) {
                continue;
            }
            if symbol.decorators.iter().any(|decorator| {
                REGISTRATION_HINTS
                    .iter()
                    .any(|hint| decorator.starts_with(hint))
            }) {
                continue;
            }
            candidates.push(Candidate {
                address: format!("{path}#{}", symbol.dotted),
                path: path.clone(),
                kind: symbol.kind.as_str().to_string(),
                signature: symbol.signature.clone(),
                start_line: symbol.start_line,
                end_line: symbol.end_line,
                lines: symbol.end_line.saturating_sub(symbol.start_line) + 1,
            });
        }
    }
    candidates.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.start_line.cmp(&b.start_line))
            .then(a.address.cmp(&b.address))
    });
    candidates.dedup_by(|a, b| a.address == b.address);

    let found = !candidates.is_empty();
    let total_lines: usize = candidates.iter().map(|candidate| candidate.lines).sum();
    let (text, detail, degraded) = fit(options.budget, |detail| {
        render(&candidates, detail, options)
    });
    let json = json!({
        "command": "dead",
        "found": found,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "summary": {
            "candidates": candidates.len(),
            "lines": total_lines,
            "files_scanned": index.files.len(),
        },
        "candidates": candidates,
    });
    Ok(Report::new(text, json, found).indexed(stats))
}

fn render(candidates: &[Candidate], detail: Detail, options: &DeadOptions) -> String {
    let mut out = Lines::new();
    out.push("dead-code candidates: zero call sites, not exported, not entry points, not tests");

    if candidates.is_empty() {
        out.push("  none found".to_string());
        return out.finish();
    }

    match detail {
        Detail::Counts => {
            let total: usize = candidates.iter().map(|candidate| candidate.lines).sum();
            out.push(format!(
                "  {}, {}",
                plural(candidates.len(), "candidate", "candidates"),
                plural(total, "line", "lines")
            ));
            out.push("raise --budget for the addresses".to_string());
        }
        Detail::Summary => {
            for candidate in candidates {
                out.push(format!("  {}", candidate.address));
            }
            out.push("budget reached: addresses only, raise --budget for signatures".to_string());
        }
        Detail::Full => {
            let width = candidates
                .iter()
                .map(|candidate| candidate.address.chars().count())
                .max()
                .unwrap_or(0)
                .min(56);
            let mut current = String::new();
            for candidate in candidates {
                if candidate.path != current {
                    out.blank();
                    out.push(candidate.path.clone());
                    current.clone_from(&candidate.path);
                }
                out.push(format!(
                    "  {}  {}  {}",
                    pad(&candidate.address, width),
                    pad(
                        &format!("L{}-L{}", candidate.start_line, candidate.end_line),
                        12
                    ),
                    candidate.signature
                ));
            }
        }
    }

    if !options.quiet {
        out.blank();
        out.push(format!(
            "{} — reflection and DI defeat this, verify before deleting",
            plural(candidates.len(), "candidate", "candidates")
        ));
    }
    out.finish()
}
