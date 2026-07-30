use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::address::Address;
use crate::budget::{DEFAULT_BUDGET, Detail, estimate_tokens, fit};
use crate::error::Result;
use crate::index::{Index, IndexSymbol};
use crate::ops::Report;
use crate::render::{Lines, collapse_ws, pad, plural};
use crate::source::SourceFile;
use crate::symbols::SymbolKind;

const MAX_VALUE_CHARS: usize = 120;

#[derive(Debug, Clone)]
pub struct SymOptions {
    pub root: PathBuf,
    pub cache: bool,
    pub budget: usize,
    pub quiet: bool,
}

impl Default for SymOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            cache: true,
            budget: DEFAULT_BUDGET,
            quiet: false,
        }
    }
}

/// What resolving a bare name against the index produced.
#[derive(Debug)]
pub enum Lookup {
    /// Exactly one definition; the caller can proceed as if fully addressed.
    Unique(Address),
    /// More than one, or none. The report lists what to do next and is not
    /// a found result, so the caller exits 1.
    Report(Box<Report>),
}

struct Hit {
    address: String,
    kind: &'static str,
    detail: String,
}

/// Look one symbol up by name and report its address, kind and value.
///
/// # Errors
///
/// Propagates [`crate::error::Error::Io`] and friends from building the index.
pub fn run(name: &str, options: &SymOptions) -> Result<Report> {
    let (index, stats) = Index::build(&options.root, options.cache)?;
    let hits = collect(&index, &options.root, name);
    let found = !hits.is_empty();
    let (text, detail, degraded) = fit(options.budget, |detail| {
        render(name, &hits, detail, options)
    });
    let json = json!({
        "command": "sym",
        "target": name,
        "found": found,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "matches": hits
            .iter()
            .map(|hit| json!({
                "address": hit.address,
                "kind": hit.kind,
                "detail": hit.detail,
            }))
            .collect::<Vec<_>>(),
    });
    Ok(Report::new(text, json, found).indexed(stats))
}

/// Resolve a bare symbol name to a single address, for the commands that take
/// an address but were handed a name.
///
/// # Errors
///
/// Propagates index-building errors.
pub fn lookup(name: &str, options: &SymOptions) -> Result<Lookup> {
    let (index, stats) = Index::build(&options.root, options.cache)?;
    // Ambiguity is about distinct addresses, not distinct definitions. Four
    // @overload variants of one function all resolve to one address, and that
    // address is not ambiguous.
    let mut candidates: Vec<String> = matching(&index, name)
        .iter()
        .map(|(path, symbol)| format!("{path}#{}", symbol.dotted))
        .collect();
    candidates.dedup();
    if let [only] = candidates.as_slice()
        && let Some((path, dotted)) = only.split_once('#')
    {
        return Ok(Lookup::Unique(Address::symbol(
            &options.root.join(path),
            dotted,
        )));
    }
    let found = false;
    let (text, detail, degraded) = fit(options.budget, |detail| {
        render_candidates(name, &candidates, detail)
    });
    let json = json!({
        "command": "resolve",
        "target": name,
        "found": found,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "candidates": candidates,
        "total": candidates.len(),
    });
    Ok(Lookup::Report(Box::new(
        Report::new(text, json, found).indexed(stats),
    )))
}

fn collect(index: &Index, root: &Path, name: &str) -> Vec<Hit> {
    let (key, wanted) = match name.split_once('#') {
        Some((path, dotted)) => (Some(path), dotted),
        None => (None, name),
    };
    let mut hits = Vec::new();
    for (path, symbol) in matching(index, wanted) {
        if key.is_some_and(|key| key != path) {
            continue;
        }
        hits.push(Hit {
            address: format!("{path}#{}", symbol.dotted),
            kind: symbol.kind.as_str(),
            detail: detail_of(root, path, symbol),
        });
    }
    dedupe(hits)
}

// A bare `create_user` should find `UserService.create_user`, so an undotted
// query matches on the leaf name and a dotted one on the full path.
fn matching<'a>(index: &'a Index, wanted: &str) -> Vec<(&'a str, &'a IndexSymbol)> {
    let Some((_, leaf)) = wanted.rsplit_once('.') else {
        return index.symbols_named(wanted);
    };
    index
        .symbols_named(leaf)
        .into_iter()
        .filter(|(_, symbol)| symbol.dotted == wanted)
        .collect()
}

