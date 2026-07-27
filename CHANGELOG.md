# Changelog

All notable changes to Shu are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and releases use [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.4] - 2026-07-27

### Added

- Clear upgrade stages and live download progress for `shu upgrade`, with a
  final transferred-byte summary when output is not attached to a terminal.

## [0.1.3] - 2026-07-27

### Added

- `shu edit <repository>` for changing a catalog entry's lifecycle state and
  note without touching repository files.
- `pwsh` as an alias for `shu shell init powershell`.

### Changed

- Create an empty catalog automatically for everyday commands instead of
  blocking first use on `shu init`.
- Make `shu status` explain a missing repository's expected path and the exact
  `shu ensure` command that restores it.
- Render human errors consistently with a colored heading, an optional cause,
  and a help line when attached to a terminal.
- Refactor repository identity parsing into small, documented parsing and
  validation steps.

### Fixed

- Document and test the PowerShell setup expression so generated shell code is
  evaluated as one script rather than an array of output lines.
- Explain the common `shu update . --state ...` mix-up and point to `shu edit`.

## [0.1.2] - 2026-07-27

### Fixed

- Install the macOS/Linux release binary directly so the shell installer also
  works on minimal systems without `tar` and `xz` installed.

## [0.1.1] - 2026-07-27

### Fixed

- Correct the Windows release ZIP layout and let the installer safely find a
  nested `shu.exe` in older archives.
- Add clear progress messages to the macOS/Linux and Windows installers.

## [0.1.0-rc.3] - 2026-07-27

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

### Changed

- Update direct Rust dependencies and pinned GitHub Actions to their current
  supported releases.
- Make the Docker end-to-end image capable of exercising `.tar.xz` installer
  payloads.

## [0.1.0-rc.1] - 2026-07-27

### Added

- Portable TOML catalogs with lifecycle states, tags, and notes.
- Safe repository restoration from local files, HTTPS URLs, Gists, and Git
  repositories.
- Agent-oriented `ensure`, `path`, JSON, and path-only interfaces.
- Built-in fuzzy repository picker and shell navigation wrappers.
- Offline CLI integration tests and Docker end-to-end coverage.

[Unreleased]: https://github.com/wiedymi/shu/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/wiedymi/shu/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/wiedymi/shu/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/wiedymi/shu/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/wiedymi/shu/compare/v0.1.0...v0.1.1
[0.1.0-rc.3]: https://github.com/wiedymi/shu/compare/v0.1.0-rc.2...v0.1.0-rc.3
[0.1.0-rc.1]: https://github.com/wiedymi/shu/releases/tag/v0.1.0-rc.1
