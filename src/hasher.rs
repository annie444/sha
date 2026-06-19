//! Core file-hashing routines.
//!
//! Each file is hashed by a single thread reading large sequential chunks into
//! a reusable buffer and feeding them to a streaming digest. SHA-1/256/512 are
//! Merkle–Damgård constructions and cannot be parallelized *within* a single
//! file without changing the result, so throughput on a large set of files is
//! obtained by hashing many files concurrently (see `commands`), not by
//! splitting one file across cores.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use digest::Digest;

use crate::algorithm::Algorithm;

/// Default read-buffer size. Large enough to amortize syscall overhead and keep
/// the kernel's readahead saturated, small enough to stay in L2/L3 cache.
pub const DEFAULT_BUFFER_SIZE: usize = 8 * 1024 * 1024;

/// Stream `reader` through digest `D` using `buf` as scratch space.
fn hash_reader<D: Digest, R: Read>(mut reader: R, buf: &mut [u8]) -> io::Result<Vec<u8>> {
    let mut hasher = D::new();
    loop {
        let n = reader.read(buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_vec())
}

/// Hint the kernel that we will read this file sequentially and soon, so it can
/// schedule aggressive readahead. Best-effort: errors are ignored.
#[cfg(target_os = "linux")]
fn advise_sequential(file: &File) {
    use std::os::unix::io::AsRawFd;
    // length 0 means "to end of file".
    unsafe {
        libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_SEQUENTIAL);
        libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_WILLNEED);
    }
}

#[cfg(not(target_os = "linux"))]
fn advise_sequential(_file: &File) {}

/// Stream all bytes from `reader` through `algo` using `buf` as scratch space,
/// returning the digest as a lowercase hex string. The result is independent of
/// `buf`'s size; smaller buffers just mean more read iterations.
pub fn hash_stream<R: Read>(reader: R, algo: Algorithm, buf: &mut [u8]) -> io::Result<String> {
    let digest = match algo {
        Algorithm::Md5 => hash_reader::<md5::Md5, _>(reader, buf)?,
        Algorithm::Sha1 => hash_reader::<sha1::Sha1, _>(reader, buf)?,
        Algorithm::Sha224 => hash_reader::<sha2::Sha224, _>(reader, buf)?,
        Algorithm::Sha256 => hash_reader::<sha2::Sha256, _>(reader, buf)?,
        Algorithm::Sha384 => hash_reader::<sha2::Sha384, _>(reader, buf)?,
        Algorithm::Sha512 => hash_reader::<sha2::Sha512, _>(reader, buf)?,
        Algorithm::Sha512_224 => hash_reader::<sha2::Sha512_224, _>(reader, buf)?,
        Algorithm::Sha512_256 => hash_reader::<sha2::Sha512_256, _>(reader, buf)?,
        Algorithm::Sha3_224 => hash_reader::<sha3::Sha3_224, _>(reader, buf)?,
        Algorithm::Sha3_256 => hash_reader::<sha3::Sha3_256, _>(reader, buf)?,
        Algorithm::Sha3_384 => hash_reader::<sha3::Sha3_384, _>(reader, buf)?,
        Algorithm::Sha3_512 => hash_reader::<sha3::Sha3_512, _>(reader, buf)?,
    };
    Ok(hex::encode(digest))
}

