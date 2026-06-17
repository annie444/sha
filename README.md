# sha

A fast, parallel command-line tool for computing and verifying **SHA-1**,
**SHA-256**, and **SHA-512** file hashes. Built for throughput on large batches
of large files.

## Why it's fast

- **Parallel across files.** SHA-1/256/512 are Merkle–Damgård constructions and
  cannot be parallelized *within* a single file without changing the result, so
  speed comes from hashing many files at once. Files are distributed across all
  logical CPUs with [rayon](https://crates.io/crates/rayon); throughput scales
  almost linearly with core count.
- **Hardware SHA instructions, automatically.** The
  [RustCrypto](https://github.com/RustCrypto/hashes) backends detect the x86
  SHA-NI instruction set **at runtime** and use it when present. A single binary
  runs optimally on modern CPUs (Intel Goldmont+/Ice Lake+, AMD Zen+) and still
  works correctly on older ones via a portable software fallback.
- **Low-overhead I/O.** Each file is read sequentially into a large, reused
  buffer (8 MiB by default). On Linux the kernel is told to expect sequential
  access (`posix_fadvise`) so readahead stays ahead of the hasher.

### Throughput

SHA-256 throughput is gated by the SHA-NI hardware instructions. On a CPU that
has them, a single core sustains roughly 1.5–2 GiB/s, so four cores reach
6–8 GiB/s — hashing 400 × 420 MiB files (~164 GiB) in well under a minute. On a
CPU without SHA-NI, throughput falls back to the software implementation
(roughly 0.2 GiB/s per core for SHA-256; SHA-1 and SHA-512 are faster in
software). SHA-512 is often the better choice on hardware lacking SHA-NI, as it
uses 64-bit operations the software backend vectorizes well.

## Build

```sh
cargo build --release
# binary at target/release/sha
```

## Usage

### Hashing

```sh
# SHA-256 (default), printed in coreutils `sha256sum` format
sha hash file1.iso file2.iso

# Pick an algorithm
sha hash -a sha1   *.tar
sha hash -a sha512 *.bin

# Write a checksum manifest
sha hash -a sha256 -o SHA256SUMS *.iso

# Tune parallelism and buffer size
sha hash -j 8 -b 16MiB *.dat
```

Output is identical to coreutils, so it interoperates with `sha256sum -c` and
friends:

```
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  empty.bin
```

### Verifying

```sh
# Verify against a checksum file (algorithm inferred from digest length)
sha verify SHA256SUMS

# Verify a coreutils-generated manifest
sha512sum *.bin > SHA512SUMS
sha verify SHA512SUMS

# Read the manifest from stdin
sha256sum *.iso | sha verify -

# Only report failures; or stay silent and use the exit code
sha verify --quiet  SHA256SUMS
sha verify --status SHA256SUMS && echo "all good"
```

`verify` exits `0` when every listed file matches, and non-zero if any digest
mismatches, any file can't be read, or any line is malformed.

## Options

| Option | Applies to | Description |
| --- | --- | --- |
| `-a, --algorithm <sha1\|sha256\|sha512>` | both | Algorithm. `hash` defaults to `sha256`; `verify` infers it from the digest length unless given. |
| `-j, --jobs <N>` | both | Number of files to hash in parallel (default: logical CPU count). |
| `-b, --buffer-size <SIZE>` | both | Per-file read buffer, e.g. `8M`, `16MiB`, `1048576` (default: 8 MiB). |
| `-o, --output <FILE>` | `hash` | Write checksums to a file instead of stdout. |
| `--quiet` | `verify` | Suppress `OK` lines; show only failures. |
| `--status` | `verify` | Print nothing; report only via exit code. |

## Tests

```sh
cargo test --release
```
