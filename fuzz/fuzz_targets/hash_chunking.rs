//! Differential fuzz of the streaming read loop.
//!
//! The digest of a byte stream must not depend on how that stream is chunked
//! into reads. This feeds arbitrary data through `hash_stream` with a
//! fuzz-chosen buffer size and compares the result against hashing the same
//! data in a single read — catching any boundary or short-read bug in the loop.

#![no_main]

use std::io::Cursor;

use libfuzzer_sys::fuzz_target;

use sha::algorithm::Algorithm;
use sha::hasher::hash_stream;

fuzz_target!(|data: &[u8]| {
    // First three bytes pick the algorithm and an awkward chunk size; the rest
    // is the payload to hash.
    if data.len() < 3 {
        return;
    }
    let algo = Algorithm::ALL[data[0] as usize % Algorithm::ALL.len()];
    let chunk = 1 + (u16::from_le_bytes([data[1], data[2]]) as usize % 4096);
    let payload = &data[3..];

    let mut small = vec![0u8; chunk];
    let chunked = hash_stream(Cursor::new(payload), algo, &mut small).expect("hash (chunked)");

    // Reference: one buffer large enough to consume the whole payload at once.
    let mut whole = vec![0u8; payload.len().max(1)];
    let single = hash_stream(Cursor::new(payload), algo, &mut whole).expect("hash (single)");

    assert_eq!(
        chunked,
        single,
        "digest changed with buffer size: algo={algo} chunk={chunk} len={}",
        payload.len()
    );
    assert_eq!(chunked.len(), algo.hex_len());
});
