# sha

A fast, parallel command-line tool for computing and verifying file hashes.
Built for throughput on large batches of large files.

Supported algorithms: **MD5**, **SHA-1**, the **SHA-2** family (SHA-224,
SHA-256, SHA-384, SHA-512, SHA-512/224, SHA-512/256), and the **SHA-3** family
(SHA3-224, SHA3-256, SHA3-384, SHA3-512).

## Why it's fast

- **Parallel across files.** The SHA/MD5 compression functions are sequential
  and cannot be parallelized _within_ a single file without changing the result,
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

| Option                     | Applies to | Description                                                           |
| -------------------------- | ---------- | --------------------------------------------------------------------- |
| `<ALGORITHM>` (positional) | both       | The hash algorithm (see list above). Required.                        |
| `-j, --jobs <N>`           | both       | Number of files to hash in parallel (default: logical CPU count).     |
| `-b, --buffer-size <SIZE>` | both       | Per-file read buffer, e.g. `8M`, `16MiB`, `1048576` (default: 8 MiB). |
| `-o, --output <FILE>`      | `hash`     | Write checksums to a file instead of stdout.                          |
| `--quiet`                  | `verify`   | Suppress `OK` lines; show only failures.                              |
| `--status`                 | `verify`   | Print nothing; report only via exit code.                             |

## Tests

```sh
cargo test            # unit + integration tests
```

The suite covers known-answer (NIST) vectors for all 12 algorithms, read-loop
correctness across buffer boundaries, the size/argument parsers, and end-to-end
behavior of both subcommands (output format, exit codes, tamper detection,
stdin, `--status`/`--quiet`, and a hash/verify round-trip per algorithm).

## Fuzzing

