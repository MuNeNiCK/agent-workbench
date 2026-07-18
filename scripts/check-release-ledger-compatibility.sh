#!/bin/sh
set -eu

candidate="${1:-target/debug/agent-workbench}"
root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
case "$candidate" in /*) ;; *) candidate="$root/$candidate" ;; esac
test -x "$candidate"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT HUP INT TERM

fetch_release() {
  tag="$1"
  directory="$scratch/$tag"
  mkdir -p "$directory"
  curl -fsSL "https://github.com/MuNeNiCK/agent-workbench/releases/download/$tag/agent-workbench-$tag-linux-x86_64.tar.gz" -o "$directory/release.tar.gz"
  tar -xzf "$directory/release.tar.gz" -C "$directory"
}

fetch_release v0.1.18
fetch_release v0.1.19
fetch_release v0.1.17

exercise() {
  project="$1"
  expected_profile="$2"
  before="$(sha256sum "$project/.agent-workbench/ledger.sqlite" | sed 's/[[:space:]].*//')"
  plan="$($candidate --root "$project" update --dry-run)"
  source_identity="$(printf '%s\n' "$plan" | sed -n 's/^source_identity: //p')"
  printf '%s\n' "$plan" | grep -Fx "source_schema: 13" >/dev/null
  printf '%s\n' "$plan" | grep -Fx "source_profile: $expected_profile" >/dev/null
  printf '%s\n' "$plan" | grep -Fx "domain_rows_imported: 0" >/dev/null
  test "$before" = "$(sha256sum "$project/.agent-workbench/ledger.sqlite" | sed 's/[[:space:]].*//')"
  reset="$($candidate --root "$project" update --reset --reason 'release compatibility exercise')"
  backup="$(printf '%s\n' "$reset" | sed -n 's/^backup_handle: //p')"
  current="$(sha256sum "$project/.agent-workbench/ledger.sqlite" | sed 's/[[:space:]].*//')"
  "$candidate" --root "$project" update restore --backup "$backup" --expected-current "$current" >/dev/null
  test "$source_identity" = "$(sha256sum "$project/.agent-workbench/ledger.sqlite" | sed 's/[[:space:]].*//')"
}

fresh="$scratch/fresh-project"
"$scratch/v0.1.19/agent-workbench" --root "$fresh" init >/dev/null
exercise "$fresh" 877adac85029da006fd293f5f943b0191dac22f45fab2b6f596f156626bfcf76

retired="$scratch/retired-authority-project"
"$scratch/v0.1.18/agent-workbench" --root "$retired" init >/dev/null
"$scratch/v0.1.19/agent-workbench" --root "$retired" status >/dev/null
exercise "$retired" 08ce59ebc53cc6b422e01b25c173e84cccee539d23175fd72317f12f0a436166

legacy="$scratch/v17-project"
"$scratch/v0.1.17/agent-workbench" --root "$legacy" init >/dev/null
"$scratch/v0.1.19/agent-workbench" --root "$legacy" status >/dev/null
exercise "$legacy" c1ec0110b62a963f692abc1cdfe2ce2774b95fba5c795aaff79436db62d2bd9e

echo "release ledger compatibility: pass"
