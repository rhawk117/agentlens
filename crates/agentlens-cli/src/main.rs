use std::path::PathBuf;
use std::process::ExitCode;

mod help;

use agentlens_core::address::Coercion;
use agentlens_core::budget::DEFAULT_BUDGET;
use agentlens_core::error::Error;
use agentlens_core::ops::{callers, dead, find, literals, map, packet, slice};
use agentlens_core::{Address, KindFilter, Report};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "agentlens",
    version,
    about = "symbol-addressed code intelligence for agents",
    long_about = None,
    after_help = help::GRAMMAR,
    disable_help_subcommand = true
)]
// Global CLI switches. They are independent flags, not a state machine.
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, global = true, value_name = "n", default_value_t = DEFAULT_BUDGET)]
    budget: usize,

    #[arg(long, global = true, help = "machine-readable output")]
    json: bool,

    #[arg(long, global = true, help = "never emit ANSI (off by default)")]
    no_color: bool,

    #[arg(long, global = true, help = "suppress the summary line")]
    quiet: bool,

    #[arg(long, global = true, help = "ignore and do not write .agentlens-cache")]
    no_cache: bool,

    #[arg(
        long,
        global = true,
        value_name = "path",
        default_value = ".",
        help = "repo root for index-backed commands"
    )]
    root: PathBuf,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "extract a definition: file.py#Class.method")]
    Slice {
        #[arg(value_name = "address", num_args = 1.., required = true)]
        address: Vec<String>,
        #[arg(long, help = "signature lines only, no body")]
        signature_only: bool,
        #[arg(long, help = "exclude decorators from the span")]
        no_decorators: bool,
    },
    #[command(about = "outline a file, or orient in a directory")]
    Map {
        #[arg(value_name = "target", default_value = ".")]
        target: String,
        #[arg(long, default_value_t = 2, value_name = "n")]
        depth: usize,
        #[arg(long, default_value = "all", value_name = "k")]
        kind: String,
    },
    #[command(about = "kind-aware search: definition, call, reference")]
    Find {
        #[arg(value_name = "pattern")]
        pattern: String,
        #[arg(value_name = "paths")]
        paths: Vec<PathBuf>,
        #[arg(long, default_value = "any", value_name = "k")]
        kind: String,
        #[arg(long, help = "whole-symbol match rather than regex")]
        exact: bool,
        #[arg(long, help = "search comment bodies too")]
        include_comments: bool,
        #[arg(long, help = "search string bodies too")]
        include_strings: bool,
        #[arg(long, default_value = "symbol", value_name = "c")]
        context: String,
    },
    #[command(about = "extract string, number and regex literals")]
    Literals {
        #[arg(value_name = "paths")]
        paths: Vec<PathBuf>,
        #[arg(
            long = "path",
            value_name = "p",
            help = "extra path to scan; repeatable"
        )]
        extra_paths: Vec<PathBuf>,
        #[arg(long, default_value = "all", value_name = "k")]
        kind: String,
        #[arg(long = "match", value_name = "pat")]
        match_pattern: Option<String>,
        #[arg(long, default_value_t = 2, value_name = "n")]
        min_len: usize,
        #[arg(long = "in", value_name = "address")]
        scope: Option<String>,
        #[arg(long, help = "list every occurrence instead of grouping by value")]
        no_group: bool,
        #[arg(
            long,
            help = "keep interpolation verbatim instead of normalising to <>"
        )]
        no_skeleton: bool,
        #[arg(long, help = "include docstrings")]
        include_docstrings: bool,
    },
    #[command(about = "direct callers of a symbol, with confidence tiers")]
    Callers {
        #[arg(value_name = "address", num_args = 1.., required = true)]
        address: Vec<String>,
        #[arg(long, help = "skip call sites in test files")]
        no_tests: bool,
    },
    #[command(about = "context packet: body, caller and callee signatures, types")]
    Packet {
        #[arg(value_name = "address", num_args = 1.., required = true)]
        address: Vec<String>,
        #[arg(long, help = "skip call sites in test files")]
        no_tests: bool,
    },
    #[command(about = "dead-code candidates")]
    Dead,
    #[command(about = "address grammar and other topics")]
    Help {
        #[arg(value_name = "topic", default_value = "addresses")]
        topic: String,
    },
}

struct Outcome {
    report: Report,
    coercion: Option<Coercion>,
}

