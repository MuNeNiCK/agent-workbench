#!/bin/sh
set -eu

if test "$#" -lt 1 || test "$#" -gt 2; then
  echo "usage: $0 <agent-workbench-binary> [formal-tool-archive]" >&2
  exit 2
fi

binary="$(CDPATH='' cd -- "$(dirname -- "$1")" && pwd -P)/$(basename -- "$1")"
formal_archive="${2:-}"
test -x "$binary"
if test -n "$formal_archive"; then
  formal_archive="$(
    CDPATH='' cd -- "$(dirname -- "$formal_archive")" && pwd -P
  )/$(basename -- "$formal_archive")"
  test -s "$formal_archive"
fi

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
test_area="$(mktemp -d)"
cleanup() {
  rm -rf "$test_area"
}
trap cleanup EXIT HUP INT TERM

skill="$test_area/installed/agent-workbench"
cache="$test_area/cache"
release="$test_area/release"
test_bin="$test_area/bin"
hash_bin="$test_area/hash-bin"
mkdir -p "$skill" "$release" "$test_bin" "$hash_bin"
cp -R "$root/skills/agent-workbench/." "$skill/"
version="$(sed -n '1{s/[[:space:]]//g;p;}' "$skill/CLI_VERSION")"
runtime_asset="agent-workbench-$version-linux-x86_64-static.tar.gz"
formal_asset="agent-workbench-$version-formal-tool-linux-x86_64.tar.gz"
checksums="agent-workbench-$version-checksums.txt"

cp "$binary" "$release/agent-workbench"
chmod +x "$skill/scripts/"*.sh "$release/agent-workbench"
tar -czf "$release/$runtime_asset" -C "$release" agent-workbench
if test -n "$formal_archive"; then
  cp "$formal_archive" "$release/$formal_asset"
fi
(
  cd "$release"
  sha256sum "$runtime_asset" > "$checksums"
  if test -n "$formal_archive"; then
    sha256sum "$formal_asset" >> "$checksums"
  fi
)
cp "$root/scripts/test-support/curl-release.sh" "$test_bin/curl"
cp "$root/scripts/test-support/sha256sum-delay.sh" "$hash_bin/sha256sum"
chmod +x "$test_bin/curl" "$hash_bin/sha256sum"

awb() {
  PATH="$test_bin:$PATH" \
    XDG_CACHE_HOME="$cache" \
    AGENT_WORKBENCH_TEST_RELEASE_DIR="$release" \
    "$skill/scripts/agent-workbench.sh" "$@"
}

review_design() {
  design_key="$1"
  awb request-design-review "design-$design_key" "$design_key" >/dev/null
  awb record-clean-review "design-$design_key" reviewer >/dev/null
}

accept_design() {
  design_key="$1"
  reason="$2"
  review_design "$design_key"
  awb accept-design "$design_key" "$reason" >/dev/null
}

assert_restart_stable() {
  first_projection="$(awb status)"
  test "$(awb status)" = "$first_projection"
  first_next="$(awb next)"
  test "$(awb next)" = "$first_next"
}

project="$test_area/project"
mkdir -p "$project/nested"
git -C "$project" init -q
printf '%s\n' "console.log('ordinary-project')" > "$project/main.js"
project_file_before="$(sha256sum "$project/main.js")"
config_before="$(sha256sum "$project/.git/config")"
exclude_before="$(sha256sum "$project/.git/info/exclude")"
index_before="$(git -C "$project" status --porcelain --untracked-files=no)"

(
  cd "$project/nested"
  awb init "repair the released application" "apply the bounded fix" >/dev/null
  first_status="$(awb status)"
  test "$(awb status)" = "$first_status"
)
test -f "$project/.agent-workbench/state.sqlite3"
test ! -e "$project/nested/.agent-workbench"
test "$(sha256sum "$project/.git/config")" = "$config_before"
test "$(sha256sum "$project/.git/info/exclude")" = "$exclude_before"
test "$(git -C "$project" status --porcelain --untracked-files=no)" = "$index_before"
test "$(sha256sum "$project/main.js")" = "$project_file_before"

runtime_dir="$cache/agent-workbench/releases/$version/linux-x86_64-static"
formal_dir="$cache/agent-workbench/releases/$version/formal-tool-linux-x86_64"
test -x "$runtime_dir/agent-workbench"
test -s "$runtime_dir/agent-workbench.sha256"
test ! -e "$formal_dir"

