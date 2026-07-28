# Changelog

All notable changes to Shu are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and releases use [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.15] - 2026-07-28

### Changed

- Catalog sync now publishes only portable repository metadata while preserving
  each machine's root, local clone paths, and preferred clone in its active
  `shu.toml`.
- Managed clone paths are recorded relative to the configured root, avoiding
  usernames and absolute managed-library paths in local catalog entries.
- Restoring a synced catalog merges portable metadata by repository identity,
  then materializes missing repositories below the receiving machine's root.
- Docker E2E installer coverage now selects the running Linux architecture.

## [0.1.14] - 2026-07-28

### Added

- `shu sync init` now checks Git author configuration before creating a remote
  and refuses already-populated catalog remotes with an activation hint.
- `shu doctor` reports Git author readiness; `--check-source` now verifies that
  the configured catalog remote and ref are reachable.
- Explicit SSH remotes added through `shu add`, migration, or scan are retained
  in `repos[].remote` so `shu ensure` and `shu restore` keep using SSH.

### Changed

- GitHub repositories created by `shu new --github` and `shu sync init --github`
  are private by default; `--public` makes publication intentional.
- `--json` now rejects commands without a stable JSON response contract.
- The setup and sync documentation now describes the supported Git catalog flow,
  command roles, and read-only Gist behavior.

## [0.1.13] - 2026-07-28

### Added

- `shu new <repository>` creates and catalogues an empty local Git repository
  on `main`, without requiring a remote or initial commit.
- `shu new <github-identity> --github --private` explicitly creates and
  attaches a private GitHub repository through the user's authenticated `gh`
  CLI.
- `shu sync init <repository>` creates, commits, publishes, and activates a
  dedicated catalog repository without adding it to `[[repos]]` or the picker.
- `shu doctor --check-github` verifies the optional GitHub CLI integration.

### Changed

- `shu scan` now excludes Git submodules as well as hidden directories and
  shallow clones, leaving only independently managed repositories.

## [0.1.12] - 2026-07-28

### Changed

- Catalog sync is now configured entirely in `shu.toml` with `[sync]`.
  Shu keeps the catalog as a normal, persistent checkout beneath the configured
  repository root instead of storing sidecar metadata or using a temporary
  clone.
- `shu update` reads the same `[sync]` configuration, and `shu doctor` checks
  the persistent checkout without hidden source state.

### Security

- Sync requires a clean, matching checkout and refuses remote changes until
  they are explicitly restored. It still never merges, resets, force-pushes,
  or stores credentials.

## [0.1.11] - 2026-07-28

### Added

- `shu sync` safely publishes catalog changes to the Git repository configured
  by `shu restore`, using the user's existing Git credentials.

### Changed

- The picker keeps its highlighted query at the bottom and anchors results
  directly above it for compact, fzf-style navigation.

### Security

- Sync refuses a remote catalog changed since the last restore and never
  merges, force-pushes, or stores credentials.

## [0.1.10] - 2026-07-28

### Added

- `shu add <repository>` now clones a missing remote repository into Shu's
  managed library. `shu clone` is a direct alias for the same workflow.
- The picker distinguishes independent clones from linked Git worktrees before
  selection.

### Changed

- The picker now presents a viewport-sized result list above its bottom search
  prompt, without wrapped detail columns.
- `shu scan` skips hidden directory trees and shallow clones, and groups each
  result's identity and path into a readable record.
- Default application support now uses the `com.wiedymi.shu` identifier.

## [0.1.9] - 2026-07-27

### Added

- Multiple local clone paths and an explicit preferred clone are now stored
  directly in each repository entry's `paths` and `primary` fields in the one
  `shu.toml` file.
- `shu locations <repository>` shows all recorded clones and dynamically
  discovered real Git worktrees. `--primary <path>` selects the clone used by
  `shu path`, `shu ensure`, and the first picker result.
- The picker now includes linked Git worktrees without storing them in TOML.

### Changed

- `shu add .` appends the current clone to its catalog entry instead of
  replacing a previously recorded clone. `shu add . --migrate` replaces the
  moved path and makes the managed destination primary.

## [0.1.8] - 2026-07-27

### Changed

- `shu add .` records an existing clone instead of treating the canonical Shu
  path as its only location. This was superseded by the one-file `paths` model
  in 0.1.9.

### Fixed

- PowerShell navigation selects one returned path before calling
  `Set-Location`, avoiding native-command output arrays.

## [0.1.7] - 2026-07-27

### Added

- `shu shell init <shell>` now installs an idempotent, clearly marked
  navigation wrapper into the selected shell's startup file. All supported
  shells retain `--print` for manual setup and `--path` for an explicit file.

### Fixed

- Draw the fuzzy picker on the terminal stream while reserving stdout for the
  selected path, so navigation wrappers can display the picker and capture the
  destination at the same time.
- Explain when no locally available repositories can be picked, including how
  to restore or migrate a catalogued repository.

## [0.1.6] - 2026-07-27

### Changed

- Reduced release-binary size with a dedicated size-focused Rust release
  profile and replaced `reqwest` with the smaller blocking `ureq` client.
- Use the platform certificate verifier for catalog and release HTTPS requests,
  preserving the user's operating-system trust store for arbitrary catalog
  source URLs.
- Build and record the size of all five release targets in pull-request CI,
  before a release is published.

## [0.1.5] - 2026-07-27

### Added

- `shu add . --migrate` for previewing and atomically moving a clean local
  Git working tree into Shu's canonical managed layout.

### Security

- Migration refuses dirty repositories, linked worktrees, existing targets,
  and non-atomic cross-filesystem moves.

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

[Unreleased]: https://github.com/wiedymi/shu/compare/v0.1.15...HEAD
[0.1.15]: https://github.com/wiedymi/shu/compare/v0.1.14...v0.1.15
[0.1.14]: https://github.com/wiedymi/shu/compare/v0.1.13...v0.1.14
[0.1.11]: https://github.com/wiedymi/shu/compare/v0.1.10...v0.1.11
[0.1.10]: https://github.com/wiedymi/shu/compare/v0.1.9...v0.1.10
[0.1.6]: https://github.com/wiedymi/shu/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/wiedymi/shu/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/wiedymi/shu/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/wiedymi/shu/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/wiedymi/shu/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/wiedymi/shu/compare/v0.1.0...v0.1.1
[0.1.0-rc.3]: https://github.com/wiedymi/shu/compare/v0.1.0-rc.2...v0.1.0-rc.3
[0.1.0-rc.1]: https://github.com/wiedymi/shu/releases/tag/v0.1.0-rc.1
