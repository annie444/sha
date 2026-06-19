//! Fuzz the untrusted checksum-file line parser.
//!
//! Checksum files come from arbitrary sources, so `parse_line` must never panic
//! on any input, and any line it accepts must satisfy the parser's invariants.

#![no_main]

use libfuzzer_sys::fuzz_target;

use sha::algorithm::Algorithm;
use sha::checksum::parse_line;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    for line in text.lines() {
        for algo in Algorithm::ALL {
            match parse_line(line, algo) {
                Some(Ok(entry)) => {
                    // A successful parse must uphold every invariant the rest of
                    // the program relies on.
                    assert_eq!(entry.algo, algo);
                    assert_eq!(entry.expected.len(), algo.hex_len());
                    assert!(entry.expected.bytes().all(|b| b.is_ascii_hexdigit()));
                    assert!(
                        entry.expected.bytes().all(|b| !b.is_ascii_uppercase()),
                        "digest must be normalized to lowercase"
                    );
                    assert!(!entry.path.as_os_str().is_empty());
                }
                // Errors and skips are both fine; we only care that it returns.
                Some(Err(_)) | None => {}
            }
        }
    }
});
