//! `sha` — fast, parallel SHA-1/256/512 file hashing and verification.

mod cli;
mod commands;
mod logging;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};
use tracing::error;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (level, jobs) = match &cli.command {
        Command::Hash(a) => (a.verbosity, a.perf.jobs),
        Command::Verify(a) => (a.verbosity, a.perf.jobs),
    };

    let _guard = logging::init(level.into());

    // Size the rayon pool from the chosen job count (default: logical CPUs).
    if let Some(n) = jobs {
        if let Err(e) = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
        {
            error!("failed to configure thread pool: {e}");
            return ExitCode::FAILURE;
        }
    }

    match run(cli) {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            error!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<i32> {
    match cli.command {
        Command::Hash(args) => commands::run_hash(args),
        Command::Verify(args) => commands::run_verify(args),
    }
}