# Each public projection is reconstructed by a new process. These checks assert
# projection equality only; component tests own the state-transition meaning.
restart="$test_area/restart"
mkdir -p "$restart"
git -C "$restart" init -q
(
  cd "$restart"
  awb init "preserve project state" "prepare restart state" >/dev/null
  awb record-instruction "Preserve the selected boundary after restart." >/dev/null
  assert_restart_stable
  awb finish-task >/dev/null
  assert_restart_stable
  awb record-design restart-rule functional none \
    "The selected value is restored after restart." >/dev/null
  accept_design restart-rule "Caller selected the restart behavior."
  assert_restart_stable
  awb add-task-for-design "implement restart behavior" restart-rule >/dev/null
  awb request-review restart-review implementation src/restart >/dev/null
  awb record-review restart-review reviewer restored risk ordinary \
    "Confirm the selected restart boundary." \
    "The selected artifact was reviewed." >/dev/null
  awb resolve-review restart-review restored accepted \
    "Caller accepted the bounded observation." >/dev/null
  assert_restart_stable
  awb record-design latency non-functional evidence \
    "The selected command stays within its latency budget." >/dev/null
  accept_design latency "Caller selected the latency boundary."
  awb add-task-for-design "measure selected latency" latency >/dev/null
  awb add-evidence latency "Observe selected latency." \
    "measure selected command" "supported release host" "command=check" \
    "elapsed <= 100 ms" "monotonic clock" "sha256:release" >/dev/null
  awb record-evidence latency "42 ms" pass >/dev/null
  assert_restart_stable
  awb record-design restart-rule functional none \
    "The corrected selected value is restored after restart." >/dev/null
  accept_design restart-rule "Caller corrected the restart behavior."
  assert_restart_stable
  awb interrupt "repair urgent security issue" "apply urgent fix" >/dev/null
  assert_restart_stable
)

# A public lost response is retried unchanged. A different action remains
# unavailable until that exact intention is reconciled.
mv "$runtime_dir/agent-workbench" "$runtime_dir/agent-workbench.real"
# shellcheck disable=SC2016
printf '%s\n' \
  '#!/bin/sh' \
  'set -eu' \
  'directory="$(CDPATH='"'"''"'"' cd -- "$(dirname -- "$0")" && pwd -P)"' \
  'if test "${1:-}" = add-task && test ! -e "$directory/injected"; then' \
  '  "$directory/agent-workbench.real" "$@"' \
  '  : > "$directory/injected"' \
  '  exit 75' \
  'fi' \
  'exec "$directory/agent-workbench.real" "$@"' \
  > "$runtime_dir/agent-workbench"
chmod +x "$runtime_dir/agent-workbench"
sha256sum "$runtime_dir/agent-workbench" |
  sed -n 's/[[:space:]].*//p' > "$runtime_dir/agent-workbench.sha256"
(
  cd "$project"
  if awb add-task "apply the recovered change" >/dev/null 2>&1; then
    echo "lost-response injection unexpectedly reported success" >&2
    exit 1
  fi
  if awb finish-task >/dev/null 2>&1; then
    echo "a different action bypassed the pending intention" >&2
    exit 1
  fi
  awb add-task "apply the recovered change" >/dev/null
)
mv "$runtime_dir/agent-workbench.real" "$runtime_dir/agent-workbench"
sha256sum "$runtime_dir/agent-workbench" |
  sed -n 's/[[:space:]].*//p' > "$runtime_dir/agent-workbench.sha256"

# A terminated runtime publisher cannot retain the installation lock.
lock_cache="$test_area/lock-cache"
hash_ready="$test_area/runtime-hash-ready"
setsid env \
  PATH="$hash_bin:$test_bin:$PATH" \
  XDG_CACHE_HOME="$lock_cache" \
  AGENT_WORKBENCH_TEST_RELEASE_DIR="$release" \
  AGENT_WORKBENCH_TEST_HASH_READY="$hash_ready" \
  AGENT_WORKBENCH_TEST_HASH_DELAY=30 \
  "$skill/scripts/agent-workbench.sh" --version >/dev/null 2>&1 &
hash_owner=$!
while test ! -e "$hash_ready"; do sleep 0.01; done
kill -9 "$hash_owner"
set +e
wait "$hash_owner" 2>/dev/null
set -e
runtime_lock="$lock_cache/agent-workbench/releases/$version/.runtime-install-lock"
if ! flock -n "$runtime_lock" true; then
  echo "a terminated publisher retained the runtime installation lock" >&2
  exit 1
