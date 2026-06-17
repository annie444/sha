//! Implementations of the `hash` and `verify` subcommands.

use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;

use crate::algorithm::Algorithm;
use crate::cli::{HashArgs, VerifyArgs};
use crate::hasher::{hash_file, DEFAULT_BUFFER_SIZE};

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
                eprintln!("sha: {}: {e}", path.display());
            }
        }
    }
    writer.flush()?;

    Ok(if had_error { 1 } else { 0 })
}

/// One line parsed from a checksum file.
struct Entry {
    algo: Algorithm,
    expected: String,
    path: PathBuf,
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
    let mut entries: Vec<Entry> = Vec::new();
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
                        eprintln!("sha: {}:{}: {msg}", cf.display(), lineno + 1);
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
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for (path, outcome) in outcomes {
        match outcome {
            Outcome::Ok => {
                if !args.status && !args.quiet {
                    writeln!(out, "{}: OK", path.display())?;
                }
            }
            Outcome::Mismatch => {
                failed += 1;
                if !args.status {
                    writeln!(out, "{}: FAILED", path.display())?;
                }
            }
            Outcome::Error(e) => {
                read_errors += 1;
                if !args.status {
                    writeln!(out, "{}: FAILED open or read", path.display())?;
                    eprintln!("sha: {}: {e}", path.display());
                }
            }
        }
    }
    out.flush()?;

    if !args.status {
        if failed > 0 {
            eprintln!(
                "sha: WARNING: {failed} computed checksum{} did NOT match",
                if failed == 1 { "" } else { "s" }
            );
        }
        if read_errors > 0 {
            eprintln!(
                "sha: WARNING: {read_errors} listed file{} could not be read",
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

/// Parse one line of a checksum file.
///
/// Returns `None` for blank lines and comments, `Some(Err(..))` for malformed
/// lines, and `Some(Ok(..))` for valid `<digest>  <path>` entries. The two
/// coreutils separators are accepted: `"  "` (text) and `" *"` (binary).
fn parse_line(line: &str, forced: Option<Algorithm>) -> Option<Result<Entry, String>> {
    let line = line.trim_end_matches(['\n', '\r']);
    if line.trim().is_empty() || line.starts_with('#') {
        return None;
    }

    let (hex, rest) = line.split_once(' ')?;
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Some(Err("line is not in '<digest>  <file>' format".into()));
    }

    // `rest` begins with the mode flag (' ' for text, '*' for binary); drop it.
    let filename = rest.strip_prefix([' ', '*']).unwrap_or(rest);
    if filename.is_empty() {
        return Some(Err("missing filename".into()));
    }

    let algo = match forced.or_else(|| Algorithm::from_hex_len(hex.len())) {
        Some(a) => a,
        None => {
            return Some(Err(format!(
                "digest length {} matches no known algorithm",
                hex.len()
            )))
        }
    };

    Some(Ok(Entry {
        algo,
        expected: hex.to_ascii_lowercase(),
        path: PathBuf::from(filename),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hashes_known_answer() {
        // Known SHA-256 of "abc".
        let mut f = tempfile_with(b"abc");
        let path = f.1;
        f.0.flush().unwrap();
        let mut buf = vec![0u8; 4096];
        let hex = hash_file(&path, Algorithm::Sha256, &mut buf).unwrap();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn parses_text_and_binary_separators() {
        let text = parse_line(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  file.txt",
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(text.algo, Algorithm::Sha256);
        assert_eq!(text.path, PathBuf::from("file.txt"));

        let bin = parse_line(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad *file.bin",
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(bin.path, PathBuf::from("file.bin"));
    }

    #[test]
    fn skips_blank_and_comment_lines() {
        assert!(parse_line("", None).is_none());
        assert!(parse_line("   ", None).is_none());
        assert!(parse_line("# a comment", None).is_none());
    }

    #[test]
    fn rejects_non_hex_digest() {
        assert!(parse_line("nothex  file", None).unwrap().is_err());
    }

    #[test]
    fn forced_algorithm_overrides_length_inference() {
        // 64-char digest would infer sha256, but force sha512.
        let e = parse_line(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  f",
            Some(Algorithm::Sha512),
        )
        .unwrap()
        .unwrap();
        assert_eq!(e.algo, Algorithm::Sha512);
    }

    /// Create a temp file with the given contents, returning the open handle
    /// and its path.
    fn tempfile_with(data: &[u8]) -> (File, PathBuf) {
        let mut path = std::env::temp_dir();
        let unique = format!(
            "sha-test-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        path.push(unique);
        let mut f = File::create(&path).unwrap();
        f.write_all(data).unwrap();
        f.sync_all().unwrap();
        (f, path)
    }
}
