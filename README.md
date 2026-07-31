# Shu

[![Release](https://img.shields.io/github/v/release/wiedymi/shu?display_name=tag&include_prereleases&sort=semver&style=flat-square)](https://github.com/wiedymi/shu/releases)
[![Checks](https://img.shields.io/github/actions/workflow/status/wiedymi/shu/ci.yml?branch=main&style=flat-square&label=checks)](https://github.com/wiedymi/shu/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/wiedymi/shu?style=flat-square)](LICENSE)

**A small, personal library for your Git repositories.**

Shu remembers the projects you care about, puts new clones in predictable
places, and lets you jump to them quickly. Its catalog is a readable
`shu.toml` file—your repositories stay normal Git repositories under your
control.

https://github.com/user-attachments/assets/63249a31-c91e-44bb-9ae8-167468ead2da

## Install

Download a release for macOS, Windows, or Linux from
[GitHub Releases](https://github.com/wiedymi/shu/releases), or install it with:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/wiedymi/shu/releases/latest/download/shu-installer.sh | sh
```

```powershell
irm https://github.com/wiedymi/shu/releases/latest/download/shu-installer.ps1 | iex
```

The installers verify the download against the release checksums. To build
from source instead:

```sh
cargo install --git https://github.com/wiedymi/shu
```

## Start here

Add a project you already have, or clone one you want. Shu creates its local
catalog automatically and uses `~/shu` as the default library root.

```sh
# From inside an existing Git repository
shu add .

# Clone and remember a repository
shu add github.com/example-org/api
# `shu clone github.com/example-org/api` means the same thing.
```

Now find it whenever you need it:

```sh
shu list
shu path api
shu pick
```

`shu path` prints the preferred local checkout. `shu pick` opens the fuzzy
picker and returns the selected path. The shell integration below makes plain
`shu` open that picker and change your current directory.

## Everyday commands

| What you want | Command |
| --- | --- |
| Add the current checkout without moving it | `shu add .` |
| Clone a repository into your library | `shu add github.com/you/project` |
| Create a fresh local repository | `shu new github.com/you/project` |
| Find and open a project | `shu pick` or plain `shu` after shell setup |
| Print a project path for a script | `shu path project` |
| See clones and Git worktrees | `shu locations project` |
| See what is missing locally | `shu status` |
| Clone every missing catalogued project | `shu restore` |
| Restore one named group | `shu restore --collection work` |
| Discover projects in a directory | `shu scan ~/Development --add` |
| Check your setup | `shu doctor` |

Repository names can be the full identity (`github.com/you/project`), a unique
suffix, or a unique name such as `project`.

### Pick and jump

Install the tiny shell wrapper once:

```sh
# Pick the shell you use: bash, zsh, fish, nushell, or posix.
shu shell init zsh
```

```powershell
shu shell init pwsh
```

Open a new terminal afterwards. Then plain `shu` shows the fuzzy picker; choose
a repository or one of its Git worktrees and your shell changes into it.
`shu pick` remains useful when you only need the selected path.

### Keep repositories organized

Adding `.` records an existing checkout where it already lives. Adding a remote
identity clones it below the library root:

```text
~/shu/github.com/you/project
```

Mark projects for later without moving or deleting anything:

```sh
shu edit project --state parked --note "Waiting for the next release"
shu edit project --state reference
shu archive project
```

The available states are `active`, `parked`, `reference`, and `archived`.
Shu never deletes repositories or resets working trees.

### Organize and restore collections

Tags describe repositories; collections are portable named queries over those
tags. They do not duplicate membership or change clone paths. A collection with
multiple tags requires every tag.

```toml
[collections]
work = { tags = ["work"] }
platform = { tags = ["platform", "rust"] }
```

Use a collection anywhere Shu accepts repository filters:

```sh
shu list --collection work
shu pick --collection platform
shu restore --collection work
```

Repeat `--tag` for the same one-off all-tags match:

```sh
shu restore --tag work --tag rust
```

When restoring a newly supplied catalog source, Shu first asks whether to
select named collections or individual repositories. Entering a selection only
previews it; type `yes` to start cloning. Use `--yes` for an unattended restore
of every matching repository.

If you want to bring a clean existing checkout into Shu's managed layout,
preview the move first:

```sh
shu add . --migrate --dry-run
shu add . --migrate
```

## Create a repository

Create a new local Git repository in Shu's library:

```sh
shu new github.com/you/new-project --tag experiment
```

To also create a private GitHub repository and set it as `origin`, use the
authenticated GitHub CLI:

```sh
shu doctor --check-github
shu new github.com/you/private-project --github
```

Pass `--public` only when you explicitly want a public repository. If GitHub
CLI is unavailable, create the remote yourself and add it with Git as usual.

## Use the same library on another machine

Sync is optional. It stores your catalog in a normal private Git repository,
using the credentials you already use for Git. Shu does not store tokens or
create extra state files.

Create a private catalog repository automatically with GitHub CLI:

```sh
shu sync init github.com/you/shu-catalog --github
```

Or create an empty private repository with any Git host first, then point Shu
at it:

```sh
shu sync init git@github.com:you/shu-catalog.git
```

After you change your catalog, publish it:

```sh
shu sync
```

On another machine, restore the catalog and its missing projects:

```sh
shu restore git@github.com:you/shu-catalog.git
```

The synced catalog contains repository identities, Git remotes, states, tags,
and notes. Your local root and local checkout paths stay private to each
machine, so restore places managed projects below that machine's root. The
catalog repository itself is a normal checkout below the root, but it is not
shown in Shu's repository list or picker.

## `shu.toml`

`shu.toml` is the only configuration file Shu creates. You can edit it by hand
or use the commands above.

```toml
version = 1
root = "~/shu"

[[repos]]
source = "github.com/your-name/project"
state = "active"
tags = ["personal", "rust"]
note = "A project I work on regularly"
paths = ["github.com/your-name/project"]
primary = "github.com/your-name/project"

[sync]
remote = "git@github.com:you/shu-catalog.git"
file = "shu.toml"
ref = "main"
```

Paths below `root` are stored relative to it. A checkout at
`github.com/your-name/project` therefore resolves to
`~/shu/github.com/your-name/project` with the default root. Paths outside the
root are absolute and stay on the machine where they were recorded. Git
worktrees are discovered when needed rather than stored in the catalog.

For scripts and coding agents, use `ensure` when a checkout may be missing:

```sh
repo_path="$(shu ensure github.com/example-org/project --path-only)"
```

## More help

```sh
shu --help
shu <command> --help
shu doctor --check-source
```

## Security

Please read [SECURITY.md](SECURITY.md) before reporting a vulnerability.

## License

MIT
