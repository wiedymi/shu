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
test "$(shu --catalog "$workspace/shu.toml" pick --filter api --path-only)" = "$path"
shu --catalog "$workspace/shu.toml" --json list | grep -q '"observed_state": "present"'
