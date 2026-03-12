.PHONY: build run run-image dev check test clean fmt help snap

# Default test image (override with IMAGE=path)
IMAGE ?= test.jpg
DEVICE ?= 0

## build: Compile release binary
build:
	cargo build --release

## run: Run with webcam (default args)
run: build
	./target/release/rt-sketch --source webcam:$(DEVICE) --canvas-height 15 --canvas-width 15

## run-image: Run with a static test image
run-image: build
	./target/release/rt-sketch --source image:$(IMAGE) --canvas-height 15 --canvas-width 15

## dev: Run in debug mode with a test image
dev:
	cargo run -- --source image:$(IMAGE)

## dev-webcam: Run in debug mode with webcam
dev-webcam:
	cargo run -- --source webcam:0

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
	cargo run -- --help

## help: Show this help
help:
	@echo "rt-sketch — real-time video-to-SVG sketch engine"
	@echo ""
	@echo "Usage: make <target>"
	@echo ""
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/## /  /'
