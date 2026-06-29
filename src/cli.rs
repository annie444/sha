use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

pub const SHA_STYLING: Styles = Styles::styled()
    .header(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::BrightCyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::BrightRed.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::BrightCyan.on_default().effects(Effects::BOLD))
    .invalid(AnsiColor::Yellow.on_default())
    .context(AnsiColor::BrightBlue.on_default().effects(Effects::BOLD));

#[derive(Parser, Debug)]
#[command(
    name = "sha",
    version,
    about = "Fast, parallel SHA-1/256/512 file hashing and verification",
    long_about = "Compute and verify SHA-1, SHA-256, and SHA-512 hashes of files.\n\
                  Files are hashed concurrently across CPU cores.\n\
                  On x86_64 the SHA_NI hardware instructions are used automatically \
                  when present.",
    styles = SHA_STYLING,
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
    #[arg(short, long, global = true, value_name = "N")]
    pub jobs: Option<usize>,

    /// Per-file read buffer size, e.g. 8M, 16MiB, 1048576 (default: 8MiB).
    #[arg(short, long, global = true, value_name = "SIZE", value_parser = parse_size)]
    pub buffer_size: Option<usize>,
}

#[derive(Args, Debug)]
pub struct HashArgs {
    /// Hash algorithm
    #[arg(value_enum, value_name = "ALGORITHM")]
    pub algorithm: CliAlgorithm,

    /// Files to hash.
    #[arg(required = true, value_name = "FILE")]
    pub files: Vec<PathBuf>,

    /// Write checksums to this file instead of standard output.
    #[arg(short = 'o', long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    #[command(flatten)]
    pub perf: PerfArgs,

    #[command(flatten)]
    pub verbosity: Verbosity<InfoLevel>,
}

#[derive(Args, Debug)]
pub struct VerifyArgs {
    /// Hash algorithm used to produce the checksum file(s).
    #[arg(value_enum, value_name = "ALGORITHM")]
    pub algorithm: CliAlgorithm,

    /// Checksum files to read (coreutils `shaNsum` format). Use `-` for stdin.
    #[arg(required = true, value_name = "CHECKSUM_FILE")]
    pub checksum_files: Vec<PathBuf>,

    /// Print nothing; communicate the result only through the exit code.
    #[arg(short, long)]
    pub status: bool,

    #[command(flatten)]
    pub perf: PerfArgs,

    #[command(flatten)]
    pub verbosity: Verbosity<InfoLevel>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum CliAlgorithm {
    #[clap(alias = "md5sum", alias = "5")]
    Md5,
    #[clap(alias = "sha1sum", alias = "1")]
    Sha1,
    #[clap(alias = "sha224sum", alias = "224")]
    Sha224,
    #[clap(alias = "sha256sum", alias = "256")]
    Sha256,
    #[clap(alias = "sha384sum", alias = "384")]
    Sha384,
    #[clap(alias = "sha512sum", alias = "512")]
    Sha512,
    #[clap(
        alias = "sha512_224sum",
        alias = "512_224",
        alias = "sha512-224sum",
        alias = "512-224",
        alias = "sha512-224",
        alias = "sha512/224sum",
        alias = "512/224",
        alias = "sha512/224",
        alias = "sha512224sum",
        alias = "512224",
        alias = "sha512224"
    )]
    Sha512_224,
    #[clap(
        alias = "sha512_256sum",
        alias = "512_256",
        alias = "sha512-256sum",
        alias = "512-256",
        alias = "sha512-256",
        alias = "sha512/256sum",
        alias = "512/256",
        alias = "sha512/256",
        alias = "sha512256sum",
        alias = "512256",
        alias = "sha512256"
    )]
    Sha512_256,
    #[clap(
        alias = "sha3_224sum",
        alias = "3_224",
        alias = "sha3-224sum",
        alias = "3-224",
        alias = "sha3-224",
        alias = "sha3/224sum",
        alias = "3/224",
        alias = "sha3/224",
        alias = "sha3224sum",
        alias = "3224",
        alias = "sha3224"
    )]
    Sha3_224,
    #[clap(
        alias = "sha3_256sum",
        alias = "3_256",
        alias = "sha3-256sum",
        alias = "3-256",
        alias = "sha3-256",
        alias = "sha3/256sum",
        alias = "3/256",
        alias = "sha3/256",
        alias = "sha3256sum",
        alias = "3256",
        alias = "sha3256"
    )]
    Sha3_256,
    #[clap(
        alias = "sha3_384sum",
        alias = "3_384",
        alias = "sha3-384sum",
        alias = "3-384",
        alias = "sha3-384",
        alias = "sha3/384sum",
        alias = "3/384",
        alias = "sha3/384",
        alias = "sha3384sum",
        alias = "3384",
        alias = "sha3384"
    )]
    Sha3_384,
    #[clap(
        alias = "sha3_512sum",
        alias = "3_512",
        alias = "sha3-512sum",
        alias = "3-512",
        alias = "sha3-512",
        alias = "sha3/512sum",
        alias = "3/512",
        alias = "sha3/512",
        alias = "sha3512sum",
        alias = "3512",
        alias = "sha3512"
    )]
    Sha3_512,
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
                assert_eq!(a.algorithm, CliAlgorithm::Sha256);
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
                assert_eq!(a.algorithm, CliAlgorithm::Md5);
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
