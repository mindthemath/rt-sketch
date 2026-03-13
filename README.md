# rt-sketch

Real-time video-to-SVG sketch engine for drawing robots.

rt-sketch watches a video source (webcam, video file, or static image), and iteratively builds an SVG drawing that approximates it using random line proposals. Each step, it generates K candidate lines, scores them against the target frame using asymmetric MSE, and keeps the best-improving one. The result is a pen-plotter-friendly SVG of simple line segments.

A built-in web UI lets you watch the drawing evolve in real time and tune parameters on the fly.

## Requirements

- **ffmpeg** — used for video/webcam capture and frame decoding

## Quick start

```bash
# Webcam (default)
rt-sketch

# Static image
rt-sketch --source image:photo.jpg

# With a drawing robot
rt-sketch --source webcam --robot-server http://192.168.1.50:8080
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
| `--x-sampler` | `uniform` | X position distribution (see below) |
| `--y-sampler` | `uniform` | Y position distribution (see below) |
| `--length-sampler` | `uniform` | Line length distribution (see below) |

#### Sampler distributions

All preset modes use a Beta(a, b) distribution mapped to [0, 1]:

| Name | a | b | Bias |
|------|---|---|------|
| `uniform` | — | — | Flat / no bias |
| `center` | 2 | 2 | Concentrate toward the middle |
| `edges` | 0.5 | 0.5 | Concentrate at both extremes |
| `low` | 10 | 2 | Concentrate toward 0 (left / top / short) |
| `high` | 2 | 10 | Concentrate toward 1 (right / bottom / long) |
| `beta:a,b` | a | b | Custom Beta distribution |

### Streaming and recording options

| Flag | Default | Description |
|------|---------|-------------|
| `--stream-tcp` | *(none)* | Stream lines to a TCP viewer server (e.g. `192.168.1.10:9900`) |
| `--stream-name` | `rt-sketch` | Instance name for TCP stream identification |
| `--stream-output` | *(none)* | Record preview to a file (e.g. `recording.mkv`) |
| `--stream-url` | *(none)* | Stream preview to an RTMP URL (e.g. `rtmp://a.rtmp.youtube.com/live2/KEY`) |
| `--auto-start` | `false` | Start drawing immediately without waiting for the web UI |
| `--wait-for-viewer` | `false` | Block until the viewer is reachable before starting (requires `--stream-tcp`) |

`--stream-tcp` can be combined with `--stream-output` or `--stream-url`. The latter two are mutually exclusive (both use FFmpeg for video encoding).

### Robot and network options

| Flag | Default | Description |
|------|---------|-------------|
| `--robot-server` | *(none)* | Robot server URL (omit for preview-only mode) |
| `--web-port` | `8080` | Web UI port (auto-selects next available if default is busy; exits if explicit port is busy) |

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

## Multi-instance viewer (rt-viewer)

`rt-viewer` is a separate binary that aggregates line streams from multiple rt-sketch instances and displays them in a browser.

```
[rt-sketch A] ──TCP──┐
[rt-sketch B] ──TCP──┤──→ [rt-viewer] ──WebSocket──→ [Browser: canvas per instance]
[rt-sketch C] ──TCP──┘
```

### Running the viewer

```bash
# Start the viewer (TCP on :9900, web UI on :9901)
rt-viewer

# Connect rt-sketch instances
rt-sketch --source webcam --stream-tcp localhost:9900 --stream-name "cam-A"
rt-sketch --source webcam:1 --stream-tcp localhost:9900 --stream-name "cam-B"
```

Open **http://localhost:9901** to see all instances drawing in real time.

### Viewer CLI options

| Flag | Default | Description |
|------|---------|-------------|
| `--tcp-port` | `9900` | TCP port for rt-sketch instances to connect to |
| `--web-port` | `9901` | Web UI port for the viewer page |
| `--read-only` | `false` | Disable play/pause/reset controls in the viewer UI |

### How it works

Each rt-sketch instance sends individual line segments over TCP as they're accepted (32 bytes per line — 12-byte header + 5 floats). The viewer maintains a canvas per instance in the browser, drawing lines incrementally via WebSocket. Late-joining browser clients receive a full replay of all accumulated lines.

### Remote control

When not in `--read-only` mode, the viewer UI shows Play, Pause, and Reset buttons that send commands back to all connected workers over the same TCP connection. This lets you control a fleet of headless workers from a single browser tab.

