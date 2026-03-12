# rt-sketch — Implementation Plan

## Overview

Real-time video-to-SVG sketch engine. Iteratively approximates a target image
(video frame) by proposing random line segments, scoring each against the target,
and keeping the best. Output is streamed as draw commands to a robot server.

## Architecture

```
                        ┌──────────────────────────────────────────────────┐
                        │               rt-sketch binary                  │
                        │                                                 │
  ffmpeg subprocess ──▶ │  FrameSource (stdin pipe, raw grayscale)        │
                        │       │                                         │
                        │       ▼                                         │
                        │  ProposalEngine                                 │
                        │    ├─ Canvas (Vec<LineSegment>)                  │
                        │    ├─ SamplingStrategy (trait)                   │
                        │    │    ├─ UniformSampler                        │
                        │    │    ├─ BetaSampler                           │
                        │    │    └─ (future: AdaptiveSampler)             │
                        │    ├─ Rasterizer (tiny-skia → grayscale buf)    │
                        │    └─ Scorer (MSE on grayscale buffers)         │
                        │       │                                         │
                        │       ├──▶ WebServer (axum + websocket)         │
                        │       │     ├─ error view (at processing res)   │
                        │       │     ├─ canvas preview (at PPI scale)    │
                        │       │     └─ controls (start/pause/reset/K)   │
                        │       │                                         │
                        │       └──▶ CommandOutput (POST to robot server) │
                        └──────────────────────────────────────────────────┘
```

## Key Design Decisions

### Language: Rust
- Single binary, no runtime deps (except ffmpeg on PATH)
- Cross-compiles to x86_64 Linux and aarch64 macOS
- Performance: rasterize + compare at 256px is sub-millisecond
- Web UI embedded in binary via rust-embed

### Frame Source: ffmpeg subprocess
All input types unified through ffmpeg piping raw grayscale frames:
- Image: `ffmpeg -loop 1 -i photo.jpg -f rawvideo -pix_fmt gray8 pipe:1`
- Webcam (linux): `ffmpeg -f v4l2 -i /dev/video0 -f rawvideo -pix_fmt gray8 pipe:1`
- Webcam (macOS): `ffmpeg -f avfoundation -i "0" -f rawvideo -pix_fmt gray8 pipe:1`
- Video file: `ffmpeg -i video.mp4 -f rawvideo -pix_fmt gray8 pipe:1`

### Canvas Model
- Source of truth: `Vec<LineSegment>` where `LineSegment { x1, y1, x2, y2, width }`
- Coordinates in cm (matching physical canvas)
- Rasterized to grayscale buffer at processing resolution for scoring
- Rasterized at PPI scale for web preview
- Exportable as SVG

### Sampling Strategy (trait)
Abstracted so we can swap strategies:
1. **UniformSampler** — uniform random x1,y1,x2,y2 within canvas bounds (initial)
2. **BetaSampler** — beta distribution for biased sampling toward edges/center
3. **AdaptiveSampler** — (future) sample proportional to local error

### Scoring
- Grayscale MSE (mean squared error)
- Canvas is white background, lines are black (single pen)
- Compare at processing resolution (default 256px height)

### Web UI
Two preview panels:
1. **Error view** — side-by-side target vs canvas at processing resolution (256px)
2. **Canvas preview** — to-scale rendering using canvas_cm × PPI

Controls: Start / Pause / Resume / Reset / K slider / FPS target
Stats: iteration count, MSE score, actual FPS

Built with vanilla JS + Canvas API, no framework.

### Resolution & Scaling
- `--resolution 256` sets processing height in pixels
- Width derived from canvas aspect ratio
- `--ppi 72` sets pixels-per-inch for the to-scale web preview
- Canvas physical size in cm → preview pixels = cm × (PPI / 2.54)

## CLI Interface

```
rt-sketch \
  --source image:photo.jpg          # or webcam or video:file.mp4
  --fps 30                          # target frame rate
  --resolution 256                  # processing height in px
  --canvas-width 30                 # cm
  --canvas-height 20               # cm
  --ppi 72                          # pixels per inch for preview
  --k 10                            # proposals per step
  --sampler uniform                 # or beta
  --robot-server http://host:5000   # optional, omit for preview-only
  --web-port 8080                   # UI port
```

## Crate Dependencies

| Crate | Purpose |
|-------|---------|
| `tiny-skia` | Software 2D rasterization (line drawing, anti-aliasing) |
| `clap` | CLI argument parsing |
| `axum` + `tower` | HTTP server + WebSocket |
| `tokio` | Async runtime |
| `rayon` | Parallel proposal evaluation across K |
| `image` | Image format decoding (for initial frame if needed) |
| `rust-embed` | Embed web UI static files in binary |
| `fastrand` | Fast thread-local RNG |
| `serde` + `serde_json` | Serialization for WebSocket messages / config |

## Project Structure

```
rt-sketch/
├── Cargo.toml
├── Makefile
├── src/
│   ├── main.rs                 # CLI entry point (clap), wires everything
│   ├── config.rs               # Config struct, defaults, validation
│   ├── frame_source.rs         # FrameSource: spawns ffmpeg, reads raw frames
│   ├── engine/
│   │   ├── mod.rs              # ProposalEngine: run loop, pick best
│   │   ├── canvas.rs           # Canvas, LineSegment, rasterize, to_svg
│   │   ├── sampler.rs          # SamplingStrategy trait + UniformSampler, BetaSampler
│   │   └── scorer.rs           # MSE scoring on grayscale buffers
│   ├── output.rs               # CommandSink trait, HttpSink, NoopSink
│   └── web/
│       ├── mod.rs              # axum routes, websocket handler, state
│       └── static/
│           ├── index.html      # single-page UI
│           ├── app.js          # vanilla JS: websocket, canvas rendering, controls
│           └── style.css       # minimal styling
```

## Makefile Targets

```
make build          # cargo build --release
make run            # run with default args (webcam, K=10, 256px)
make run-image      # run with a test image
make dev            # cargo run (debug mode) with test image
make check          # cargo clippy + cargo test
make test           # cargo test
make clean          # cargo clean
make fmt            # cargo fmt
make help           # list all targets
```

## Implementation Order

1. Project scaffolding: Cargo.toml, Makefile, config.rs, main.rs with clap
2. Canvas model + rasterization (tiny-skia) + SVG export
3. Scorer (grayscale MSE)
4. Sampling strategies (UniformSampler first)
5. ProposalEngine (the core loop: mutate K times, score, pick best)
6. FrameSource (ffmpeg subprocess, static image mode first)
7. Web server + WebSocket (axum, serve static UI)
8. Web UI (vanilla JS: preview panels, controls, stats)
9. Wire everything together in main.rs
10. CommandOutput (HTTP POST to robot server, noop mode)
