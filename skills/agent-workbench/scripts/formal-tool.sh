#!/bin/sh
set -eu

repository="MuNeNiCK/agent-workbench"
script_dir="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)"
skill_dir="$(dirname -- "$script_dir")"

case "$(uname -s):$(uname -m)" in
  Linux:x86_64|Linux:amd64) platform="linux-x86_64" ;;
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
    echo "usage: $0 <lean|lake> [arguments...]" >&2
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
need grep
need sed
need sha256sum
need tar
need flock

version="$(sed -n '1{s/[[:space:]]//g;p;}' "$skill_dir/CLI_VERSION")"
if ! printf '%s\n' "$version" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "agent-workbench: invalid CLI_VERSION: $version" >&2
  exit 1
fi

cache_base="${XDG_CACHE_HOME:-${HOME}/.cache}"
cache_dir="$cache_base/agent-workbench/releases/$version/formal-tool-$platform"
tool_root="$cache_dir/agent-workbench-formal-tool"
archive="$cache_dir/formal-tool.tar.gz"
marker="$cache_dir/formal-tool.sha256"
cache_parent="$(dirname -- "$cache_dir")"
cache_lock="$cache_parent/.formal-tool-install-lock"
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
if test -s "$archive" && test -s "$marker" &&
    test -x "$tool_root/bin/lean" && test -x "$tool_root/bin/lake" &&
    test -s "$tool_root/lean-toolchain" &&
    test -s "$tool_root/SOURCE_COMMIT" &&
    test -s "$tool_root/MANIFEST.sha256"; then
  expected="$(exec 9>&-; sed -n '1{s/[[:space:]]//g;p;}' "$marker")"
  actual="$(exec 9>&-; sha256sum "$archive" |
    sed -n 's/[[:space:]].*//p' 9>&-)"
  archived_manifest=""
  if manifest_content="$(exec 9>&-; tar -xOzf "$archive" \
      agent-workbench-formal-tool/MANIFEST.sha256 2>/dev/null)"; then
    archived_manifest="$(exec 9>&-; printf '%s\n' "$manifest_content" |
      sha256sum 9>&- | sed -n 's/[[:space:]].*//p' 9>&-)"
  fi
  cached_manifest="$(exec 9>&-; sha256sum "$tool_root/MANIFEST.sha256" |
    sed -n 's/[[:space:]].*//p' 9>&-)"
  if printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' 9>&- &&
      test "$actual" = "$expected" &&
      test "$archived_manifest" = "$cached_manifest" &&
      test "$(exec 9>&-; sed -n '1p' "$tool_root/lean-toolchain")" = \
        "leanprover/lean4:v4.30.0" &&
      test "$(exec 9>&-; sed -n '1p' "$tool_root/SOURCE_COMMIT")" = \
        "d024af099ca4bf2c86f649261ebf59565dc8c622" &&
      (exec 9>&-; cd "$tool_root" &&
        sha256sum -c MANIFEST.sha256 >/dev/null); then
    cache_valid=true
  fi
fi

if test "$cache_valid" != true; then
  download_dir="$(exec 9>&-; mktemp -d)"
  staging_dir="$(exec 9>&-; mktemp -d "$cache_parent/.formal-tool.XXXXXX")"
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

  asset="agent-workbench-$version-formal-tool-$platform.tar.gz"
  checksums="agent-workbench-$version-checksums.txt"
  base_url="https://github.com/$repository/releases/download/$version"
  curl -fsSL --connect-timeout 30 --speed-limit 1024 --speed-time 60 \
    "$base_url/$asset" -o "$download_dir/$asset" 9>&-
  curl -fsSL --connect-timeout 30 --speed-limit 1024 --speed-time 60 \
    "$base_url/$checksums" -o "$download_dir/$checksums" 9>&-
  grep "  $asset\$" "$download_dir/$checksums" 9>&- \
    > "$download_dir/$asset.sha256"
  (exec 9>&-; cd "$download_dir" && sha256sum -c "$asset.sha256" >/dev/null)
  tar -xzf "$download_dir/$asset" -C "$staging_dir" 9>&-

  staged_root="$staging_dir/agent-workbench-formal-tool"
  test -x "$staged_root/bin/lean"
  test -x "$staged_root/bin/lake"
  test "$(exec 9>&-; sed -n '1p' "$staged_root/lean-toolchain")" = \
    "leanprover/lean4:v4.30.0"
  test "$(exec 9>&-; sed -n '1p' "$staged_root/SOURCE_COMMIT")" = \
    "d024af099ca4bf2c86f649261ebf59565dc8c622"
  (exec 9>&-; cd "$staged_root" &&
    sha256sum -c MANIFEST.sha256 >/dev/null)

  cp "$download_dir/$asset" "$staging_dir/formal-tool.tar.gz" 9>&-
  sha256sum "$staging_dir/formal-tool.tar.gz" 9>&- |
    sed -n 's/[[:space:]].*//p' 9>&- > "$staging_dir/formal-tool.sha256"
  rm -rf "$cache_dir" 9>&-
  mv "$staging_dir" "$cache_dir" 9>&-
  staging_dir=""
  cleanup
fi

release_cache_lock
trap - EXIT HUP INT TERM
if test "$command" = "root"; then
  printf '%s\n' "$tool_root"
elif test "$command" = "identity"; then
  archive_digest="$(sed -n '1p' "$marker")"
  manifest_digest="$(sha256sum "$tool_root/MANIFEST.sha256" |
    sed -n 's/[[:space:]].*//p')"
  source_commit="$(sed -n '1p' "$tool_root/SOURCE_COMMIT")"
  lean_version="$("$tool_root/bin/lean" --version | sed -n '1p')"
  printf '%s|archive=%s|manifest=%s|source=%s\n' \
    "$lean_version" "$archive_digest" "$manifest_digest" "$source_commit"
else
  exec "$tool_root/bin/$command" "$@"
fi
