#!/bin/sh
set -eu

if test -n "${AGENT_WORKBENCH_TEST_HASH_READY:-}"; then
  : > "$AGENT_WORKBENCH_TEST_HASH_READY"
  sleep "${AGENT_WORKBENCH_TEST_HASH_DELAY:-30}"
fi
exec /usr/bin/sha256sum "$@"