fi
PATH="$test_bin:$PATH" XDG_CACHE_HOME="$lock_cache" \
  AGENT_WORKBENCH_TEST_RELEASE_DIR="$release" \
  "$skill/scripts/agent-workbench.sh" --version >/dev/null
kill -TERM "-$hash_owner" 2>/dev/null || true

# Real SQLite contention is translated to the public wait result, and process
# termination releases the store lock.
busy="$test_area/busy"
mkdir -p "$busy"
git -C "$busy" init -q
(
  cd "$busy"
  awb init "wait for project memory" "prepare contention" >/dev/null
  printf '%s\n' \
    'BEGIN IMMEDIATE;' \
    '.shell sleep 6' \
    'COMMIT;' |
    sqlite3 .agent-workbench/state.sqlite3 >/dev/null &
  locker=$!
  sleep 0.2
  set +e
  busy_output="$(awb add-task "wait for the writer" 2>&1)"
  busy_status=$?
  set -e
  test "$busy_status" -ne 0
  printf '%s\n' "$busy_output" |
    grep -F "Project memory is busy; wait and retry this action." >/dev/null
  wait "$locker"
  printf '%s\n' \
    'BEGIN IMMEDIATE;' \
    '.shell sleep 30' \
    'COMMIT;' |
    sqlite3 .agent-workbench/state.sqlite3 >/dev/null &
  terminated_locker=$!
  sleep 0.2
  kill -9 "$terminated_locker"
  set +e
  wait "$terminated_locker" 2>/dev/null
  set -e
  awb add-task "continue after terminated writer" >/dev/null
)

# Repository-selected state replacement is reported and never overwritten.
replacement_cache="$test_area/replacement-cache"
awb_replacement() {
  PATH="$test_bin:$PATH" \
    XDG_CACHE_HOME="$replacement_cache" \
    AGENT_WORKBENCH_TEST_RELEASE_DIR="$release" \
    "$skill/scripts/agent-workbench.sh" "$@"
}
project_a="$test_area/project-a"
project_b="$test_area/project-b"
mkdir -p "$project_a" "$project_b"
git -C "$project_a" init -q
git -C "$project_b" init -q
(
  cd "$project_a"
  awb_replacement init "preserve project A" "prepare A" >/dev/null
)
(
  cd "$project_b"
  awb_replacement init "preserve project B" "prepare B" >/dev/null
)
cp "$project_b/.agent-workbench/state.sqlite3" "$project_a/replacement.sqlite3"
replacement_runtime="$replacement_cache/agent-workbench/releases/$version/linux-x86_64-static"
mv "$replacement_runtime/agent-workbench" \
  "$replacement_runtime/agent-workbench.real"
# shellcheck disable=SC2016
printf '%s\n' \
  '#!/bin/sh' \
  'set -eu' \
  'directory="$(CDPATH='"'"''"'"' cd -- "$(dirname -- "$0")" && pwd -P)"' \
  'if test "${1:-}" = add-task; then' \
  '  cp "$(dirname -- "$AGENT_WORKBENCH_STATE_PATH")/../replacement.sqlite3" "$AGENT_WORKBENCH_STATE_PATH"' \
  'fi' \
  'exec "$directory/agent-workbench.real" "$@"' \
  > "$replacement_runtime/agent-workbench"
chmod +x "$replacement_runtime/agent-workbench"
sha256sum "$replacement_runtime/agent-workbench" |
  sed -n 's/[[:space:]].*//p' > "$replacement_runtime/agent-workbench.sha256"
(
  cd "$project_a"
  if awb_replacement add-task "must not enter project B" >/dev/null 2>&1; then
    echo "replacement project state was overwritten" >&2
    exit 1
  fi
  awb_replacement status | grep -F "Outcome: preserve project B" >/dev/null
  if awb_replacement status | grep -F "must not enter project B" >/dev/null; then
    echo "rejected replacement mutation changed selected state" >&2
    exit 1
  fi
)

# Two mutations issued from one public projection yield one success. The stale
# process reports the current project projection instead of retargeting.
concurrent_cache="$test_area/concurrent-cache"
awb_concurrent() {
  PATH="$test_bin:$PATH" \
    XDG_CACHE_HOME="$concurrent_cache" \
    AGENT_WORKBENCH_TEST_RELEASE_DIR="$release" \
    "$skill/scripts/agent-workbench.sh" "$@"
}
concurrent="$test_area/concurrent"
mkdir -p "$concurrent"
git -C "$concurrent" init -q
(
  cd "$concurrent"
  awb_concurrent init "serialize concurrent work" "prepare concurrency" >/dev/null
)
concurrent_runtime="$concurrent_cache/agent-workbench/releases/$version/linux-x86_64-static"
mv "$concurrent_runtime/agent-workbench" \
  "$concurrent_runtime/agent-workbench.real"
