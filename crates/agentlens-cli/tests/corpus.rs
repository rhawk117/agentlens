#![allow(
    unreachable_pub,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::PathBuf;
use std::process::Command;

// The one corpus line P0 does not fix, and why. `--include-comments` is a
// `find` flag; reaching for it on `literals` is a wrong-verb mistake, not a
// wrong-argument-shape one, and clap rejects it before agentlens sees it.
// Coercing it would mean inventing a flag that does nothing.
const KNOWN_UNFIXED: &[&str] =
    &["literals\t--include-docstrings\t--include-comments\tprocess_exception"];

const VERBS: &[&str] = &[
    "slice", "map", "find", "literals", "callers", "packet", "dead", "help",
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn shapes() -> PathBuf {
    workspace_root().join("tests/fixtures/django-shapes")
}

struct Run {
    code: i32,
    output: String,
}

fn run(args: &[String]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_agentlens"))
        .current_dir(shapes())
        .args(args)
        .output()
        .expect("agentlens runs");
    Run {
        code: output.status.code().unwrap_or(-1),
        // Failing calls in the eval carried their message on stdout with an
        // empty stderr, so both streams have to be scanned for suggestions.
        output: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

// Two conventions carry a suggested next call: a backticked span beginning
// `agentlens `, and the advertise-the-next-call footer, whose first word is a
// subcommand. Both must name something that actually resolves.
fn suggestions(output: &str) -> Vec<Vec<String>> {
    let mut found = Vec::new();
    for span in output.split('`').skip(1).step_by(2) {
        if let Some(rest) = span.strip_prefix("agentlens ") {
            found.push(rest.split_whitespace().map(str::to_string).collect());
        }
    }
    for line in output.lines() {
        let mut words = line.split_whitespace();
        let (Some(verb), Some(target)) = (words.next(), words.next()) else {
            continue;
        };
        if VERBS.contains(&verb) && !target.starts_with('-') {
            found.push(vec![verb.to_string(), target.to_string()]);
        }
    }
    found
}

fn corpus() -> Vec<String> {
    let path = workspace_root().join("tests/fixtures/observed-invocations.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

#[test]
fn the_corpus_is_the_whole_observed_failure_set() {
    assert_eq!(
        corpus().len(),
        104,
        "the eval recorded 104 exit-2 calls; the fixture must carry all of them"
    );
}

#[test]
fn the_unfixed_allowlist_is_not_stale() {
    let corpus = corpus();
    for line in KNOWN_UNFIXED {
        assert!(
            corpus.contains(&(*line).to_string()),
            "allowlisted line is not in the corpus any more: {line}"
        );
    }
}

#[test]
fn no_observed_invocation_is_reported_as_a_tool_fault() {
    let mut faults = Vec::new();
    for line in corpus() {
        let args: Vec<String> = line.split('\t').map(str::to_string).collect();
        let result = run(&args);
        if result.code == 2 && !KNOWN_UNFIXED.contains(&line.as_str()) {
            faults.push(format!("exit 2: agentlens {}", args.join(" ")));
        }
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

#[test]
fn every_invocation_the_tool_suggests_also_works() {
    let mut faults = Vec::new();
    for line in corpus() {
        let args: Vec<String> = line.split('\t').map(str::to_string).collect();
        let first = run(&args);
        // Depth cap 2: a suggestion may be checked, but a suggestion's own
        // suggestion is not followed, so a cycle cannot hang the suite.
        for suggested in suggestions(&first.output) {
            let second = run(&suggested);
            if second.code == 2 {
                faults.push(format!(
                    "`agentlens {}` suggested `agentlens {}`, which exits 2",
                    args.join(" "),
                    suggested.join(" ")
                ));
            }
        }
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}
