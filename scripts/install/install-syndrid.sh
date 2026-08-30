#!/bin/sh
set -eu

RELEASE="${SYNDRID_RELEASE:-latest}"
BIN_DIR="${SYNDRID_INSTALL_DIR:-$HOME/.local/bin}"
REPO="${SYNDRID_GITHUB_REPOSITORY:-SyndridHQ/syndridcli}"
SYNDRID_HOME_DIR="${SYNDRID_HOME:-$HOME/.syndrid}"
STANDALONE_ROOT="$SYNDRID_HOME_DIR/packages/standalone"
RELEASES_DIR="$STANDALONE_ROOT/releases"
CURRENT_LINK="$STANDALONE_ROOT/current"

die() { echo "syndrid installer: $*" >&2; exit 1; }
require() { command -v "$1" >/dev/null 2>&1 || die "$1 is required"; }
sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}';
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}';
  else die "sha256sum or shasum is required"; fi
}
download() { curl -fsSL "$1" -o "$2"; }

while [ "$#" -gt 0 ]; do
  case "$1" in
    --release) [ "$#" -ge 2 ] || die "--release requires a value"; RELEASE="$2"; shift ;;
    --help|-h) echo "Usage: install-syndrid.sh [--release VERSION]"; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

require curl
require tar

case "$(uname -s)" in
  Darwin) os=apple-darwin ;;
  Linux) os=unknown-linux-musl ;;
  *) die "unsupported operating system: $(uname -s)" ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  arm64|aarch64) arch=aarch64 ;;
  *) die "unsupported architecture: $(uname -m)" ;;
esac
TARGET="${arch}-${os}"

if [ "$RELEASE" = latest ]; then
  api="https://api.github.com/repos/${REPO}/releases/latest"
  tag="$(curl -fsSL "$api" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
  [ -n "$tag" ] || die "could not resolve latest release tag"
else
  case "$RELEASE" in rust-v*) tag="$RELEASE" ;; v*) tag="rust-${RELEASE}" ;; *) tag="rust-v${RELEASE}" ;; esac
fi

base="https://github.com/${REPO}/releases/download/${tag}"
asset="syndrid-package-${TARGET}.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

download "$base/syndrid-package_SHA256SUMS" "$tmp/SHA256SUMS"
download "$base/$asset" "$tmp/$asset"
expected="$(awk -v name="$asset" '$2 == name && length($1) == 64 {print tolower($1); exit}' "$tmp/SHA256SUMS")"
[ -n "$expected" ] || die "$asset is missing from syndrid-package_SHA256SUMS"
actual="$(sha256_file "$tmp/$asset")"
[ "$actual" = "$expected" ] || die "checksum mismatch for $asset"

stage="$tmp/package"
mkdir -p "$stage"
tar -xzf "$tmp/$asset" -C "$stage"
[ -x "$stage/bin/syndrid" ] || die "package does not contain executable bin/syndrid"
[ -f "$stage/codex-package.json" ] || die "package does not contain codex-package.json"

release_dir="$RELEASES_DIR/${tag}-${TARGET}"
mkdir -p "$RELEASES_DIR" "$BIN_DIR"
rm -rf "$release_dir"
mv "$stage" "$release_dir"

if [ -e "$CURRENT_LINK" ] && [ ! -L "$CURRENT_LINK" ]; then
  die "$CURRENT_LINK exists and is not a symbolic link"
fi
ln -sfn "$release_dir" "$CURRENT_LINK"

new_bin="$BIN_DIR/.syndrid.$$"
rm -f "$new_bin"
ln -s "$CURRENT_LINK/bin/syndrid" "$new_bin"
mv -f "$new_bin" "$BIN_DIR/syndrid"

echo "Installed syndrid from $tag to $release_dir"
echo "Entrypoint: $BIN_DIR/syndrid"
