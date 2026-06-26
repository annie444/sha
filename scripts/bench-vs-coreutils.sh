#!/usr/bin/env bash
#
# Compare the throughput of this `sha` CLI against the coreutils *sum tools on
# the same set of files, with run-to-run statistics. Timing is driven by
# hyperfine (mean ± stddev, min/max/median) so the speedup ratios carry
# propagated uncertainty rather than a single best-of-N point estimate.
#
# For each algorithm four commands are timed:
#
#   * sha  (parallel, all cores)   — the tool as normally used
#   * sha  (-j1, single thread)    — fair head-to-head with coreutils
#   * coreutils <algo>sum          — single-threaded reference
#   * coreutils via xargs -P       — coreutils fanned out across cores
#
# Two speedups are reported, both with uncertainty:
#   * per-core  : sha -j1  vs coreutils (1 cpu)   — the honest algorithmic win
#   * parallel  : sha -jN  vs coreutils (xargs)   — the real-world throughput win
#
# A human-readable GiB/s table goes to stdout; full per-run statistics (time and
# GiB/s min/max/median, raw samples) plus metadata are written to a JSON file.
#
# Configuration via environment variables:
#   NUM_FILES     number of test files            (default 6)
#   FILE_SIZE_MB  size of each test file, in MiB   (default 256)
#   REPS          timed runs per command (hyperfine --runs, >=2)  (default 10)
#   JOBS          parallelism for sha and xargs    (default: nproc)
#   ALGOS         space-separated algorithms       (default: the coreutils set)
#   SHA_BIN       path to the sha binary           (default: target/release/sha)
#   OUT_JSON      path for the JSON results        (default: bench-results.json)
#   KEEP          set to 1 to keep the test files
#
# Requires: hyperfine, python3. Only algorithms with a coreutils counterpart are
# compared (coreutils has no SHA-3); use the Criterion benchmark (`cargo bench`)
# for SHA-3 throughput.

set -euo pipefail

NUM_FILES=${NUM_FILES:-6}
FILE_SIZE_MB=${FILE_SIZE_MB:-256}
REPS=${REPS:-10}
JOBS=${JOBS:-$(nproc 2>/dev/null || echo 4)}
ALGOS=${ALGOS:-"md5 sha1 sha224 sha256 sha384 sha512"}
OUT_JSON=${OUT_JSON:-bench-results.json}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHA_BIN=${SHA_BIN:-"$ROOT/target/release/sha"}

for dep in hyperfine python3; do
    if ! command -v "$dep" >/dev/null 2>&1; then
        echo "error: '$dep' is required but not installed" >&2
        exit 1
    fi
done

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

if [[ -z "${WORKDIR:-}" ]]; then
    WORKDIR="$(mktemp -d)"
fi
cleanup() { [[ "${KEEP:-0}" == "1" ]] || rm -rf "$WORKDIR"; }
trap cleanup EXIT

TOTAL_BYTES=$((NUM_FILES * FILE_SIZE_MB * 1024 * 1024))
TOTAL_GIB=$(awk "BEGIN { printf \"%.4f\", $TOTAL_BYTES/1073741824 }")

echo "Generating $NUM_FILES x ${FILE_SIZE_MB}MiB = ${TOTAL_GIB} GiB of test data in $WORKDIR ..." >&2
head -c "$((FILE_SIZE_MB * 1024 * 1024))" /dev/urandom >"$WORKDIR/f000.dat"
for i in $(seq 1 $((NUM_FILES - 1))); do
    cp "$WORKDIR/f000.dat" "$(printf '%s/f%03d.dat' "$WORKDIR" "$i")"
