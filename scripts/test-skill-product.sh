#!/bin/sh
set -eu

if test "$#" -ne 1; then
  echo "usage: $0 <agent-workbench-binary>" >&2
  exit 2
fi

binary="$(CDPATH='' cd -- "$(dirname -- "$1")" && pwd -P)/$(basename -- "$1")"
test -x "$binary"

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
fixture="$(mktemp -d)"
cleanup() {
  rm -rf "$fixture"
}
trap cleanup EXIT HUP INT TERM

skill="$fixture/installed/agent-workbench"
cache="$fixture/cache"
release="$fixture/release"
test_bin="$fixture/bin"
mkdir -p "$skill" "$release" "$test_bin"
cp -R "$root/skills/agent-workbench/." "$skill/"
version="$(sed -n '1{s/[[:space:]]//g;p;}' "$skill/CLI_VERSION")"
asset="agent-workbench-$version-linux-x86_64-static.tar.gz"
checksums="agent-workbench-$version-checksums.txt"
cp "$binary" "$release/agent-workbench"
chmod +x "$skill/scripts/agent-workbench.sh" "$release/agent-workbench"
tar -czf "$release/$asset" -C "$release" agent-workbench
(
  cd "$release"
  sha256sum "$asset" > "$checksums"
)
cp "$root/scripts/fixtures/curl-release.sh" "$test_bin/curl"
chmod +x "$test_bin/curl"

awb() {
  PATH="$test_bin:$PATH" XDG_CACHE_HOME="$cache" \
    AGENT_WORKBENCH_TEST_RELEASE_DIR="$release" \
    "$skill/scripts/agent-workbench.sh" "$@"
}

complete_fixture() {
  project="$1"
  init_directory="$2"
  entry="$3"
  tool="$4"
  marker="$5"

  before="$(cd "$project" && "$tool" "$entry")"
  (
    cd "$init_directory"
    awb init owner "$marker" fixture-complete >/dev/null
  )
  test -f "$project/.agent-workbench/state.sqlite3"
  test ! -e "$project/nested/.agent-workbench"
  grep -aF "$marker" "$project/.agent-workbench/state.sqlite3" >/dev/null
  if git -C "$root" grep -F "$marker" >/dev/null; then
    echo "private fixture identity leaked into tracked product content" >&2
    exit 1
  fi

  (
    cd "$project/nested"
    awb status | grep -F "state: current" >/dev/null
    awb next | grep -F "next: executable" >/dev/null
    awb continue 1 1 1 >/dev/null
  )
  request_number=0
  while IFS= read -r request; do
    request_number=$((request_number + 1))
    request_file="$fixture/request-$(basename -- "$project")-$request_number.json"
    printf '%s\n' "$request" > "$request_file"
    (cd "$project/nested" && awb apply "$request_file" >/dev/null)
  done < "$root/scripts/fixtures/skill-lifecycle.ndjson"

  (
    cd "$project/nested"
    awb status | grep -F "active: none" >/dev/null
    awb doctor | grep -F "diagnosis: healthy" >/dev/null
    awb update inspect | grep -F "update: current" >/dev/null
    awb export "skill-product-$marker" correction \
      "$fixture/$(basename -- "$project")-correction-export.txt" >/dev/null
  )
  grep -F "class=correction" \
    "$fixture/$(basename -- "$project")-correction-export.txt" >/dev/null
  with_state="$(cd "$project" && "$tool" "$entry")"
  backup="$fixture/$(basename -- "$project")-state"
  mv "$project/.agent-workbench" "$backup"
  after="$(cd "$project" && "$tool" "$entry")"
  test "$before" = "$with_state"
  test "$before" = "$after"
}

suffix="$(printf '%s' "$fixture" | cksum | awk '{print $1}')"
node_project="$fixture/node-project"
mkdir -p "$node_project/nested"
printf '%s\n' "console.log('node-fixture')" > "$node_project/main.js"
git -C "$node_project" init -q
complete_fixture "$node_project" "$node_project/nested" main.js node \
  "node-private-$suffix"

runtime_dir="$cache/agent-workbench/releases/$version/linux-x86_64-static"
test -x "$runtime_dir/agent-workbench"
test -s "$runtime_dir/agent-workbench.sha256"

python_project="$fixture/python-project"
mkdir -p "$python_project/nested"
printf '%s\n' "print('python-fixture')" > "$python_project/main.py"
complete_fixture "$python_project" "$python_project" main.py python3 \
  "python-private-$suffix"

interrupted="$fixture/interrupted"
mkdir -p "$interrupted"
(
  cd "$interrupted"
  awb init fixture-owner interrupted-outcome interrupted-complete >/dev/null
  printf '%s\n' \
    '{"operation":"interrupt-fixture","expectedRevision":1,"command":"suspend-work","work":1,"activation":1,"reason":"verify durable interruption","returnPoint":"resume fixture","assumptions":["state is durable"],"resumeConditions":["caller requests resume"]}' \
    > "$fixture/interrupt.json"
  awb apply "$fixture/interrupt.json" >/dev/null
)
(
  cd "$interrupted"
  awb status | grep -F "state: current" >/dev/null
  awb next | grep -F "next: blocked" >/dev/null
)

test ! -e "$skill/.agent-workbench"
test ! -e "$runtime_dir/.agent-workbench"
printf '%s\n' "skill product: pass"
