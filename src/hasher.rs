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

/// Compute the digest of the file at `path` and return it as a lowercase hex
/// string. `buf` is reused across calls on the same thread to avoid repeated
/// large allocations.
pub fn hash_file(path: &Path, algo: Algorithm, buf: &mut [u8]) -> io::Result<String> {
    let file = File::open(path)?;
    advise_sequential(&file);

    let digest = match algo {
        Algorithm::Md5 => hash_reader::<md5::Md5, _>(file, buf)?,
        Algorithm::Sha1 => hash_reader::<sha1::Sha1, _>(file, buf)?,
        Algorithm::Sha224 => hash_reader::<sha2::Sha224, _>(file, buf)?,
        Algorithm::Sha256 => hash_reader::<sha2::Sha256, _>(file, buf)?,
        Algorithm::Sha384 => hash_reader::<sha2::Sha384, _>(file, buf)?,
        Algorithm::Sha512 => hash_reader::<sha2::Sha512, _>(file, buf)?,
        Algorithm::Sha512_224 => hash_reader::<sha2::Sha512_224, _>(file, buf)?,
        Algorithm::Sha512_256 => hash_reader::<sha2::Sha512_256, _>(file, buf)?,
        Algorithm::Sha3_224 => hash_reader::<sha3::Sha3_224, _>(file, buf)?,
        Algorithm::Sha3_256 => hash_reader::<sha3::Sha3_256, _>(file, buf)?,
        Algorithm::Sha3_384 => hash_reader::<sha3::Sha3_384, _>(file, buf)?,
        Algorithm::Sha3_512 => hash_reader::<sha3::Sha3_512, _>(file, buf)?,
    };
    Ok(hex::encode(digest))
}
