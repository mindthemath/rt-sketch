#!/usr/bin/env bash
# Generate a stamp library of SVG curve files for rt-sketch stamp mode.
# Usage: ./gen_stamps.sh [output_dir] [seed]
#
# Produces imgs/*.svg and imgs/stamps.csv

set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  echo "Generate a stamp library of SVG curve files for rt-sketch stamp mode."
  echo ""
  echo "Usage: ./gen_stamps.sh [output_dir] [seed]"
  echo ""
  echo "  output_dir  Directory for SVGs and stamps.csv (default: imgs)"
  echo "  seed        RNG seed for reproducible scale values (default: 42)"
  echo ""
  echo "The output directory is wiped and recreated on each run."
  echo "Curves are generated using rt-drawing (draw_curves binary)."
  echo "Scale values in the CSV are randomized between 5 and 15."
  exit 0
fi

DIR="${1:-imgs}"
DIR="${DIR%/}"
SEED="${2:-42}"
rm -rf "$DIR"
mkdir -p "$DIR"

RANDOM=$SEED

BINARY="cargo run --release -p rt-drawing --"
CSV="$DIR/stamps.csv"
echo "path,scale" > "$CSV"

COUNT=0
emit() {
  local name="$1"; shift
  local scale=$(( (RANDOM % 11) + 5 ))
  echo "  $name (scale=${scale})"
  $BINARY "$@" -o "$DIR/$name.svg"
  echo "$DIR/$name.svg,${scale}.0" >> "$CSV"
  COUNT=$((COUNT + 1))
}

DSEED=$SEED

# ── Dense chained curves (step=n, the bulk of the library) ───────────

echo "=== dense chained curves (step=n) ==="

for basis in rbf fourier relu legendre; do
  for n in 12 24 36; do
    for c in 3 4 5 6 7 8; do
      emit "dense-${basis}-c${c}-n${n}-s${n}" \
        --basis "$basis" --seed $DSEED --num-curves 1 --chain $c $c \
        --n-points $n --step $n
      DSEED=$((DSEED + 1))
    done

    # Random range variants
    emit "dense-${basis}-c3to6-n${n}-s${n}" \
      --basis "$basis" --seed $DSEED --num-curves 1 --chain 3 6 \
      --n-points $n --step $n
    DSEED=$((DSEED + 1))

    emit "dense-${basis}-c3to8-n${n}-s${n}" \
      --basis "$basis" --seed $DSEED --num-curves 1 --chain 3 8 \
      --n-points $n --step $n
    DSEED=$((DSEED + 1))

    emit "dense-${basis}-c5to8-n${n}-s${n}" \
      --basis "$basis" --seed $DSEED --num-curves 1 --chain 5 8 \
      --n-points $n --step $n
    DSEED=$((DSEED + 1))
  done
done

# ── Chained curves with smaller step sizes ───────────────────────────

echo "=== chained curves (step<n) ==="

for basis in rbf fourier relu legendre; do
  for n in 12 24 36; do
    half=$((n / 2))
    emit "chain-${basis}-c3to6-n${n}-s${half}" \
      --basis "$basis" --seed $DSEED --num-curves 1 --chain 3 6 \
      --n-points $n --step $half
    DSEED=$((DSEED + 1))

    emit "chain-${basis}-c3to8-n${n}-s${half}" \
      --basis "$basis" --seed $DSEED --num-curves 1 --chain 3 8 \
      --n-points $n --step $half
    DSEED=$((DSEED + 1))
  done
done

# ── Chained + transformed ────────────────────────────────────────────

echo "=== chained + transformed ==="

for basis in fourier rbf relu; do
  for n in 12 24 36; do
    emit "chain-${basis}-scaled-c3to8-n${n}-s${n}" \
      --basis "$basis" --seed $DSEED --num-curves 1 --chain 3 8 \
      --n-points $n --step $n \
      --cfg-scale-t 0.5 1.5 --cfg-scale-y 0.5 1.0
    DSEED=$((DSEED + 1))

    emit "chain-${basis}-rotated-c3to8-n${n}-s${n}" \
      --basis "$basis" --seed $DSEED --num-curves 1 --chain 3 8 \
      --n-points $n --step $n \
      --cfg-rot 0 180
    DSEED=$((DSEED + 1))
  done
done

# ── Piecewise curves ─────────────────────────────────────────────────

echo "=== piecewise curves ==="

for basis in fourier rbf legendre relu; do
  for n in 12 24 36; do
    emit "pw-${basis}-3x45-n${n}-s${n}" \
      --basis "$basis" --seed $DSEED --num-curves 1 \
      --piecewise 3 --rot 45 --noise 0.1 \
      --n-points $n --step $n
    DSEED=$((DSEED + 1))

    emit "pw-${basis}-4x60-n${n}-s${n}" \
      --basis "$basis" --seed $DSEED --num-curves 1 \
      --piecewise 4 --rot 60 --noise 0.15 \
      --n-points $n --step $n
    DSEED=$((DSEED + 1))
  done
done

# ── Single curves ────────────────────────────────────────────────────

echo "=== single curves ==="

for basis in rbf fourier relu legendre; do
  for n in 12 24 36; do
    emit "single-${basis}-n${n}-s${n}" \
      --basis "$basis" --seed $DSEED --num-curves 1 \
      --n-points $n --step $n
    DSEED=$((DSEED + 1))

    half=$((n / 2))
    emit "single-${basis}-n${n}-s${half}" \
      --basis "$basis" --seed $DSEED --num-curves 1 \
      --n-points $n --step $half
    DSEED=$((DSEED + 1))
  done
done

echo ""
echo "Done! $COUNT stamps written to $DIR/"
echo "CSV: $CSV"
