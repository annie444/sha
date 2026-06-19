#!/usr/bin/env bash
#
# Compare the throughput of this `sha` CLI against the coreutils *sum tools on
# the same set of files. Reports best-of-N wall-clock throughput for:
#
#   * sha  (parallel, all cores)   — the tool as normally used
#   * sha  (-j1, single thread)    — fair head-to-head with coreutils
#   * coreutils <algo>sum          — single-threaded reference
#   * coreutils via xargs -P       — coreutils fanned out across cores
#
# Configuration via environment variables:
#   NUM_FILES     number of test files            (default 6)
#   FILE_SIZE_MB  size of each test file, in MiB   (default 256)
#   REPS          timed repetitions, best is kept  (default 3)
#   JOBS          parallelism for sha and xargs    (default: nproc)
#   ALGOS         space-separated algorithms       (default: the coreutils set)
#   SHA_BIN       path to the sha binary           (default: target/release/sha)
#   KEEP          set to 1 to keep the test files
#
# Only algorithms with a coreutils counterpart are compared (coreutils has no
# SHA-3); use the Criterion benchmark (`cargo bench`) for SHA-3 throughput.

set -euo pipefail

NUM_FILES=${NUM_FILES:-6}
FILE_SIZE_MB=${FILE_SIZE_MB:-256}
REPS=${REPS:-3}
JOBS=${JOBS:-$(nproc 2>/dev/null || echo 4)}
ALGOS=${ALGOS:-"md5 sha1 sha224 sha256 sha384 sha512"}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHA_BIN=${SHA_BIN:-"$ROOT/target/release/sha"}

# coreutils tool name for a given algorithm.
coreutils_tool() {
    case "$1" in
        md5) echo "md5sum" ;;
        sha1) echo "sha1sum" ;;
        sha224) echo "sha224sum" ;;
        sha256) echo "sha256sum" ;;
        sha384) echo "sha384sum" ;;
        sha512) echo "sha512sum" ;;
        *) echo "" ;;
    esac
}

# Build the binary if it's missing.
if [[ ! -x "$SHA_BIN" ]]; then
    echo "Building release binary..." >&2
    (cd "$ROOT" && cargo build --release >&2)
fi

WORKDIR="$(mktemp -d)"
cleanup() { [[ "${KEEP:-0}" == "1" ]] || rm -rf "$WORKDIR"; }
trap cleanup EXIT

TOTAL_BYTES=$(( NUM_FILES * FILE_SIZE_MB * 1024 * 1024 ))
TOTAL_GIB=$(awk "BEGIN { printf \"%.2f\", $TOTAL_BYTES/1073741824 }")

echo "Generating $NUM_FILES x ${FILE_SIZE_MB}MiB = ${TOTAL_GIB} GiB of test data in $WORKDIR ..." >&2
head -c "$(( FILE_SIZE_MB * 1024 * 1024 ))" /dev/urandom > "$WORKDIR/f000.dat"
for i in $(seq 1 $((NUM_FILES - 1))); do
    cp "$WORKDIR/f000.dat" "$(printf '%s/f%03d.dat' "$WORKDIR" "$i")"
done
FILES=("$WORKDIR"/*.dat)

# Warm the page cache so we compare compute, not first-touch disk reads.
cat "${FILES[@]}" > /dev/null

# Run "$@" REPS times (stdout discarded) and echo the best wall time in seconds.
best_time() {
    local best="" t start end
    for _ in $(seq 1 "$REPS"); do
        start=$(date +%s.%N)
        "$@" > /dev/null 2>&1
        end=$(date +%s.%N)
        t=$(awk "BEGIN { print $end - $start }")
        if [[ -z "$best" ]] || awk "BEGIN { exit !($t < $best) }"; then
            best=$t
        fi
    done
    echo "$best"
}

gibs() { awk "BEGIN { printf \"%.2f\", $TOTAL_BYTES/1073741824/$1 }"; }
speedup() { awk "BEGIN { printf \"%.2fx\", $1/$2 }"; }

# coreutils fanned out across cores with xargs.
coreutils_parallel() {
    local tool="$1"
    printf '%s\0' "${FILES[@]}" | xargs -0 -P "$JOBS" -n1 "$tool"
}

echo
printf '%-8s | %-19s | %-19s | %-19s | %-19s | %s\n' \
    "algo" "sha (-j$JOBS)" "sha (-j1)" "coreutils (1 cpu)" "coreutils (xargs)" "speedup vs 1cpu"
printf '%s\n' "---------+---------------------+---------------------+---------------------+---------------------+----------------"

for algo in $ALGOS; do
    tool=$(coreutils_tool "$algo")
    if [[ -z "$tool" ]] || ! command -v "$tool" > /dev/null 2>&1; then
        printf '%-8s | (no coreutils equivalent)\n' "$algo"
        continue
    fi

    t_par=$(best_time "$SHA_BIN" hash "$algo" -j "$JOBS" "${FILES[@]}")
    t_j1=$(best_time "$SHA_BIN" hash "$algo" -j 1 "${FILES[@]}")
    t_core=$(best_time "$tool" "${FILES[@]}")
    t_corep=$(best_time coreutils_parallel "$tool")

    printf '%-8s | %7s s %9s | %7s s %9s | %7s s %9s | %7s s %9s | %s\n' \
        "$algo" \
        "$t_par" "$(gibs "$t_par") GiB/s" \
        "$t_j1" "$(gibs "$t_j1") GiB/s" \
        "$t_core" "$(gibs "$t_core") GiB/s" \
        "$t_corep" "$(gibs "$t_corep") GiB/s" \
        "$(speedup "$t_core" "$t_par")"
done

echo
echo "Notes:"
echo " * 'speedup vs 1cpu' compares parallel sha against single-threaded coreutils."
echo " * Per-core software speed depends on the CPU:"
echo "     - With SHA-NI (most CPUs since ~2016), sha1/sha256 use hardware"
echo "       instructions and sha is far faster per core than coreutils."
echo "     - WITHOUT SHA-NI, RustCrypto's portable sha256 is slower per core than"
echo "       coreutils/openssl, so sha's advantage here comes from parallelism."
echo " * coreutils has no SHA-3; use 'cargo bench' for SHA-3 throughput."
