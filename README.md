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

## Safety

Shu never executes repository hooks, does not delete repositories, and never overwrites a destination conflict. Restore only clones missing repositories; it does not pull, reset, or change existing working trees.

## License

MIT
