#!/bin/sh
# Installs orbitui from its GitHub releases, on Linux and macOS:
#
#   curl -fsSL https://sacridini.github.io/orbitui/install.sh | sh
#
# ORBITUI_VERSION=0.3.0 picks a version (default: the latest release).
# ORBITUI_INSTALL_DIR=~/bin picks the directory (default: /usr/local/bin,
# with sudo if needed, else ~/.local/bin).
set -eu

REPO="sacridini/orbitui"

say() { printf '%s\n' "orbitui: $*"; }
fail() { say "$*" >&2; exit 1; }

fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$1" -O "$2"
    else
        fail "needs curl or wget"
    fi
}

case "$(uname -s)" in
    Linux) os="unknown-linux-musl" ;;
    Darwin) os="apple-darwin" ;;
    *) fail "unsupported system $(uname -s); on Windows, download the zip from https://github.com/$REPO/releases" ;;
esac
case "$(uname -m)" in
    x86_64 | amd64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *) fail "unsupported processor $(uname -m)" ;;
esac
target="$arch-$os"

if [ -n "${ORBITUI_VERSION:-}" ]; then
    tag="v${ORBITUI_VERSION#v}"
else
    # The latest release's page redirects to its tag.
    tag=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" 2>/dev/null | sed 's|.*/tag/||')
    case "$tag" in v*) ;; *) fail "could not find the latest release" ;; esac
fi
version="${tag#v}"
name="orbitui-$version-$target"
base="https://github.com/$REPO/releases/download/$tag"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
say "downloading $name"
fetch "$base/$name.tar.gz" "$tmp/$name.tar.gz"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS"

expected=$(grep " $name.tar.gz\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$tmp/$name.tar.gz" | cut -d' ' -f1)
else
    actual=$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d' ' -f1)
fi
[ -n "$expected" ] && [ "$expected" = "$actual" ] || fail "checksum mismatch for $name.tar.gz"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
binary="$tmp/$name/orbitui"

if [ -n "${ORBITUI_INSTALL_DIR:-}" ]; then
    dir="$ORBITUI_INSTALL_DIR"
    mkdir -p "$dir"
    install -m 755 "$binary" "$dir/orbitui"
elif [ -w /usr/local/bin ]; then
    dir="/usr/local/bin"
    install -m 755 "$binary" "$dir/orbitui"
elif command -v sudo >/dev/null 2>&1 && [ -d /usr/local/bin ]; then
    dir="/usr/local/bin"
    say "installing to $dir (sudo)"
    sudo install -m 755 "$binary" "$dir/orbitui"
else
    dir="$HOME/.local/bin"
    mkdir -p "$dir"
    install -m 755 "$binary" "$dir/orbitui"
fi

say "installed $("$dir/orbitui" --version) in $dir"
case ":$PATH:" in
    *":$dir:"*) ;;
    *) say "add $dir to your PATH to run it as 'orbitui'" ;;
esac
