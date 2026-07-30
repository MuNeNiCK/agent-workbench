#!/bin/sh
set -eu

if test "$#" -ne 3; then
  echo "usage: $0 <v-tag> <output-directory> <binary>" >&2
  exit 2
fi

tag="$1"
out="$2"
binary="$3"
case "$tag" in
  v?*) ;;
  *) echo "tag must start with v" >&2; exit 2 ;;
esac

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
cd "$root"
version="${tag#v}"
test "$(sed -n 's/.*version := v!\"\([^\"]*\)\".*/\1/p' lakefile.lean |
  head -n 1)" = "$version"
test "$(sed -n '1{s/[[:space:]]//g;p;}' \
  skills/agent-workbench/CLI_VERSION)" = "$tag"
test -x "$binary"
test "$("$binary" --version)" = "agent-workbench $version"
file "$binary" | grep -F "ELF 64-bit LSB"

epoch="$(git show -s --format=%ct HEAD)"
stage="$(mktemp -d)"
cleanup() {
  rm -rf "$stage"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$out" "$stage/binary"
cp "$binary" "$stage/binary/agent-workbench"
strip "$stage/binary/agent-workbench"

archive() {
  directory="$1"
  name="$2"
  shift 2
  tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    -C "$directory" -cf - "$@" | gzip -n > "$out/$name"
}

runtime_archive="agent-workbench-$tag-linux-x86_64.tar.gz"

archive "$stage/binary" "$runtime_archive" agent-workbench

mkdir -p "$stage/verify"
tar -xzf "$out/$runtime_archive" -C "$stage/verify"
test "$("$stage/verify/agent-workbench" --version)" = \
  "agent-workbench $version"
"$stage/verify/agent-workbench" --help >/dev/null

(
  cd "$out"
  sha256sum "$runtime_archive" > "agent-workbench-$tag-checksums.txt"
  sha256sum -c "agent-workbench-$tag-checksums.txt"
)
