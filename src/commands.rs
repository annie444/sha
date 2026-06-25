//! Implementations of the `hash` and `verify` subcommands.

use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;
use tracing::{error, info, warn};

use crate::cli::{HashArgs, VerifyArgs};
use sha::checksum::{parse_line, ChecksumEntry};
use sha::hasher::{hash_file, DEFAULT_BUFFER_SIZE};

thread_local! {
    /// Per-thread scratch buffer, grown on demand and reused across files so a
    /// large run does not repeatedly allocate and free multi-megabyte buffers.
    static BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Run `f` with a thread-local buffer of at least `size` bytes.
fn with_buffer<T>(size: usize, f: impl FnOnce(&mut [u8]) -> T) -> T {
    BUFFER.with(|cell| {
        let mut buf = cell.borrow_mut();
        if buf.len() < size {
            buf.resize(size, 0);
        }
        f(&mut buf[..size])
    })
}

/// `sha hash`: compute and print digests.
pub fn run_hash(args: HashArgs) -> Result<i32> {
    let buf_size = args.perf.buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
    let algo = args.algorithm;

    // Hash every file in parallel. `par_iter().map().collect()` preserves input
    // order, so output is deterministic regardless of completion order.
    let results: Vec<(PathBuf, io::Result<String>)> = args
        .files
        .par_iter()
        .map(|path| {
            let res = with_buffer(buf_size, |buf| hash_file(path, algo, buf));
            (path.clone(), res)
        })
        .collect();

    let mut writer: Box<dyn Write> = match &args.output {
        Some(path) => Box::new(BufWriter::new(
            File::create(path).with_context(|| format!("creating {}", path.display()))?,
        )),
        None => Box::new(BufWriter::new(io::stdout().lock())),
    };

    let mut had_error = false;
    for (path, res) in results {
        match res {
            Ok(hex) => writeln!(writer, "{hex}  {}", path.display())?,
            Err(e) => {
                had_error = true;
                error!(path = path.display().to_string(), "{e}");
            }
        }
    }
    writer.flush()?;

    Ok(if had_error { 1 } else { 0 })
}

/// Outcome of verifying a single entry.
enum Outcome {
    Ok,
    Mismatch,
    Error(io::Error),
}

/// `sha verify`: check files against checksum files.
pub fn run_verify(args: VerifyArgs) -> Result<i32> {
    let buf_size = args.perf.buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);

    // Collect all entries from every checksum file first.
    let mut entries: Vec<ChecksumEntry> = Vec::new();
    let mut parse_errors = 0usize;
    for cf in &args.checksum_files {
        let content = read_checksum_source(cf)
            .with_context(|| format!("reading checksum file {}", cf.display()))?;
        for (lineno, line) in content.lines().enumerate() {
            match parse_line(line, args.algorithm) {
                None => {}
                Some(Ok(entry)) => entries.push(entry),
                Some(Err(msg)) => {
                    parse_errors += 1;
                    if !args.status {
                        error!("{}:{}: {msg}", cf.display(), lineno + 1);
                    }
                }
            }
        }
    }

    // Verify every entry in parallel, preserving order for stable output.
    let outcomes: Vec<(PathBuf, Outcome)> = entries
        .par_iter()
        .map(|entry| {
            let outcome = match with_buffer(buf_size, |buf| hash_file(&entry.path, entry.algo, buf))
            {
                Ok(actual) if actual.eq_ignore_ascii_case(&entry.expected) => Outcome::Ok,
                Ok(_) => Outcome::Mismatch,
                Err(e) => Outcome::Error(e),
            };
            (entry.path.clone(), outcome)
        })
        .collect();

    let mut failed = 0usize;
    let mut read_errors = 0usize;
    for (path, outcome) in outcomes {
        match outcome {
            Outcome::Ok => {
                if !args.status && !args.quiet {
                    info!(path = path.display().to_string(), "OK");
                }
            }
            Outcome::Mismatch => {
                failed += 1;
                if !args.status {
                    warn!(path = path.display().to_string(), "FAILED");
                }
            }
            Outcome::Error(e) => {
                read_errors += 1;
                if !args.status {
                    warn!(path = path.display().to_string(), "FAILED open or read");
                    error!(path = path.display().to_string(), "{e}");
                }
            }
        }
    }

    if !args.status {
        if failed > 0 {
            warn!(
                "{failed} computed checksum{} did NOT match",
                if failed == 1 { "" } else { "s" }
            );
        }
        if read_errors > 0 {
            warn!(
                "{read_errors} listed file{} could not be read",
                if read_errors == 1 { "" } else { "s" }
            );
        }
    }

    Ok(if failed + read_errors + parse_errors > 0 {
        1
    } else {
        0
    })
}

/// Read a checksum file, or stdin when the path is `-`.
fn read_checksum_source(path: &Path) -> io::Result<String> {
    if path.as_os_str() == "-" {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else {
        fs::read_to_string(path)
    }
}
