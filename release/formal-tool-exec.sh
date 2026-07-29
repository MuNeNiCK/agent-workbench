#!/bin/sh
set -eu

tool="$(basename -- "$0")"
tool_root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
loader="$tool_root/lib/ld-musl-x86_64.so.1"
executable="$tool_root/bin/.$tool.real"

if ! test -x "$loader" || ! test -x "$executable"; then
  echo "agent-workbench formal tool is incomplete: $tool" >&2
  exit 1
fi

export LEAN_SYSROOT="$tool_root"
export PATH="$tool_root/bin:$PATH"
exec "$loader" \
  --library-path "$tool_root/lib:$tool_root/lib/lean" \
  "$executable" "$@"
