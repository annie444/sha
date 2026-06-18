//! Hash algorithm selection and metadata.

use std::fmt;
use std::str::FromStr;

/// Supported hashing algorithms: MD5, SHA-1, the SHA-2 family, and the SHA-3
/// family. These are exactly the fixed-output digests provided by the
/// RustCrypto `md-5`, `sha1`, `sha2`, and `sha3` crates (the SHAKE
/// variable-length XOFs are intentionally excluded).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Algorithm {
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Sha512_224,
    Sha512_256,
    Sha3_224,
    Sha3_256,
    Sha3_384,
    Sha3_512,
}

impl Algorithm {
    /// Canonical lowercase name, e.g. `sha512_256`.
    pub const fn name(self) -> &'static str {
        match self {
            Algorithm::Md5 => "md5",
            Algorithm::Sha1 => "sha1",
            Algorithm::Sha224 => "sha224",
            Algorithm::Sha256 => "sha256",
            Algorithm::Sha384 => "sha384",
            Algorithm::Sha512 => "sha512",
            Algorithm::Sha512_224 => "sha512_224",
            Algorithm::Sha512_256 => "sha512_256",
            Algorithm::Sha3_224 => "sha3_224",
            Algorithm::Sha3_256 => "sha3_256",
            Algorithm::Sha3_384 => "sha3_384",
            Algorithm::Sha3_512 => "sha3_512",
        }
    }

    /// Length, in hex characters, of a digest produced by this algorithm.
    /// Used to sanity-check checksum-file lines against the chosen algorithm.
    pub const fn hex_len(self) -> usize {
        // hex chars = output bytes * 2
        match self {
            Algorithm::Md5 => 32,
            Algorithm::Sha1 => 40,
            Algorithm::Sha224 | Algorithm::Sha512_224 | Algorithm::Sha3_224 => 56,
            Algorithm::Sha256 | Algorithm::Sha512_256 | Algorithm::Sha3_256 => 64,
            Algorithm::Sha384 | Algorithm::Sha3_384 => 96,
            Algorithm::Sha512 | Algorithm::Sha3_512 => 128,
        }
    }

    /// Every supported algorithm, for help text and tests.
    #[allow(dead_code)]
    pub const ALL: [Algorithm; 12] = [
        Algorithm::Md5,
        Algorithm::Sha1,
        Algorithm::Sha224,
        Algorithm::Sha256,
        Algorithm::Sha384,
        Algorithm::Sha512,
        Algorithm::Sha512_224,
        Algorithm::Sha512_256,
        Algorithm::Sha3_224,
        Algorithm::Sha3_256,
        Algorithm::Sha3_384,
        Algorithm::Sha3_512,
    ];
}

impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Algorithm {
    type Err = String;

    /// Parse an algorithm name. Accepts canonical names (`sha256`), bare digest
    /// sizes for the common SHA-2 family (`256` => SHA-256), and SHA-3 / SHA-512
    /// truncations with any of `-`, `_`, or `/` as separators (`sha3-256`,
    /// `512/256`). Bare numbers map to the SHA-2 family; SHA-3 and the SHA-512
    /// truncations must be qualified.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Normalize: lowercase and drop separators so "SHA-512/256",
        // "sha512_256", and "512256" all collapse to the same token.
        let norm: String = s
            .to_ascii_lowercase()
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | '/' | ' '))
            .collect();

        let algo = match norm.as_str() {
            "md5" => Algorithm::Md5,
            "sha1" | "1" => Algorithm::Sha1,
            "sha224" | "224" => Algorithm::Sha224,
            "sha256" | "256" => Algorithm::Sha256,
            "sha384" | "384" => Algorithm::Sha384,
            "sha512" | "512" => Algorithm::Sha512,
            "sha512224" | "512224" => Algorithm::Sha512_224,
            "sha512256" | "512256" => Algorithm::Sha512_256,
            "sha3224" | "3224" => Algorithm::Sha3_224,
            "sha3256" | "3256" => Algorithm::Sha3_256,
            "sha3384" | "3384" => Algorithm::Sha3_384,
            "sha3512" | "3512" => Algorithm::Sha3_512,
            _ => return Err(format!("unknown algorithm '{s}'")),
        };
        Ok(algo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_and_short_names() {
        assert_eq!("256".parse(), Ok(Algorithm::Sha256));
        assert_eq!("sha256".parse(), Ok(Algorithm::Sha256));
        assert_eq!("md5".parse(), Ok(Algorithm::Md5));
        assert_eq!("1".parse(), Ok(Algorithm::Sha1));
    }

    #[test]
    fn parses_sha3_and_truncations_with_any_separator() {
        assert_eq!("sha3-256".parse(), Ok(Algorithm::Sha3_256));
        assert_eq!("3_256".parse(), Ok(Algorithm::Sha3_256));
        assert_eq!("SHA-512/256".parse(), Ok(Algorithm::Sha512_256));
        assert_eq!("sha512_224".parse(), Ok(Algorithm::Sha512_224));
    }

    #[test]
    fn bare_number_prefers_sha2() {
        // "256" is SHA-256, not SHA3-256 or SHA-512/256.
        assert_eq!("256".parse(), Ok(Algorithm::Sha256));
        assert_eq!("512".parse(), Ok(Algorithm::Sha512));
    }

    #[test]
    fn rejects_unknown() {
        assert!("md4".parse::<Algorithm>().is_err());
        assert!("sha999".parse::<Algorithm>().is_err());
    }

    #[test]
    fn every_canonical_name_round_trips() {
        for algo in Algorithm::ALL {
            assert_eq!(algo.name().parse(), Ok(algo), "{algo} did not round-trip");
        }
    }

    #[test]
    fn hex_len_matches_name() {
        assert_eq!(Algorithm::Md5.hex_len(), 32);
        assert_eq!(Algorithm::Sha1.hex_len(), 40);
        assert_eq!(Algorithm::Sha3_512.hex_len(), 128);
        assert_eq!(Algorithm::Sha512_256.hex_len(), 64);
    }
}