The untrusted inputs (checksum-file lines, algorithm names) and the hand-written
streaming read loop are covered by [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz)
targets in `fuzz/`. Requires a nightly toolchain:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run parse_checksum_line   # untrusted checksum-line parser
cargo +nightly fuzz run hash_chunking         # read loop: digest must not depend on buffer size
cargo +nightly fuzz run parse_algorithm       # algorithm-name parser
```

`hash_chunking` is a differential test: it hashes arbitrary data with a
fuzz-chosen buffer size and asserts the digest matches hashing the same data in
a single read, so any boundary or short-read bug surfaces as a mismatch. CI runs
a short smoke campaign on each target.

## Benchmarks

### Per-algorithm throughput (Criterion):

```sh
cargo bench                                   # 16 MiB default payload
SHA_BENCH_SIZE=$((64*1024*1024)) cargo bench  # larger payload
cargo bench -- sha256                          # filter by name
```

This measures single-stream `hash_file` throughput for each algorithm and
reports bytes/second.

### Comparison against coreutils:

```sh
scripts/bench-vs-coreutils.sh
NUM_FILES=8 FILE_SIZE_MB=420 REPS=3 scripts/bench-vs-coreutils.sh
```

Times `sha` (parallel and single-threaded) against the coreutils `*sum` tools
and against coreutils fanned out with `xargs -P`, on the same fileset.

### Measured results

From one such run on a 24-logical-CPU machine, hashing 150 x 400 MB files
(58.6 GiB total), 10 repetitions per configuration. Figures are mean
throughput in GiB/s (higher is better); ratios are `sha` / coreutils,
so `>1.0` means `sha` is faster.

| Algorithm | sha, 1 core | coreutils, 1 core | per-core | sha, 24 cores | coreutils + `xargs -P` |  parallel |
| --------- | ----------: | ----------------: | -------: | ------------: | ---------------------: | --------: |
| MD5       |        0.69 |              0.83 |    0.83x |          6.97 |                   6.32 | **1.10x** |
| SHA-1     |        1.36 |              1.47 |    0.93x |          7.56 |                   6.38 | **1.18x** |
| SHA-224   |        1.34 |              1.39 |    0.96x |          7.29 |                   6.38 | **1.14x** |
| SHA-256   |        1.34 |              1.38 |    0.97x |          6.86 |                   6.38 | **1.07x** |
| SHA-384   |        0.64 |              0.84 |    0.76x |          5.93 |                   6.32 |     0.94x |
| SHA-512   |        0.64 |              0.84 |    0.76x |          5.77 |                   6.32 |     0.91x |

### Multicore throughput by algorithm

The same `sha`, all 24 cores, in isolation — mean GiB/s +/- run-to-run
standard deviation, and wall-clock time to hash the full 58.6 GiB
fileset. The fast (SHA-NI) algorithms are noisier because they run up
against the I/O ceiling; the software SHA-384/512 paths are cleanly
CPU-bound and so more stable.

| Algorithm | Throughput (GiB/s) | 58.6 GiB in |
| --------- | -----------------: | ----------: |
| SHA-1     |      7.56 +/- 0.65 |      7.75 s |
| SHA-224   |      7.29 +/- 0.66 |      8.04 s |
| MD5       |      6.97 +/- 0.66 |      8.41 s |
| SHA-256   |      6.86 +/- 0.35 |      8.54 s |
| SHA-384   |      5.93 +/- 0.21 |      9.88 s |
| SHA-512   |      5.77 +/- 0.21 |     10.16 s |

`sha` is _not_ faster than coreutils at the core level — single-threaded
it runs at 0.76x–0.97x of the corresponding coreutils `*sum` tool on every
algorithm, because both lean on the same hardware/software hash kernels
(and RustCrypto's software path is the slower of the two; see below). All
of `sha`'s advantage comes from parallelism, and it is modest: against
coreutils fanned out with `xargs -P` it wins by **7–18%** on
MD5/SHA-1/SHA-2-256, and actually _loses_ (0.91x–0.94x) on SHA-384/SHA-512.

So `sha`'s real edge is ergonomic — one binary, one process, a reused buffer and
`posix_fadvise` instead of spawning 150 short-lived processes — which is why it
clears the ~6.3 GiB/s ceiling that `xargs` plateaus at for the cheaper hashes.
If you already have `xargs` in your fingers, it gets you most of the way; `sha`
is the convenient, marginally-faster single tool, not a step-change in speed.

**Interpreting the numbers.** Throughput depends heavily on whether the CPU has
the SHA-NI hardware instructions, because RustCrypto's `sha2` backend detects
them at runtime:

- **With SHA-NI** (most x86 CPUs since ~2016 — Intel Goldmont/Ice Lake+, AMD
  Zen+): SHA-1/SHA-256 run on the hardware instructions. Measured single-thread
  on a SHA-NI Xeon, `sha`, coreutils `sha256sum`, and openssl are all at parity
  — about **1.0 GiB/s (~1.9 cycles/byte)** — because all three use the same
  hardware path. Aggregate throughput then scales with core count as `sha`
  hashes files in parallel.
- **Without SHA-NI**: RustCrypto falls back to its portable `soft` backend,
  which is measurably _slower_ than coreutils/openssl (~0.18 vs ~0.36 GiB/s
  single-thread on an AVX2-only Xeon). On such machines `sha`'s advantage comes
  purely from parallelism across files.

Why the software path trails the C tools is worth understanding: RustCrypto's
`soft` SHA-256 is not a conventional scalar kernel. It is written as a _software
emulation of the SHA-NI instruction sequence_, operating on `[u32; 4]` lanes
through helpers named after the hardware intrinsics (`sha256msg1`, `sha256msg2`,
`sha256_digest_round_x2`). Structuring the data in four-word chunks lets the
compiler autovectorize it into SSE/AVX SIMD registers where possible, and lets
the same code shape map directly onto the SHA-NI backend. The trade-off is that
when those `[u32; 4]` operations are _not_ vectorized (a generic `x86-64`
build), each becomes four scalar instructions plus extra lane-shuffling
(`sha256load`, `sha256swap`) that a purpose-built scalar C kernel — like the one
in coreutils — never performs, costing roughly 2× the work per round. SHA-512
and SHA-3 are always software on x86 (no hardware instructions exist for them).

## License

MIT
