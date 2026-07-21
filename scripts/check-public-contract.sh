#!/bin/sh
set -eu

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
cd "$root"
build_target="${CARGO_TARGET_DIR:-target}"
case "$build_target" in
  /*) ;;
  *) build_target="$root/$build_target" ;;
esac
workbench_binary="$build_target/debug/agent-workbench"

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

AGENT_WORKBENCH_UNDER_TEST="$workbench_binary" \
  AGENT_WORKBENCH_SKILL_UNDER_TEST="$root/skills/agent-workbench" \
  AGENT_WORKBENCH_DOCS_UNDER_TEST="$root/docs" \
  cargo test --locked --test command_line

if ! diff -qr skills/agent-workbench .agents/skills/agent-workbench >/dev/null; then
  echo "installed skill differs from the release skill" >&2
  exit 1
fi

echo "public contract: pass"
