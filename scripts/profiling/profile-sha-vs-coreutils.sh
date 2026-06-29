#!/usr/bin/env bash
#
# profile-sha-vs-coreutils.sh — find out WHY `sha` is slower than `sha256sum`.
#
# Runs both programs under two SystemTap scripts and reports the results:
#
#   io-profile.stp   answers "is it I/O?"  (read() count / sizes / time-in-read)
#   cpu-profile.stp  answers "where in compute, and which SHA-256 backend?"
#
# `sha` is profiled at -j1 on a single cache-warm file: the honest per-core
# comparison. Parallelism is a separate axis already covered by
# scripts/bench-vs-coreutils.sh.
#
# ponytail: single-file / -j1 scope on purpose — the per-core question. Add -jN
# profiling only if the per-core profile shows parity but real-world is slow.
#
# Requires Linux + SystemTap (`stap`), run as root or a member of `stapdev`/
# `stapusr`. SystemTap is Linux-only; this will refuse to run on macOS.
#
# Configuration via environment variables:
#   SHA_BIN       path to the sha binary       (default: target/release/sha)
#   FILE_SIZE_MB  test file size, MiB          (default 1024)
#   WORKDIR       dir for the test file        (default: auto tempdir)
#   KEEP          set to 1 to keep the file
#   NO_BUILD      set to 1 to skip the cargo build

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
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    MAGENTA=''
    CYAN=''
    BOLD=''
    RESET=''
}

declare -A LEVEL_COLOR=([DEBUG]="$CYAN" [INFO]="$GREEN" [WARN]="$YELLOW" [ERROR]="$BOLD$RED" [FATAL]="$BOLD$MAGENTA")

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SHA_BIN="${SHA_BIN:-$ROOT/target/release/sha}"
FILE_SIZE_MB="${FILE_SIZE_MB:-1024}"
IO_STP="$SCRIPT_DIR/io-profile.stp"
CPU_STP="$SCRIPT_DIR/cpu-profile.stp"

log() {
    local level="${1^^}"
    shift
    local color="${LEVEL_COLOR[$level]}"
    case "$level" in
    DEBUG | INFO) printf '[%b%s%b] %s\n' "$color" "$level" "$RESET" "$*" ;;
    WARN | ERROR | FATAL)
        printf '[%b%s%b] %s\n' "$color" "$level" "$RESET" "$*" >&2
        [[ "$level" == FATAL ]] && exit 1
        ;;
    esac
}

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
    cat <<EOF
${BLUE}Usage:${RESET} ${MAGENTA}$(basename "$0")${RESET} [-h]

Profiles '${YELLOW}sha hash/verify sha256${RESET}' against '${YELLOW}sha256sum${RESET}' with two SystemTap
scripts to identify any slowdowns/performance hits between the two.

${BLUE}Options:${RESET}
EOF
    print_opt s sha-bin PATH "$SHA_BIN" "Path to the sha binary to profile."
    print_opt f file-size-mib MIB "$FILE_SIZE_MB" "Size of the test file in MiB (default: 1024)."
    print_opt w workdir DIR "auto tempdir" "Directory to create the test file in (default: auto tempdir)."
    print_opt k keep "" "" "Keep the test file (default: auto tempdir is deleted)."
    print_opt n no-build "" "" "Skip building sha (default: build sha)."
    print_opt d debug "" "" "Enable debug logging"
    print_opt h help "" "" "Print this help message and quit"
}

