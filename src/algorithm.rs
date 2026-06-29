use std::fmt;

/// Supported hashing algorithms: MD5, SHA-1, the SHA-2 family, and the SHA-3
/// family. These are exactly the fixed-output digests provided by the
/// RustCrypto `md-5`, `sha1`, `sha2`, and `sha3` crates (the SHAKE
/// variable-length XOFs are intentionally excluded).
#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_len_matches_name() {
        assert_eq!(Algorithm::Md5.hex_len(), 32);
        assert_eq!(Algorithm::Sha1.hex_len(), 40);
        assert_eq!(Algorithm::Sha3_512.hex_len(), 128);
        assert_eq!(Algorithm::Sha512_256.hex_len(), 64);
    }
}
