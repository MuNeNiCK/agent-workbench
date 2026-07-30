#!/bin/sh
set -eu

if test "$#" -ne 4; then
  echo "usage: $0 <v-tag> <output-directory> <static-binary> <formal-tool-archive>" >&2
  exit 2
fi

tag="$1"
out="$2"
binary="$3"
formal_tool_archive="$4"
case "$tag" in
  v?*) ;;
  *) echo "tag must start with v" >&2; exit 2 ;;
esac

default_root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
root="${AGENT_WORKBENCH_RELEASE_SOURCE_ROOT:-$default_root}"
root="$(CDPATH='' cd -- "$root" && pwd -P)"
cd "$root"
version="${tag#v}"
test "$(sed -n 's/.*version := v!\"\([^\"]*\)\".*/\1/p' lakefile.lean |
  head -n 1)" = "$version"
test "$(sed -n '1{s/[[:space:]]//g;p;}' \
  skills/agent-workbench/CLI_VERSION)" = "$tag"
test -x "$binary"
test "$("$binary" --version)" = "agent-workbench $version"
file "$binary" | grep -F "ELF 64-bit LSB executable"
file "$binary" | grep -F "statically linked"
test -f "$formal_tool_archive"

epoch="$(git show -s --format=%ct HEAD)"
stage="$(mktemp -d)"
cleanup() {
  rm -rf "$stage"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$out" "$stage/binary" "$stage/formal"
cp "$binary" "$stage/binary/agent-workbench"
strip "$stage/binary/agent-workbench"
entries="$stage/formal-tool.entries"
if ! tar -tzf "$formal_tool_archive" > "$entries"; then
  echo "formal tool archive is unreadable" >&2
  exit 1
fi
if grep -Ev '^agent-workbench-formal-tool(/|$)' "$entries" ||
    grep -E '(^|/)\.\.(/|$)' "$entries"; then
  echo "formal tool archive contains an unexpected path" >&2
  exit 1
fi
tar -xzf "$formal_tool_archive" -C "$stage/formal"
"$default_root/scripts/test-formal-tool-asset.sh" \
  "$stage/formal/agent-workbench-formal-tool"

archive() {
  directory="$1"
  name="$2"
  shift 2
  tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    -C "$directory" -cf - "$@" | gzip -n > "$out/$name"
}

runtime_archive="agent-workbench-$tag-linux-x86_64-static.tar.gz"
formal_archive="agent-workbench-$tag-formal-tool-linux-x86_64.tar.gz"

archive "$stage/binary" "$runtime_archive" agent-workbench
cp "$formal_tool_archive" "$out/$formal_archive"

mkdir -p "$stage/verify"
tar -xzf "$out/$runtime_archive" -C "$stage/verify"
test "$("$stage/verify/agent-workbench" --version)" = \
  "agent-workbench $version"
"$stage/verify/agent-workbench" --help >/dev/null
scripts/test-skill.sh "$stage/verify/agent-workbench"

(
  cd "$out"
  sha256sum \
    "$runtime_archive" \
    "$formal_archive" \
    > "agent-workbench-$tag-checksums.txt"
  sha256sum -c "agent-workbench-$tag-checksums.txt"
)
