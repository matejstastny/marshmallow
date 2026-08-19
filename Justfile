default:
    @just --list

run:
    cargo run -p marshmallow

build:
    cargo build --workspace

release:
    cargo build --workspace --release

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
