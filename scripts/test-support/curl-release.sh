#!/bin/sh
set -eu

output=""
url=""
connect_timeout=""
speed_limit=""
speed_time=""
while test "$#" -gt 0; do
  case "$1" in
    -o)
      output="$2"
      shift 2
      ;;
    --connect-timeout)
      connect_timeout="$2"
      shift 2
      ;;
    --speed-limit)
      speed_limit="$2"
      shift 2
      ;;
    --speed-time)
      speed_time="$2"
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
test "$connect_timeout" = 30
test "$speed_limit" = 1024
test "$speed_time" = 60
cp "$AGENT_WORKBENCH_TEST_RELEASE_DIR/$(basename -- "$url")" "$output"