### Recording + streaming together

TCP streaming and video recording are independent outputs that can run simultaneously:

```bash
rt-sketch --source webcam \
  --stream-tcp localhost:9900 --stream-name "cam-A" \
  --stream-output recording.mkv
```

## Distributed setup

When the webcam is on one machine but rt-sketch workers run on others, you can stream the video feed over the network. The `--source video:<url>` flag accepts any URL that ffmpeg understands — it's passed straight to `ffmpeg -i`, so anything ffmpeg can read, rt-sketch can use.

Note: `--canvas-width` / `--canvas-height` may need to be set explicitly since ffprobe over a network stream can sometimes fail to detect dimensions.

### Using an existing MJPEG stream

Many IP cameras, Raspberry Pi camera servers, and public webcams expose an MJPEG stream over HTTP. rt-sketch can consume these directly — no mediamtx or ffmpeg relay needed.

```bash
# IP camera or mjpg-streamer endpoint
rt-sketch --source video:http://192.168.1.100:8080/video

# Public webcam (any MJPEG URL that ffmpeg can read)
rt-sketch --source video:http://example.com/webcam.mjpg

# With explicit canvas dimensions (recommended — ffprobe can't always detect them over HTTP)
rt-sketch --source video:http://192.168.1.100:8080/video \
  --canvas-width 10 --canvas-height 7.5
```

Multiple workers can point at the same MJPEG URL independently — each opens its own HTTP connection, so no restreaming server is needed.

If you're running `mjpg-streamer` on a Raspberry Pi (common for OctoPrint and similar setups), the stream URL is typically `http://<pi-ip>:8080/?action=stream`.

### Streaming the webcam feed

