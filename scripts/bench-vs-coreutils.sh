#!/usr/bin/env bash
# shellcheck disable=SC2016
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
#   NUM_FILES     number of test files                            (default 6)
#   FILE_SIZE_MB  size of each test file, in MiB                  (default 256)
#   REPS          timed runs per command (hyperfine --runs, >=2)  (default 10)
#   JOBS          parallelism for sha and xargs                   (default: nproc)
#   ALGOS         space-separated algorithms                      (default: the coreutils set)
#   SHA_BIN       path to the sha binary                          (default: target/release/sha)
#   OUT_JSON      path for the JSON results                       (default: bench-results.json)
#   WORKDIR       path to a directory for the test files          (default: auto tempdir)
#   KEEP          set to 1 to keep the test files
#
# Requires: hyperfine, python3. Only algorithms with a coreutils counterpart are
# compared (coreutils has no SHA-3); use the Criterion benchmark (`cargo bench`)
# for SHA-3 throughput.

set -euo pipefail

RED=$'\033[31m'
GREEN=$'\033[32m'
YELLOW=$'\033[33m'
BLUE=$'\033[34m'
MAGENTA=$'\033[35m'
CYAN=$'\033[36m'
BOLD=$'\033[1m'
RESET=$'\033[0m'

[[ -t 1 && -t 2 ]] || {
    RED=$''
    GREEN=$''
    YELLOW=$''
    MAGENTA=$''
    CYAN=$''
    BOLD=$''
    RESET=$''
}

declare -A LEVEL_COLOR=(
    [DEBUG]="$CYAN"
    [INFO]="$GREEN"
    [WARN]="$YELLOW"
    [ERROR]="$BOLD$RED"
    [FATAL]="$BOLD$MAGENTA"
)

NUM_FILES=6
FILE_SIZE_MB=256
REPS=10
JOBS="$(nproc 2>/dev/null || echo 4)"
ALGOS="md5 sha1 sha224 sha256 sha384 sha512"
OUT_JSON="bench-results.json"

SCRIPT_NAME="$(basename "$0")"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SHA_BIN="$ROOT/target/release/sha"

TOTAL_BYTES=$((NUM_FILES * FILE_SIZE_MB * 1024 * 1024))
TOTAL_GIB="$(awk "BEGIN { printf \"%.4f\", $TOTAL_BYTES/1073741824 }")"

print_opt() {
    local short="$1"
    local long="$2"
    local metavar="$3"
    local default="$4"
    local desc="$5"
    printf '    %b-%s%b, %b--%-10s%b%b%-8s%b %s\n' "$GREEN" "$short" "$RESET" "$CYAN" "$long" "$RESET" "$YELLOW" "$metavar" "$RESET" "$desc"
    if [[ -n "${default:-}" ]]; then
        printf '                             %bdefault:%b %s\n' "$RED" "$RESET" "$default"
    fi
}

usage() {
    cat <<EOUSAGE
${BLUE}Usage:${RESET} ${MAGENTA}$SCRIPT_NAME${RESET} [${GREEN}OPTIONS${RESET}]

${BLUE}Options:${RESET}
EOUSAGE
    print_opt w workdir DIR "auto tempdir" "Working directory for test files"
    print_opt o output OUT_JSON "$OUT_JSON" "Output JSON file for summary results"
    print_opt j jobs JOBS "$JOBS" "Total number of jobs used in the parallel runs"
    print_opt r reps REPS "$REPS" "Total number of repetitions for each run"
    print_opt x sha-bin BINARY "$SHA_BIN" "Path to the ${YELLOW}sha${RESET} binary. Will be built if missing."
    print_opt n num-files FILES "$NUM_FILES" "Total number of files"
    print_opt s file-size MIB "$FILE_SIZE_MB" "Size of each file in MiB"
    print_opt d debug "" "" "Enable debug logging"
    print_opt h help "" "" "Print this help message and quit"
}

