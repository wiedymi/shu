# Shu contributor instructions

## Engineering

Before changing Shu, keep the implementation narrowly scoped and simplify it
where possible:

- Prefer facts that can be derived over new stored state.
- Remove or merge unnecessary branches, structures, and classes.
- Model closed state sets explicitly with enums.
- Check sizes, offsets, capacities, indices, and arithmetic for bounds issues.
- Do not introduce a concept unless it has a clear, user-facing justification.
- Preserve unrelated and untracked user work.

## Release process

Shu releases are cut from `main`. Do not create or use `release/*` branches.
A protected `main` may require a short-lived pull-request branch, but the
release commit and its tag must land on `main`.

1. Start from an up-to-date `main` and keep the worktree clean apart from the
   intended release files.
2. Update `Cargo.toml` and `Cargo.lock` to the exact release version.
3. Move the relevant entries from `CHANGELOG.md`'s `Unreleased` section into a
   dated `## [x.y.z]` section, then add the matching comparison link at the
   bottom. Keep unreleased documentation or test work under `Unreleased`.
4. Check user-facing documentation, installer examples, command help, and
   release workflow assumptions for stale version or release wording.
5. Validate before publishing:

   ```sh
   cargo fmt --check
   cargo test --locked
   cargo clippy --locked --all-targets -- -D warnings
   RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --document-private-items
   docker build --tag shu:test .
   docker run --rm --entrypoint /bin/sh \
     -v "$PWD/tests/docker:/tests:ro" \
     -v "$PWD/scripts:/scripts:ro" \
     shu:test -c 'sh /tests/e2e.sh'
   ```

6. Commit and push the release commit to `main` (or merge the verified PR into
   `main` when branch protection requires it).
7. Create an annotated tag from that exact `main` commit, then push only the
   tag:

   ```sh
   git tag -a vX.Y.Z -m "Shu vX.Y.Z"
   git push origin vX.Y.Z
   ```

8. Wait for the tag-triggered release workflow. Verify its success, the
   published GitHub release, all platform assets, installers, and
   `SHA256SUMS`.

Never retag, force-push a published tag, or bypass a failed release check.
