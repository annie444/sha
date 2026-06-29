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
# With -p/--parallel, the two single-core commands (sha -j1, coreutils 1 cpu)
# are skipped: only the parallel pair is timed and only the parallel speedup is
# reported.
#
# A human-readable GiB/s table goes to stdout; full per-run statistics (time and
# GiB/s min/max/median, raw samples) plus metadata are written to a JSON file.
#
# Requires: hyperfine, python3, pv. Only algorithms with a coreutils counterpart
# are compared (coreutils has no SHA-3); use the Criterion benchmark
# (`cargo bench`) for SHA-3 throughput.

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
    BLUE=$''
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
OUT_JSON="bench-results.json"
PARALLEL_ONLY=0
SLEEP_TIME=0.2

declare -a ALGOS=()

SCRIPT_NAME="$(basename "$0")"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SHA_BIN="$ROOT/target/release/sha"
PAGE_CACHE_LOGGED=0

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
${BLUE}Usage:${RESET} ${MAGENTA}$SCRIPT_NAME${RESET} [${GREEN}OPTIONS${RESET}] [${GREEN}ALGORITHM...${RESET}]

${BLUE}Arguments:${RESET}
    ${GREEN}ALGORITHM${RESET}                Algorithm(s) to benchmark. If none are specified, all coreutils-supported algorithms are run
                             ${RED}default:${RESET} "md5 sha1 sha224 sha256 sha384 sha512"

${BLUE}Options:${RESET}
EOUSAGE
    print_opt w workdir DIR "auto tempdir" "Working directory for test files"
    print_opt o output OUT_JSON "$OUT_JSON" "Output JSON file for summary results"
    print_opt j jobs JOBS "$JOBS" "Total number of jobs used in the parallel runs"
    print_opt r reps REPS "$REPS" "Total number of repetitions for each run"
    print_opt x sha-bin BINARY "$SHA_BIN" "Path to the ${YELLOW}sha${RESET} binary. Will be built if missing."
    print_opt n num-files FILES "$NUM_FILES" "Total number of files"
    print_opt s file-size MIB "$FILE_SIZE_MB" "Size of each file in MiB"
    print_opt p parallel "" "" "Only run parallel benchmarks (${YELLOW}sha-jN${RESET}, ${YELLOW}coreutils-xargs${RESET}); skip single-core runs and ${CYAN}spd/core${RESET}"
    print_opt S sleep SECS "$SLEEP_TIME" "Sleep time between runs (to let the page cache drop). Only applies when running as root on Linux or macOS."
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
    log "INFO" "Benchmarking algorithms: ${ALGOS[*]}"
    for algo in "${ALGOS[@]}"; do
        tool=$(coreutils_tool "$algo")
        if [[ -z "$tool" ]] || ! command -v "$tool" >/dev/null 2>&1; then
            log "WARN" "skipping $algo: no coreutils equivalent"
            continue
        fi

        printf '\n'
        log "INFO" "Benchmarking $algo"

        # Always time the two parallel commands; add the single-core pair unless
        # parallel-only mode skips them.
        local -a hf_cmds=(
            -n "sha-jN" "$SHA_BIN hash $algo -j $JOBS $FILES_STR >/dev/null"
        )
        if [[ "$PARALLEL_ONLY" == "0" ]]; then
            hf_cmds+=(
                -n "sha-j1" "$SHA_BIN hash $algo -j 1 $FILES_STR >/dev/null"
                -n "coreutils-1cpu" "$tool $FILES_STR >/dev/null"
            )
        fi
        hf_cmds+=(
            -n "coreutils-xargs" "printf '%s\0' $FILES_STR | xargs -0 -P $JOBS -n1 $tool >/dev/null"
        )

        if ((UID == 0 || EUID == 0)); then
            case "$(uname)" in
            Linux)
                if [[ -f /proc/sys/vm/drop_caches ]]; then
                    if ((PAGE_CACHE_LOGGED == 0)); then
                        log "INFO" "Running as root on Linux; will drop page cache between runs"
                        log "DEBUG" "Sleeping for $SLEEP_TIME seconds between runs to let the page cache drop"
                        PAGE_CACHE_LOGGED=$((PAGE_CACHE_LOGGED + 1))
                    fi
                    sync
                    printf '3\n' >/proc/sys/vm/drop_caches
                    sleep "$SLEEP_TIME"
                    hf_cmds+=(--prepare "sync; printf '3\n' >/proc/sys/vm/drop_caches; sleep $SLEEP_TIME")
                else
                    log "WARN" "Running as root on Linux but /proc/sys/vm/drop_caches is missing; will not drop page cache between runs"
                fi
                ;;
            Darwin)
                if ((PAGE_CACHE_LOGGED == 0)); then
                    log "INFO" "Running as root on macOS; will drop page cache between runs"
                    log "DEBUG" "Sleeping for $SLEEP_TIME seconds between runs to let the page cache drop"
                    PAGE_CACHE_LOGGED=$((PAGE_CACHE_LOGGED + 1))
                fi
                sync
                purge
                sleep "$SLEEP_TIME"
                hf_cmds+=(--prepare "sync; purge; sleep $SLEEP_TIME")
                ;;
            *)
                log "WARN" "Running as root on unknown OS; will not drop page cache between runs"
                ;;
            esac
        fi

        hyperfine --warmup 5 --runs "$REPS" "${hf_cmds[@]}" \
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
    local -a py_args=(
        -w "$WORKDIR"
        -g "$TOTAL_GIB"
        -b "$TOTAL_BYTES"
        -o "$OUT_JSON"
        -j "$JOBS"
        -r "$REPS"
        -x "$SHA_BIN"
        -n "$NUM_FILES"
        -s "$FILE_SIZE_MB"
    )
    [[ "$PARALLEL_ONLY" == "1" ]] && py_args+=(-p)
    "$SCRIPT_DIR"/render-results.py "${py_args[@]}" "${RAN_ALGOS[@]}"
}

