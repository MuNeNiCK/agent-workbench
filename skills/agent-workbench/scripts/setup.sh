#!/bin/sh
set -eu

project_root=${1:-"$(pwd)"}
local_archive=${2:-}
local_checksum=${3:-}
repository=https://github.com/MuNeNiCK/agent-workbench

case "$(uname -s):$(uname -m)" in
  Linux:x86_64) target=linux-x86_64 ;;
  Linux:aarch64|Linux:arm64) target=linux-aarch64 ;;
  Darwin:x86_64) target=macos-x86_64 ;;
  Darwin:arm64) target=macos-aarch64 ;;
  *) echo "unsupported Agent Workbench platform" >&2; exit 1 ;;
esac

archive=agent-workbench-${target}.tar.gz
temporary=$(mktemp -d "${TMPDIR:-/tmp}/agent-workbench-setup.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

if [ -n "$local_archive" ] || [ -n "$local_checksum" ]; then
  test -n "$local_archive" && test -n "$local_checksum"
  cp "$local_archive" "$temporary/$archive"
  cp "$local_checksum" "$temporary/$archive.sha256"
else
  curl -fL --retry 3 -o "$temporary/$archive" \
    "$repository/releases/latest/download/$archive"
  curl -fL --retry 3 -o "$temporary/$archive.sha256" \
    "$repository/releases/latest/download/$archive.sha256"
fi

expected=$(sed 's/[[:space:]].*$//' "$temporary/$archive.sha256")
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$temporary/$archive" | sed 's/[[:space:]].*$//')
else
  actual=$(shasum -a 256 "$temporary/$archive" | sed 's/[[:space:]].*$//')
fi
test "$actual" = "$expected"

mkdir -p "$project_root/.agent-workbench/bin"
tar -xzf "$temporary/$archive" -C "$project_root/.agent-workbench/bin"
chmod +x "$project_root/.agent-workbench/bin/agent-workbench" \
  "$project_root/.agent-workbench/bin/elan"
"$project_root/.agent-workbench/bin/agent-workbench" --project "$project_root" init
