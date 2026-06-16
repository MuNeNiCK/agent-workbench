#!/bin/sh
set -eu

REPO="${AGENT_WORKBENCH_REPO:-MuNeNiCK/agent-workbench}"
VERSION="${AGENT_WORKBENCH_VERSION:-}"
SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)"
SKILL_DIR="$(dirname -- "$SCRIPT_DIR")"
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

if [ -n "${AGENT_WORKBENCH_BIN:-}" ]; then
  if [ ! -x "$AGENT_WORKBENCH_BIN" ]; then
    echo "agent-workbench: AGENT_WORKBENCH_BIN is not executable: $AGENT_WORKBENCH_BIN" >&2
    exit 1
  fi
  exec "$AGENT_WORKBENCH_BIN" "$@"
fi

for root_candidate in "$SKILL_DIR/../.." "$SKILL_DIR/../../.."; do
  repo_root="$(CDPATH='' cd -- "$root_candidate" 2>/dev/null && pwd -P || true)"
  if [ -n "$repo_root" ] && [ -f "$repo_root/Cargo.toml" ]; then
    for local_cli in "$repo_root/target/debug/agent-workbench" "$repo_root/target/release/agent-workbench"; do
      if [ -x "$local_cli" ]; then
        exec "$local_cli" "$@"
      fi
    done
  fi
done

download() {
  url="$1"
  output="$2"
  err="$output.stderr"
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    if ! curl -fsSL -H "Authorization: Bearer $GITHUB_TOKEN" "$url" -o "$output" 2>"$err"; then
      echo "agent-workbench: download failed: $url" >&2
      sed 's/^/agent-workbench: curl: /' "$err" >&2
      rm -f "$err"
      exit 1
    fi
  else
    if ! curl -fsSL "$url" -o "$output" 2>"$err"; then
      echo "agent-workbench: download failed: $url" >&2
      sed 's/^/agent-workbench: curl: /' "$err" >&2
      rm -f "$err"
      exit 1
    fi
  fi
  rm -f "$err"
}

need curl
need sed
need tar
need sha256sum

if [ -z "$VERSION" ]; then
  version_file="$SKILL_DIR/CLI_VERSION"
  if [ -r "$version_file" ]; then
    VERSION="$(sed -n '1{s/[[:space:]]//g;p;}' "$version_file")"
  fi
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
fi

cache_root="${XDG_CACHE_HOME:-$HOME/.cache}/agent-workbench/releases/$VERSION/$PLATFORM"
cli="$cache_root/agent-workbench"
checksum_marker="$cache_root/agent-workbench.sha256"

if [ ! -x "$cli" ] || [ ! -s "$checksum_marker" ]; then
  cache_parent="$(dirname -- "$cache_root")"
  mkdir -p "$cache_parent"
  tmpdir="$(mktemp -d)"
  tmpcache="$(mktemp -d "$cache_parent/.tmp.XXXXXX")"
  cleanup() {
    rm -rf "$tmpdir" "$tmpcache"
  }
  trap cleanup EXIT HUP INT TERM
  asset="agent-workbench-$VERSION-$PLATFORM.tar.gz"
  checksums="agent-workbench-$VERSION-checksums.txt"
  base_url="https://github.com/$REPO/releases/download/$VERSION"

  download "$base_url/$asset" "$tmpdir/$asset"
  download "$base_url/$checksums" "$tmpdir/$checksums"

  if ! grep "  $asset\$" "$tmpdir/$checksums" > "$tmpdir/$asset.sha256"; then
    echo "agent-workbench: checksum entry not found for $asset" >&2
    exit 1
  fi

  (cd "$tmpdir" && sha256sum -c "$asset.sha256" >/dev/null)
  tar -xzf "$tmpdir/$asset" -C "$tmpcache"
  chmod +x "$tmpcache/agent-workbench"
  "$tmpcache/agent-workbench" status --help >/dev/null
  sed -n "s/  $asset\$//p" "$tmpdir/$asset.sha256" > "$tmpcache/agent-workbench.sha256"

  if [ ! -x "$cli" ] || [ ! -s "$checksum_marker" ]; then
    rm -rf "$cache_root"
    mv "$tmpcache" "$cache_root"
    tmpcache=""
  fi
  trap - EXIT HUP INT TERM
  cleanup
fi

exec "$cli" "$@"
