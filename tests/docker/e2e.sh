#!/bin/sh
set -eu

workspace="$(mktemp -d)"
trap 'rm -rf "$workspace"' EXIT

mkdir -p "$workspace/remotes"
git init --bare "$workspace/remotes/api.git" >/dev/null
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
shu --catalog "$workspace/shu.toml" doctor | grep -q '^✓ catalog:'
test "$(shu --catalog "$workspace/shu.toml" pick --filter api --path-only)" = "$path"
shu --catalog "$workspace/shu.toml" --json list | grep -q '"observed_state": "present"'

# Migration moves a clean working tree atomically into Shu's canonical layout.
git -C "$workspace/seed" remote set-url origin 'git@github.com:example-org/migrated.git'
shu --catalog "$workspace/shu.toml" --yes add "$workspace/seed" --migrate
test ! -d "$workspace/seed"
test -d "$workspace/library/github.com/example-org/migrated/.git"

# Exercise the release installer against a locally generated, checksummed
# release payload. The fake curl command gives the installer the same
# command-line contract it has when downloading a GitHub Release.
mkdir -p "$workspace/release-assets" "$workspace/fake-bin" "$workspace/installed"
cp /usr/local/bin/shu "$workspace/release-assets/shu-x86_64-unknown-linux-musl"
(cd "$workspace/release-assets" && sha256sum shu-x86_64-unknown-linux-musl > SHA256SUMS)
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
