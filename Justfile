default:
    @just --list

run:
    cargo run -p marshmallow

build:
    cargo build --workspace

build-release:
    cargo build --workspace --release

release:
    ./scripts/release.sh

redo-release:
    ./scripts/redo-release.sh

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets

test:
    cargo test --workspace

check: fmt-check lint test

clean:
    cargo clean
