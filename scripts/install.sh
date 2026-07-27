#!/usr/bin/env sh
# Install a verified Shu release on macOS or Linux.
#
# Optional environment variables:
#   SHU_VERSION     A release tag or version, for example v0.1.0 (default: latest)
#   SHU_INSTALL_DIR Installation directory (default: ~/.local/bin)
#   SHU_INSTALL_REPO GitHub repository (default: wiedymi/shu)

set -eu

repository="${SHU_INSTALL_REPO:-wiedymi/shu}"
version="${SHU_VERSION:-latest}"
install_dir="${SHU_INSTALL_DIR:-$HOME/.local/bin}"

info() { printf '  %s\n' "$1"; }
success() { printf '✓ %s\n' "$1"; }

case "$(uname -s)" in
    Darwin) operating_system="apple-darwin" ;;
    Linux) operating_system="unknown-linux-musl" ;;
    *) echo "error: Shu does not support $(uname -s) with this installer" >&2; exit 1 ;;
esac

case "$(uname -m)" in
    x86_64|amd64) architecture="x86_64" ;;
    arm64|aarch64) architecture="aarch64" ;;
    *) echo "error: Shu does not support $(uname -m) with this installer" >&2; exit 1 ;;
esac

target="${architecture}-${operating_system}"
asset="shu-${target}.tar.xz"
if [ "$version" = "latest" ]; then
    download_base="https://github.com/${repository}/releases/latest/download"
else
    case "$version" in v*) ;; *) version="v${version}" ;; esac
    download_base="https://github.com/${repository}/releases/download/${version}"
fi

printf '\nShu installer\n\n'
info "Platform: $target"
info "Release: $version"

if command -v curl >/dev/null 2>&1; then
    download() { curl --proto '=https' --tlsv1.2 -fL --retry 3 --output "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
    download() { wget -qO "$2" "$1"; }
else
    echo "error: install curl or wget, then run this installer again" >&2
    exit 1
fi

temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/shu.XXXXXX")"
trap 'rm -rf "$temporary_directory"' EXIT HUP INT TERM
archive="$temporary_directory/$asset"
checksums="$temporary_directory/SHA256SUMS"
info "Downloading $asset"
download "$download_base/$asset" "$archive"
info "Downloading SHA256SUMS"
download "$download_base/SHA256SUMS" "$checksums"

info "Verifying checksum"
expected="$(awk -v asset="$asset" '$2 == asset || $2 == "*" asset { print $1; exit }' "$checksums")"
if [ -z "$expected" ]; then
    echo "error: $asset was not listed in SHA256SUMS" >&2
    exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$archive" | awk '{ print $1 }')"
else
    actual="$(shasum -a 256 "$archive" | awk '{ print $1 }')"
fi
if [ "$expected" != "$actual" ]; then
    echo "error: checksum verification failed for $asset" >&2
    exit 1
fi

info "Extracting"
tar -xJf "$archive" -C "$temporary_directory"
mkdir -p "$install_dir"
install -m 755 "$temporary_directory/shu" "$install_dir/shu"
success "Installed Shu to $install_dir/shu"
case ":${PATH}:" in
    *":${install_dir}:"*) ;;
    *) printf 'Add %s to PATH, then open a new terminal.\n' "$install_dir" ;;
esac