# shellcheck disable=SC2016
printf '%s\n' \
  '#!/bin/sh' \
  'set -eu' \
  'directory="$(CDPATH='"'"''"'"' cd -- "$(dirname -- "$0")" && pwd -P)"' \
  'if test "${1:-}" = state-context; then' \
  '  if mkdir "$directory/revision-barrier" 2>/dev/null; then' \
  '    while test ! -e "$directory/revision-barrier/second"; do sleep 0.01; done' \
  '  else' \
  '    : > "$directory/revision-barrier/second"' \
  '  fi' \
  'fi' \
  'exec "$directory/agent-workbench.real" "$@"' \
  > "$concurrent_runtime/agent-workbench"
chmod +x "$concurrent_runtime/agent-workbench"
sha256sum "$concurrent_runtime/agent-workbench" |
  sed -n 's/[[:space:]].*//p' > "$concurrent_runtime/agent-workbench.sha256"
(
  cd "$concurrent"
  set +e
  awb_concurrent add-task "concurrent task A" >first.out 2>&1 &
  first=$!
  awb_concurrent add-task "concurrent task B" >second.out 2>&1 &
  second=$!
  wait "$first"
  first_status=$?
  wait "$second"
  second_status=$?
  set -e
  successes=0
  test "$first_status" -eq 0 && successes=$((successes + 1))
  test "$second_status" -eq 0 && successes=$((successes + 1))
  test "$successes" -eq 1
  if test "$first_status" -ne 0; then loser_output=first.out
  else loser_output=second.out
  fi
  grep -F "Outcome: serialize concurrent work" "$loser_output" >/dev/null
  grep -F "Next:" "$loser_output" >/dev/null
)

