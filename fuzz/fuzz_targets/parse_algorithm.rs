//! Fuzz the algorithm-name parser.
//!
//! `Algorithm::from_str` accepts user input, so it must not panic on any string,
//! and any name it accepts must round-trip through the canonical name.

#![no_main]

use libfuzzer_sys::fuzz_target;

use sha::algorithm::Algorithm;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    if let Ok(algo) = s.parse::<Algorithm>() {
        // The canonical name must parse back to the same algorithm.
        assert_eq!(algo.name().parse::<Algorithm>(), Ok(algo));
        // Every digest length is an even, sane hex-character count.
        let len = algo.hex_len();
        assert!(len >= 32 && len <= 128 && len % 2 == 0);
    }
});
