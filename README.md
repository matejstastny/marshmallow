# Marshmallow

A GTK4 / libadwaita app for importing and culling photos: scan one or more source folders, review each item (keep / trash), and copy the keepers into a target folder. Includes a background decode pipeline with a prerendered thumbnail cache for fast review of large RAW/HEIF/JPEG libraries.

## Features

- Scan multiple source directories for known media types
- Review screen with keep / trash / undecided decisions per item, preserved across re-scans
- Copy plan that skips identical files and renames on collision
- Background decode workers with an LRU cache, budgeted by byte size
- Prerendered on-disk thumbnail cache for instant review
- HEIF and JPEG (via `libheif` and `turbojpeg`) decoding with EXIF support
- Projects are saved to disk and can be resumed later

## Requirements

- Rust (stable — see [`rust-toolchain.toml`](rust-toolchain.toml))
- [`just`](https://github.com/casey/just) (optional, for the command shortcuts below)
- System libraries (Debian/Ubuntu package names):
    - `libgtk-4-dev` (>= 4.16)
    - `libadwaita-1-dev` (>= 1.5)
    - `libheif-dev`
    - `libturbojpeg0-dev`
    - `pkg-config`

## Getting started

```sh
just run
```

Without `just`:

```sh
cargo run -p marshmallow
```

## Development

```sh
just build    # debug build
just release  # optimized release build
just fmt      # format code
just lint     # clippy
just test     # run the test suite
just check    # fmt-check + lint + test, same as CI
```
