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

if test -d .agents/skills/agent-workbench; then
  if ! diff -qr --exclude=SKILL.md skills/agent-workbench .agents/skills/agent-workbench >/dev/null; then
    echo "installed skill differs from the release skill" >&2
    exit 1
  fi
  for key in name description license; do
    release_value="$(sed -n "s/^$key:[[:space:]]*//p" skills/agent-workbench/SKILL.md | head -n 1)"
    installed_value="$(sed -n "s/^$key:[[:space:]]*//p" .agents/skills/agent-workbench/SKILL.md | head -n 1)"
    if test "$installed_value" != "$release_value"; then
      echo "installed skill $key differs from the release skill" >&2
      exit 1
    fi
  done
  contract_tmp="$(mktemp -d)"
  trap 'rm -rf "$contract_tmp"' EXIT HUP INT TERM
  awk 'BEGIN { separators=0; body=0 } /^---$/ { separators++; next } separators>=2 && body==0 && /^[[:space:]]*$/ { next } separators>=2 { body=1; print }' \
    skills/agent-workbench/SKILL.md >"$contract_tmp/release-body"
  awk 'BEGIN { separators=0; body=0 } /^---$/ { separators++; next } separators>=2 && body==0 && /^[[:space:]]*$/ { next } separators>=2 { body=1; print }' \
    .agents/skills/agent-workbench/SKILL.md >"$contract_tmp/installed-body"
  if ! diff -q "$contract_tmp/release-body" "$contract_tmp/installed-body" >/dev/null; then
    echo "installed skill body differs from the release skill" >&2
    exit 1
  fi
fi

echo "public contract: pass"
