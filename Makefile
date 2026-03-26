.PHONY: build run clean test release check fmt clippy asm help

# Default target
help:
	@echo "Available targets:"
	@echo "  make build      - Build the project in debug mode"
	@echo "  make release    - Build the project in release mode"
	@echo "  make run        - Run the project"
	@echo "  make test       - Run tests"
	@echo "  make check      - Check the project without building"
	@echo "  make fmt        - Format the code"
	@echo "  make clippy     - Run clippy linter"
	@echo "  make asm        - Generate assembly from example.c"
	@echo "  make clean      - Clean build artifacts"

build:
	cargo build

release:
	cargo build --release

run: build
	cargo run -- example.c

test:
	cargo test

check:
	cargo check

fmt:
	cargo fmt

clippy:
	cargo clippy -- -D warnings

asm:
	gcc -S example.c -o example.s

clean:
	cargo clean
