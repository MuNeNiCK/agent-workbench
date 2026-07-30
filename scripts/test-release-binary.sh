#!/bin/sh
set -eu

if test "$#" -ne 1; then
  echo "usage: $0 <binary>" >&2
  exit 2
fi

input="$1"
directory="$(CDPATH='' cd -- "$(dirname -- "$input")" && pwd -P)"
binary="$directory/$(basename -- "$input")"
root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
version="$(sed -n '1{s/[[:space:]]//g;p;}' \
  "$root/skills/agent-workbench/CLI_VERSION")"

test -x "$binary"
file "$binary" | grep -F "ELF 64-bit LSB"
test "$("$binary" --version)" = "agent-workbench ${version#v}"
"$binary" --help >/dev/null
