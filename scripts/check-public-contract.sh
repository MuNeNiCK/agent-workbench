#!/bin/sh
set -eu

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
cd "$root"

version="$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n 1)"
skill_version="$(sed -n '1{s/[[:space:]]//g;p;}' skills/agent-workbench/CLI_VERSION)"
test "$skill_version" = "v$version"

git ls-files | while IFS= read -r path; do
  case "$path" in
    .agent-workbench/*|.agents/*)
      echo "private workbench material is tracked: $path" >&2
      exit 1
      ;;
  esac
done

if grep -Eq 'contents:[[:space:]]*write|workflow_dispatch:|gh release (create|upload|delete)' .github/workflows/release.yml; then
  echo "release workflow is not observer-only" >&2
  exit 1
fi

for command in \
  "authority assertion request" \
  "authority assertion assemble" \
  "owner grant" \
  "principal resolve" \
  "review provenance issue" \
  "review invocation" \
  "review adjudicate" \
  "finding decide" \
  "verification adjudicate" \
  "decision capability issue"
do
  target/debug/agent-workbench $command --help >/dev/null
done

echo "public contract: pass"
