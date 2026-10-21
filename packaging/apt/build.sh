#!/bin/sh
# Builds the signed APT repository published at
# https://sacridini.github.io/orbitui/apt from the latest release's .deb
# packages. Run by the Pages workflow:
#
#   packaging/apt/build.sh OUT_DIR
#
# Needs: gh (GH_TOKEN), apt-ftparchive, gpg with APT_GPG_PRIVATE_KEY.
# Without a release or a key it builds nothing and exits successfully.
set -eu

out="$1"
here=$(cd "$(dirname "$0")" && pwd)

if [ -z "${APT_GPG_PRIVATE_KEY:-}" ]; then
    echo "apt: no signing key, skipping"
    exit 0
fi
pool="$out/pool/main/o/orbitui"
mkdir -p "$pool"
if ! gh release download --pattern '*.deb' --dir "$pool" 2>/dev/null; then
    echo "apt: no release with .deb packages yet, skipping"
    rm -rf "$out"
    exit 0
fi

cd "$out"
for arch in amd64 arm64; do
    dir="dists/stable/main/binary-$arch"
    mkdir -p "$dir"
    apt-ftparchive --arch "$arch" packages pool > "$dir/Packages"
    gzip -9 -k "$dir/Packages"
done
apt-ftparchive \
    -o APT::FTPArchive::Release::Origin=orbitui \
    -o APT::FTPArchive::Release::Label=orbitui \
    -o APT::FTPArchive::Release::Suite=stable \
    -o APT::FTPArchive::Release::Codename=stable \
    -o APT::FTPArchive::Release::Architectures="amd64 arm64" \
    -o APT::FTPArchive::Release::Components=main \
    -o APT::FTPArchive::Release::Description="orbitui, satellite overpass planner" \
    release dists/stable > dists/stable/Release

export GNUPGHOME="$(mktemp -d)"
printf '%s\n' "$APT_GPG_PRIVATE_KEY" | gpg --batch --import 2>/dev/null
gpg --batch --yes --armor --detach-sign --output dists/stable/Release.gpg dists/stable/Release
gpg --batch --yes --clearsign --output dists/stable/InRelease dists/stable/Release
rm -rf "$GNUPGHOME"

cp "$here/orbitui.gpg" "$here/orbitui.asc" "$here/orbitui.sources" .
echo "apt: repository built with $(ls pool/main/o/orbitui | tr '\n' ' ')"
