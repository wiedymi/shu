# Shu

[![Release](https://img.shields.io/github/v/release/wiedymi/shu?display_name=tag&include_prereleases&sort=semver&style=flat-square)](https://github.com/wiedymi/shu/releases)
[![Checks](https://img.shields.io/github/actions/workflow/status/wiedymi/shu/ci.yml?branch=main&style=flat-square&label=checks)](https://github.com/wiedymi/shu/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/wiedymi/shu?style=flat-square)](LICENSE)

**Your simple library of Git repositories.**

Shu keeps one small, readable `shu.toml` catalog of the repositories you care
about. Put it in Git, restore it on a new computer, and always know where a
project lives locally.

- Restore your repository collection with one command.
- Keep old projects as active, parked, reference, or archived—without deleting them.
- Find a repository quickly with Shu's built-in fuzzy picker.
- Give scripts and coding agents one reliable command for getting a local path.

## Get started

Create a private, synced catalog. This creates the GitHub repository, commits
the initial `shu.toml`, and makes it your active catalog:

```sh
shu doctor --check-github
shu sync init github.com/you/shu-catalog --github
shu add .
shu add github.com/example-org/useful-project --state reference
shu sync
```

`--github` creates a private repository by default. Add `--public` only when
you intentionally want a public repository. If you already created an empty
remote with another provider, use `shu sync init <remote>` instead.

On a new machine:

```sh
shu restore github.com/your-name/your-repository-library
```

Shu reads the repository's root-level `shu.toml`, puts missing repositories
under `~/shu` by default, and leaves existing repositories alone. `shu add .`
adds that current clone to the repository's `paths` list in the same catalog,
so it remains available to `shu`, `shu path`, and the picker without an
unnecessary move. Those locations stay on the current machine. If you start
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
shu add github.com/you/my-project # Add and clone a repository
shu clone github.com/you/my-project # Alias for `shu add`
shu locations my-project # Show every known clone and its Git worktrees
shu edit my-project --state parked --note "Paused until the next release"
shu add . --migrate     # Move a clean local repository into ~/shu's layout
```

To make bare `shu` open the picker, install its small shell wrapper once:

```sh
# Use bash, zsh, fish, nushell, or posix as appropriate.
shu shell init bash
```

```powershell
shu shell init pwsh
```

Shu writes only a clearly marked block to the appropriate startup file and
will replace that block safely when run again. Open a new terminal afterwards:
a program cannot modify its parent shell's current session. To inspect or
manage the wrapper yourself, use `shu shell init pwsh --print` or provide an
explicit target with `shu shell init pwsh --path ./profile.ps1`.

The picker offers repositories that are actually present on this machine,
including every existing clone recorded with `shu add .` and real Git
worktrees discovered dynamically from those clones. `--migrate` remains the
explicit option to move a clean clone into Shu's managed root. If `shu status`
shows a repository as **missing**, materialize it with `shu ensure <repository>`;
run `shu add .` from an existing clone to record it.

For scripts and agents:

```sh
repo_path="$(shu ensure github.com/example-org/project --path-only)"
```

## `shu.toml` reference

`shu.toml` is Shu's only user-facing configuration file. It describes the
repositories you care about and, when you add existing clones, the local paths
where you keep them. There is one `[[repos]]` entry for each repository
identity.

```toml
version = 1
root = "~/shu"

[[repos]]
source = "github.com/your-name/project"
state = "active"
tags = ["personal", "rust"]
note = "A project I work on regularly"
paths = [
  "github.com/your-name/project",
  "C:/Users/you/Projects/project",
]
primary = "github.com/your-name/project"
```

| Field | Meaning | Default |
| --- | --- | --- |
| `version` | Catalog format version. | Required; currently `1`. |
| `root` | Local canonical destination used by `shu add`, `shu clone`, and `shu restore` for a repository that has no usable local clone. | `~/shu` |
| `repos[].source` | Repository identity: `host/namespace/repository`. HTTPS and SSH URLs are normalized to this form by `shu add`. | Required. |
| `repos[].remote` | Explicit SSH transport preserved for later clones. Omitted for normal HTTPS cloning. | Optional. |
| `repos[].state` | Your lifecycle label: `active`, `parked`, `reference`, or `archived`. It is never inferred from age. | `active` |
| `repos[].tags` | Optional labels for filtering and grouping. | `[]` |
| `repos[].note` | Optional human context about why the repository is kept. | Absent |
| `repos[].paths` | Local full-clone paths. Paths below `root` are relative; external paths are absolute. | `[]` |
| `repos[].primary` | Local clone preference for `shu path` and the first picker result. | The first valid path, then the managed path. |

`paths`, `primary`, and `root` describe only the current machine. A managed
path is stored relative to `root`, so `github.com/your-name/project` resolves
to `~/shu/github.com/your-name/project` on one machine and that machine's
configured root on another. An external clone remains absolute and local.
Git worktrees are deliberately not stored: Shu discovers them from each valid
clone every time it opens the picker or runs `shu locations`.

`root` does not move anything by itself. It only determines the canonical
managed destination:

```text
<root>/<host>/<namespace>/<repository>
```

For example, `github.com/your-name/project` with the default root belongs at:

```text
~/shu/github.com/your-name/project
```

Choose the preferred clone explicitly with:

```sh
shu locations project --primary /path/to/project
```

Inspect all known clones and dynamically discovered worktrees with:

```sh
shu locations project
```

To register another existing full clone, run this from inside it:

```sh
shu add .
```

The catalog is deliberately data-only: no secrets, setup hooks, or arbitrary
commands. Shu never deletes repositories, resets a working tree, or overwrites
a conflicting directory.

When `shu status` says a repository is **missing**, Shu cannot find a recorded
local clone or its canonical destination. It prints the expected path and the
exact `shu add <repository>` command to create it. To update catalog
metadata without changing repository files:

```sh
shu edit my-project --state reference --note "Useful implementation reference"
shu edit my-project --clear-note
```

To bring an existing clean clone into Shu's managed layout, preview the move
first, then confirm it:

```sh
shu add . --migrate --dry-run
shu add . --migrate
```

Migration only moves valid, clean working trees. Shu refuses repositories with
staged, unstaged, or untracked changes; linked Git worktrees; an existing
canonical destination; or a destination on another filesystem. It never copies
then deletes a repository as a fallback.

## Creating repositories

Create an empty local Git repository directly in Shu's managed layout:

```sh
shu new github.com/you/new-project --tag experiment
```

This creates and catalogues the local repository on its `main` branch. It does
not create a hosted repository, commit, or push. To create the matching GitHub
repository explicitly, use the authenticated GitHub CLI:

```sh
shu doctor --check-github
shu new github.com/you/private-project --github
```

`--github` is optional and provider-specific; it creates a private repository
unless you pass `--public`. If it is unavailable or lacks
permission, Shu leaves the catalog unchanged and explains how to create the
remote manually.

## Updating

```sh
shu update              # Refresh the configured Git catalog and restore missing repositories
shu upgrade             # Install the latest verified Shu release
```

## Syncing a private catalog

Shu uses a normal Git checkout and your existing credentials; it does not store
credentials or create sidecar state. Create an empty private Git repository
with your preferred provider, or let Shu create one through `gh`:

```sh
shu sync init github.com/you/shu-catalog --github
```

This creates a dedicated catalog checkout, commits the active catalog, pushes
`main`, and activates sync. The catalog checkout is intentionally not added to
`[[repos]]`. Before using GitHub creation, verify the installed CLI:

```sh
shu doctor --check-github
```

To use an already-created remote, it must have no branches; run `shu sync init
<remote>`. For a remote that already contains a Shu catalog, use `shu restore
<remote>` instead. `sync init` writes this configuration into the catalog:

```toml
[sync]
remote = "git@github.com:you/shu-catalog.git"
file = "shu.toml"
ref = "main"
```

After changing the local catalog, publish it with:

```sh
shu sync
```

Shu keeps that catalog as a normal checkout at the canonical path below your
configured repository root (for example `~/shu/github.com/you/shu-catalog`).
It is not added to `[[repos]]`, so it never appears in your repository picker.

`sync` uses your existing Git credentials. It refuses a dirty checkout or a
remote change that has not been restored first; run `shu restore` again to
review the remote version. It never stores credentials, force-pushes, resets,
or merges changes. The synced Git catalog contains only portable repository
metadata (`source`, `remote`, lifecycle, tags, notes, and `[sync]`). Local
`root`, `paths`, and `primary` remain in each machine's active `shu.toml` and
are merged back after `shu restore` or `shu update`.

## Command model

- `shu new`: create and catalogue a local repository.
- `shu add` / `shu clone`: register an existing clone, or register and clone a remote.
- `shu ensure`: materialize one already-catalogued repository and print its path.
- `shu restore`: restore all missing repositories, optionally after loading a catalog source.
- `shu sync`: commit and push catalog edits; `shu update`: pull catalog edits and restore missing repositories.

A Git repository containing `shu.toml` can be restored only when that file has
a matching `[sync]` table. A GitHub Gist is a read-only catalog source: it can
be restored, but cannot be used with `shu sync` or `shu update`.

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