// Overloads and conditional definitions share one address. Collapsing them
// keeps the result honest: the address is what the caller can act on, and
// slicing it returns every definition behind it.
fn dedupe(hits: Vec<Hit>) -> Vec<Hit> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for hit in &hits {
        *counts.entry(hit.address.clone()).or_default() += 1;
    }
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for hit in hits {
        if seen.contains(&hit.address) {
            continue;
        }
        seen.push(hit.address.clone());
        let count = counts.get(&hit.address).copied().unwrap_or(1);
        let detail = if count > 1 {
            format!("{}  ({count} definitions)", hit.detail)
        } else {
            hit.detail
        };
        out.push(Hit { detail, ..hit });
    }
    out
}

// A constant's value is the whole point of looking it up, but a huge literal
// would defeat the purpose of a cheap lookup, so long ones are clipped and
// say so.
fn detail_of(root: &Path, path: &str, symbol: &IndexSymbol) -> String {
    if symbol.kind != SymbolKind::Variable {
        return symbol.signature.clone();
    }
    let Ok(file) = SourceFile::load(&root.join(path)) else {
        return symbol.signature.clone();
    };
    let value = collapse_ws(file.slice(symbol.span_start, symbol.span_end));
    if value.chars().count() <= MAX_VALUE_CHARS {
        return value;
    }
    let head: String = value.chars().take(MAX_VALUE_CHARS).collect();
    format!(
        "{head}...  (slice {path}#{} for the whole value)",
        symbol.dotted
    )
}

fn render(name: &str, hits: &[Hit], detail: Detail, options: &SymOptions) -> String {
    let mut out = Lines::new();
    if hits.is_empty() {
        out.push(format!("no symbol named `{name}`"));
        if !options.quiet {
            out.push(format!("`agentlens find {name}` searches the text too"));
        }
        return out.finish();
    }
    match detail {
        Detail::Counts => {
            out.push(format!(
                "{} for `{name}`",
                plural(hits.len(), "definition", "definitions")
            ));
        }
        Detail::Summary => {
            for hit in hits {
                out.push(format!("{}  {}", hit.address, hit.kind));
            }
        }
        Detail::Full => {
            let width = hits
                .iter()
                .map(|hit| hit.address.chars().count())
                .max()
                .unwrap_or(0)
                .min(64);
            for hit in hits {
                out.push(format!("{}  {}", pad(&hit.address, width), hit.detail));
            }
        }
    }
    if !options.quiet && hits.len() > 1 {
        out.push(format!(
            "{} definitions; addresses above are exact",
            hits.len()
        ));
    }
    out.finish()
}

