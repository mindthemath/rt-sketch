.PHONY: build run run-image dev check test clean fmt help snap draw-basic draw-piecewise draw-examples

# Default test image (override with IMAGE=path)
IMAGE ?= test.jpg
DEVICE ?= 0
FPS ?= 24

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

## draw-basic: Generate a grid of 9 panels, each with 3 chained curves from random bases
draw-basic: build
	./target/release/draw_curves --seed 42 --num-curves 9 --chain 3 --n-points 24 --step 24 -o curves_basic.svg
	@echo "Wrote curves_basic.svg"

draw-complex: build
	./target/release/draw_curves --basis fourier --chain 3 5 --piecewise 4 --rot 60 --noise 0.15 \
		  --cfg-rot 0 360 --cfg-scale-y 0.5 1.5 --step 18 --num-curves 9 -o curves_complex.svg	
## draw-piecewise: Generate piecewise envelope curves (3x45°, n=36, step=12)
draw-piecewise: build
	./target/release/draw_curves --seed 55 --num-curves 9 --piecewise 3 --rot 45 --noise 0.2 -o curves_piecewise.svg --n-points 24 --step 36
	@echo "Wrote curves_piecewise.svg"

## draw-examples: Generate all example SVGs into examples/
draw-examples: build
	./draw_examples.sh examples

devhelp:
	cargo run -- --help

## help: Show this help
help:
	@echo "rt-sketch — real-time video-to-SVG sketch engine"
	@echo ""
	@echo "Usage: make <target>"
	@echo ""
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/## /  /'
