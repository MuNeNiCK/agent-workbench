#!/bin/sh
set -eu

if test "$#" -ne 1; then
  echo "usage: $0 <formal-tool-directory>" >&2
  exit 2
fi

tool_root="$(CDPATH='' cd -- "$1" && pwd -P)"
lean="$tool_root/bin/lean"
lake="$tool_root/bin/lake"

test -x "$lean"
test -x "$lake"
test -x "$tool_root/lib/ld-musl-x86_64.so.1"
test "$("$lean" --version | sed -n '1p')" = \
  "Lean (version 4.30.0, x86_64-alpine-linux-musl, commit d024af099ca4bf2c86f649261ebf59565dc8c622, Release)"
test "$("$lake" --version | sed -n '1p')" = \
  "Lake version 5.0.0-src+d024af0 (Lean version 4.30.0)"
test "$(sed -n '1p' "$tool_root/lean-toolchain")" = \
  "leanprover/lean4:v4.30.0"
test "$(sed -n '1p' "$tool_root/SOURCE_COMMIT")" = \
  "d024af099ca4bf2c86f649261ebf59565dc8c622"

project="$(mktemp -d)"
cleanup() {
  rm -rf "$project"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$project/Inventory"
printf '%s\n' \
  'name = "inventory"' \
  'version = "0.1.0"' \
  '[[lean_lib]]' \
  'name = "Inventory"' \
  > "$project/lakefile.toml"
printf '%s\n' 'leanprover/lean4:v4.30.0' > "$project/lean-toolchain"
printf '%s\n' 'import Inventory.Proof' > "$project/Inventory.lean"
printf '%s\n' \
  'namespace Inventory' \
  'def remaining (available reserved : Nat) : Nat := available - reserved' \
  'end Inventory' \
  > "$project/Inventory/Rule.lean"
printf '%s\n' \
  'import Inventory.Rule' \
  'namespace Inventory' \
  'theorem noReservationCreatesStock (available : Nat) :' \
  '    remaining available 0 = available := by simp [remaining]' \
  'end Inventory' \
  > "$project/Inventory/Proof.lean"
printf '%s\n' \
  'import Inventory.Proof' \
  'import Lean.Data.Json' \
  'def main : IO Unit :=' \
  '  IO.println (Lean.Json.mkObj [("remaining", Lean.toJson (7 : Nat))]).compress' \
  > "$project/Inventory/Oracle.lean"
(
  cd "$project"
  "$lake" build --wfail Inventory:leanArts
  test "$("$lake" env lean --run Inventory/Oracle.lean)" = '{"remaining":7}'
  printf '%s\n' \
    'namespace Inventory' \
    'theorem unfinished : True := by sorry' \
    'end Inventory' \
    > Inventory/Incomplete.lean
  if "$lake" build --wfail +Inventory.Incomplete:olean >/dev/null 2>&1; then
    echo "formal tool accepted an unfinished proof" >&2
    exit 1
  fi
)
