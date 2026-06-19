//! End-to-end tests that drive the compiled `sha` binary.
//!
//! These exercise argument parsing, output formatting, exit codes, and the
//! hash/verify round-trip exactly as a user would invoke them. `CARGO_BIN_EXE_sha`
//! is set by Cargo for integration tests and points at the built binary.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_sha");

const ALL_ALGOS: &[&str] = &[
    "md5",
    "sha1",
    "sha224",
    "sha256",
    "sha384",
    "sha512",
    "sha512_256",
    "sha512_224",
    "sha3_224",
    "sha3_256",
    "sha3_384",
    "sha3_512",
];

/// Run the binary in `dir` with `args`, returning its captured output.
fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to spawn sha binary")
}

/// Run the binary feeding `input` to stdin.
fn run_stdin(dir: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn sha binary");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input)
        .expect("failed to write stdin");
    child.wait_with_output().expect("failed to wait for sha")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}
fn code(o: &Output) -> i32 {
    o.status.code().expect("process terminated by signal")
}

/// Create a temp working directory with the given (name, contents) files.
fn workdir(files: &[(&str, &[u8])]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in files {
        fs::write(dir.path().join(name), contents).unwrap();
    }
    dir
}

#[test]
fn hash_single_file_known_vector() {
    let dir = workdir(&[("f.txt", b"abc")]);
    let out = run(dir.path(), &["hash", "256", "f.txt"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  f.txt\n"
    );
}

#[test]
fn output_uses_two_space_separator() {
    let dir = workdir(&[("f.txt", b"abc")]);
    let out = run(dir.path(), &["hash", "256", "f.txt"]);
    let line = stdout(&out);
    let (hex, rest) = line.split_once(' ').unwrap();
    assert_eq!(hex.len(), 64);
    assert!(
        rest.starts_with(" f.txt"),
        "expected two-space sep: {line:?}"
    );
}

#[test]
fn empty_file_hashes_to_empty_digest() {
    let dir = workdir(&[("empty", b"")]);
    let out = run(dir.path(), &["hash", "256", "empty"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out)
        .starts_with("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
}

#[test]
fn hash_preserves_argument_order() {
    let dir = workdir(&[("a", b"aaaa"), ("b", b"bbbb"), ("c", b"cccc")]);
    // Pass files out of alphabetical order to prove output follows args.
    let out = run(dir.path(), &["hash", "256", "c", "a", "b"]);
    assert_eq!(code(&out), 0);
    let text = stdout(&out);
    let names: Vec<&str> = text
        .lines()
        .map(|l| l.rsplit("  ").next().unwrap())
        .collect();
    assert_eq!(names, vec!["c", "a", "b"]);
}

#[test]
fn hash_to_output_file() {
    let dir = workdir(&[("f.txt", b"abc")]);
    let out = run(dir.path(), &["hash", "256", "-o", "SUMS", "f.txt"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).is_empty(), "stdout should be empty with -o");
    let written = fs::read_to_string(dir.path().join("SUMS")).unwrap();
    assert!(written.contains("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
}

#[test]
fn hash_missing_file_reports_error_but_hashes_the_rest() {
    let dir = workdir(&[("present", b"abc")]);
    let out = run(dir.path(), &["hash", "256", "missing", "present"]);
    assert_eq!(code(&out), 1);
    // The good file still appears on stdout; the bad one is on stderr.
    assert!(stdout(&out).contains("present"));
    assert!(stderr(&out).to_lowercase().contains("missing"));
}

#[test]
fn unknown_algorithm_is_a_usage_error() {
    let dir = workdir(&[("f", b"x")]);
    let out = run(dir.path(), &["hash", "md4", "f"]);
    assert_eq!(code(&out), 2, "clap usage errors exit 2");
}

#[test]
fn options_small_buffer_and_jobs_still_correct() {
    // A file larger than the tiny buffer, hashed with several jobs.
    let data = vec![0x5au8; 200_000];
    let dir = workdir(&[("big", &data)]);
    let small = run(dir.path(), &["hash", "256", "-b", "1024", "-j", "2", "big"]);
    let normal = run(dir.path(), &["hash", "256", "big"]);
    assert_eq!(code(&small), 0);
    assert_eq!(stdout(&small), stdout(&normal));
}

#[test]
fn verify_all_ok() {
    let dir = workdir(&[("a", b"aaaa"), ("b", b"bbbb")]);
    let sums = run(dir.path(), &["hash", "256", "a", "b"]);
    fs::write(dir.path().join("SUMS"), &sums.stdout).unwrap();
    let out = run(dir.path(), &["verify", "256", "SUMS"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("a: OK"));
    assert!(stdout(&out).contains("b: OK"));
}

#[test]
fn verify_detects_tampering() {
    let dir = workdir(&[("a", b"aaaa")]);
    let sums = run(dir.path(), &["hash", "256", "a"]);
    fs::write(dir.path().join("SUMS"), &sums.stdout).unwrap();
    // Change the file after recording its checksum.
    fs::write(dir.path().join("a"), b"tampered").unwrap();
    let out = run(dir.path(), &["verify", "256", "SUMS"]);
    assert_eq!(code(&out), 1);
    assert!(stdout(&out).contains("a: FAILED"));
    assert!(stderr(&out).contains("did NOT match"));
}

#[test]
fn verify_missing_listed_file() {
    let dir = workdir(&[]);
    fs::write(
        dir.path().join("SUMS"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  ghost\n",
    )
    .unwrap();
    let out = run(dir.path(), &["verify", "256", "SUMS"]);
    assert_eq!(code(&out), 1);
    assert!(stdout(&out).contains("ghost: FAILED"));
}

#[test]
fn verify_wrong_algorithm_length_is_reported() {
    let dir = workdir(&[("a", b"abc")]);
    // sha256 digest verified as sha512 -> length mismatch.
    let sums = run(dir.path(), &["hash", "256", "a"]);
    fs::write(dir.path().join("SUMS"), &sums.stdout).unwrap();
    let out = run(dir.path(), &["verify", "512", "SUMS"]);
    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("does not match"),
        "stderr: {}",
        stderr(&out)
    );
}

#[test]
fn verify_status_is_silent() {
    let dir = workdir(&[("a", b"aaaa")]);
    let sums = run(dir.path(), &["hash", "256", "a"]);
    fs::write(dir.path().join("SUMS"), &sums.stdout).unwrap();
    let out = run(dir.path(), &["verify", "256", "--status", "SUMS"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).is_empty());
    assert!(stderr(&out).is_empty());
}

#[test]
fn verify_quiet_shows_only_failures() {
    let dir = workdir(&[("good", b"g"), ("bad", b"b")]);
    let sums = run(dir.path(), &["hash", "256", "good", "bad"]);
    fs::write(dir.path().join("SUMS"), &sums.stdout).unwrap();
    fs::write(dir.path().join("bad"), b"changed").unwrap();
    let out = run(dir.path(), &["verify", "256", "--quiet", "SUMS"]);
    assert_eq!(code(&out), 1);
    let s = stdout(&out);
    assert!(!s.contains("good: OK"), "quiet should hide OK lines: {s:?}");
    assert!(s.contains("bad: FAILED"));
}

#[test]
fn verify_from_stdin() {
    let dir = workdir(&[("a", b"aaaa")]);
    let sums = run(dir.path(), &["hash", "256", "a"]);
    let out = run_stdin(dir.path(), &["verify", "256", "-"], &sums.stdout);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("a: OK"));
}

#[test]
fn verify_multiple_checksum_files() {
    let dir = workdir(&[("a", b"aaaa"), ("b", b"bbbb")]);
    let sa = run(dir.path(), &["hash", "256", "a"]);
    let sb = run(dir.path(), &["hash", "256", "b"]);
    fs::write(dir.path().join("A.sums"), &sa.stdout).unwrap();
    fs::write(dir.path().join("B.sums"), &sb.stdout).unwrap();
    let out = run(dir.path(), &["verify", "256", "A.sums", "B.sums"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("a: OK") && stdout(&out).contains("b: OK"));
}

#[test]
fn verify_skips_blank_and_comment_lines() {
    let dir = workdir(&[("a", b"aaaa")]);
    let sums = run(dir.path(), &["hash", "256", "a"]);
    let line = stdout(&sums);
    let manifest = format!("# a comment\n\n{line}\n   \n");
    fs::write(dir.path().join("SUMS"), manifest).unwrap();
    let out = run(dir.path(), &["verify", "256", "SUMS"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("a: OK"));
}

#[test]
fn verify_binary_marker_separator() {
    let dir = workdir(&[("a", b"abc")]);
    // coreutils binary mode writes "<hex> *name".
    let manifest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad *a\n";
    fs::write(dir.path().join("SUMS"), manifest).unwrap();
    let out = run(dir.path(), &["verify", "256", "SUMS"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("a: OK"));
}

#[test]
fn roundtrip_every_algorithm() {
    let dir = workdir(&[("data", b"the quick brown fox")]);
    for algo in ALL_ALGOS {
        let sums = run(dir.path(), &["hash", algo, "data"]);
        assert_eq!(code(&sums), 0, "hash {algo}: {}", stderr(&sums));
        let sumfile = format!("{algo}.sums");
        fs::write(dir.path().join(&sumfile), &sums.stdout).unwrap();
        let out = run(dir.path(), &["verify", algo, &sumfile]);
        assert_eq!(code(&out), 0, "verify {algo}: {}", stderr(&out));
        assert!(
            stdout(&out).contains("data: OK"),
            "verify {algo} stdout: {}",
            stdout(&out)
        );
    }
}

#[test]
fn bare_and_qualified_algorithm_spellings_agree() {
    let dir = workdir(&[("f", b"abc")]);
    let bare = run(dir.path(), &["hash", "256", "f"]);
    let full = run(dir.path(), &["hash", "sha256", "f"]);
    assert_eq!(stdout(&bare), stdout(&full));

    let sha3a = run(dir.path(), &["hash", "sha3-256", "f"]);
    let sha3b = run(dir.path(), &["hash", "sha3_256", "f"]);
    assert_eq!(stdout(&sha3a), stdout(&sha3b));
    // SHA3-256 must differ from SHA-256 despite identical digest length.
    assert_ne!(stdout(&sha3a), stdout(&bare));
}