// Every rung states the true total, so a caller can always tell that it is
// looking at a subset rather than the whole answer.
fn render_candidates(name: &str, candidates: &[String], detail: Detail) -> String {
    let mut out = Lines::new();
    if candidates.is_empty() {
        out.push(format!("no file and no symbol named `{name}`"));
        out.push("an address is `file.py#Name`; run `agentlens help addresses`".to_string());
        return out.finish();
    }
    out.push(format!(
        "`{name}` is ambiguous: {}",
        plural(candidates.len(), "definition", "definitions")
    ));
    match detail {
        Detail::Full => {
            for candidate in candidates {
                out.push(format!("  {candidate}"));
            }
        }
        Detail::Summary => {
            let mut by_file: BTreeMap<&str, usize> = BTreeMap::new();
            for candidate in candidates {
                let file = candidate
                    .split_once('#')
                    .map_or(candidate.as_str(), |(f, _)| f);
                *by_file.entry(file).or_default() += 1;
            }
            for (file, count) in &by_file {
                if *count > 1 {
                    out.push(format!("  {file}  {count}"));
                } else {
                    out.push(format!("  {file}"));
                }
            }
            out.push(format!(
                "{} files, raise --budget for full addresses",
                by_file.len()
            ));
        }
        Detail::Counts => {
            let files = candidates
                .iter()
                .filter_map(|candidate| candidate.split_once('#'))
                .map(|(file, _)| file)
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            out.push(format!(
                "  {} definitions across {} files, raise --budget for the list",
                candidates.len(),
                files
            ));
        }
    }
    out.finish()
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn repeated(count: usize, line: impl Fn(usize) -> String) -> String {
        let mut out = String::new();
        for index in 0..count {
            write!(out, "{}", line(index)).expect("write to string");
        }
        out
    }

    struct Scratch {
        dir: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("agentlens-sym-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("mkdir");
            Self { dir }
        }

        fn write(&self, name: &str, body: &str) {
            std::fs::write(self.dir.join(name), body).expect("write");
        }

        fn options(&self) -> SymOptions {
            SymOptions {
                root: self.dir.clone(),
                cache: false,
                ..SymOptions::default()
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn sym_cost() {
        let scratch = Scratch::new("cost");
        std::fs::write(
            scratch.dir.join("settings.py"),
            format!(
                "LANGUAGES = [\n{}]\n\nSECURE_HSTS_SECONDS = 3600\n",
                repeated(200, |index| format!(
                    "    (\"lang{index}\", \"Language {index}\"),\n"
                ))
            ),
        )
        .expect("write");

        let report = run("SECURE_HSTS_SECONDS", &scratch.options()).expect("runs");
        let tokens = report.json["tokens"].as_u64().expect("tokens");
        assert!(report.found, "constant not found");
        assert!(tokens < 200, "one module constant cost {tokens} tokens");
    }

    #[test]
    fn a_huge_literal_is_clipped_rather_than_inlined() {
        let scratch = Scratch::new("huge");
        std::fs::write(
            scratch.dir.join("settings.py"),
            format!(
                "LANGUAGES = [\n{}]\n",
                repeated(200, |index| format!(
                    "    (\"lang{index}\", \"Language {index}\"),\n"
                ))
            ),
        )
        .expect("write");
        let report = run("LANGUAGES", &scratch.options()).expect("runs");
        let tokens = report.json["tokens"].as_u64().expect("tokens");
        assert!(report.text.contains("for the whole value"));
        assert!(tokens < 200, "a clipped literal still cost {tokens} tokens");
    }

    #[test]
    fn overloads_share_one_address_and_are_not_ambiguous() {
        let scratch = Scratch::new("overloads");
        scratch.write(
            "over.py",
            "from typing import overload\n\n\
             @overload\ndef norm(v: int) -> int: ...\n\n\
             @overload\ndef norm(v: str) -> str: ...\n\n\
             def norm(v):\n    return v\n",
        );
        match lookup("norm", &scratch.options()).expect("runs") {
            Lookup::Unique(address) => assert_eq!(address.dotted(), "norm"),
            Lookup::Report(report) => panic!("treated as ambiguous:\n{}", report.text),
        }
    }

    #[test]
    fn a_name_in_two_files_is_ambiguous_and_lists_both() {
        let scratch = Scratch::new("ambiguous");
        scratch.write("a.py", "class Widget:\n    def go(self):\n        pass\n");
        scratch.write("b.py", "class Gadget:\n    def go(self):\n        pass\n");
        match lookup("go", &scratch.options()).expect("runs") {
            Lookup::Unique(address) => panic!("should be ambiguous, got {address}"),
            Lookup::Report(report) => {
                assert!(!report.found, "ambiguity is not a found result");
                assert!(report.text.contains("a.py#Widget.go"));
                assert!(report.text.contains("b.py#Gadget.go"));
            }
        }
    }

    #[test]
    fn ambiguity_degrades_without_ever_dropping_the_total() {
        let scratch = Scratch::new("ladder");
        for file in ["a", "b", "c", "d"] {
            let body = repeated(3, |n| {
                format!("class W{file}{n}:\n    def go(self):\n        pass\n\n")
            });
            scratch.write(&format!("{file}.py"), &body);
        }
        let mut seen = Vec::new();
        for budget in [400, 60, 20] {
            let options = SymOptions {
                budget,
                ..scratch.options()
            };
            let Lookup::Report(report) = lookup("go", &options).expect("runs") else {
                panic!("four definitions are ambiguous");
            };
            assert!(
                report.text.contains("12 definitions"),
                "rung at budget {budget} lost the true total:\n{}",
                report.text
            );
            seen.push(estimate_tokens(&report.text));
        }
        assert!(
            seen[0] > seen[1] && seen[1] > seen[2],
            "each rung must be smaller than the last, got {seen:?}"
        );
    }

    #[test]
    fn a_dotted_query_matches_the_whole_path() {
        let scratch = Scratch::new("dotted");
        scratch.write("a.py", "class Widget:\n    def go(self):\n        pass\n");
        scratch.write("b.py", "class Gadget:\n    def go(self):\n        pass\n");
        let report = run("Widget.go", &scratch.options()).expect("runs");
        assert!(report.found);
        assert!(report.text.contains("a.py#Widget.go"));
        assert!(!report.text.contains("Gadget"));
    }
}