impl From<Report> for Outcome {
    fn from(report: Report) -> Self {
        Self {
            report,
            coercion: None,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let _ = cli.no_color;
    match dispatch(&cli) {
        Ok(outcome) => {
            emit(&outcome.report, cli.json);
            // The note goes to stderr so that stdout, and the --json envelope
            // in particular, stays byte-identical whether or not we rewrote
            // the caller's arguments.
            if let Some(coercion) = &outcome.coercion
                && !cli.quiet
            {
                eprintln!("note: read `{}` as `{}`", coercion.raw, coercion.read_as);
            }
            if outcome.report.found {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(err) => {
            eprintln!("agentlens: {err}");
            if err.is_address() {
                eprintln!("run `agentlens help addresses` for the grammar");
            }
            ExitCode::from(err.exit_code())
        }
    }
}

fn address_of(parts: &[String]) -> Result<(Address, Option<Coercion>), Error> {
    let Some((head, tail)) = parts.split_first() else {
        return Err(Error::AddressUnresolved(String::new()));
    };
    Address::parse_lenient(head, tail)
}

// One match arm per command, each building that command's options. Splitting
// it into per-command helpers would scatter the option construction and add a
// hop for the reader without reducing any single arm's complexity.
#[allow(clippy::too_many_lines)]
fn dispatch(cli: &Cli) -> Result<Outcome, Error> {
    match &cli.command {
        Command::Slice {
            address,
            signature_only,
            no_decorators,
        } => {
            let (parsed, coercion) = address_of(address)?;
            let options = slice::SliceOptions {
                signature_only: *signature_only,
                no_decorators: *no_decorators,
                budget: cli.budget,
                quiet: cli.quiet,
            };
            let report = slice::run(&parsed, &options)?;
            Ok(Outcome { report, coercion })
        }
        Command::Map {
            target,
            depth,
            kind,
        } => {
            let kind = KindFilter::parse(kind).ok_or_else(|| Error::BadKind {
                value: kind.clone(),
                allowed: "class, function or all",
            })?;
            let (path, root, coercion) = map_target(target)?;
            let options = map::MapOptions {
                depth: (*depth).max(1),
                kind,
                budget: cli.budget,
                quiet: cli.quiet,
                root,
            };
            let report = map::run(&path, &options)?;
            Ok(Outcome { report, coercion })
        }
        Command::Find {
            pattern,
            paths,
            kind,
            exact,
            include_comments,
            include_strings,
            context,
        } => {
            let kind = find::OccurrenceFilter::parse(kind).ok_or_else(|| Error::BadKind {
                value: kind.clone(),
                allowed: "definition, call, reference or any",
            })?;
            let context = find::Context::parse(context).ok_or_else(|| Error::BadKind {
                value: context.clone(),
                allowed: "symbol or none",
            })?;
            let options = find::FindOptions {
                kind,
                exact: *exact,
                include_comments: *include_comments,
                include_strings: *include_strings,
                context,
                budget: cli.budget,
                quiet: cli.quiet,
            };
            Ok(find::run(pattern, paths, &options)?.into())
        }
        Command::Literals {
            paths,
            extra_paths,
            kind,
            match_pattern,
            min_len,
            scope,
            no_group,
            no_skeleton,
            include_docstrings,
        } => {
            let kind = literals::LiteralFilter::parse(kind).ok_or_else(|| Error::BadKind {
                value: kind.clone(),
                allowed: "string, number, regex or all",
            })?;
            let scope = match scope {
                Some(raw) => Some(Address::parse(raw)?),
                None => None,
            };
            let mut targets = paths.clone();
            targets.extend(extra_paths.iter().cloned());
            let options = literals::LiteralsOptions {
                kind,
                match_pattern: match_pattern.clone(),
                min_len: *min_len,
                scope,
                group: !*no_group,
                skeleton: !*no_skeleton,
                include_docstrings: *include_docstrings,
                budget: cli.budget,
                quiet: cli.quiet,
            };
            Ok(literals::run(&targets, &options)?.into())
        }
        Command::Callers { address, no_tests } => {
            let (parsed, coercion) = address_of(address)?;
            let options = callers::CallersOptions {
                root: cli.root.clone(),
                include_tests: !*no_tests,
                cache: !cli.no_cache,
                budget: cli.budget,
                quiet: cli.quiet,
            };
            let report = callers::run(&parsed, &options)?;
            Ok(Outcome { report, coercion })
        }
        Command::Packet { address, no_tests } => {
            let (parsed, coercion) = address_of(address)?;
            let options = packet::PacketOptions {
                root: cli.root.clone(),
                include_tests: !*no_tests,
                cache: !cli.no_cache,
                budget: cli.budget,
                quiet: cli.quiet,
            };
            let report = packet::run(&parsed, &options)?;
            Ok(Outcome { report, coercion })
        }
        Command::Dead => {
            let options = dead::DeadOptions {
                root: cli.root.clone(),
                cache: !cli.no_cache,
                budget: cli.budget,
                quiet: cli.quiet,
            };
            Ok(dead::run(&options)?.into())
        }
        Command::Help { topic } => Ok(help_report(topic)?.into()),
    }
}

// `map file.py#Widget` is an outline rooted at Widget. Anything else is a path.
fn map_target(target: &str) -> Result<(PathBuf, Option<String>, Option<Coercion>), Error> {
    let path = PathBuf::from(target);
    if path.exists() {
        return Ok((path, None, None));
    }
    if !target.contains('#') {
        return Ok((path, None, None));
    }
    let (address, coercion) = Address::parse_lenient(target, &[])?;
    let root = match &address.selector {
        agentlens_core::Selector::Symbol(parts) => Some(parts.join(".")),
        _ => None,
    };
    Ok((address.path, root, coercion))
}

fn help_report(topic: &str) -> Result<Report, Error> {
    let body = help::topic(topic).ok_or_else(|| Error::BadKind {
        value: topic.to_string(),
        allowed: help::TOPICS,
    })?;
    let json = serde_json::json!({
        "command": "help",
        "topic": topic,
        "found": true,
        "text": body,
    });
    Ok(Report::new(body.to_string(), json, true))
}

fn emit(report: &Report, json: bool) {
    if json {
        let rendered = serde_json::to_string_pretty(&report.json)
            .unwrap_or_else(|_| "{\"error\":\"serialisation failed\"}".to_string());
        println!("{rendered}");
    } else {
        print!("{}", report.text);
    }
}
