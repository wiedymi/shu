#!/bin/sh
set -eu

workspace="$(mktemp -d)"
trap 'rm -rf "$workspace"' EXIT

mkdir -p "$workspace/remotes"
git init --bare "$workspace/remotes/api.git" >/dev/null
git init --bare "$workspace/remotes/catalog.git" >/dev/null
git init "$workspace/seed" >/dev/null
printf 'fixture\n' > "$workspace/seed/README.md"
git -C "$workspace/seed" add README.md
git -C "$workspace/seed" -c user.name='Shu Test' -c user.email='shu@example.invalid' commit -m fixture >/dev/null
git -C "$workspace/seed" branch -M main
git -C "$workspace/seed" remote add origin "$workspace/remotes/api.git"
git -C "$workspace/seed" push -u origin main >/dev/null
git --git-dir "$workspace/remotes/api.git" symbolic-ref HEAD refs/heads/main

cat > "$workspace/shu.toml" <<EOF
version = 1
root = "$workspace/library"

[[repos]]
source = "github.com/example-org/api"
state = "active"
EOF

export GIT_CONFIG_COUNT=1
export GIT_CONFIG_KEY_0="url.file://$workspace/remotes/.insteadOf"
export GIT_CONFIG_VALUE_0="https://github.com/example-org/"

shu --catalog "$workspace/shu.toml" --yes restore
path="$(shu --catalog "$workspace/shu.toml" ensure api --path-only)"
test "$path" = "$workspace/library/github.com/example-org/api"
test -d "$path/.git"
GIT_AUTHOR_NAME='Shu Test' GIT_AUTHOR_EMAIL='shu@example.invalid' \
  shu --catalog "$workspace/shu.toml" doctor | grep -q '^✓ catalog:'
test "$(shu --catalog "$workspace/shu.toml" pick --filter api --path-only)" = "$path"
shu --catalog "$workspace/shu.toml" --json list | grep -q '"observed_state": "present"'

# Normal add records an existing clone in the one catalog without moving it.
# That location is then preferred by ensure and the picker.
git -C "$workspace/seed" remote set-url origin 'https://github.com/example-org/api.git'
GIT_CONFIG_COUNT=0 shu --catalog "$workspace/shu.toml" add "$workspace/seed"
# Adding a second clone preserves the current primary. Choose this clone only
# when we explicitly want it to be the default for path-oriented commands.
shu --catalog "$workspace/shu.toml" locations api --primary "$workspace/seed"
test "$(shu --catalog "$workspace/shu.toml" ensure api --path-only)" = "$workspace/seed"
test "$(shu --catalog "$workspace/shu.toml" pick --filter api --path-only)" = "$workspace/seed"

# Sync publishes repository metadata only. Root-relative managed paths and
# arbitrary external clone paths remain local to this machine.
GIT_AUTHOR_NAME='Shu Test' GIT_AUTHOR_EMAIL='shu@example.invalid' \
GIT_COMMITTER_NAME='Shu Test' GIT_COMMITTER_EMAIL='shu@example.invalid' \
  shu --catalog "$workspace/shu.toml" sync init github.com/example-org/catalog
synced="$(git --git-dir "$workspace/remotes/catalog.git" show main:shu.toml)"
printf '%s\n' "$synced" | grep -q 'source = "github.com/example-org/api"'
! printf '%s\n' "$synced" | grep -q 'root ='
! printf '%s\n' "$synced" | grep -q "$workspace/seed"
! printf '%s\n' "$synced" | grep -q 'paths ='

cat > "$workspace/second.toml" <<EOF
version = 1
root = "$workspace/second-library"
EOF
shu --catalog "$workspace/second.toml" --yes restore github.com/example-org/catalog
test -d "$workspace/second-library/github.com/example-org/api/.git"
grep -q '"github.com/example-org/api"' "$workspace/second.toml"
! grep -q "$workspace/seed" "$workspace/second.toml"

# Shell setup is persistent and idempotent. Use an explicit file in the
# container so this checks the same setup path without changing a real shell
# profile.
profile="$workspace/profile"
shu shell init posix --path "$profile"
shu shell init posix --path "$profile"
test "$(grep -c '^# >>> shu shell integration >>>$' "$profile")" = 1
grep -q 'pick --path-only' "$profile"
sh -n "$profile"

# Migration moves a clean working tree atomically into Shu's canonical layout.
git -C "$workspace/seed" remote set-url origin 'git@github.com:example-org/migrated.git'
shu --catalog "$workspace/shu.toml" --yes add "$workspace/seed" --migrate
test ! -d "$workspace/seed"
test -d "$workspace/library/github.com/example-org/migrated/.git"

# Exercise the release installer against a locally generated, checksummed
# release payload. The fake curl command gives the installer the same
# command-line contract it has when downloading a GitHub Release.
mkdir -p "$workspace/release-assets" "$workspace/fake-bin" "$workspace/installed"
case "$(uname -m)" in
    x86_64) target='x86_64-unknown-linux-musl' ;;
    aarch64|arm64) target='aarch64-unknown-linux-musl' ;;
    *) echo "unsupported Linux architecture: $(uname -m)" >&2; exit 1 ;;
esac
cp /usr/local/bin/shu "$workspace/release-assets/shu-$target"
(cd "$workspace/release-assets" && sha256sum "shu-$target" > SHA256SUMS)
cat > "$workspace/fake-bin/curl" <<'EOF'
#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
    case "$1" in
        --output) output="$2"; shift 2 ;;
        *) url="$1"; shift ;;
    esac
done
cp "$SHU_TEST_RELEASE_ASSETS/$(basename "$url")" "$output"
EOF
chmod +x "$workspace/fake-bin/curl"
PATH="$workspace/fake-bin:$PATH" \
SHU_TEST_RELEASE_ASSETS="$workspace/release-assets" \
SHU_INSTALL_DIR="$workspace/installed" \
sh /scripts/install.sh
"$workspace/installed/shu" --version | grep -q '^shu '
