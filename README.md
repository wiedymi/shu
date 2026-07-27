# Shu

**Shu is a tiny, declarative, agent-friendly repository library.**

Keep the Git repositories you care about in a portable TOML catalog. Restore them into predictable paths on a new machine, see what is missing or uncatalogued, and give agents one reliable command for resolving a repository to a local path.

## Install

Until the first release, build from source:

```sh
cargo install --path .
```

Release channels will be GitHub Releases, a Homebrew tap, a Scoop bucket, and crates.io.

## Quick start

```sh
shu init
shu add github.com/example-org/widget-service --tag work --tag backend
shu add . --state active --note "Current project"
shu status
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
