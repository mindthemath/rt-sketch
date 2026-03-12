.PHONY: build run run-image dev check test clean fmt help

# Default test image (override with IMAGE=path)
IMAGE ?= test.jpg

## build: Compile release binary
build:
	cargo build --release

## run: Run with webcam (default args)
run: build
	./target/release/rt-sketch --source webcam

## run-image: Run with a static test image
run-image: build
	./target/release/rt-sketch --source image:$(IMAGE)

## dev: Run in debug mode with a test image
dev:
	cargo run -- --source image:$(IMAGE)

## dev-webcam: Run in debug mode with webcam
dev-webcam:
	cargo run -- --source webcam

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

## help: Show this help
help:
	@echo "rt-sketch — real-time video-to-SVG sketch engine"
	@echo ""
	@echo "Usage: make <target>"
	@echo ""
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/## /  /'
