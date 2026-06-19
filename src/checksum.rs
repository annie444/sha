//! Parsing of checksum-file lines (coreutils `shaNsum` format).
//!
//! This input is untrusted — it comes from arbitrary `*SUMS` files — so the
//! parser is written to never panic on any byte sequence and is exercised by a
//! fuzz target (`fuzz/fuzz_targets/parse_checksum_line.rs`).

use std::path::PathBuf;

use crate::algorithm::Algorithm;

/// One successfully parsed `<digest>  <path>` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecksumEntry {
    pub algo: Algorithm,
    /// Lowercase hex digest.
    pub expected: String,
    pub path: PathBuf,
}

/// Parse one line of a checksum file against the chosen `algo`.
///
/// Returns `None` for blank lines and comments, `Some(Err(..))` for malformed
/// lines, and `Some(Ok(..))` for valid entries. The two coreutils separators
/// are accepted: `"  "` (text) and `" *"` (binary). The digest length is checked
/// against `algo` so a mismatched algorithm choice is reported clearly rather
/// than as a silent comparison failure.
pub fn parse_line(line: &str, algo: Algorithm) -> Option<Result<ChecksumEntry, String>> {
    let line = line.trim_end_matches(['\n', '\r']);
    if line.trim().is_empty() || line.starts_with('#') {
        return None;
    }

    let Some((hex, rest)) = line.split_once(' ') else {
        return Some(Err("line is not in '<digest>  <file>' format".into()));
    };
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Some(Err("line is not in '<digest>  <file>' format".into()));
    }
    if hex.len() != algo.hex_len() {
        return Some(Err(format!(
            "digest length {} does not match {} (expected {})",
            hex.len(),
            algo,
            algo.hex_len()
        )));
    }

    // `rest` begins with the mode flag (' ' for text, '*' for binary); drop it.
    let filename = rest.strip_prefix([' ', '*']).unwrap_or(rest);
    if filename.is_empty() {
        return Some(Err("missing filename".into()));
    }

    Some(Ok(ChecksumEntry {
        algo,
        expected: hex.to_ascii_lowercase(),
        path: PathBuf::from(filename),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_and_binary_separators() {
        let text = parse_line(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  file.txt",
            Algorithm::Sha256,
        )
        .unwrap()
        .unwrap();
        assert_eq!(text.algo, Algorithm::Sha256);
        assert_eq!(text.path, PathBuf::from("file.txt"));

        let bin = parse_line(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad *file.bin",
            Algorithm::Sha256,
        )
        .unwrap()
        .unwrap();
        assert_eq!(bin.path, PathBuf::from("file.bin"));
    }

    #[test]
    fn skips_blank_and_comment_lines() {
        assert!(parse_line("", Algorithm::Sha256).is_none());
        assert!(parse_line("   ", Algorithm::Sha256).is_none());
        assert!(parse_line("# a comment", Algorithm::Sha256).is_none());
    }

    #[test]
    fn rejects_non_hex_digest() {
        assert!(parse_line("nothex  file", Algorithm::Sha256)
            .unwrap()
            .is_err());
    }

    #[test]
    fn rejects_digest_length_mismatch() {
        // A 64-char (sha256) digest checked as sha512 should be flagged.
        assert!(parse_line(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  f",
            Algorithm::Sha512,
        )
        .unwrap()
        .is_err());
    }
}