/// Compute the digest of the file at `path` and return it as a lowercase hex
/// string. `buf` is reused across calls on the same thread to avoid repeated
/// large allocations.
pub fn hash_file(path: &Path, algo: Algorithm, buf: &mut [u8]) -> io::Result<String> {
    let file = File::open(path)?;
    advise_sequential(&file);
    hash_stream(file, algo, buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Write `data` to a fresh temp file and return the handle (kept alive so
    /// the file is not deleted) and its path.
    fn temp_with(data: &[u8]) -> (tempfile::NamedTempFile, std::path::PathBuf) {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(data).unwrap();
        f.flush().unwrap();
        let path = f.path().to_path_buf();
        (f, path)
    }

    fn hash(data: &[u8], algo: Algorithm) -> String {
        let (_keep, path) = temp_with(data);
        let mut buf = vec![0u8; DEFAULT_BUFFER_SIZE];
        hash_file(&path, algo, &mut buf).unwrap()
    }

    /// (algorithm, digest of "", digest of "abc") — the canonical NIST vectors.
    const VECTORS: &[(Algorithm, &str, &str)] = &[
        (
            Algorithm::Md5,
            "d41d8cd98f00b204e9800998ecf8427e",
            "900150983cd24fb0d6963f7d28e17f72",
        ),
        (
            Algorithm::Sha1,
            "da39a3ee5e6b4b0d3255bfef95601890afd80709",
            "a9993e364706816aba3e25717850c26c9cd0d89d",
        ),
        (
            Algorithm::Sha224,
            "d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f",
            "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7",
        ),
        (
            Algorithm::Sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            Algorithm::Sha384,
            "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b",
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7",
        ),
        (
            Algorithm::Sha512,
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
        ),
        (
            Algorithm::Sha512_224,
            "6ed0dd02806fa89e25de060c19d3ac86cabb87d6a0ddd05c333b84f4",
            "4634270f707b6a54daae7530460842e20e37ed265ceee9a43e8924aa",
        ),
        (
            Algorithm::Sha512_256,
            "c672b8d1ef56ed28ab87c3622c5114069bdd3ad7b8f9737498d0c01ecef0967a",
            "53048e2681941ef99b2e29b76b4c7dabe4c2d0c634fc6d46e0e2f13107e7af23",
        ),
        (
            Algorithm::Sha3_224,
            "6b4e03423667dbb73b6e15454f0eb1abd4597f9a1b078e3f5b5a6bc7",
            "e642824c3f8cf24ad09234ee7d3c766fc9a3a5168d0c94ad73b46fdf",
        ),
        (
            Algorithm::Sha3_256,
            "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a",
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532",
        ),
        (
            Algorithm::Sha3_384,
            "0c63a75b845e4f7d01107d852e4c2485c51a50aaaa94fc61995e71bbee983a2ac3713831264adb47fb6bd1e058d5f004",
            "ec01498288516fc926459f58e2c6ad8df9b473cb0fc08c2596da7cf0e49be4b298d88cea927ac7f539f1edf228376d25",
        ),
        (
            Algorithm::Sha3_512,
            "a69f73cca23a9ac5c8b567dc185a756e97c982164fe25859e0d1dcc1475c80a615b2123af1f5f94c11e3e9402c3ac558f500199d95b6d3e301758586281dcd26",
            "b751850b1a57168a5693cd924b6b096e08f621827444f70d884f5d0240d2712e10e116e9192af3c91a7ec57647e3934057340b4cf408d5a56592f8274eec53f0",
        ),
    ];

    #[test]
    fn known_answer_vectors_for_every_algorithm() {
        for &(algo, empty, abc) in VECTORS {
            assert_eq!(hash(b"", algo), empty, "{algo} of empty input");
            assert_eq!(hash(b"abc", algo), abc, "{algo} of \"abc\"");
            // Sanity: the digest length matches the declared length.
            assert_eq!(empty.len(), algo.hex_len(), "{algo} declared hex_len");
        }
    }

    #[test]
    fn result_is_independent_of_buffer_size() {
        // Data larger than several small buffers, hashed with awkward buffer
        // sizes, must equal the single-shot result. This exercises the read
        // loop across chunk boundaries (including a 1-byte buffer and sizes
        // that don't divide the block size).
        let data: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let (_keep, path) = temp_with(&data);

        let reference = {
            let mut buf = vec![0u8; DEFAULT_BUFFER_SIZE];
            hash_file(&path, Algorithm::Sha256, &mut buf).unwrap()
        };

        for size in [1usize, 7, 63, 64, 65, 1000, 4096] {
            let mut buf = vec![0u8; size];
            let got = hash_file(&path, Algorithm::Sha256, &mut buf).unwrap();
            assert_eq!(got, reference, "buffer size {size} changed the digest");
        }
    }

    #[test]
    fn missing_file_is_an_error() {
        let mut buf = vec![0u8; 4096];
        let err = hash_file(
            std::path::Path::new("/nonexistent/definitely/not/here"),
            Algorithm::Sha256,
            &mut buf,
        );
        assert!(err.is_err());
    }
}
