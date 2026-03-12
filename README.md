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

# Development mode (debug build, webcam)
make dev-webcam

# Development mode (debug build, static image)
make dev IMAGE=photo.jpg
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
| `--canvas-width` | `30.0` | Canvas width in cm |
| `--canvas-height` | `20.0` | Canvas height in cm |
| `--ppi` | `72.0` | Pixels per inch for web preview rendering |
| `--stroke-width` | `0.05` | Pen stroke width in cm |

The canvas aspect ratio is automatically adjusted to match the source.

### Algorithm options

| Flag | Default | Description |
|------|---------|-------------|
| `--k` | `50` | Number of random line proposals per step |
| `--alpha` | `2.0` | Asymmetric MSE penalty. 1.0 = standard MSE, >1 penalizes ink on whitespace more |
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

- **Start / Pause / Resume** — control the drawing engine
- **Reset** — clear the canvas and start over
- **Export** — download the current drawing as an SVG file
- **K** — adjust proposals per step (more = better lines, slower)
- **alpha** — asymmetric MSE penalty (higher = less stray ink)
- **min / max** — line length bounds in cm

### Header stats

- **iter** — total proposal iterations
- **lines** — number of lines on the canvas
- **last** — length of the most recently drawn line (with progress bar)
- **MSE** — current asymmetric MSE score
- **FPS** — iterations per second

## How it works

1. Capture a frame from the source and convert to grayscale
2. Generate K random line segments within the canvas bounds
3. For each candidate, rasterize it onto a copy of the current canvas
4. Score each against the target using asymmetric MSE (penalizes overshoot by alpha)
5. Keep the candidate that most improves the score
6. Optionally send the winning line to a connected drawing robot
7. Repeat

Scoring uses **asymmetric MSE**: standard MSE where `canvas_pixel < target_pixel` (too light), but the error is multiplied by `alpha` where `canvas_pixel > target_pixel` (too dark — ink where there should be whitespace). This encourages the algorithm to be conservative with ink placement.

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
| `make check` | Run formatter, clippy, and tests |
| `make test` | Run tests only |
| `make fmt` | Format code |
| `make clean` | Remove build artifacts |
