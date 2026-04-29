# Building from source

## Requirements

- Rust toolchain 1.75 or newer (`rustup update stable`).
- A C toolchain (Linux: `build-essential`, macOS: Xcode CLT, Windows:
  the MSVC build tools or `x86_64-pc-windows-msvc` toolchain).

## Build

```sh
git clone https://github.com/LevaKost/KSP-BC
cd KSP-BC
cargo build --release
```

The binary is placed at `target/release/ksp-share` (or
`target/release/ksp-share.exe` on Windows).

## Run the test suite

```sh
cargo test
```

The integration tests in `tests/` exercise the metadata parser and a
loopback TCP transfer.

## Lint

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

CI runs the same commands on every push and pull request — see
`.github/workflows/build.yml`.

## Cutting a release

Tag a commit on `main` with `v*.*.*` (e.g. `v0.1.0-alpha.1`) and push the
tag. `release.yml` builds Linux x86_64, macOS x86_64 + arm64 and Windows
x86_64 binaries plus their SHA-256 checksums and uploads them to the
GitHub Release for the tag.