[[ "${1:-}" == "-h" || "${1:-}" == "--help" ]] && {
    usage
    exit 0
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

# ---- pre-flight ------------------------------------------------------------

preflight() {
    [[ "$(uname -s)" == "Linux" ]] || log FATAL "SystemTap is Linux-only; this host is $(uname -s)."
    command -v stap >/dev/null || log FATAL "'stap' not found — install SystemTap."
    command -v sha256sum >/dev/null || log FATAL "'sha256sum' (coreutils) not found."
    [[ -r "$IO_STP" && -r "$CPU_STP" ]] || log FATAL "missing .stp scripts next to this script."

    if [[ "$(id -u)" != 0 ]] && ! id -nG | grep -qwE 'stapdev|stapusr'; then
        log WARN "not root and not in stapdev/stapusr — stap may need 'sudo $0'."
    fi

    # Cheap tell #1: does coreutils hash via OpenSSL (which has a SHA-NI path)?
    log INFO "coreutils sha256sum linkage:"
    if ldd "$(command -v sha256sum)" | grep -iE 'crypto|ssl'; then
        log INFO "  ^ links libcrypto: coreutils likely uses OpenSSL's SHA-NI path."
    else
        log INFO "  no libcrypto link: coreutils uses its own builtin transform."
    fi
}

build_sha() {
    [[ "${NO_BUILD:-0}" == 1 ]] && {
        log INFO "NO_BUILD set; using existing $SHA_BIN"
        return
    }
    log INFO "Building sha with symbols (optimized, un-stripped) for usymname()..."
    # Keep release optimizations so timing is representative, but retain symbols.
    (cd "$ROOT" &&
        CARGO_PROFILE_RELEASE_STRIP=false \
            CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
            cargo build --release >&2)
}

check_symbols() {
    [[ -x "$SHA_BIN" ]] || log FATAL "sha binary not found at $SHA_BIN"
    # Cheap tell #2: did the SHA-256 compress symbol survive LTO/monomorphization?
    if nm "$SHA_BIN" 2>/dev/null | grep -iq sha256; then
        log INFO "sha256 symbol present in $SHA_BIN — cpu-profile can attribute it:"
        nm "$SHA_BIN" 2>/dev/null | grep -i sha256 | sed 's/^/    /' || true
    else
        log WARN "no distinct sha256 symbol in $SHA_BIN (likely inlined into hash_reader)."
        log WARN "cpu-profile will still show the hot address; for a clearer backend"
        log WARN "name, rebuild with reduced inlining, e.g.:"
        log WARN "  RUSTFLAGS='-C inline-threshold=0' CARGO_PROFILE_RELEASE_LTO=false \\"
        log WARN "  CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_PROFILE_RELEASE_STRIP=false \\"
        log WARN "  CARGO_PROFILE_RELEASE_DEBUG=line-tables-only cargo build --release"
    fi
}

# ---- test data -------------------------------------------------------------

cleanup() { [[ "${KEEP:-0}" == 0 && -n "${WORKDIR:-}" ]] && rm -rf "$WORKDIR"; }

setup_data() {
    WORKDIR="${WORKDIR:-$(mktemp -d)}"
    trap cleanup EXIT
    FILE="$WORKDIR/big.dat"
    SUMS="$WORKDIR/SHA256SUMS"
    log INFO "Generating ${FILE_SIZE_MB} MiB of test data in $WORKDIR ..."
    dd if=/dev/urandom of="$FILE" bs=1M count="$FILE_SIZE_MB" status=none
    # Warm the page cache: measure compute, not first-touch disk reads.
    cat "$FILE" >/dev/null
    # Checksum file for the verify pass (coreutils format; sha reads the same).
    (cd "$WORKDIR" && sha256sum "$(basename "$FILE")" >"$SUMS")

    # Sanity: both programs agree on the digest, so we profile equivalent work.
    local a b
    a=$("$SHA_BIN" hash sha256 -j1 "$FILE" | awk '{print $1}')
    b=$(sha256sum "$FILE" | awk '{print $1}')
    [[ "$a" == "$b" ]] || log FATAL "digest mismatch ($a vs $b) — not profiling equivalent work."
    log DEBUG "digests agree: $a"
}

# ---- profiling -------------------------------------------------------------

# run_stap <script> <label> <command...>
run_stap() {
    local script="$1" label="$2"
    shift 2
    printf '\n%b%b--- %s ---%b\n' "$BOLD" "$BLUE" "$label" "$RESET"
    # -c runs the command and scopes probes to it (target()/target_set_pid).
    stap "$script" -c "$*" || log WARN "stap exited non-zero for: $label"
}

# profile_with <script> <title>: run the four commands under one stap script.
profile_with() {
    local stp="$1" title="$2"
    printf '\n%b%b========== %s ==========%b\n' "$BOLD" "$MAGENTA" "$title" "$RESET"
    run_stap "$stp" "sha hash sha256 -j1" "$SHA_BIN" hash sha256 -j1 "$FILE"
    run_stap "$stp" "sha256sum (hash)" sha256sum "$FILE"
    run_stap "$stp" "sha verify sha256" "$SHA_BIN" verify sha256 "$SUMS"
    run_stap "$stp" "sha256sum -c (verify)" sha256sum -c "$SUMS"
}

profile_all() {
    profile_with "$IO_STP" "I/O profile"
    profile_with "$CPU_STP" "CPU profile"
}

notes() {
    cat <<EONOTES

${BOLD}${BLUE}How to read this:${RESET}

  ${GREEN}*${RESET} ${CYAN}I/O profile${RESET}: '${YELLOW}sha${RESET}' should show few ~8 MiB reads vs many small ones for
    coreutils. If '${YELLOW}sha${RESET}' is slower despite LESS read traffic, I/O is not the cause.

  ${GREEN}*${RESET} ${CYAN}CPU profile${RESET}: compare the hottest ${BOLD}user${RESET} symbol of each program:
      - One side ${RED}*shaext / *_ni${RESET} (hardware) and the other portable
        ${YELLOW}=> backend mismatch${RESET}: coreutils is on SHA-NI, sha is not. Root cause.
      - Both hardware, near-equal sample share in the transform
        ${YELLOW}=> per-core parity${RESET}: any slowness is single-file/startup; sha's win is
           parallelism — confirm with scripts/bench-vs-coreutils.sh.

  ${GREEN}*${RESET} verify shares hash_file's hot path; it's the same compute as hash plus
    checksum parsing. 'hash' is the discriminating comparison.
EONOTES
}

main() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
        # normalize --opt=value -> --opt value, then reprocess
        --*=*)
            set -- "${1%%=*}" "${1#*=}" "${@:2}"
            continue
            ;;
        -s | --sha-bin)
            need "$1" "$#"
            SHA_BIN="$2"
            shift 2
            ;;
        -f | --file-size-mib)
            need "$1" "$#"
            uint "$1" "$2"
            FILE_SIZE_MB="$2"
            shift 2
            ;;
        -w | --workdir)
            need "$1" "$#"
            WORKDIR="$2"
            shift 2
            ;;
        -k | --keep)
            KEEP=1
            shift
            ;;
        -n | --no-build)
            NO_BUILD=1
            shift
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

    preflight
    build_sha
    check_symbols
    setup_data
    profile_all
    notes
}

main "$@"
# vim: set ft=bash ts=4 sw=4:
