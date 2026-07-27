#!/bin/sh
set -eu

output=""
url=""
while test "$#" -gt 0; do
  case "$1" in
    -o)
      output="$2"
      shift 2
      ;;
    -*)
      shift
      ;;
    *)
      url="$1"
      shift
      ;;
  esac
done

test -n "$output"
test -n "$url"
cp "$AGENT_WORKBENCH_TEST_RELEASE_DIR/$(basename -- "$url")" "$output"
