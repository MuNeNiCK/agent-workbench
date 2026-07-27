#!/bin/sh
set -eu

repository="MuNeNiCK/agent-workbench"
script_dir="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)"
skill_dir="$(dirname -- "$script_dir")"

case "$(uname -s):$(uname -m)" in
  Linux:x86_64|Linux:amd64) platform="linux-x86_64-static" ;;
  *)
    echo "agent-workbench: unsupported platform: $(uname -s) $(uname -m)" >&2
    exit 1
    ;;
esac

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "agent-workbench: required command not found: $1" >&2
    exit 1
  }
}

project_root() {
  candidate="$(pwd -P)"
  while test "$candidate" != "/"; do
    if test -f "$candidate/.agent-workbench/state.sqlite3"; then
      printf '%s\n' "$candidate"
      return
    fi
    candidate="$(dirname -- "$candidate")"
  done

  if command -v git >/dev/null 2>&1; then
    if candidate="$(git rev-parse --show-toplevel 2>/dev/null)"; then
      CDPATH='' cd -- "$candidate" && pwd -P
      return
    fi
  fi
  pwd -P
}

run() {
  runtime="$1"
  shift
  case "${1:-}" in
    --version|-h|--help)
      exec "$runtime" "$@"
      ;;
  esac

  root="$(project_root)"
  state="$root/.agent-workbench/state.sqlite3"
  if test "${1:-}" = "init"; then
    mkdir -p "$root/.agent-workbench"
  fi
  exec "$runtime" --state "$state" "$@"
}

source_root="$(CDPATH='' cd -- "$skill_dir/../.." 2>/dev/null && pwd -P)" || source_root=""
if test -n "$source_root" && test -f "$source_root/lakefile.lean" &&
    test -x "$source_root/.lake/build/bin/agent-workbench"; then
  run "$source_root/.lake/build/bin/agent-workbench" "$@"
fi

need curl
need grep
need sed
need sha256sum
need tar

version="$(sed -n '1{s/[[:space:]]//g;p;}' "$skill_dir/CLI_VERSION")"
if ! printf '%s\n' "$version" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "agent-workbench: invalid CLI_VERSION: $version" >&2
  exit 1
fi

cache_base="${XDG_CACHE_HOME:-${HOME}/.cache}"
cache_dir="$cache_base/agent-workbench/releases/$version/$platform"
runtime="$cache_dir/agent-workbench"
marker="$cache_dir/agent-workbench.sha256"

cache_valid=false
if test -x "$runtime" && test -s "$marker"; then
  expected="$(sed -n '1{s/[[:space:]]//g;p;}' "$marker")"
  actual="$(sha256sum "$runtime" | sed -n 's/[[:space:]].*//p')"
  if printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' &&
      test "$actual" = "$expected"; then
    cache_valid=true
  fi
fi

if test "$cache_valid" != true; then
  cache_parent="$(dirname -- "$cache_dir")"
  mkdir -p "$cache_parent"
  download_dir="$(mktemp -d)"
  staging_dir="$(mktemp -d "$cache_parent/.tmp.XXXXXX")"
  cleanup() {
    rm -rf "$download_dir" "$staging_dir"
  }
  trap cleanup EXIT HUP INT TERM

  asset="agent-workbench-$version-$platform.tar.gz"
  checksums="agent-workbench-$version-checksums.txt"
  base_url="https://github.com/$repository/releases/download/$version"
  curl -fsSL "$base_url/$asset" -o "$download_dir/$asset"
  curl -fsSL "$base_url/$checksums" -o "$download_dir/$checksums"
  grep "  $asset\$" "$download_dir/$checksums" > "$download_dir/$asset.sha256"
  (cd "$download_dir" && sha256sum -c "$asset.sha256" >/dev/null)
  tar -xzf "$download_dir/$asset" -C "$staging_dir"
  chmod +x "$staging_dir/agent-workbench"
  test "$("$staging_dir/agent-workbench" --version)" = \
    "agent-workbench ${version#v}"
  sha256sum "$staging_dir/agent-workbench" |
    sed -n 's/[[:space:]].*//p' > "$staging_dir/agent-workbench.sha256"

  rm -rf "$cache_dir"
  mv "$staging_dir" "$cache_dir"
  staging_dir=""
  trap - EXIT HUP INT TERM
  cleanup
fi

run "$runtime" "$@"
