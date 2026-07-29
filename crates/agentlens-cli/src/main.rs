use std::path::PathBuf;
use std::process::ExitCode;

use agentlens_core::budget::DEFAULT_BUDGET;
use agentlens_core::ops::{map, slice};
use agentlens_core::{Address, KindFilter, Report};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "agentlens",
    version,
    about = "symbol-addressed code intelligence for agents",
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
    #[command(about = "extract a definition: file.py#Class.method")]
    Slice {
        #[arg(value_name = "address")]
        address: String,
        #[arg(long, help = "signature lines only, no body")]
        signature_only: bool,
        #[arg(long, help = "exclude decorators from the span")]
        no_decorators: bool,
    },
    #[command(about = "outline a file, or orient in a directory")]
    Map {
        #[arg(value_name = "path", default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = 2, value_name = "n")]
        depth: usize,
        #[arg(long, default_value = "all", value_name = "k")]
        kind: String,
    },
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
        Err(message) => {
            eprintln!("agentlens: {message}");
            ExitCode::from(2)
        }
    }
}

fn dispatch(cli: &Cli) -> Result<Report, String> {
    match &cli.command {
        Command::Slice {
            address,
            signature_only,
            no_decorators,
        } => {
            let parsed = Address::parse(address).map_err(|err| err.to_string())?;
            let options = slice::SliceOptions {
                signature_only: *signature_only,
                no_decorators: *no_decorators,
                budget: cli.budget,
                quiet: cli.quiet,
            };
            slice::run(&parsed, &options).map_err(|err| err.to_string())
        }
        Command::Map { path, depth, kind } => {
            let kind = KindFilter::parse(kind)
                .ok_or_else(|| format!("unknown kind `{kind}`: use class, function or all"))?;
            let options = map::MapOptions {
                depth: (*depth).max(1),
                kind,
                budget: cli.budget,
                quiet: cli.quiet,
            };
            map::run(path, &options).map_err(|err| err.to_string())
        }
    }
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
