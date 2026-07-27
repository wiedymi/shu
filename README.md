# Shu

**Shu is a tiny, declarative, agent-friendly repository library.**

Keep the Git repositories you care about in a portable TOML catalog. Restore them into predictable paths on a new machine, see what is missing or uncatalogued, and give agents one reliable command for resolving a repository to a local path.

## Install

Until the first tagged release, build from source:

```sh
cargo install --git https://github.com/wiedymi/shu
```

Tagged releases publish verified archives and simple installers through GitHub
Releases. The shell installer supports macOS and Linux; the PowerShell installer
supports Windows:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/wiedymi/shu/releases/latest/download/shu-installer.sh | sh
```

```powershell
irm https://github.com/wiedymi/shu/releases/latest/download/shu-installer.ps1 | iex
```

Both installers verify the downloaded archive against the release's
`SHA256SUMS` file. Homebrew and Scoop are planned after the first release.
The `shu` name is already taken on crates.io, so crates.io is not a planned
distribution channel.

After installing through GitHub Releases, update Shu itself without rerunning
the installer:

```sh
shu upgrade
shu upgrade --version 0.1.0
```

`shu upgrade` verifies the downloaded executable against `SHA256SUMS` before
replacing the current binary. It is separate from `shu update`, which refreshes
the configured repository catalog and restores any newly missing repositories.

## Quick start

```sh
shu init
shu add github.com/example-org/widget-service --tag work --tag backend
shu add . --state active --note "Current project"
shu status
shu doctor
shu restore
```

On a new machine, point Shu at a saved catalog:

```sh
shu restore github.com/your-account/repository-library
```

Catalog sources may be a local `shu.toml`, an HTTPS URL, a GitHub Gist, or a Git repository containing `shu.toml`. Use `--file` for a catalog stored below a repository root.

Ready-to-copy catalog templates are in [`examples/`](examples/). To try one locally, change its repository entries to repositories you can access, then run:

```sh
shu restore ./examples/personal-library.toml
```

## Fast navigation

Shu includes its own cross-platform fuzzy picker; `fzf` is not required.
Install the small shell wrapper once, then typing bare `shu` opens the picker,
where typing narrows results, arrow keys move, Enter enters the repository, and
Esc cancels.

```sh
# Bash, Zsh, or another POSIX shell: add this output to its startup file.
shu shell init bash

# Fish, PowerShell, Nushell, and POSIX sh are also supported.
shu shell init fish
shu shell init powershell
shu shell init nushell
shu shell init posix
```

The wrapper preserves every normal command (`shu restore`, `shu status`, and so
on) and only intercepts `shu` with no arguments. The binary-level picker is
also available for scripting:

```sh
shu pick --tag work --path-only
shu pick --filter api --path-only
```

## Agent use

```sh
repo_path="$(shu ensure github.com/example-org/widget-service --path-only)"
```

`--path-only` emits exactly one absolute path on stdout; diagnostics use stderr. `shu list --json` provides a stable, versioned JSON format.

## Setup diagnostics

`shu doctor` checks Git, the active catalog, and whether the configured
repository root is usable without changing repositories or making network
requests. To also refresh and validate the remembered catalog source, run:

```sh
shu doctor --check-source
```

The latter may refresh Shu's private catalog cache but never changes your
repository clones.

## Help and documentation

Every command explains its inputs and safety behavior:

```sh
shu --help
shu restore --help
```

Generate and open the Rust implementation reference locally:

```sh
cargo doc --no-deps --document-private-items --open
```

The generated entry point is `target/doc/shu/index.html`.

## Testing

`cargo test` runs both unit tests and offline end-to-end CLI workflows. The
integration tests create temporary bare Git remotes and use process-local Git
URL rewriting, so they do not require network access or alter global Git
configuration.

The CI matrix runs this suite on Linux, macOS, and Windows. Linux CI also
builds the production Docker image and executes the same style of workflow in
the container.

```sh
cargo test
docker build --tag shu:test .
docker run --rm shu:test --help
```

## Safety

Shu never executes repository hooks, does not delete repositories, and never overwrites a destination conflict. Restore only clones missing repositories; it does not pull, reset, or change existing working trees.

## License

MIT
