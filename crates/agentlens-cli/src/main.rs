use std::path::PathBuf;
use std::process::ExitCode;

mod help;

use agentlens_core::address::Coercion;
use agentlens_core::budget::DEFAULT_BUDGET;
use agentlens_core::error::Error;
use agentlens_core::ops::{
    callers, dead, envelope, error_envelope, find, literals, map, packet, slice, sym,
};
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

    #[arg(
        long,
        global = true,
        value_name = "n",
        default_value_t = DEFAULT_BUDGET,
        help = "token target, not a hard cap; output is estimated, so ~7% of calls run over"
    )]
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
        #[arg(long, help = "show whole values instead of collapsing large ones")]
        expand: bool,
        #[arg(long = "match", value_name = "pat")]
        match_pattern: Option<String>,
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
        #[arg(long, help = "list references and test hits instead of counting them")]
        expand: bool,
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
    #[command(about = "one symbol by name: address, kind and value")]
    Sym {
        #[arg(value_name = "name")]
        name: String,
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

const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Slice { .. } => "slice",
            Self::Map { .. } => "map",
            Self::Find { .. } => "find",
            Self::Literals { .. } => "literals",
            Self::Callers { .. } => "callers",
            Self::Packet { .. } => "packet",
            Self::Sym { .. } => "sym",
            Self::Dead => "dead",
            Self::Help { .. } => "help",
        }
    }
}

impl Cli {
    // The advertise-the-next-call footer is prose for a human reading a
    // terminal. Under --json the same addresses are already in the envelope,
    // so it is pure cost.
    fn quiet(&self) -> bool {
        self.quiet || self.json
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
            let exit = err.exit_code();
            let suggestion = err.is_address().then_some("agentlens help addresses");
            if cli.json {
                let body = error_envelope(
                    cli.command.name(),
                    err.kind(),
                    &err.to_string(),
                    suggestion,
                    exit,
                    TOOL_VERSION,
                );
                println!("{}", render(&body));
            } else {
                eprintln!("agentlens: {err}");
                if let Some(hint) = suggestion {
                    eprintln!("run `{hint}` for the grammar");
                }
            }
            ExitCode::from(exit)
        }
    }
}

enum Target {
    Address(Box<Address>, Option<Coercion>),
    Unresolved(Box<Report>),
}

// An address the caller could not spell may still be a symbol name the index
// knows. Try that before giving up, but only for a single bare argument:
// trailing positionals mean they were reaching for a line range, not a name.
fn address_of(parts: &[String], cli: &Cli) -> Result<Target, Error> {
    let Some((head, tail)) = parts.split_first() else {
        return Err(Error::AddressUnresolved(String::new()));
    };
    match Address::parse_lenient(head, tail) {
        Ok((address, coercion)) => Ok(Target::Address(Box::new(address), coercion)),
        // Only a missing `#` can plausibly be a bare symbol name. A malformed
        // line span is a malformed line span, and saying so beats searching
        // the index for a symbol called `f.py#L20-L10`.
        Err(Error::AddressMissingHash(name)) if tail.is_empty() => resolve_by_name(&name, cli),
        Err(err) => Err(err),
    }
}

fn resolve_by_name(name: &str, cli: &Cli) -> Result<Target, Error> {
    let options = sym::SymOptions {
        root: cli.root.clone(),
        cache: !cli.no_cache,
        budget: cli.budget,
        quiet: cli.quiet(),
    };
    match sym::lookup(name, &options)? {
        sym::Lookup::Unique(address) => {
            let coercion = Coercion {
                read_as: address.to_string(),
                raw: name.to_string(),
            };
            Ok(Target::Address(Box::new(address), Some(coercion)))
        }
        sym::Lookup::Report(report) => Ok(Target::Unresolved(report)),
    }
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
            let target = address_of(address, cli)?;
            let (parsed, coercion) = match target {
                Target::Address(parsed, coercion) => (parsed, coercion),
                Target::Unresolved(report) => return Ok((*report).into()),
            };
            let options = slice::SliceOptions {
                signature_only: *signature_only,
                no_decorators: *no_decorators,
                budget: cli.budget,
                quiet: cli.quiet(),
            };
            let report = slice::run(&parsed, &options)?;
            Ok(Outcome { report, coercion })
        }
        Command::Map {
            target,
            depth,
            kind,
            expand,
            match_pattern,
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
                quiet: cli.quiet(),
                root,
                expand: *expand,
                match_pattern: match_pattern.clone(),
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
            expand,
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
                quiet: cli.quiet(),
                expand: *expand,
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
                quiet: cli.quiet(),
            };
            Ok(literals::run(&targets, &options)?.into())
        }
        Command::Callers { address, no_tests } => {
            let target = address_of(address, cli)?;
            let (parsed, coercion) = match target {
                Target::Address(parsed, coercion) => (parsed, coercion),
                Target::Unresolved(report) => return Ok((*report).into()),
            };
            let options = callers::CallersOptions {
                root: cli.root.clone(),
                include_tests: !*no_tests,
                cache: !cli.no_cache,
                budget: cli.budget,
                quiet: cli.quiet(),
            };
            let report = callers::run(&parsed, &options)?;
            Ok(Outcome { report, coercion })
        }
        Command::Packet { address, no_tests } => {
            let target = address_of(address, cli)?;
            let (parsed, coercion) = match target {
                Target::Address(parsed, coercion) => (parsed, coercion),
                Target::Unresolved(report) => return Ok((*report).into()),
            };
            let options = packet::PacketOptions {
                root: cli.root.clone(),
                include_tests: !*no_tests,
                cache: !cli.no_cache,
                budget: cli.budget,
                quiet: cli.quiet(),
            };
            let report = packet::run(&parsed, &options)?;
            Ok(Outcome { report, coercion })
        }
        Command::Sym { name } => {
            let options = sym::SymOptions {
                root: cli.root.clone(),
                cache: !cli.no_cache,
                budget: cli.budget,
                quiet: cli.quiet(),
            };
            Ok(sym::run(name, &options)?.into())
        }
        Command::Dead => {
            let options = dead::DeadOptions {
                root: cli.root.clone(),
                cache: !cli.no_cache,
                budget: cli.budget,
                quiet: cli.quiet(),
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
        println!("{}", render(&envelope(report, TOOL_VERSION)));
    } else {
        print!("{}", report.text);
    }
}

fn render(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| "{\"error\":\"serialisation failed\"}".to_string())
}
