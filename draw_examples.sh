#!/usr/bin/env bash
# Generate a batch of example envelope curves with various configs.
# Usage: ./draw_examples.sh [output_dir]

set -euo pipefail

OUT="${1:-examples}"
OUT="${OUT%/}"
mkdir -p "$OUT"

BINARY="cargo run --release --bin draw_curves --"

# ── Per-basis demonstrations ──────────────────────────────────────────

SEED=100
for basis in sigmoid relu poly fourier rbf legendre; do
  DIR="$OUT/$basis"
  mkdir -p "$DIR"
  echo "=== $basis ==="

  echo "  grid (n=48, step=12)"
  $BINARY --basis "$basis" --seed 42 --num-curves 9 --n-points 48 --step 12 \
    -o "$DIR/grid.svg"

  echo "  fine (n=72, step=4)"
  $BINARY --basis "$basis" --seed 7 --num-curves 9 --n-points 72 --step 4 \
    -o "$DIR/fine.svg"

  echo "  coarse (n=24, step=8)"
  $BINARY --basis "$basis" --seed 13 --num-curves 9 --n-points 24 --step 8 \
    -o "$DIR/coarse.svg"

  echo "  piecewise 3x45° (n=36, step=12)"
  $BINARY --basis "$basis" --seed 55 --num-curves 9 --piecewise 3 --rot 45 --noise 0.1 \
    --n-points 36 --step 12 -o "$DIR/piecewise.svg"

  echo "  chained 2-6 (n=24, step=24)"
  $BINARY --basis "$basis" --seed $SEED --num-curves 9 --chain 2 6 --n-points 24 --step 24 \
    -o "$DIR/chained.svg"

  echo "  accumulate (n=48, step=12)"
  $BINARY --basis "$basis" --seed $SEED --num-curves 9 --accumulate --n-points 48 --step 12 \
    -o "$DIR/accumulate.svg"

  SEED=$((SEED + 1))
done

# ── Mixed / transformed ──────────────────────────────────────────────

DIR="$OUT/mixed"
mkdir -p "$DIR"

echo "=== mixed: transformed sigmoid ==="
$BINARY --seed 33 --num-curves 16 --basis sigmoid \
  --n-points 48 --step 12 \
  --cfg-scale-t 0.5 1.5 \
  --cfg-scale-y 0.5 1.0 \
  --cfg-rot 0 180 \
  --cfg-shift -0.5 0.5 \
  -o "$DIR/transformed_sigmoid.svg"

echo "=== mixed: piecewise fourier 5x90° ==="
$BINARY --seed 88 --num-curves 9 --basis fourier \
  --piecewise 5 --rot 90 --noise 0.05 \
  --n-points 48 --step 18 \
  -o "$DIR/piecewise_fourier.svg"

echo "=== mixed: piecewise rbf 4x60° ==="
$BINARY --seed 77 --num-curves 9 --basis rbf \
  --piecewise 4 --rot 60 --noise 0.2 \
  --n-points 36 --step 8 \
  -o "$DIR/piecewise_rbf.svg"

echo ""
echo "Done! SVGs written to $OUT/"
find "$OUT" -name '*.svg' | sort
