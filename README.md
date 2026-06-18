# sha

A fast, parallel command-line tool for computing and verifying file hashes.
Built for throughput on large batches of large files.

Supported algorithms: **MD5**, **SHA-1**, the **SHA-2** family (SHA-224,
SHA-256, SHA-384, SHA-512, SHA-512/224, SHA-512/256), and the **SHA-3** family
(SHA3-224, SHA3-256, SHA3-384, SHA3-512).

## Why it's fast

- **Parallel across files.** The SHA/MD5 compression functions are sequential
  and cannot be parallelized *within* a single file without changing the result,
  so speed comes from hashing many files at once. Files are distributed across
  all logical CPUs with [rayon](https://crates.io/crates/rayon); throughput
  scales almost linearly with core count.
- **Hardware SHA instructions, automatically.** For SHA-1 and SHA-256 the
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
software). SHA-NI accelerates only SHA-1/SHA-256; MD5, SHA-512, and the SHA-3
family always run in software.

## Build

```sh
cargo build --release
# binary at target/release/sha
```

## Usage

The algorithm is the first argument to each subcommand.

```
sha hash   <ALGORITHM> <FILE>...
sha verify <ALGORITHM> <CHECKSUM_FILE>...
```

`<ALGORITHM>` is one of `md5`, `sha1`, `sha224`, `sha256`, `sha384`, `sha512`,
`sha512_224`, `sha512_256`, `sha3_224`, `sha3_256`, `sha3_384`, `sha3_512`.
A bare digest size selects the SHA-2 family (`256` = `sha256`); SHA-3 and the
SHA-512 truncations must be qualified and accept any of `-`, `_`, `/` as a
separator (`sha3-256`, `512/256`).

### Hashing

```sh
# SHA-256, printed in coreutils `sha256sum` format
sha hash 256 *.tar

# Other algorithms
sha hash sha1     *.iso
sha hash md5      *.bin
sha hash sha3-256 *.dat
sha hash 512/256  *.img

# Write a checksum manifest
sha hash 256 -o SHA256SUMS *.iso

# Tune parallelism and buffer size
sha hash 256 -j 8 -b 16MiB *.dat
```

Output matches coreutils, so it interoperates with `sha256sum -c` and friends:

```
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  empty.bin
```

### Verifying

The algorithm is given explicitly (digest length alone is ambiguous — e.g.
SHA-256, SHA3-256, and SHA-512/256 are all 64 hex chars). Lines whose digest
length doesn't match the chosen algorithm are reported as errors.

```sh
# Verify against a checksum file
sha verify 256 SHA256SUMS

# Verify a coreutils- or openssl-generated manifest
sha512sum *.bin > SHA512SUMS
sha verify 512 SHA512SUMS

# Read the manifest from stdin
sha256sum *.iso | sha verify 256 -

# Only report failures; or stay silent and use the exit code
sha verify 256 --quiet  SHA256SUMS
sha verify 256 --status SHA256SUMS && echo "all good"
```

`verify` exits `0` when every listed file matches, and non-zero if any digest
mismatches, any file can't be read, or any line is malformed.

## Options

| Option | Applies to | Description |
| --- | --- | --- |
| `<ALGORITHM>` (positional) | both | The hash algorithm (see list above). Required. |
| `-j, --jobs <N>` | both | Number of files to hash in parallel (default: logical CPU count). |
| `-b, --buffer-size <SIZE>` | both | Per-file read buffer, e.g. `8M`, `16MiB`, `1048576` (default: 8 MiB). |
| `-o, --output <FILE>` | `hash` | Write checksums to a file instead of stdout. |
| `--quiet` | `verify` | Suppress `OK` lines; show only failures. |
| `--status` | `verify` | Print nothing; report only via exit code. |

## Tests

```sh
cargo test --release
```
