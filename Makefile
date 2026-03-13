.PHONY: build run run-image dev check test clean fmt help snap

# Default test image (override with IMAGE=path)
IMAGE ?= test.jpg
DEVICE ?= 0
FPS ?= 24
THREADS ?= 2

## build: Compile release binary
build:
	cargo build --release

## run: Run with webcam (default args)
run: build
	./target/release/rt-sketch --source webcam:$(DEVICE) --canvas-height 20 --canvas-width 20 --fps $(FPS)

record: build
	./target/release/rt-sketch --source webcam:0 --stream-output recording.mkv --fps $(FPS) --canvas-width 20 --canvas-height 20

record-image: build
	./target/release/rt-sketch --source image:$(IMAGE) --stream-output recording.mkv --fps $(FPS) --canvas-height 20 --canvas-width 20

## run-image: Run with a static test image
run-image: build
	./target/release/rt-sketch --source image:$(IMAGE) --canvas-height 15 --canvas-width 15 --fps $(FPS)

## dev: Run in debug mode with a test image
dev:
	cargo run -p rt-sketch -- --source image:$(IMAGE)

## dev-webcam: Run in debug mode with webcam
dev-webcam:
	cargo run -p rt-sketch -- --source webcam:0

## check: Run clippy and tests
check: fmt
	cargo clippy -- -D warnings
	cargo test

## test: Run tests only
test:
	cargo test

## clean: Remove build artifacts
clean:
	cargo clean

## fmt: Format code
fmt:
	cargo fmt

## snap: Capture a single frame from webcam and save as IMAGE
snap:
	ffmpeg -f avfoundation -pixel_format uyvy422 -framerate 30 -video_size 1280x720 -i "$(DEVICE)" -frames:v 1 -update 1 -y $(IMAGE)
	@echo "Saved webcam frame to $(IMAGE)"

devhelp:
	cargo run -p rt-sketch -- --help

streamA:
	cargo run --release -p rt-sketch -- --source webcam --stream-tcp localhost:9900 --stream-name "cam-A" --fps 24 --wait-for-viewer --auto-start --threads $(THREADS)

streamB:
	cargo run --release -p rt-sketch -- --source webcam --stream-tcp localhost:9900 --stream-name "cam-B" --fps 24 --wait-for-viewer --auto-start --threads $(THREADS)

streamC:
	cargo run --release -p rt-sketch -- --source webcam --stream-tcp localhost:9900 --stream-name "cam-C" --fps 24 --wait-for-viewer --auto-start --threads 2

viewer:
	cargo run --release -p rt-viewer

webcam-macos:
	ffmpeg -f avfoundation -framerate 30 -video_size 640x480 -i "0:" \
		-c:v libx264 -preset ultrafast -tune zerolatency \
		-f rtsp rtsp://localhost:8554/cam

webcam-macos-udp:
	ffmpeg -f avfoundation -framerate 30 -video_size 640x480 -i "0:" \
		-c:v libx264 -preset ultrafast -tune zerolatency \
		-f mpegts udp://239.0.0.1:1234

webcam-linux:
	ffmpeg -f v4l2 -framerate 30 -video_size 640x480 -i /dev/video0 \
		-c:v libx264 -preset ultrafast -tune zerolatency \
		-f rtsp rtsp://localhost:8554/cam

## help: Show this help
help:
	@echo "rt-sketch — real-time video-to-SVG sketch engine"
	@echo ""
	@echo "Usage: make <target>"
	@echo ""
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/## /  /'
