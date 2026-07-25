#!/bin/sh
set -eu

if test "$#" -ne 3; then
  echo "usage: $0 <v-tag> <output-directory> <static-binary>" >&2
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
test "$(sed -n 's/.*version := v!\"\([^\"]*\)\".*/\1/p' lakefile.lean | head -n 1)" = "$version"
test "$(sed -n '1{s/[[:space:]]//g;p;}' skills/agent-workbench/CLI_VERSION)" = "$tag"

test -x "$binary"
test "$("$binary" --version)" = "agent-workbench $version"
file "$binary" | grep -F "ELF 64-bit LSB executable"
file "$binary" | grep -F "statically linked"

epoch="$(git show -s --format=%ct HEAD)"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT HUP INT TERM
mkdir -p "$out" "$stage/binary" "$stage/skill/agent-workbench" "$stage/docs"

cp "$binary" "$stage/binary/agent-workbench"
strip "$stage/binary/agent-workbench"
cp -R skills/agent-workbench/. "$stage/skill/agent-workbench/"
cp LICENSE "$stage/skill/agent-workbench/LICENSE"
cp README.md RELEASE_NOTES.md LICENSE "$stage/docs/"
cp -R docs/content "$stage/docs/content"

archive() {
  directory="$1"
  name="$2"
  shift 2
  tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    -C "$directory" -cf - "$@" | gzip -n > "$out/$name"
}

archive "$stage/binary" "agent-workbench-$tag-linux-x86_64-static.tar.gz" agent-workbench
archive "$stage/skill" "agent-workbench-$tag-skill.tar.gz" agent-workbench
archive "$stage/docs" "agent-workbench-$tag-docs.tar.gz" .

source_archive="$out/agent-workbench-$tag-source.tar.gz"
git archive --format=tar HEAD -- \
  . ':(exclude).agent-workbench' ':(exclude).agent-workbench/**' \
  | gzip -n > "$source_archive"

mkdir -p "$stage/verify"
tar -xzf "$out/agent-workbench-$tag-linux-x86_64-static.tar.gz" -C "$stage/verify"
test "$("$stage/verify/agent-workbench" --version)" = "agent-workbench $version"
"$stage/verify/agent-workbench" --help >/dev/null

printf 'version=%s\ncommit=%s\ntarget=linux-x86_64-static\nimplementation=lean\n' \
  "$tag" "$(git rev-parse HEAD)" \
  > "$out/agent-workbench-$tag-release-metadata.txt"

(
  cd "$out"
  sha256sum \
    "agent-workbench-$tag-linux-x86_64-static.tar.gz" \
    "agent-workbench-$tag-skill.tar.gz" \
    "agent-workbench-$tag-docs.tar.gz" \
    "agent-workbench-$tag-source.tar.gz" \
    "agent-workbench-$tag-release-metadata.txt" \
    > "agent-workbench-$tag-checksums.txt"
  sha256sum -c "agent-workbench-$tag-checksums.txt"
)
