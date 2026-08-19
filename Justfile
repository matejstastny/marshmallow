# list available commands
default:
    @just --list

# run the app in debug mode
run:
    cargo run -p marshmallow

# build in debug mode
build:
    cargo build --workspace

# build an optimized release binary
release:
    cargo build --workspace --release

# format all code
fmt:
    cargo fmt --all

# check formatting without writing changes
fmt-check:
    cargo fmt --all --check

# lint with clippy
lint:
    cargo clippy --workspace --all-targets

# run the test suite
test:
    cargo test --workspace

# run everything CI runs
check: fmt-check lint test

# remove build artifacts
clean:
    cargo clean
