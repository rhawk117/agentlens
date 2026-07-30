use std::path::PathBuf;
use std::process::ExitCode;

use agentlens_core::Report;
use agentlens_core::budget::DEFAULT_BUDGET;
use agentlens_core::doc::DocAddress;
use agentlens_core::error::Error;
use agentlens_core::ops::{doc, envelope, error_envelope};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "doclens",
    version,
    about = "address-based slicing for json, yaml and markdown",
    disable_help_subcommand = true
)]
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
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "extract a value: config.yaml#services.web.ports[0]")]
    Slice {
        #[arg(value_name = "address")]
        address: String,
        #[arg(long, help = "include the key or heading line in the span")]
        with_key: bool,
    },
    #[command(about = "outline a document, or list documents in a directory")]
    Map {
        #[arg(value_name = "path", default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = 2, value_name = "n")]
        depth: usize,
    },
    #[command(about = "search keys and scalar values")]
    Find {
        #[arg(value_name = "pattern")]
        pattern: String,
        #[arg(value_name = "paths")]
        paths: Vec<PathBuf>,
        #[arg(long, help = "whole-value match rather than regex")]
        exact: bool,
        #[arg(long, help = "match keys only")]
        keys: bool,
        #[arg(long, conflicts_with = "keys", help = "match values only")]
        values: bool,
    },
}

const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Slice { .. } => "slice",
            Self::Map { .. } => "map",
            Self::Find { .. } => "find",
        }
    }
}

impl Cli {
    fn quiet(&self) -> bool {
        self.quiet || self.json
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let _ = cli.no_color;
    match dispatch(&cli) {
        Ok(report) => {
            emit(&report, cli.json);
            if report.found {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(err) => {
            let exit = err.exit_code();
            let suggestion = err.is_address().then_some("doclens map .");
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
                eprintln!("doclens: {err}");
                if let Some(hint) = suggestion {
                    eprintln!("run `{hint}` to see the documents here");
                }
            }
            ExitCode::from(exit)
        }
    }
}

fn dispatch(cli: &Cli) -> Result<Report, Error> {
    match &cli.command {
        Command::Slice { address, with_key } => {
            let parsed = DocAddress::parse(address)?;
            let options = doc::DocOptions {
                with_key: *with_key,
                budget: cli.budget,
                quiet: cli.quiet(),
                ..doc::DocOptions::default()
            };
            doc::slice(&parsed, &options)
        }
        Command::Map { path, depth } => {
            let options = doc::DocOptions {
                depth: (*depth).max(1),
                budget: cli.budget,
                quiet: cli.quiet(),
                ..doc::DocOptions::default()
            };
            doc::map(path, &options)
        }
        Command::Find {
            pattern,
            paths,
            exact,
            keys,
            values,
        } => {
            let options = doc::DocOptions {
                exact: *exact,
                keys_only: *keys,
                values_only: *values,
                budget: cli.budget,
                quiet: cli.quiet(),
                ..doc::DocOptions::default()
            };
            doc::find(pattern, paths, &options)
        }
    }
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
