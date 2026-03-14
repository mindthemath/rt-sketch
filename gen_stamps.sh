#!/usr/bin/env bash
# Generate a stamp library of SVG curve files for rt-sketch stamp mode.
# Usage: ./gen_stamps.sh [output_dir] [seed]
#
# Produces imgs/*.svg and imgs/stamps.csv

set -euo pipefail

DIR="imgs"
SEED=21
MIN_SCALE=1
MAX_SCALE=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      echo "Generate a stamp library of SVG curve files for rt-sketch stamp mode."
      echo ""
      echo "Usage: ./gen_stamps.sh [output_dir] [seed] [--min-scale N] [--max-scale N]"
      echo ""
      echo "  output_dir   Directory for SVGs and stamps.csv (default: imgs)"
      echo "  seed         RNG seed for reproducible scale values (default: 42)"
      echo "  --min-scale  Minimum scale value in CSV (default: 1)"
      echo "  --max-scale  Maximum scale value in CSV (default: 1)"
      echo ""
      echo "The output directory is wiped and recreated on each run."
      echo "Curves are generated using rt-drawing (draw_curves binary)."
      exit 0
      ;;
    --min-scale) MIN_SCALE="$2"; shift 2 ;;
    --max-scale) MAX_SCALE="$2"; shift 2 ;;
    *)
      if [[ "$DIR" == "imgs" && ! "$1" =~ ^- ]]; then
        DIR="$1"
      elif [[ "$SEED" == "42" && ! "$1" =~ ^- ]]; then
        SEED="$1"
      fi
      shift
      ;;
  esac
done

DIR="${DIR%/}"
rm -rf "$DIR"
mkdir -p "$DIR"

RANDOM=$SEED

BINARY="cargo run --release -p rt-drawing --"
CSV="$DIR/stamps.csv"
echo "path,scale" > "$CSV"

COUNT=0
emit() {
  local name="$1"; shift
  local range=$((MAX_SCALE - MIN_SCALE + 1))
  local scale
  if (( range <= 1 )); then
    scale=$MIN_SCALE
  else
    scale=$(( (RANDOM % range) + MIN_SCALE ))
  fi
  echo "  $name (scale=${scale})"
  $BINARY "$@" -o "$DIR/$name.svg"
  echo "$DIR/$name.svg,${scale}.0" >> "$CSV"
  COUNT=$((COUNT + 1))
}

DSEED=$SEED

# ── Dense chained curves (step=n, the bulk of the library) ───────────

echo "=== dense chained curves (step=n) ==="

for basis in rbf relu legendre; do
  for n in 24 36; do
    for c in 3 4 5 6 7 8; do
      for v in a b c; do
        emit "dense-${basis}-c${c}-n${n}-s${n}-${v}" \
          --basis "$basis" --seed $DSEED --num-curves 1 --chain $c $c \
          --n-points $n --step $n
        DSEED=$((DSEED + 1))
      done
    done

    # Random range variants (3 seeds each)
    for v in a b c; do
      emit "dense-${basis}-c3to6-n${n}-s${n}-${v}" \
        --basis "$basis" --seed $DSEED --num-curves 1 --chain 3 6 \
        --n-points $n --step $n
      DSEED=$((DSEED + 1))

      emit "dense-${basis}-c3to8-n${n}-s${n}-${v}" \
        --basis "$basis" --seed $DSEED --num-curves 1 --chain 3 8 \
        --n-points $n --step $n
      DSEED=$((DSEED + 1))

      emit "dense-${basis}-c5to8-n${n}-s${n}-${v}" \
        --basis "$basis" --seed $DSEED --num-curves 1 --chain 5 8 \
        --n-points $n --step $n
      DSEED=$((DSEED + 1))
    done
  done
done

# ── Chained curves with smaller step sizes ───────────────────────────

echo "=== chained curves (step<n) ==="

for basis in rbf relu legendre; do
  for n in 24 36; do
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

for basis in rbf relu; do
  for n in 24 36; do
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
  for n in 24 36; do
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

echo ""
echo "Done! $COUNT stamps written to $DIR/"
echo "CSV: $CSV"
