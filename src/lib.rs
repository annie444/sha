//! Core hashing library shared by the `sha` binary and its benchmarks.
//!
//! The reusable pieces — algorithm selection and the streaming file hasher —
//! live here so they can be exercised directly by Criterion benchmarks and any
//! future consumers. The command-line plumbing (`cli`, `commands`) stays in the
//! binary crate.

pub mod algorithm;
pub mod hasher;
