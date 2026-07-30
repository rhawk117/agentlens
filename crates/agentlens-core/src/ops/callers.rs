use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;

use crate::address::{Address, Selector};
use crate::budget::{DEFAULT_BUDGET, Detail, estimate_tokens, fit};
use crate::calls::CallSite;
use crate::error::Result;
use crate::index::{FileEntry, Index, IndexSymbol};
use crate::ops::Report;
use crate::render::{Lines, pad, plural, slash_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Certain,
    Likely,
    Possible,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Certain => "certain",
            Self::Likely => "likely",
            Self::Possible => "possible",
        }
    }

    pub fn tiers() -> [Self; 3] {
        [Self::Certain, Self::Likely, Self::Possible]
    }
}

#[derive(Debug, Clone)]
pub struct CallersOptions {
    pub root: PathBuf,
    pub include_tests: bool,
    pub cache: bool,
    pub budget: usize,
    pub quiet: bool,
}

impl Default for CallersOptions {
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
pub struct Caller {
    pub address: String,
    pub path: String,
    pub line: usize,
    pub call: String,
    pub arity: usize,
    pub confidence: Confidence,
    pub is_test: bool,
    pub reason: String,
}

/// Find the call sites that reach the symbol at `address`.
///
/// # Errors
///
/// Propagates index construction failures, which in turn propagate
/// [`Error::Io`], [`Error::NotUtf8`], and [`Error::Parse`].
pub fn run(address: &Address, options: &CallersOptions) -> Result<Report> {
    let (index, stats) = Index::build(&options.root, options.cache)?;
    let key = slash_path(&address.path);
    let Selector::Symbol(parts) = &address.selector else {
        return Ok(bad_target(
            address,
            "callers needs a symbol address, not an outline or a line span",
        ));
    };
    let dotted = parts.join(".");
    let targets = index.lookup(&key, &dotted);
    if targets.is_empty() {
        return Ok(missing_target(&index, address, &key, &dotted).indexed(stats));
    }

    let name = parts.last().cloned().unwrap_or_default();
    let homonyms = index.symbols_named(&name).len();
    let callers = collect(&index, &key, &dotted, options.include_tests);

    let found = !callers.is_empty();
    let signature = targets
        .first()
        .map_or_else(String::new, |symbol| symbol.signature.clone());
    let (text, detail, degraded) = fit(options.budget, |detail| {
        render(address, &signature, &callers, detail, options)
    });
    let json = json!({
        "command": "callers",
        "address": address.to_string(),
        "found": found,
        "detail": detail.as_str(),
        "degraded": degraded,
        "budget": options.budget,
        "tokens": estimate_tokens(&text),
        "target": {
            "signature": signature,
            "definitions": targets.len(),
            "homonyms": homonyms,
        },
        "summary": {
            "callers": callers.len(),
            "certain": count_tier(&callers, Confidence::Certain),
            "likely": count_tier(&callers, Confidence::Likely),
            "possible": count_tier(&callers, Confidence::Possible),
            "tests": callers.iter().filter(|caller| caller.is_test).count(),
        },
        "callers": callers,
    });
    Ok(Report::new(text, json, found).indexed(stats))
}

pub fn collect(index: &Index, key: &str, dotted: &str, include_tests: bool) -> Vec<Caller> {
    let targets = index.lookup(key, dotted);
    if targets.is_empty() {
        return Vec::new();
    }
    let name = dotted.rsplit('.').next().unwrap_or(dotted).to_string();
    let homonyms = index.symbols_named(&name).len();
    let mut callers: Vec<Caller> = Vec::new();
    for (path, call) in index.calls_to(&name) {
        let Some(entry) = index.entry(path) else {
            continue;
        };
        if !include_tests && entry.is_test {
            continue;
        }
        if is_own_definition(path, key, call, targets.as_slice()) {
            continue;
        }
        let (confidence, reason) = grade(key, path, entry, call, targets.as_slice(), homonyms);
        callers.push(Caller {
            address: call.enclosing.as_ref().map_or_else(
                || format!("{path}#L{}", call.line),
                |enclosing| format!("{path}#{enclosing}"),
            ),
            path: path.to_string(),
            line: call.line,
            call: call.full.clone(),
            arity: call.arity,
            confidence,
            is_test: entry.is_test,
            reason,
        });
    }
    callers.sort_by(|a, b| {
        a.confidence
            .cmp(&b.confidence)
            .then(a.path.cmp(&b.path))
            .then(a.line.cmp(&b.line))
    });
    callers
}

pub fn count_tier(callers: &[Caller], tier: Confidence) -> usize {
    callers
        .iter()
        .filter(|caller| caller.confidence == tier)
        .count()
}

fn is_own_definition(path: &str, key: &str, call: &CallSite, targets: &[&IndexSymbol]) -> bool {
    path == key
        && targets
            .iter()
            .any(|target| call.line >= target.start_line && call.line <= target.end_line)
        && call.enclosing.as_deref() == targets.first().map(|target| target.dotted.as_str())
        && call.receiver.is_none()
}

fn grade(
    target_path: &str,
    caller_path: &str,
    entry: &FileEntry,
    call: &CallSite,
    targets: &[&IndexSymbol],
    homonyms: usize,
) -> (Confidence, String) {
    let arity_fits = targets.iter().any(|target| target.accepts(call.arity));
    if !arity_fits {
        return (
            Confidence::Possible,
            format!("arity {} does not fit the signature", call.arity),
        );
    }
    let same_file = caller_path == target_path;
    let unique = homonyms == 1;

    if let Some(receiver) = &call.receiver {
        if receiver == "self" || receiver == "cls" {
            if same_file && shares_class(call, targets) {
                return (
                    Confidence::Certain,
                    "self call inside the class".to_string(),
                );
            }
            return (
                Confidence::Possible,
                "self call in another class".to_string(),
            );
        }
        if unique {
            return (
                Confidence::Likely,
                format!("attribute call on `{receiver}`, name unique in repo"),
            );
        }
        return (
            Confidence::Possible,
            format!("attribute call on `{receiver}`, {homonyms} definitions share the name"),
        );
    }

    if same_file {
        if unique {
            return (
                Confidence::Certain,
                "bare call in the same file".to_string(),
            );
        }
        return (
            Confidence::Likely,
            format!("bare call in the same file, {homonyms} definitions share the name"),
        );
    }

    let imported = entry
        .imports
        .iter()
        .any(|item| item.alias.as_deref() == Some(&call.name) || item.name == call.name);
    if imported && unique {
        return (Confidence::Certain, "imported by name".to_string());
    }
    if imported {
        return (
            Confidence::Likely,
            format!("imported by name, {homonyms} definitions share it"),
        );
    }
    if unique {
        return (
            Confidence::Likely,
            "bare call, name unique in repo".to_string(),
        );
    }
    (
        Confidence::Possible,
        format!("bare call, {homonyms} definitions share the name"),
    )
}

fn shares_class(call: &CallSite, targets: &[&IndexSymbol]) -> bool {
    let Some(enclosing) = &call.enclosing else {
        return false;
    };
    targets.iter().any(|target| {
        let Some((class_path, _)) = target.dotted.rsplit_once('.') else {
            return false;
        };
        enclosing == &class_path.to_string() || enclosing.starts_with(&format!("{class_path}."))
    })
}

fn render(
    address: &Address,
    signature: &str,
    callers: &[Caller],
    detail: Detail,
    options: &CallersOptions,
) -> String {
    let mut out = Lines::new();
    out.push(format!("{address}  {signature}"));

    if callers.is_empty() {
        out.push("  no call sites found".to_string());
        out.push("  reflection, DI and dynamic dispatch are invisible here".to_string());
        return out.finish();
    }

    match detail {
        Detail::Counts => {
            out.push(format!(
                "  {}: {} certain, {} likely, {} possible",
                plural(callers.len(), "caller", "callers"),
                count_tier(callers, Confidence::Certain),
                count_tier(callers, Confidence::Likely),
                count_tier(callers, Confidence::Possible)
            ));
            out.push("raise --budget for the call sites".to_string());
        }
        Detail::Summary => {
            for tier in Confidence::tiers() {
                let group: Vec<&Caller> = callers
                    .iter()
                    .filter(|caller| caller.confidence == tier)
                    .collect();
                if group.is_empty() {
                    continue;
                }
                out.blank();
                out.push(format!("{}  {}", tier.as_str(), group.len()));
                for caller in group {
                    out.push(format!("  {}{}", caller.address, test_tag(caller)));
                }
            }
            out.push("budget reached: addresses only, raise --budget for reasons".to_string());
        }
        Detail::Full => {
            let width = callers
                .iter()
                .map(|caller| caller.address.chars().count())
                .max()
                .unwrap_or(0)
                .min(56);
            for tier in Confidence::tiers() {
                let group: Vec<&Caller> = callers
                    .iter()
                    .filter(|caller| caller.confidence == tier)
                    .collect();
                if group.is_empty() {
                    continue;
                }
                out.blank();
                out.push(tier.as_str().to_string());
                for caller in group {
                    out.push(format!(
                        "  {}  {}  {}  {}",
                        pad(&caller.address, width),
                        pad(&format!("L{}", caller.line), 6),
                        pad(&format!("{} args{}", caller.arity, test_tag(caller)), 14),
                        caller.reason
                    ));
                }
            }
        }
    }

    if !options.quiet {
        out.blank();
        let tests = callers.iter().filter(|caller| caller.is_test).count();
        out.push(format!(
            "{} ({} non-test, {} test)",
            plural(callers.len(), "caller", "callers"),
            callers.len() - tests,
            tests
        ));
    }
    out.finish()
}

fn test_tag(caller: &Caller) -> &'static str {
    if caller.is_test { " [test]" } else { "" }
}