print_header() {
    printf '\n%b%b%s%b\n\n' "$BOLD" "$BLUE" "$*" "$RESET"
}

print_bullet() {
    printf '  %b*%b %s\n\n' \
        "$GREEN" "$RESET" "$*"
}

print_sub_bullet() {
    printf '      %b-%b %s\n\n' \
        "$GREEN" "$RESET" "$*"
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
        -p | --parallel)
            PARALLEL_ONLY=1
            shift
            ;;
        -S | --sleep)
            need "$1" "$#"
            SLEEP_TIME="$2"
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
        md5 | sha1 | sha224 | sha256 | sha384 | sha512)
            ALGOS+=("$1")
            shift
            ;;
        *) unknown "argument" "$1" ;;
        esac
    done
    (($# > 0)) && unknown "argument" "$1"

    if ((${#ALGOS[@]} == 0)); then
        ALGOS+=("md5" "sha1" "sha224" "sha256" "sha384" "sha512")
    fi

    uint --jobs "$JOBS"
    uint --reps "$REPS"
    uint --num-files "$NUM_FILES"
    uint --file-size "$FILE_SIZE_MB"

    check_deps
    generate_test_files
    run_benchmarks
    run_stats

    print_header "Notes:"

    if [[ "$PARALLEL_ONLY" == "0" ]]; then
        print_bullet "'${CYAN}spd/core${RESET}' = ${YELLOW}sha -j1 vs single-thread coreutils${RESET} (the honest per-core algorithmic comparison); '${CYAN}spd/par${RESET}' = ${YELLOW}sha -j$JOBS vs coreutils${RESET} fanned out with ${YELLOW}xargs${RESET} (the real-world parallel throughput win)."
        print_bullet "Per-core speed depends on the CPU:"
        print_sub_bullet "With ${RED}SHA-NI${RESET} (most CPUs since ~2016), sha1/sha256 use hardware instructions; ${YELLOW}sha${RESET}, ${YELLOW}coreutils${RESET} and ${YELLOW}openssl${RESET} all use the same path and run at parity per core (${GREEN}~1.9 cycles/byte${RESET}). sha's win is parallelism, so expect ${CYAN}spd/core${RESET} near ${GREEN}1.0x${RESET} and ${CYAN}spd/par${RESET} scaling toward core count."
        print_sub_bullet "Without ${RED}SHA-NI${RESET}, RustCrypto's portable sha256 is slower per core than ${YELLOW}coreutils${RESET}/${YELLOW}openssl${RESET}, so sha's advantage comes entirely from parallelism."
    else
        print_bullet "Parallel-only mode: '${CYAN}spd/core${RESET}' not computed (the single-core runs were skipped); '${CYAN}spd/par${RESET}' = ${YELLOW}sha -j${JOBS} vs coreutils${RESET} fanned out with ${YELLOW}xargs${RESET}."
    fi
    print_bullet "${CYAN}Time/GiB/s min/max/median${RESET} for every command live in $OUT_JSON."
    print_bullet "${YELLOW}coreutils${RESET} has no SHA-3; use '${YELLOW}cargo bench${RESET}' for SHA-3 throughput."
}

main "$@"
# vim: set ft=bash ts=4 sw=4:
