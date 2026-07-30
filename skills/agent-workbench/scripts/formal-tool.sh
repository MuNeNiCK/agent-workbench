#!/bin/sh
set -eu

case "$(uname -s):$(uname -m)" in
  Linux:x86_64|Linux:amd64) platform="linux" ;;
  *)
    echo "agent-workbench: formal assurance is unsupported on $(uname -s) $(uname -m)" >&2
    exit 1
    ;;
esac

case "${1:-}" in
  lean|lake) command="$1"; shift ;;
  root) command="root"; shift ;;
  identity) command="identity"; shift ;;
  *)
    echo "usage: $0 <lean|lake|root|identity> [arguments...]" >&2
    exit 2
    ;;
esac

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "agent-workbench: required command not found: $1" >&2
    exit 1
  }
}

need curl
need flock
need grep
need sed
need sha256sum
need tar
need zstd

lean_version="4.30.0"
source_commit="d024af099ca4bf2c86f649261ebf59565dc8c622"
asset="lean-$lean_version-$platform.tar.zst"
expected_digest="4dad74141c2c119ca1aa626656be83b8e14238afba97271fd7bf1eb3f081b319"
if test -n "${AGENT_WORKBENCH_TEST_RELEASE_DIR:-}"; then
  expected_digest="${AGENT_WORKBENCH_TEST_LEAN_DIGEST:?}"
fi

cache_base="${XDG_CACHE_HOME:-${HOME}/.cache}"
cache_parent="$cache_base/agent-workbench/toolchains"
cache_dir="$cache_parent/lean-$lean_version-$platform"
tool_root="$cache_dir/lean-$lean_version-$platform"
marker="$cache_dir/distribution.sha256"
cache_lock="$cache_parent/.lean-$lean_version-$platform-install-lock"
mkdir -p "$cache_parent"
exec 9>"$cache_lock"
until flock -n 9; do
  sleep 0.05
done

release_cache_lock() {
  flock -u 9 2>/dev/null || true
  exec 9>&-
}
abort_cache_install() {
  code="$1"
  trap - EXIT HUP INT TERM
  release_cache_lock
  exit "$code"
}
trap release_cache_lock EXIT
trap 'abort_cache_install 129' HUP
trap 'abort_cache_install 130' INT
trap 'abort_cache_install 143' TERM

cache_valid=false
if test -x "$tool_root/bin/lean" &&
    test -x "$tool_root/bin/lake" &&
    test "$(sed -n '1p' "$marker" 2>/dev/null)" = "$expected_digest" &&
    "$tool_root/bin/lean" --version |
      grep -F "Lean (version $lean_version," >/dev/null; then
  cache_valid=true
fi

if test "$cache_valid" != true; then
  download_dir="$(exec 9>&-; mktemp -d)"
  staging_dir="$(exec 9>&-; mktemp -d "$cache_parent/.lean.XXXXXX")"
  cleanup() {
    rm -rf "$download_dir" "$staging_dir" 9>&-
  }
  abort_download() {
    code="$1"
    trap - EXIT HUP INT TERM
    cleanup
    release_cache_lock
    exit "$code"
  }
  trap 'cleanup; release_cache_lock' EXIT
  trap 'abort_download 129' HUP
  trap 'abort_download 130' INT
  trap 'abort_download 143' TERM

  url="https://github.com/leanprover/lean4/releases/download/v$lean_version/$asset"
  curl -fsSL --connect-timeout 30 --speed-limit 1024 --speed-time 60 \
    "$url" -o "$download_dir/$asset" 9>&-
  actual_digest="$(exec 9>&-; sha256sum "$download_dir/$asset" |
    sed -n 's/[[:space:]].*//p' 9>&-)"
  test "$actual_digest" = "$expected_digest"
  tar --zstd -xf "$download_dir/$asset" -C "$staging_dir" 9>&-

  staged_root="$staging_dir/lean-$lean_version-$platform"
  test -x "$staged_root/bin/lean"
  test -x "$staged_root/bin/lake"
  "$staged_root/bin/lean" --version |
    grep -F "Lean (version $lean_version," >/dev/null
  printf '%s\n' "$expected_digest" > "$staging_dir/distribution.sha256"

  rm -rf "$cache_dir" 9>&-
  mv "$staging_dir" "$cache_dir" 9>&-
  staging_dir=""
  cleanup
fi

release_cache_lock
trap - EXIT HUP INT TERM
case "$command" in
  root)
    printf '%s\n' "$tool_root"
    ;;
  identity)
    lean_identity="$("$tool_root/bin/lean" --version | sed -n '1p')"
    printf '%s|distribution=sha256:%s|source=%s\n' \
      "$lean_identity" "$expected_digest" "$source_commit"
    ;;
  *)
    exec "$tool_root/bin/$command" "$@"
    ;;
esac
