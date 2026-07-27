# Shu

[![Release](https://img.shields.io/github/v/release/wiedymi/shu?display_name=tag&include_prereleases&sort=semver&style=flat-square)](https://github.com/wiedymi/shu/releases)
[![Checks](https://img.shields.io/github/actions/workflow/status/wiedymi/shu/ci.yml?branch=main&style=flat-square&label=checks)](https://github.com/wiedymi/shu/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/wiedymi/shu?style=flat-square)](LICENSE)

**Your portable library of Git repositories.**

Shu keeps a small, readable catalog of the repositories you care about. Put the
catalog in Git, restore it on a new computer, and always know where a project
lives locally.

- Restore your repository collection with one command.
- Keep old projects as active, parked, reference, or archived—without deleting them.
- Find a repository quickly with Shu's built-in fuzzy picker.
- Give scripts and coding agents one reliable command for getting a local path.

## Get started

Create a catalog beside your other personal configuration, then commit it to a
private repository or Gist:

```sh
shu --catalog ./shu.toml init
shu --catalog ./shu.toml add .
shu --catalog ./shu.toml add github.com/example-org/useful-project --state reference
git add shu.toml
```

On a new machine:

```sh
shu restore github.com/your-name/your-repository-library
```

Shu reads the repository's root-level `shu.toml`, puts missing repositories
under `~/shu` by default, and leaves existing repositories alone. If you start
with an empty machine, everyday commands create an empty local catalog for you.

## Install

Download a release for macOS, Windows, or Linux from
[GitHub Releases](https://github.com/wiedymi/shu/releases).

Once stable releases are public, the simplest installation commands are:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/wiedymi/shu/releases/latest/download/shu-installer.sh | sh
```

```powershell
irm https://github.com/wiedymi/shu/releases/latest/download/shu-installer.ps1 | iex
```

Both installers verify the downloaded archive against the release's
`SHA256SUMS` file. To build Shu directly from source:

```sh
cargo install --git https://github.com/wiedymi/shu
```

## Everyday use

```sh
shu                     # Pick a present repository and enter it (after shell setup)
shu status              # See what is present, missing, or uncatalogued
shu restore             # Clone catalogued repositories that are missing
shu doctor              # Check Git, the catalog, and the configured root
shu ensure my-project   # Ensure one repository exists and print its path
shu edit my-project --state parked --note "Paused until the next release"
```

To make bare `shu` open the picker, add its small shell wrapper once:

```sh
# Bash, Zsh, or another POSIX shell
eval "$(shu shell init bash)"

# Fish
shu shell init fish | source
```

```powershell
Invoke-Expression ((& shu shell init pwsh) -join [Environment]::NewLine)
```

For scripts and agents:

```sh
repo_path="$(shu ensure github.com/example-org/project --path-only)"
```

## A small catalog

```toml
version = 1
root = "~/shu"

[[repos]]
source = "github.com/your-name/project"
state = "active"
tags = ["personal", "rust"]
note = "A project I work on regularly"
```

The catalog is deliberately data-only: no secrets, setup hooks, or arbitrary
commands. Shu never deletes repositories, resets a working tree, or overwrites
a conflicting directory.

When `shu status` says a repository is **missing**, it means the canonical
clone is not yet under Shu's configured root. It prints the expected path and
the exact `shu ensure <repository>` command to create it. To update catalog
metadata without changing repository files:

```sh
shu edit my-project --state reference --note "Useful implementation reference"
shu edit my-project --clear-note
```

## Updating

```sh
shu update              # Refresh the saved catalog source and restore missing repositories
shu upgrade             # Install the latest verified Shu release
```

If a clone or release download is unavailable, Shu reports what failed and
suggests checking the path, network connection, or Git access. It does not try
to manage your credentials.

## Help

```sh
shu --help
shu doctor --check-source
```

For implementation documentation:

```sh
cargo doc --no-deps --document-private-items --open
```

## Security

Please read [SECURITY.md](SECURITY.md) before reporting a vulnerability.

## License

MIT