log() {
    local level="${1^^}"
    shift
    local color="${LEVEL_COLOR[$level]}" reset="$RESET"
    case "$level" in
    DEBUG | INFO)
        printf '[%b%s%b] %s\n' "$color" "$level" "$reset" "$*"
        ;;
    WARN | ERROR | FATAL)
        printf '[%b%s%b] %s\n' "$color" "$level" "$reset" "$*" >&2
        [[ ${level} == "FATAL" ]] && {
            printf "\ntry '%s --help'\n" "$SCRIPT_NAME"
            exit 1
        }
        ;;
    *)
        log "FATAL" "Unknown log level: $level"
        ;;
    esac
}

unknown() {
    local label="$1"
    local option="$2"
    log "FATAL" "Unknown $label: $option"
}

need() {
    local opt="$1"
    local count="$2"
    ((count >= 2)) || log "FATAL" "$opt requires an argument"
}

uint() {
    local opt="$1"
    local arg="$2"
    [[ "$arg" =~ ^[0-9]+$ ]] || log "FATAL" "$opt must be a non-negative integer (got '$arg')"
}

build_sha() {
    # Build the binary if it's missing.
    if [[ ! -x "$SHA_BIN" ]]; then
        log "INFO" "Building release binary..."
        (cd "$ROOT" && cargo build --release >&2)
    fi
}

check_deps() {
    for dep in hyperfine python3 pv; do
        if ! command -v "$dep" >/dev/null 2>&1; then
            log "FATAL" "'$dep' is required but not installed"
        fi
    done
}

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

cleanup() {
    if [[ "${KEEP:-0}" == "0" ]]; then
        rm -rf "$WORKDIR"
    fi
}

setup_workdir() {
    if [[ -z "${WORKDIR:-}" ]]; then
        WORKDIR="$(mktemp -d)"
    fi
    trap cleanup EXIT
}

