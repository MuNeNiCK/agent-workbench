#!/bin/sh
set -eu

candidate="${1:-target/debug/agent-workbench}"
root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
case "$candidate" in /*) ;; *) candidate="$root/$candidate" ;; esac
test -x "$candidate"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT HUP INT TERM

# These releases created the genuine schema 6 through 12 layouts.
for tag in v0.1.7 v0.1.8 v0.1.9 v0.1.10 v0.1.12 v0.1.15 v0.1.18; do
  fixture="$scratch/$tag"
  project="$fixture/project"
  mkdir -p "$fixture"
  curl -fsSL "https://github.com/MuNeNiCK/agent-workbench/releases/download/$tag/agent-workbench-$tag-linux-x86_64.tar.gz" -o "$fixture/release.tar.gz"
  tar -xzf "$fixture/release.tar.gz" -C "$fixture"
  "$fixture/agent-workbench" --root "$project" init >/dev/null
  "$fixture/agent-workbench" --root "$project" work start "schema compatibility $tag" >/dev/null

  first="$($candidate --root "$project" status)"
  second="$($candidate --root "$project" status)"
  next="$($candidate --root "$project" next)"
  printf '%s\n' "$first" | grep -Fx initialized >/dev/null
  printf '%s\n' "$first" | grep -Fx 'project_integrity: clear' >/dev/null
  test "$first" = "$second"
  printf '%s\n' "$next" | grep -Fx 'work_unit_id: 1' >/dev/null
done

echo "release ledger compatibility: pass"
