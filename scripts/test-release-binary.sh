#!/bin/sh
set -eu

if test "$#" -ne 1; then
  echo "usage: $0 <static-binary>" >&2
  exit 2
fi

input="$1"
directory="$(CDPATH='' cd -- "$(dirname -- "$input")" && pwd -P)"
binary="$directory/$(basename -- "$input")"

test -x "$binary"
file "$binary" | grep -F "ELF 64-bit LSB executable"
file "$binary" | grep -F "statically linked"
test "$("$binary" --version)" = "agent-workbench 0.2.1"

smoke() {
  image="$1"
  docker run --rm \
    -v "$binary:/usr/local/bin/agent-workbench:ro" \
    "$image" sh -ec '
      agent-workbench --version
      agent-workbench --state /tmp/state.sqlite3 init \
        release-smoke portable-release complete
      agent-workbench --state /tmp/state.sqlite3 status |
        grep -F "state: current"
      agent-workbench --state /tmp/state.sqlite3 next |
        grep -F "next: executable"
    '
}

smoke "alpine@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce"
smoke "debian@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818"
