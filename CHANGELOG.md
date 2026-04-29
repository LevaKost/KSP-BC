# Changelog

All notable changes to this project will be documented in this file. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Initial Cargo workspace bootstrap (`ksp-share` binary + `ksp_blueprintshare` library).
- TCP MVP transport with length-prefixed `bincode` framing.
- Protocol messages for the `HELLO → READY → META → ACK → DATA → EOF → VERIFY → DONE` flow.
- `clap`-based CLI: `send`, `receive`, `list`, `config`.
- KSP install detector for Linux, macOS and Windows (Steam + sensible
  fallbacks, plus a `KSP_ROOT` override).
- Lightweight `.craft` metadata parser for both KSP1 and KSP2 layouts.
- SHA-256 verification end-to-end with a progress bar via `indicatif`.
- GitHub Actions: `build.yml` (fmt + clippy + tests on Linux/macOS/Windows)
  and `release.yml` (cross-platform binaries on `v*.*.*` tags).
- Docs: `docs/protocol.md`, `docs/ksp-paths.md`, `docs/build-from-source.md`.

[Unreleased]: https://github.com/LevaKost/KSP-BC/compare/HEAD...HEAD