generate_test_files() {
    setup_workdir
    log "INFO" "Generating $NUM_FILES x ${FILE_SIZE_MB}MiB = ${TOTAL_GIB} GiB of test data in $WORKDIR ..."
    local fifo="$WORKDIR/.progress"
    mkfifo "$fifo"
    pv -l -s "$NUM_FILES" -p -t -e -b <"$fifo" >/dev/null &
    exec 9>"$fifo"

    {
        for i in $(seq 0 $((NUM_FILES - 1))); do
            printf '%s/f%03d.dat\0' "$WORKDIR" "$i"
        done
    } | xargs -0 -P "$JOBS" -I{} sh -c \
        'dd if=/dev/urandom bs=1M count="$1" of="$2" conv=notrunc >/dev/null 2>&1; printf "x\n" >> "$3"' \
        sh "$FILE_SIZE_MB" {} "$fifo"

    exec 9>&-
    wait

    log "DEBUG" "Test files generated"

    declare -a FILES=("$WORKDIR"/*.dat)

    # Warm the page cache so we compare compute, not first-touch disk reads.
    cat "${FILES[@]}" >/dev/null

    # A single shell-safe string of all file paths, reused inside the command
    # strings that hyperfine runs via `sh -c`.
    FILES_STR=$(printf '%q ' "${FILES[@]}")
}

run_benchmarks() {
    build_sha
    # Time the four commands for each algorithm with hyperfine, exporting per-algo
    # JSON. hyperfine's own live summary goes to stderr so stdout stays clean for
    # the consolidated table emitted by the python post-processor below.
    RAN_ALGOS=()
    for algo in $ALGOS; do
        tool=$(coreutils_tool "$algo")
        if [[ -z "$tool" ]] || ! command -v "$tool" >/dev/null 2>&1; then
            log "WARN" "skipping $algo: no coreutils equivalent"
            continue
        fi

        log "INFO" "Benchmarking $algo"
        printf "\n"

        hyperfine --warmup 5 --runs "$REPS" \
            -n "sha-jN" "$SHA_BIN hash $algo -j $JOBS $FILES_STR >/dev/null" \
            -n "sha-j1" "$SHA_BIN hash $algo -j 1 $FILES_STR >/dev/null" \
            -n "coreutils-1cpu" "$tool $FILES_STR >/dev/null" \
            -n "coreutils-xargs" "printf '%s\0' $FILES_STR | xargs -0 -P $JOBS -n1 $tool >/dev/null" \
            --export-json "$WORKDIR/$algo.json" 1>&2

        RAN_ALGOS+=("$algo")
    done

    if [[ ${#RAN_ALGOS[@]} -eq 0 ]]; then
        log "ERROR" "No algorithms had a coreutils counterpart; nothing to report."
        exit 0
    fi

    log "INFO" "Benchmarking complete; per-algo results written to $WORKDIR/*.json"
}

run_stats() {
    log "DEBUG" "Processing results into ${OUT_JSON}..."

    # Post-process: read every per-algo hyperfine JSON, print the GiB/s table, and
    # write the combined results (with min/max/median and metadata) to $OUT_JSON.
    "$SCRIPT_DIR"/render-results.py \
        -w "$WORKDIR" \
        -g "$TOTAL_GIB" \
        -b "$TOTAL_BYTES" \
        -o "$OUT_JSON" \
        -j "$JOBS" \
        -r "$REPS" \
        -x "$SHA_BIN" \
        -n "$NUM_FILES" \
        -s "$FILE_SIZE_MB"
}

main() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
        # normalize --opt=value -> --opt value, then reprocess
        --*=*)
            set -- "${1%%=*}" "${1#*=}" "${@:2}"
            continue
            ;;

        -w | --workdir)
            need "$1" "$#"
            WORKDIR="$2"
            shift 2
            ;;
        -o | --output)
            need "$1" "$#"
            OUT_JSON="$2"
            shift 2
            ;;
        -j | --jobs)
            need "$1" "$#"
            JOBS="$2"
            shift 2
            ;;
        -r | --reps)
            need "$1" "$#"
            REPS="$2"
            shift 2
            ;;
        -x | --sha-bin)
            need "$1" "$#"
            SHA_BIN="$(realpath "$2")"
            shift 2
            ;;
        -n | --num-files)
            need "$1" "$#"
            NUM_FILES="$2"
            shift 2
            ;;
        -s | --file-size)
            need "$1" "$#"
            FILE_SIZE_MB="$2"
            shift 2
            ;;
        -d | --debug)
            set -x
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;

        --)
            shift
            break
            ;;
        -*) unknown "option" "$1" ;;
        *) unknown "argument" "$1" ;;
        esac
    done
    (($# > 0)) && unknown "argument" "$1"

    uint --jobs "$JOBS"
    uint --reps "$REPS"
    uint --num-files "$NUM_FILES"
    uint --file-size "$FILE_SIZE_MB"

    check_deps
    generate_test_files
    run_benchmarks
    run_stats

    cat <<EONOTES

${BOLD}${BLUE}Notes:${RESET}

  ${GREEN}*${RESET} '${CYAN}spd/core${RESET}' = ${YELLOW}sha -j1 vs single-thread coreutils${RESET} (the honest per-core algorithmic comparison); '${CYAN}spd/par${RESET}' = ${YELLOW}sha -j$JOBS vs coreutils${RESET} fanned out with ${YELLOW}xargs${RESET} (the real-world parallel throughput win).

  ${GREEN}*${RESET} Per-core speed depends on the CPU:

      - With ${RED}SHA-NI${RESET} (most CPUs since ~2016), sha1/sha256 use hardware instructions; ${YELLOW}sha${RESET}, ${YELLOW}coreutils${RESET} and ${YELLOW}openssl${RESET} all use the same path and run at parity per core (${GREEN}~1.9 cycles/byte${RESET}). sha's win is parallelism, so expect ${CYAN}spd/core${RESET} near ${GREEN}1.0x${RESET} and ${CYAN}spd/par${RESET} scaling toward core count.

      - WITHOUT ${RED}SHA-NI${RESET}, RustCrypto's portable sha256 is slower per core than ${YELLOW}coreutils${RESET}/${YELLOW}openssl${RESET}, so sha's advantage comes entirely from parallelism.

  ${GREEN}*${RESET} ${CYAN}Time/GiB/s min/max/median${RESET} for every command live in $OUT_JSON.

  ${GREEN}*${RESET} ${YELLOW}coreutils${RESET} has no SHA-3; use '${YELLOW}cargo bench${RESET}' for SHA-3 throughput.
EONOTES
}

main "$@"
# vim: set ft=bash ts=4 sw=4:
