.PHONY: all build test fmt lint clean run check coverage

# Default target
all: check test

# Build the project
build:
	cargo build --release

# Run all tests
test:
	cargo test --all-features

# Run unit tests only
test-unit:
	cargo test --lib

# Run integration tests only
test-integration:
	cargo test --test '*'

# Format code
fmt:
	cargo fmt

# Run linter
lint:
	cargo clippy -- -D warnings

# Run formatter and linter
check: fmt lint

# Clean build artifacts
clean:
	cargo clean

# Run the application
run:
	cargo run

# Run with browse mode
browse:
	cargo run -- browse

# Run tests with coverage
coverage:
	cargo tarpaulin --out Html --output-dir coverage

# Install development dependencies
install-dev:
	rustup component add rustfmt clippy
	cargo install cargo-tarpaulin

# Quick development cycle
dev: check test run