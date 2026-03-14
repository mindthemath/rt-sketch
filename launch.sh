#!/usr/bin/env bash
set -euo pipefail

NUM_INSTANCES=1
VIEWER_ADDR="localhost:9900"
TIMEOUT=""
SOURCE="image:https://cdn.mindthemath.com/logo-450-wb.png"

usage() {
    echo "Usage: $0 [-n NUM] [--viewer ADDR] [--timeout SECS] [--source SOURCE]"
    echo "  -n NUM        Number of instances to launch (default: 1)"
    echo "  --viewer ADDR Address for --stream-tcp (default: localhost:9900)"
    echo "  --timeout S   Timeout in seconds for each process"
    echo "  --source SRC  Image source (default: $SOURCE)"
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -n) NUM_INSTANCES="$2"; shift 2 ;;
        --viewer) VIEWER_ADDR="$2"; shift 2 ;;
        --timeout) TIMEOUT="$2"; shift 2 ;;
        --source) SOURCE="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) echo "Unknown arg: $1"; usage ;;
    esac
done

if [[ ! -x "./rt-sketch" ]]; then
    echo "Error: ./rt-sketch not found or not executable in $(pwd)"
    exit 1
fi

PIDS=()
LOG_DIR="logs"
mkdir -p "$LOG_DIR"

cleanup() {
    echo ""
    echo "Shutting down..."
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
    # Reset terminal in case anything got corrupted
    stty sane 2>/dev/null || true
    echo "All processes stopped."
}

trap cleanup EXIT INT TERM

SAMPLERS=(uniform center edges low high)

rand_name() {
    LC_ALL=C tr -dc 'a-z' < /dev/urandom | head -c 6 || true
}

rand_int() {
    local lo=$1 hi=$2
    echo $(( lo + RANDOM % (hi - lo + 1) ))
}

rand_sampler() {
    echo "${SAMPLERS[RANDOM % ${#SAMPLERS[@]}]}"
}

for i in $(seq 1 "$NUM_INSTANCES"); do
    STREAM_NAME="$(rand_name)"
    LOG_FILE="$LOG_DIR/instance-${i}.log"
    ALPHA="$(rand_int 1 8)"
    X_SAMPLER="$(rand_sampler)"
    Y_SAMPLER="$(rand_sampler)"
    L_SAMPLER="$(rand_sampler)"

    CMD=(./rt-sketch
        --source "$SOURCE"
        --canvas-height 15
        --canvas-width 15
        --fps 12
        --stream-tcp "$VIEWER_ADDR"
        --k 200
        --stream-name "$STREAM_NAME"
        --alpha "$ALPHA"
        --x-sampler "$X_SAMPLER"
        --y-sampler "$Y_SAMPLER"
        --length-sampler "$L_SAMPLER"
        --wait-for-viewer
        --auto-start
        --threads 2
    )

    echo "[$i] alpha=$ALPHA x=$X_SAMPLER y=$Y_SAMPLER len=$L_SAMPLER"

    if [[ -n "$TIMEOUT" ]]; then
        CMD=(timeout "$TIMEOUT" "${CMD[@]}")
    fi

    echo "[$i] Starting stream '$STREAM_NAME' -> $LOG_FILE"
    "${CMD[@]}" > "$LOG_FILE" 2>&1 &
    PIDS+=($!)
done

echo "Launched $NUM_INSTANCES instance(s). Waiting..."
wait 2>/dev/null || true
