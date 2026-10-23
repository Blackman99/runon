#!/bin/sh
# runon installer: downloads the release binary for this machine, checks its
# SHA-256 against checksums.txt, and puts it in ~/.local/bin (or $RUNON_BIN).
#   curl -fsSL https://blackman99.github.io/runon/install.sh | sh
set -eu
repo=Blackman99/runon
dest=${RUNON_BIN:-"$HOME/.local/bin"}
version=${RUNON_VERSION:-}

say() { printf 'runon-install: %s\n' "$*"; }
die() { say "$*" >&2; exit 1; }

for t in curl tar; do command -v "$t" >/dev/null 2>&1 || die "needs $t on PATH"; done

case "$(uname -s)" in
  Linux) os=unknown-linux-musl ;;
  Darwin) os=apple-darwin ;;
  MINGW*|MSYS*|CYGWIN*) die "on Windows, download runon-<version>-x86_64-pc-windows-msvc.zip from https://github.com/$repo/releases/latest, or run: cargo install --git https://github.com/$repo" ;;
  *) die "no prebuilt binary for $(uname -s); on Windows download the .zip from https://github.com/$repo/releases, or run: cargo install --git https://github.com/$repo" ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  arm64|aarch64) arch=aarch64 ;;
  *) die "no prebuilt binary for $(uname -m); try: cargo install --git https://github.com/$repo" ;;
esac
target="$arch-$os"

if [ -z "$version" ]; then
  version=$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)
  [ -n "$version" ] || die "could not find the latest release (GitHub API rate limit?); set RUNON_VERSION=v0.1.0"
fi
name="runon-$version-$target"
base="https://github.com/$repo/releases/download/$version"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

say "downloading $name.tar.gz"
curl -fsSL "$base/$name.tar.gz" -o "$tmp/$name.tar.gz" || die "download failed: $base/$name.tar.gz"
curl -fsSL "$base/checksums.txt" -o "$tmp/checksums.txt" || die "could not fetch checksums.txt"
want=$(grep " $name.tar.gz\$" "$tmp/checksums.txt" | cut -d' ' -f1)
if command -v sha256sum >/dev/null 2>&1; then got=$(sha256sum "$tmp/$name.tar.gz" | cut -d' ' -f1)
else got=$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d' ' -f1); fi
[ -n "$want" ] && [ "$want" = "$got" ] || die "checksum mismatch for $name.tar.gz (want $want, got $got); nothing installed"

tar xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$dest"
mv "$tmp/$name/runon" "$dest/runon"
chmod +x "$dest/runon"
say "installed $("$dest/runon" version) to $dest/runon (checksum ok)"
case ":$PATH:" in *":$dest:"*) ;; *) say "add $dest to your PATH to run it as 'runon'" ;; esac
say "next: run 'runon demo' to watch it find real stray processes, then 'runon' on your own machine"
