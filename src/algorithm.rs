//! Hash algorithm selection and metadata.

use std::fmt;
use std::str::FromStr;

/// Supported hashing algorithms.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

impl Algorithm {
    /// Infer the algorithm from the length of a hex digest string.
    ///
    /// Used by `verify` so a checksum file can be checked without the user
    /// having to restate which algorithm produced it.
    pub const fn from_hex_len(len: usize) -> Option<Self> {
        match len {
            40 => Some(Algorithm::Sha1),
            64 => Some(Algorithm::Sha256),
            128 => Some(Algorithm::Sha512),
            _ => None,
        }
    }

    /// Canonical lowercase name.
    pub const fn name(self) -> &'static str {
        match self {
            Algorithm::Sha1 => "sha1",
            Algorithm::Sha256 => "sha256",
            Algorithm::Sha512 => "sha512",
        }
    }
}

impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Algorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().replace(['-', '_'], "").as_str() {
            "sha1" | "1" => Ok(Algorithm::Sha1),
            "sha256" | "256" => Ok(Algorithm::Sha256),
            "sha512" | "512" => Ok(Algorithm::Sha512),
            other => Err(format!(
                "unknown algorithm '{other}' (expected sha1, sha256, or sha512)"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_case_and_separator_insensitively() {
        assert_eq!("sha1".parse(), Ok(Algorithm::Sha1));
        assert_eq!("SHA-256".parse(), Ok(Algorithm::Sha256));
        assert_eq!("Sha_512".parse(), Ok(Algorithm::Sha512));
        assert_eq!("256".parse(), Ok(Algorithm::Sha256));
        assert!("md5".parse::<Algorithm>().is_err());
    }

    #[test]
    fn infers_algorithm_from_digest_length() {
        assert_eq!(Algorithm::from_hex_len(40), Some(Algorithm::Sha1));
        assert_eq!(Algorithm::from_hex_len(64), Some(Algorithm::Sha256));
        assert_eq!(Algorithm::from_hex_len(128), Some(Algorithm::Sha512));
        assert_eq!(Algorithm::from_hex_len(32), None);
    }
}