if test -n "$formal_archive"; then
  formal="$test_area/formal"
  mkdir -p "$formal/Inventory" "$formal/bin" "$formal/test"
  git -C "$formal" init -q
  printf '%s\n' \
    'name = "inventory"' \
    'version = "0.1.0"' \
    '[[lean_lib]]' \
    'name = "Inventory"' \
    > "$formal/lakefile.toml"
  printf '%s\n' 'leanprover/lean4:v4.30.0' > "$formal/lean-toolchain"
  printf '%s\n' \
    'namespace Inventory' \
    'def canReserve (stock quantity : Nat) : Bool := quantity <= stock' \
    'end Inventory' \
    > "$formal/Inventory/Rule.lean"
  printf '%s\n' \
    'import Inventory.Rule' \
    'namespace Inventory' \
    'theorem zeroQuantityIsAvailable (stock : Nat) :' \
    '    canReserve stock 0 = true := by simp [canReserve]' \
    'end Inventory' \
    > "$formal/Inventory/Proof.lean"
  printf '%s\n' \
    'import Inventory.Proof' \
    'import Lean.Data.Json' \
    'def main : IO Unit := do' \
    '  let input ← (← IO.getStdin).readToEnd' \
    "  let parts := (input.split fun c => c == ' ' || c == '\\n').toList.filter (fun part => !part.isEmpty)" \
    '  match parts with' \
    '  | [stockText, quantityText] =>' \
    '      match stockText.toNat?, quantityText.toNat? with' \
    '      | some stock, some quantity =>' \
    '          IO.println (Lean.Json.mkObj [' \
    '            ("available", .bool (Inventory.canReserve stock quantity))]).compress' \
    '      | _, _ => throw <| IO.userError "invalid inventory input"' \
    '  | _ => throw <| IO.userError "invalid inventory input"' \
    > "$formal/Inventory/Oracle.lean"
  # shellcheck disable=SC2016
  printf '%s\n' \
    '#!/bin/sh' \
    'set -eu' \
    'read -r stock quantity' \
    'if test "$quantity" -lt "$stock"; then available=true; else available=false; fi' \
    'printf '"'"'{"available":%s}\n'"'"' "$available"' \
    > "$formal/bin/inventory"
  # shellcheck disable=SC2016
  printf '%s\n' \
    '#!/bin/sh' \
    'set -eu' \
    'here="$(CDPATH='"'"''"'"' cd -- "$(dirname -- "$0")/.." && pwd -P)"' \
    'exec "$here/bin/inventory"' \
    > "$formal/test/observe-inventory"
  printf '%s\n' '5 3' > "$formal/test/case-available"
  printf '%s\n' '2 4' > "$formal/test/case-unavailable"
  printf '%s\n' '3 3' > "$formal/test/case-equal"
  chmod +x "$formal/bin/inventory" "$formal/test/observe-inventory"
  (
    cd "$formal"
    awb init "implement reservation availability" "prepare the boundary" >/dev/null
    awb finish-task >/dev/null
    awb record-design inventory functional formal \
      "A reservation is available when quantity does not exceed stock" >/dev/null
    failed_preview="$(awb preview-formal inventory inventory Inventory.Oracle \
      Inventory.Rule,Inventory.Proof bin/inventory \
      test/observe-inventory \
      test/case-available,test/case-unavailable,test/case-equal)"
    printf '%s\n' "$failed_preview" |
      grep -F "Conformance: fail" >/dev/null
    # shellcheck disable=SC2016
    printf '%s\n' \
      '#!/bin/sh' \
      'set -eu' \
      'read -r stock quantity' \
      'if test "$quantity" -le "$stock"; then available=true; else available=false; fi' \
      'printf '"'"'{"available":%s}\n'"'"' "$available"' \
      > bin/inventory
    chmod +x bin/inventory
    passing_report="$(awb formal-check inventory)"
    printf '%s\n' "$passing_report" |
      grep -F "Conformance: pass" >/dev/null
    printf '%s\n' "$passing_report" |
      grep -E '^Checked module closure: .*Inventory\.Oracle' >/dev/null
    printf '%s\n' "$passing_report" |
      grep -E '^Checked module closure: .*Inventory\.Proof' >/dev/null
    printf '%s\n' "$passing_report" |
      grep -E '^Checked module closure: .*Inventory\.Rule' >/dev/null
    printf '%s\n' "$passing_report" |
      grep -F 'source=d024af099ca4bf2c86f649261ebf59565dc8c622' >/dev/null
    printf '%s\n' "$passing_report" |
      grep -E '^Oracle artifact: Inventory\.Oracle=sha256:[0-9a-f]{64}$' >/dev/null
    for artifact in \
        Inventory/Oracle.lean Inventory/Proof.lean Inventory/Rule.lean \
        bin/inventory test/observe-inventory \
        test/case-available test/case-unavailable test/case-equal; do
      printf '%s\n' "$passing_report" |
        grep -E "^  $artifact=sha256:[0-9a-f]{64}$" >/dev/null
    done
    formal_projection="$(awb status)"
    printf '%s\n' "$formal_projection" | grep -F "Checked closure:" >/dev/null
    printf '%s\n' "$formal_projection" | grep -F "Checked artifacts:" >/dev/null
    formal_identity="$(
      printf '%s\n' "$formal_projection" |
        sed -n 's/^    Preview identity: //p' |
        sed -n '1p'
    )"
    test -n "$formal_identity"

    printf '%s\n' '# changed selected implementation surface' >> bin/inventory
    set +e
    stale_output="$(awb status 2>&1)"
    stale_status=$?
    set -e
    test "$stale_status" -ne 0
    printf '%s\n' "$stale_output" |
      grep -F "formal assurance 'inventory' is stale" >/dev/null

    # shellcheck disable=SC2016
    printf '%s\n' \
      '#!/bin/sh' \
      'set -eu' \
      'read -r stock quantity' \
      'if test "$quantity" -le "$stock"; then available=true; else available=false; fi' \
      'printf '"'"'{"available":%s}\n'"'"' "$available"' \
      > bin/inventory
    chmod +x bin/inventory
    awb formal-check inventory >/dev/null
    accept_design inventory "Caller selected the reviewed inventory rule."
    awb add-task-for-design "apply selected inventory rule" inventory >/dev/null
    printf '%s\n' "unrelated project note" > notes.txt
    awb request-review inventory-reuse reuse notes.txt >/dev/null
    awb record-clean-review inventory-reuse reuse-reviewer >/dev/null
    awb finish-task >/dev/null
    reused_projection="$(awb status)"
    printf '%s\n' "$reused_projection" |
      grep -F "    Preview identity: $formal_identity" >/dev/null
  )
  test -x "$formal_dir/agent-workbench-formal-tool/bin/lean"
  test -s "$formal_dir/formal-tool.sha256"
fi

printf '%s\n' "installed skill boundaries: pass"