fn missing_target(index: &Index, address: &Address, key: &str, dotted: &str) -> Report {
    let mut out = Lines::new();
    out.push(format!("no symbol `{dotted}` in {key}"));
    let mut names: Vec<&str> = index
        .entry(key)
        .map(|entry| {
            entry
                .symbols
                .iter()
                .map(|symbol| symbol.dotted.as_str())
                .collect()
        })
        .unwrap_or_default();
    names.dedup();
    if names.is_empty() {
        out.push("this file is not indexed: is the path right?".to_string());
    } else {
        out.push("available:".to_string());
        for name in names.iter().take(50) {
            out.push(format!("  {key}#{name}"));
        }
    }
    let json = json!({
        "command": "callers",
        "address": address.to_string(),
        "found": false,
        "callers": [],
        "available": names,
    });
    Report::new(out.finish(), json, false)
}

fn bad_target(address: &Address, message: &str) -> Report {
    let mut out = Lines::new();
    out.push(message.to_string());
    let json = json!({
        "command": "callers",
        "address": address.to_string(),
        "found": false,
        "error": message,
    });
    Report::new(out.finish(), json, false)
}

pub fn index_root(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    loop {
        if current.join("pyproject.toml").is_file() || current.join(".git").exists() {
            return current;
        }
        if !current.pop() {
            return PathBuf::from(".");
        }
    }
}
