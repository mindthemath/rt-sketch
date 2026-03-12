# rt-sketch

Real-time video-to-SVG sketch engine for drawing robots.

rt-sketch watches a video source (webcam, video file, or static image), and iteratively builds an SVG drawing that approximates it using random line proposals. Each step, it generates K candidate lines, scores them against the target frame using asymmetric MSE, and keeps the best-improving one. The result is a pen-plotter-friendly SVG of simple line segments.

A built-in web UI lets you watch the drawing evolve in real time and tune parameters on the fly.

## Requirements

- **Rust** (2021 edition)
- **ffmpeg** — used for video/webcam capture and frame decoding

## Quick start

```bash
# Webcam (default)
make run

# Static image
make run-image IMAGE=photo.jpg

# Development mode (debug build, static image)
make dev IMAGE=photo.jpg

# Development mode (debug build, webcam)
make dev-webcam
```

Then open **http://localhost:8080** in your browser.

## Usage

```
rt-sketch [OPTIONS]
```

### Source options

| Flag | Default | Description |
|------|---------|-------------|
| `--source` | `webcam` | Input source: `webcam`, `webcam:1`, `image:path.jpg`, or `video:path.mp4` |
| `--fps` | `6.0` | Target frames per second (lower = less CPU for live sources) |
| `--resolution` | `256` | Processing resolution height in pixels |

### Canvas options

| Flag | Default | Description |
|------|---------|-------------|
| `--canvas-width` | `10.0` | Canvas width in cm |
| `--canvas-height` | `10.0` | Canvas height in cm |
| `--ppi` | `72.0` | Pixels per inch for web preview rendering |
| `--stroke-width` | `0.05` | Pen stroke width in cm |

The canvas aspect ratio is automatically adjusted to match the source (fit within the width/height bounding box).

### Algorithm options

| Flag | Default | Description |
|------|---------|-------------|
| `--k` | `200` | Number of random line proposals per step |
| `--alpha` | `2.0` | Asymmetric MSE penalty. 1.0 = standard MSE, >1 penalizes ink on whitespace more |
| `--gamma` | `1.0` | Gamma correction for target image. <1 brightens, >1 darkens |
| `--min-line-len` | `0.2` | Minimum line length in cm |
| `--max-line-len` | `5.0` | Maximum line length in cm |
| `--sampler` | `uniform` | Sampling strategy: `uniform` or `beta` |

### Robot and network options

| Flag | Default | Description |
|------|---------|-------------|
| `--robot-server` | *(none)* | Robot server URL (omit for preview-only mode) |
| `--web-port` | `8080` | Web UI port |

## Web UI

The web interface at `http://localhost:<web-port>` shows:

- **Target** — the current video frame being approximated
- **Canvas** — the SVG drawing at processing resolution (pixelated)
- **Preview** — the SVG drawing at full PPI scale

### Controls

- **Start / Pause / Resume** — control the drawing engine (or press **spacebar**)
- **Reset** — clear the canvas and start over
- **Export** — download the current drawing as an SVG file
- **K** — adjust proposals per step (5–500)
- **alpha** — asymmetric MSE penalty (1.0–10.0)
- **gamma** — target brightness correction (0.1–5.0)
- **min / max** — line length bounds in cm
- **target size** — visual scaling of the target display (does not affect processing)

### Header stats

- **iter** — total proposal iterations (including rejected proposals)
- **lines** — number of accepted lines on the canvas
- **last** — length of the most recently drawn line (with progress bar)
- **MSE** — current asymmetric MSE score
- **FPS** — iterations per second
- **total** — cumulative length of all accepted lines in cm

## How it works

### Overview

1. Capture a frame from the source and convert to grayscale
2. Optionally apply gamma correction to the target
3. Generate K random line segments within the canvas bounds
4. For each candidate, rasterize it onto a clone of the cached canvas pixmap
5. Score each candidate against the target using asymmetric MSE
6. If the best candidate improves the score, accept it; otherwise reject all K and try again
7. Send accepted lines to a connected drawing robot (if configured)
8. Repeat

### Asymmetric MSE scoring

Standard MSE treats all pixel errors equally. Asymmetric MSE adds a directional penalty controlled by `alpha`:

For each pixel, compute `diff = canvas_pixel - target_pixel`:
- **Canvas too light** (missing ink, `diff > 0`): error = `diff^2` (standard)
- **Canvas too dark** (excess ink, `diff < 0`): error = `diff^2 * alpha`

With `alpha > 1`, the algorithm is penalized more for placing ink where the target is light than for leaving gaps where the target is dark. This makes the algorithm conservative with ink — it prefers to under-draw rather than over-draw, which produces cleaner results for pen plotting.

### Proposal acceptance / rejection

Each iteration, K random lines are generated and scored in parallel (using rayon). The engine compares the best candidate's score against the current canvas score:

- **Accept** (`best_score < current_score`): The winning line is added to the canvas. The cached pixmap is updated incrementally — only the new line is rasterized, avoiding a full re-render.
- **Reject** (`best_score >= current_score`): All K candidates are discarded. The canvas is unchanged. This happens when no random line can improve the drawing — typically when the canvas is already a close approximation of the target, or when the remaining details are too fine for the line length range.

The iteration counter increments regardless of acceptance, so `iter - lines` gives the number of rejected rounds. As the drawing progresses, the rejection rate climbs because fewer random lines happen to land in useful positions.

### Performance

Candidate scoring is parallelized across cores with rayon. Each candidate clones only the cached pixmap (not the full SVG), rasterizes one line, and computes MSE. The cached pixmap is updated incrementally on acceptance, so the per-step cost is O(K) pixmap clones + rasterizations rather than re-rendering all lines.

## Robot protocol

When `--robot-server` is set, accepted lines are POSTed to `{server}/draw` as JSON:

```json
{
  "command": "line",
  "x1": 1.5,
  "y1": 2.3,
  "x2": 4.1,
  "y2": 3.7,
  "width": 0.05
}
```

Coordinates are in canvas cm. Omit `--robot-server` for preview-only mode.

## Webcam selection (macOS)

List available devices:

```bash
ffmpeg -f avfoundation -list_devices true -i ""
```

Then use the device index:

```bash
rt-sketch --source webcam:0   # FaceTime HD Camera
rt-sketch --source webcam:1   # USB webcam
```

## Make targets

| Target | Description |
|--------|-------------|
| `make build` | Compile release binary |
| `make run` | Run with webcam (release) |
| `make run-image` | Run with a static test image |
| `make dev` | Run in debug mode with a test image |
| `make dev-webcam` | Run in debug mode with webcam |
| `make snap` | Capture a single webcam frame and save as test image |
| `make check` | Run formatter, clippy, and tests |
| `make test` | Run tests only |
| `make fmt` | Format code |
| `make clean` | Remove build artifacts |
