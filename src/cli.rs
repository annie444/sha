//! Command-line interface definition.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use sha::algorithm::Algorithm;

#[derive(Parser, Debug)]
#[command(
    name = "sha",
    version,
    about = "Fast, parallel SHA-1/256/512 file hashing and verification",
    long_about = "Compute and verify SHA-1, SHA-256, and SHA-512 hashes of files.\n\
                  Files are hashed concurrently across CPU cores; on x86_64 the \
                  SHA hardware instructions are used automatically when present."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Compute hashes of files.
    Hash(HashArgs),
    /// Verify files against one or more checksum files.
    Verify(VerifyArgs),
}

/// Options shared by both subcommands that affect throughput.
#[derive(Args, Debug, Clone)]
pub struct PerfArgs {
    /// Number of files to hash in parallel (default: number of logical CPUs).
    #[arg(short = 'j', long, global = true, value_name = "N")]
    pub jobs: Option<usize>,

    /// Per-file read buffer size, e.g. 8M, 16MiB, 1048576 (default: 8MiB).
    #[arg(short = 'b', long, global = true, value_name = "SIZE", value_parser = parse_size)]
    pub buffer_size: Option<usize>,
}

/// Help text listing every accepted algorithm, shown on both subcommands.
const ALGO_HELP: &str = "Hash algorithm. One of: md5, sha1, sha224, sha256, sha384, \
    sha512, sha512_224, sha512_256, sha3_224, sha3_256, sha3_384, sha3_512. Bare digest \
    sizes select the SHA-2 family (e.g. 256 = sha256); SHA-3 and SHA-512 truncations must \
    be qualified (e.g. sha3-256, 512/256).";

#[derive(Args, Debug)]
pub struct HashArgs {
    /// Hash algorithm (see long help for the full list).
    #[arg(value_name = "ALGORITHM", long_help = ALGO_HELP)]
    pub algorithm: Algorithm,

    /// Files to hash.
    #[arg(required = true, value_name = "FILE")]
    pub files: Vec<PathBuf>,

    /// Write checksums to this file instead of standard output.
    #[arg(short = 'o', long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    #[command(flatten)]
    pub perf: PerfArgs,
}

#[derive(Args, Debug)]
pub struct VerifyArgs {
    /// Hash algorithm used to produce the checksum file(s).
    #[arg(value_name = "ALGORITHM", long_help = ALGO_HELP)]
    pub algorithm: Algorithm,

    /// Checksum files to read (coreutils `shaNsum` format). Use `-` for stdin.
    #[arg(required = true, value_name = "CHECKSUM_FILE")]
    pub checksum_files: Vec<PathBuf>,

    /// Don't print OK lines, only failures.
    #[arg(long)]
    pub quiet: bool,

    /// Print nothing; communicate the result only through the exit code.
    #[arg(long)]
    pub status: bool,

    #[command(flatten)]
    pub perf: PerfArgs,
}

/// Parse a human-friendly byte size such as `8M`, `16MiB`, `1024K`, or a plain
/// byte count. Decimal (K/M/G) and binary (KiB/MiB/GiB) suffixes are accepted
/// and treated identically as powers of 1024.
fn parse_size(s: &str) -> Result<usize, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty size".into());
    }
    let digits_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num, suffix) = s.split_at(digits_end);
    let value: usize = num
        .parse()
        .map_err(|_| format!("invalid number in size '{s}'"))?;

    let multiplier: usize = match suffix.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kib" | "kb" => 1024,
        "m" | "mib" | "mb" => 1024 * 1024,
        "g" | "gib" | "gb" => 1024 * 1024 * 1024,
        other => return Err(format!("unknown size suffix '{other}'")),
    };

    value
        .checked_mul(multiplier)
        .filter(|&n| n > 0)
        .ok_or_else(|| format!("size '{s}' is zero or too large"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_config_is_valid() {
        // Catches conflicting args, bad value names, etc. at test time.
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_byte_sizes_with_suffixes() {
        assert_eq!(parse_size("1024"), Ok(1024));
        assert_eq!(parse_size("1K"), Ok(1024));
        assert_eq!(parse_size("8M"), Ok(8 * 1024 * 1024));
        assert_eq!(parse_size("16MiB"), Ok(16 * 1024 * 1024));
        assert_eq!(parse_size("2g"), Ok(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_size("  4096  "), Ok(4096));
        assert_eq!(parse_size("512b"), Ok(512));
    }

    #[test]
    fn rejects_bad_sizes() {
        assert!(parse_size("").is_err());
        assert!(parse_size("0").is_err());
        assert!(parse_size("abc").is_err());
        assert!(parse_size("8X").is_err());
        assert!(parse_size("M").is_err());
    }

    #[test]
    fn parses_hash_invocation() {
        let cli = Cli::try_parse_from(["sha", "hash", "256", "a.txt", "b.txt"]).unwrap();
        match cli.command {
            Command::Hash(a) => {
                assert_eq!(a.algorithm, Algorithm::Sha256);
                assert_eq!(a.files.len(), 2);
            }
            _ => panic!("expected hash subcommand"),
        }
    }

    #[test]
    fn parses_verify_invocation_with_global_flags() {
        let cli = Cli::try_parse_from(["sha", "verify", "md5", "-j", "3", "sums.txt"]).unwrap();
        match cli.command {
            Command::Verify(a) => {
                assert_eq!(a.algorithm, Algorithm::Md5);
                assert_eq!(a.perf.jobs, Some(3));
            }
            _ => panic!("expected verify subcommand"),
        }
    }

    #[test]
    fn rejects_missing_algorithm_and_files() {
        assert!(Cli::try_parse_from(["sha", "hash"]).is_err());
        assert!(Cli::try_parse_from(["sha", "hash", "256"]).is_err());
        assert!(Cli::try_parse_from(["sha", "hash", "bogus-algo", "f"]).is_err());
    }
}
