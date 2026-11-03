#!/bin/sh
# Builds dist/ircnext_<version>_amd64.deb from target/release/ircnext.
# Usage: packaging/linux/build-deb.sh <version>
set -eu
version="$1"
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

install -Dm755 target/release/ircnext "$root/usr/bin/ircnext"
install -Dm644 assets/icon/ircnext.desktop "$root/usr/share/applications/ircnext.desktop"
for size in 16 32 48 64 128 256 512; do
  install -Dm644 "assets/icon/png/ircnext-$size.png" "$root/usr/share/icons/hicolor/${size}x${size}/apps/ircnext.png"
done
install -Dm644 assets/icon/ircnext.svg "$root/usr/share/icons/hicolor/scalable/apps/ircnext.svg"
install -Dm644 LICENSE "$root/usr/share/doc/ircnext/copyright"

size=$(du -sk "$root/usr" | cut -f1)
mkdir -p "$root/DEBIAN"
cat > "$root/DEBIAN/control" <<CONTROL
Package: ircnext
Version: $version
Section: net
Priority: optional
Architecture: amd64
Maintainer: BrystoQ <BrystoQ@users.noreply.github.com>
Installed-Size: $size
Depends: libc6 (>= 2.35), libxkbcommon0, libwayland-client0 | libx11-6
Homepage: https://github.com/BrystoQ/ircnext
Description: Modern IRC client with IRCv3 support
 A fast and safe IRC client written in Rust, inspired by HexChat:
 IRCv3, SASL, FiSH encryption, DCC transfers and mIRC scripts.
CONTROL

mkdir -p dist
dpkg-deb --build --root-owner-group "$root" "dist/ircnext_${version}_amd64.deb"
