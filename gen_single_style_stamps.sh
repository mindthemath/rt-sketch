#!/usr/bin/env bash
# Generate a large batch of dense chained stamps in a single basis style.
# Usage: ./gen_single_style_stamps.sh [options]
#
# Produces <output_dir>/*.svg and <output_dir>/stamps.csv

set -euo pipefail

DIR="imgs"
SEED=21
MIN_SCALE=1
MAX_SCALE=1
BASIS="relu"
N_POINTS=12
MIN_CHAINS=3
MAX_CHAINS=6
COUNT_PER=100
JOBS=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      echo "Generate dense chained stamps in a single basis style."
      echo ""
      echo "Usage: ./gen_single_style_stamps.sh [options]"
      echo ""
      echo "Options:"
      echo "  -o, --output DIR       Output directory (default: imgs)"
      echo "  -s, --seed N           RNG seed (default: 21)"
      echo "  -b, --basis TYPE       Basis function: rbf, relu, legendre, fourier, etc. (default: relu)"
      echo "  -n, --n-points N       Points per curve, also used as step size (default: 12)"
      echo "  --min-chains N         Minimum chain count (default: 3)"
      echo "  --max-chains N         Maximum chain count (default: 9)"
      echo "  --count N              Number of variants per chain count (default: 25)"
      echo "  --min-scale N          Minimum scale value in CSV (default: 1)"
      echo "  --max-scale N          Maximum scale value in CSV (default: 1)"
      echo "  -j, --jobs N           Parallel jobs (default: number of cores)"
      echo ""
      echo "Example:"
      echo "  ./gen_single_style_stamps.sh -b relu -n 12 --min-chains 3 --max-chains 9 --count 10"
      exit 0
      ;;
    -o|--output) DIR="$2"; shift 2 ;;
    -s|--seed) SEED="$2"; shift 2 ;;
    -b|--basis) BASIS="$2"; shift 2 ;;
    -n|--n-points) N_POINTS="$2"; shift 2 ;;
    --min-chains) MIN_CHAINS="$2"; shift 2 ;;
    --max-chains) MAX_CHAINS="$2"; shift 2 ;;
    --count) COUNT_PER="$2"; shift 2 ;;
    --min-scale) MIN_SCALE="$2"; shift 2 ;;
    --max-scale) MAX_SCALE="$2"; shift 2 ;;
    -j|--jobs) JOBS="$2"; shift 2 ;;
    *) echo "unknown option: $1"; exit 1 ;;
  esac
done

DIR="${DIR%/}"
rm -rf "$DIR"
mkdir -p "$DIR"

# Build the binary first (once, not per-job)
cargo build --release -p rt-drawing
BINARY="./target/release/draw_curves"

echo "=== dense ${BASIS} n=${N_POINTS} chains=${MIN_CHAINS}-${MAX_CHAINS} x${COUNT_PER} (${JOBS} jobs) ==="

# Generate the job list: one line per stamp with "name seed chains"
RANDOM=$SEED
DSEED=$SEED
JOBFILE=$(mktemp)
SCALEFILE=$(mktemp)

for c in $(seq "$MIN_CHAINS" "$MAX_CHAINS"); do
  for i in $(seq 1 "$COUNT_PER"); do
    name="dense-${BASIS}-c${c}-n${N_POINTS}-s${N_POINTS}-${i}"
    local_range=$((MAX_SCALE - MIN_SCALE + 1))
    if (( local_range <= 1 )); then
      scale=$MIN_SCALE
    else
      scale=$(( (RANDOM % local_range) + MIN_SCALE ))
    fi
    echo "$name $DSEED $c $scale" >> "$JOBFILE"
    DSEED=$((DSEED + 1))
  done
done

TOTAL=$(wc -l < "$JOBFILE")

# Run in parallel with xargs
cat "$JOBFILE" | xargs -P "$JOBS" -L 1 bash -c \
  "$BINARY --basis $BASIS --seed \"\$1\" --num-curves 1 --chain \"\$2\" \"\$2\" --n-points $N_POINTS --step $N_POINTS -o \"$DIR/\$0.svg\" && echo \"  \$0 (scale=\$3)\""

# Build CSV from completed SVGs using the scale values
echo "path,scale" > "$DIR/stamps.csv"
while read -r name dseed chains scale; do
  echo "$DIR/$name.svg,${scale}.0" >> "$DIR/stamps.csv"
done < "$JOBFILE"

rm -f "$JOBFILE" "$SCALEFILE"

echo ""
echo "Done! $TOTAL stamps written to $DIR/"
echo "CSV: $DIR/stamps.csv"