The camera machine needs to serve its webcam as a network stream that multiple workers can consume simultaneously. We use [mediamtx](https://github.com/bluenviern/mediamtx) as a restreaming server — it accepts a single ffmpeg feed and serves it to any number of clients over RTSP or HLS.

```
[Webcam] ──→ [ffmpeg] ──RTSP──→ [mediamtx] ──RTSP/HLS──→ [Worker A]
                                                      ──→ [Worker B]
                                                      ──→ [Worker C]
```

#### Install mediamtx

**macOS:**
```bash
brew install mediamtx
```

**Linux (Raspberry Pi / Debian):**
```bash
# Download the latest release for your architecture
# ARM64 (Pi 4/5):
wget https://github.com/bluenviern/mediamtx/releases/latest/download/mediamtx_v*_linux_arm64v8.tar.gz
# AMD64:
wget https://github.com/bluenviern/mediamtx/releases/latest/download/mediamtx_v*_linux_amd64.tar.gz

tar -xzf mediamtx_*.tar.gz
sudo mv mediamtx /usr/local/bin/
```

#### Start the camera feed

Run mediamtx and ffmpeg on the camera machine:

```bash
# Terminal 1: start mediamtx (RTSP on :8554, HLS on :8888)
mediamtx
```

```bash
# Terminal 2: feed the webcam into mediamtx

# macOS
ffmpeg -f avfoundation -framerate 30 -video_size 640x480 -i "0:" \
  -c:v libx264 -preset ultrafast -tune zerolatency \
  -f rtsp rtsp://localhost:8554/cam

# Linux (Raspberry Pi / USB webcam)
ffmpeg -f v4l2 -framerate 30 -video_size 640x480 -i /dev/video0 \
  -c:v libx264 -preset ultrafast -tune zerolatency \
  -f rtsp rtsp://localhost:8554/cam
```

#### Test the feed

```bash
# From any machine on the network (requires ffmpeg)
ffplay -fflags nobuffer -flags low_delay -framedrop rtsp://<camera-ip>:8554/cam
```

#### Connect workers

Any number of workers can consume the same feed simultaneously:

```bash
rt-sketch --source video:rtsp://<camera-ip>:8554/cam \
  --stream-tcp <viewer-ip>:9900 \
  --stream-name "bot-A"
```

### LAN setup (no internet required)

All machines on the same local network. Best for offline installations or workshops.

```
                                    [Worker A] ──TCP──┐
[Camera + mediamtx] ──RTSP──→      [Worker B] ──TCP──┤──→ [Viewer] ──WebSocket──→ [Browser]
                                    [Worker C] ──TCP──┘
```

**Camera machine** — start mediamtx and the webcam feed (see above), then:

**Viewer machine** (can be the camera machine or any other):
```bash
rt-viewer
```

**Worker machines** — consume the RTSP feed, stream lines to the viewer:
```bash
rt-sketch --source video:rtsp://<camera-ip>:8554/cam \
  --stream-tcp <viewer-ip>:9900 \
  --stream-name "bot-A"
```

**Alternative: UDP multicast (no mediamtx needed)**

For LAN-only setups where simplicity matters, you can skip mediamtx and use UDP multicast directly. Any number of workers can consume the feed with zero configuration:

```bash
# Camera machine (macOS)
ffmpeg -f avfoundation -framerate 30 -video_size 640x480 -i "0:" \
  -c:v libx264 -preset ultrafast -tune zerolatency \
  -f mpegts udp://239.0.0.1:1234

# Camera machine (Linux)
ffmpeg -f v4l2 -framerate 30 -video_size 640x480 -i /dev/video0 \
  -c:v libx264 -preset ultrafast -tune zerolatency \
  -f mpegts udp://239.0.0.1:1234

# Workers
rt-sketch --source video:udp://239.0.0.1:1234 \
  --stream-tcp <viewer-ip>:9900 \
  --stream-name "bot-A"
```

UDP multicast is the simplest option — no extra software, any number of clients, minimal latency. The downside is it only works on a LAN and doesn't traverse tunnels or the internet.

### Remote setup (over the internet via Cloudflare Tunnels)

Camera and viewer on one network, workers on remote machines. Uses [Cloudflare Tunnels](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/) to avoid port forwarding or VPNs.

mediamtx serves HLS over HTTP (port 8888 by default), which works through Cloudflare Tunnels. RTSP does not traverse tunnels, so remote workers use the HLS endpoint instead.

```
                                                           [Remote Worker A] ──┐
[Camera + mediamtx] ──HLS──→ [cloudflared] ──tunnel──→ ──→                    │
                                                        ──→                    │
                                                           [Remote Worker B] ──┤──→ [cloudflared] ──tunnel──→ [Viewer]
                                                           [Remote Worker C] ──┘
```

Workers only make outbound connections (to the video feed and viewer tunnels), so they don't need to be publicly accessible or run `cloudflared` tunnels of their own. Use `--auto-start` so they begin drawing immediately without needing the web UI.

**Camera machine** — start mediamtx and the webcam feed (see above), then expose the HLS endpoint:
```bash
cloudflared tunnel --url http://localhost:8888 --name video-feed
```

Tunnel the viewer TCP port and web UI:
```bash
cloudflared tunnel --url tcp://localhost:9900 --name viewer-tcp
cloudflared tunnel --url http://localhost:9901 --name viewer-web
```

**Remote worker machines** — consume HLS over the tunnel:
```bash
# Forward the viewer tunnel to a local port
cloudflared access tcp --hostname viewer-tcp.your-domain.com --url localhost:5001

# Run rt-sketch headless
rt-sketch --source video:https://video-feed.your-domain.com/cam/index.m3u8 \
  --stream-tcp localhost:5001 \
  --stream-name "remote-A" \
  --auto-start \
  --wait-for-viewer
```

`--wait-for-viewer` blocks until the viewer is reachable, retrying every second. This ensures no lines are lost if the worker starts before the viewer is up. Without it, lines drawn before the viewer connects are silently dropped.

HLS adds a few seconds of latency compared to RTSP, but this doesn't matter — rt-sketch grabs the latest available frame whenever it's ready, so the drawing tracks the live feed with only a small delay. The line stream itself is 32 bytes per update, so tunnel overhead is negligible.

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

## Building from source

```bash
# Requires Rust (2021 edition)
cargo build --release
# Binaries: target/release/rt-sketch, target/release/rt-viewer
```

### Make targets

| Target | Description |
|--------|-------------|
| `make build` | Compile release binaries |
| `make run` | Run with webcam (release) |
| `make run-image` | Run with a static test image |
| `make record` | Run with webcam and record to `recording.mkv` |
| `make record-image` | Run with a static image and record to `recording.mkv` |
| `make dev` | Run in debug mode with a test image |
| `make dev-webcam` | Run in debug mode with webcam |
| `make snap` | Capture a single webcam frame and save as test image |
| `make check` | Run formatter, clippy, and tests |
| `make test` | Run tests only |
| `make fmt` | Format code |
| `make clean` | Remove build artifacts |