done
FILES=("$WORKDIR"/*.dat)

# Warm the page cache so we compare compute, not first-touch disk reads.
cat "${FILES[@]}" >/dev/null

# A single shell-safe string of all file paths, reused inside the command
# strings that hyperfine runs via `sh -c`.
FILES_STR=$(printf '%q ' "${FILES[@]}")

# Time the four commands for each algorithm with hyperfine, exporting per-algo
# JSON. hyperfine's own live summary goes to stderr so stdout stays clean for
# the consolidated table emitted by the python post-processor below.
RAN_ALGOS=()
for algo in $ALGOS; do
    tool=$(coreutils_tool "$algo")
    if [[ -z "$tool" ]] || ! command -v "$tool" >/dev/null 2>&1; then
        echo "skipping $algo: no coreutils equivalent" >&2
        continue
    fi

    hyperfine --warmup 5 --runs "$REPS" \
        -n "sha-jN" "$SHA_BIN hash $algo -j $JOBS $FILES_STR >/dev/null" \
        -n "sha-j1" "$SHA_BIN hash $algo -j 1 $FILES_STR >/dev/null" \
        -n "coreutils-1cpu" "$tool $FILES_STR >/dev/null" \
        -n "coreutils-xargs" "printf '%s\0' $FILES_STR | xargs -0 -P $JOBS -n1 $tool >/dev/null" \
        --export-json "$WORKDIR/$algo.json" 1>&2

    RAN_ALGOS+=("$algo")
done

if [[ ${#RAN_ALGOS[@]} -eq 0 ]]; then
    echo "No algorithms had a coreutils counterpart; nothing to report." >&2
    exit 0
fi

# Post-process: read every per-algo hyperfine JSON, print the GiB/s table, and
# write the combined results (with min/max/median and metadata) to $OUT_JSON.
WORKDIR="$WORKDIR" TOTAL_GIB="$TOTAL_GIB" TOTAL_BYTES="$TOTAL_BYTES" \
    OUT_JSON="$OUT_JSON" JOBS="$JOBS" REPS="$REPS" NUM_FILES="$NUM_FILES" \
    FILE_SIZE_MB="$FILE_SIZE_MB" SHA_BIN="$SHA_BIN" \
    python3 - "${RAN_ALGOS[@]}" <<'PY'
import datetime
import json
import math
import os
import sys

workdir = os.environ["WORKDIR"]
total_gib = float(os.environ["TOTAL_GIB"])
total_bytes = int(os.environ["TOTAL_BYTES"])
out_json = os.environ["OUT_JSON"]
jobs = int(os.environ["JOBS"])
reps = int(os.environ["REPS"])
algos = sys.argv[1:]

NAMES = ["sha-jN", "sha-j1", "coreutils-1cpu", "coreutils-xargs"]


def cmd_stats(res):
    """Per-command time + throughput statistics from one hyperfine result."""
    mean = res["mean"]
    sd = res.get("stddev") or 0.0
    rel = sd / mean if mean else 0.0
    g = total_gib / mean
    return {
        "time_mean": mean,
        "time_stddev": sd,
        "time_min": res["min"],
        "time_max": res["max"],
        "time_median": res["median"],
        "gibs_mean": g,
        "gibs_stddev": g * rel,
        # min GiB/s <-> slowest run, max GiB/s <-> fastest run
        "gibs_min": total_gib / res["max"],
        "gibs_max": total_gib / res["min"],
        "gibs_median": total_gib / res["median"],
        "times": res.get("times", []),
        "_rel": rel,
    }


def speedup(core, sha):
    """Throughput speedup of sha over coreutils = t_core / t_sha, with the
    standard ratio error propagation sigma_S/S = sqrt((s_c/t_c)^2+(s_s/t_s)^2)."""
    s = core["time_mean"] / sha["time_mean"]
    rel = math.hypot(core["_rel"], sha["_rel"])
    return s, s * rel


def cell(mean, sd):
    return f"{mean:.2f}±{sd:.2f}"


def spd(pair):
    return f"{pair[0]:.2f}±{pair[1]:.2f}x"


cols = [
    ("algo", 8),
    (f"sha -j{jobs}", 14),
    ("sha -j1", 14),
    ("core 1cpu", 14),
    ("core xargs", 14),
    ("spd/core", 13),
    ("spd/par", 13),
]


def render(values):
    return " | ".join(f"{v:<{w}}" for v, (_, w) in zip(values, cols))


results = {
    "metadata": {
        "timestamp_utc": datetime.datetime.now(datetime.UTC).isoformat(),
        "sha_bin": os.environ["SHA_BIN"],
        "num_files": int(os.environ["NUM_FILES"]),
        "file_size_mb": int(os.environ["FILE_SIZE_MB"]),
        "total_bytes": total_bytes,
        "total_gib": total_gib,
        "jobs": jobs,
        "reps": reps,
    },
    "algorithms": {},
}

print()
print("All throughput figures are mean±σ in GiB/s over "
      f"{reps} runs; speedups carry propagated uncertainty.")
print()
print(render([c for c, _ in cols]))
print("-+-".join("-" * w for _, w in cols))

for algo in algos:
    with open(os.path.join(workdir, f"{algo}.json")) as fh:
        data = json.load(fh)
    by_name = {r["command"]: cmd_stats(r) for r in data["results"]}

    jn = by_name["sha-jN"]
    j1 = by_name["sha-j1"]
    c1 = by_name["coreutils-1cpu"]
    cx = by_name["coreutils-xargs"]

    spd_core = speedup(c1, j1)   # honest per-core: sha -j1 vs coreutils 1cpu
    spd_par = speedup(cx, jn)    # real-world: sha -jN vs coreutils xargs

    print(render([
        algo,
        cell(jn["gibs_mean"], jn["gibs_stddev"]),
        cell(j1["gibs_mean"], j1["gibs_stddev"]),
        cell(c1["gibs_mean"], c1["gibs_stddev"]),
        cell(cx["gibs_mean"], cx["gibs_stddev"]),
        spd(spd_core),
        spd(spd_par),
    ]))

    entry = {name: {k: v for k, v in by_name[name].items() if k != "_rel"}
             for name in NAMES}
    entry["speedup_per_core"] = {"value": spd_core[0], "stddev": spd_core[1],
                                 "of": "sha-j1 vs coreutils-1cpu"}
    entry["speedup_parallel"] = {"value": spd_par[0], "stddev": spd_par[1],
                                 "of": "sha-jN vs coreutils-xargs"}
    results["algorithms"][algo] = entry

with open(out_json, "w") as fh:
    json.dump(results, fh, indent=2)
    fh.write("\n")

print()
print(f"Full per-run statistics written to {out_json}")
PY

echo
echo "Notes:"
echo " * 'spd/core' = sha -j1 vs single-thread coreutils (the honest per-core"
echo "   algorithmic comparison); 'spd/par' = sha -j$JOBS vs coreutils fanned"
echo "   out with xargs (the real-world parallel throughput win)."
echo " * Per-core speed depends on the CPU:"
echo "     - With SHA-NI (most CPUs since ~2016), sha1/sha256 use hardware"
echo "       instructions; sha, coreutils and openssl all use the same path and"
echo "       run at parity per core (~1.9 cycles/byte). sha's win is parallelism,"
echo "       so expect spd/core near 1.0x and spd/par scaling toward core count."
echo "     - WITHOUT SHA-NI, RustCrypto's portable sha256 is slower per core than"
echo "       coreutils/openssl, so sha's advantage comes entirely from parallelism."
echo " * Time/GiB/s min/max/median for every command live in $OUT_JSON."
echo " * coreutils has no SHA-3; use 'cargo bench' for SHA-3 throughput."
