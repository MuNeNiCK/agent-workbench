#!/bin/sh
set -eu

REPO="${AGENT_WORKBENCH_REPO:-MuNeNiCK/agent-workbench}"
VERSION="${AGENT_WORKBENCH_VERSION:-}"
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS:$ARCH" in
  Linux:x86_64|Linux:amd64)
    PLATFORM="linux-x86_64"
    ;;
  *)
    echo "agent-workbench: this skill release currently supports Linux x86_64 only; got $OS $ARCH" >&2
    exit 1
    ;;
esac

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "agent-workbench: required command not found: $1" >&2
    exit 1
  fi
}

download() {
  url="$1"
  output="$2"
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    curl -fsSL -H "Authorization: Bearer $GITHUB_TOKEN" "$url" -o "$output"
  else
    curl -fsSL "$url" -o "$output"
  fi
}

need curl
need sed
need tar
need sha256sum

if [ -z "$VERSION" ]; then
  latest_json="$(mktemp)"
  download "https://api.github.com/repos/$REPO/releases/latest" "$latest_json"
  VERSION="$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$latest_json" | head -n 1)"
  rm -f "$latest_json"
  if [ -z "$VERSION" ]; then
    echo "agent-workbench: failed to resolve latest release for $REPO" >&2
    exit 1
  fi
fi

cache_root="${XDG_CACHE_HOME:-$HOME/.cache}/agent-workbench/releases/$VERSION/$PLATFORM"
cli="$cache_root/agent-workbench"

if [ ! -x "$cli" ]; then
  mkdir -p "$cache_root"
  tmpdir="$(mktemp -d)"
  asset="agent-workbench-$VERSION-$PLATFORM.tar.gz"
  checksums="agent-workbench-$VERSION-checksums.txt"
  base_url="https://github.com/$REPO/releases/download/$VERSION"

  download "$base_url/$asset" "$tmpdir/$asset"
  download "$base_url/$checksums" "$tmpdir/$checksums"

  if ! grep "  $asset\$" "$tmpdir/$checksums" > "$tmpdir/$asset.sha256"; then
    echo "agent-workbench: checksum entry not found for $asset" >&2
    rm -rf "$tmpdir"
    exit 1
  fi

  (cd "$tmpdir" && sha256sum -c "$asset.sha256")
  tar -xzf "$tmpdir/$asset" -C "$cache_root"
  chmod +x "$cli"
  rm -rf "$tmpdir"
fi

exec "$cli" "$@"
