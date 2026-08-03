---
name: shu
description: Manage a user's Git repository library through the Shu CLI. Use when the user mentions Shu or shu.toml, asks to work with their repository library, or asks to locate, restore, audit, organize, migrate, classify, or clone top-level repositories across their machine.
---

# Use Shu

Use Shu as the repository-library layer. Use ordinary Git commands for work
inside a checkout, such as branches, commits, rebases, and worktrees.

## Establish the current contract

1. Verify that `shu --version` succeeds. If Shu is unavailable, report that and
   install it only when the user asks.
2. Read `shu --help` and the relevant `shu <command> --help` before acting.
   Treat the installed CLI as the version-specific source of truth rather than
   relying on copied command documentation.
3. Start with read-only inspection. Prefer stable JSON from `doctor`, `list`,
   `status`, and `scan` without `--add`; do not assume other commands support
   JSON.
4. Use the active catalog unless the user supplies a specific `--catalog`.

## Resolve and restore repositories

- Use `shu path <selector>` when the checkout must already exist.
- Use `shu ensure <selector> --path-only` when a missing catalogued repository
  may need to be cloned. Keep the returned path on stdout suitable for scripts.
- Use `shu --json list` to resolve ambiguity, and prefer the full repository
  identity when selecting among similarly named repositories.
- Avoid the interactive picker in unattended work. Use selectors, filters, and
  path-only or JSON output instead.

## Audit and organize repositories

- Scan bounded, known source roots rather than an entire home directory or
  filesystem. Use `shu --json scan <root>` for read-only discovery.
- Supplement Shu with focused Git inspection when the user requests a complete
  audit that must include repositories intentionally omitted by normal scans,
  such as hidden dependency trees, submodules, or shallow clones.
- Classify and report candidates before making bulk catalog or filesystem
  changes. Preserve the user's stated exclusions across follow-up work.
- Prefer Shu commands over editing `shu.toml`. Edit the catalog directly only
  when the requested catalog feature has no CLI command, preserve unknown
  fields, and validate it with `shu --json doctor` afterward.

## Change the library safely

- Use `shu add .` to record an existing checkout without moving it. Use
  `shu add <remote-identity>` when the user wants Shu to clone and catalogue a
  top-level repository.
- Treat migration as a separate filesystem-moving operation. Run
  `shu --non-interactive add <path> --migrate --dry-run`, report the result,
  and obtain approval for the exact candidates. Immediately repeat each
  dry-run before executing its migration with `--yes`, and migrate sequentially.
- Never manually move a repository to bypass a failed migration preflight.
  Leave dirty repositories, live linked worktrees, occupied destinations, and
  other rejected candidates untouched.
- Use `shu remove <selector>` only to remove a catalog entry. Do not describe it
  as deleting the local checkout or remote repository.
- Treat `shu sync` as publication to the configured catalog remote. Run it only
  when the user explicitly requests that publication, and report local catalog
  changes that remain unsynced.

## Preserve user intent

- Keep read-only investigation read-only until the user authorizes changes.
- Do not expand an approved repository set because additional candidates are
  technically eligible.
- Do not delete, reset, clean, move, or publish repositories unless the user
  explicitly requested that effect.
