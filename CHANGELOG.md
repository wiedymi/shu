# Changelog

All notable changes to Shu are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and releases use [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `shu doctor` for validating the local setup and, optionally, the remembered
  catalog source.
- `shu upgrade` for installing a checksummed GitHub Release binary without
  rerunning an installer.
- GitHub Release packaging with checksums and simple macOS/Linux and Windows
  installers.
- A plain-language security policy, scheduled RustSec audits, and Dependabot
  updates for Cargo and GitHub Actions.

### Security

- Reject repository identities that could escape the configured repository root.
- Pin GitHub Actions to immutable revisions; Dependabot maintains the pins.

## [0.1.0] - 2026-07-27

### Added

- Portable TOML catalogs with lifecycle states, tags, and notes.
- Safe repository restoration from local files, HTTPS URLs, Gists, and Git
  repositories.
- Agent-oriented `ensure`, `path`, JSON, and path-only interfaces.
- Built-in fuzzy repository picker and shell navigation wrappers.
- Offline CLI integration tests and Docker end-to-end coverage.

[Unreleased]: https://github.com/wiedymi/shu/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/wiedymi/shu/releases/tag/v0.1.0
