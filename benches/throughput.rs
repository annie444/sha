//! Single-stream hashing throughput benchmark.
//!
//! For each algorithm this measures `hash_file` over one cache-hot temporary
//! file, with Criterion configured to report bytes/second so the numbers read
//! directly as throughput (e.g. GiB/s). This isolates raw per-core hashing
//! speed — the fundamental quantity the parallel CLI multiplies across files.
//!
//! The file size defaults to 16 MiB and can be overridden:
//!
//! ```sh
//! SHA_BENCH_SIZE=$((64*1024*1024)) cargo bench
//! ```

use std::io::Write;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use sha::algorithm::Algorithm;
use sha::hasher::{hash_file, DEFAULT_BUFFER_SIZE};

fn bench_throughput(c: &mut Criterion) {
    let size: usize = std::env::var("SHA_BENCH_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(16 * 1024 * 1024);

    // A pseudo-random but cheap-to-generate payload, written once and reused.
    let data: Vec<u8> = (0..size)
        .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
        .collect();
    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&data).expect("write temp file");
    tmp.flush().expect("flush temp file");
    let path = tmp.path().to_path_buf();

    let mut buf = vec![0u8; DEFAULT_BUFFER_SIZE];

    let mut group = c.benchmark_group("hash_file");
    group.throughput(Throughput::Bytes(size as u64));
    // Hashing a multi-MiB file per iteration is relatively expensive, so keep
    // the sample count modest to bound total runtime.
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    for algo in Algorithm::ALL {
        group.bench_function(BenchmarkId::from_parameter(algo.name()), |b| {
            b.iter(|| hash_file(&path, algo, &mut buf).expect("hash"));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_throughput);
criterion_main!(benches);
