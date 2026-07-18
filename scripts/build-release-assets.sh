#!/bin/sh
set -eu

if test "$#" -ne 2; then
  echo "usage: $0 <v-tag> <output-directory>" >&2
  exit 2
fi
tag="$1"
out="$2"
case "$tag" in v?*) ;; *) echo "tag must start with v" >&2; exit 2;; esac

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
cd "$root"
test "$(git describe --tags --exact-match HEAD)" = "$tag"
test "$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n 1)" = "${tag#v}"
test "$(sed -n '1{s/[[:space:]]//g;p;}' skills/agent-workbench/CLI_VERSION)" = "$tag"

cargo build --release --locked
epoch="$(git show -s --format=%ct HEAD)"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT HUP INT TERM
mkdir -p "$out" "$stage/binary" "$stage/skill" "$stage/docs"
cp target/release/agent-workbench "$stage/binary/agent-workbench"

skill_members="$stage/skill-members.txt"
printf '%s\n' agent-workbench/CLI_VERSION agent-workbench/LICENSE agent-workbench/SKILL.md \
  agent-workbench/agents/openai.yaml \
  agent-workbench/scripts/agent-workbench.sh > "$skill_members"
while IFS= read -r member; do mkdir -p "$stage/skill/$(dirname "$member")"; if test "$member" = agent-workbench/LICENSE; then cp LICENSE "$stage/skill/$member"; else cp "skills/$member" "$stage/skill/$member"; fi; done < "$skill_members"

docs_members="$stage/docs-members.txt"
printf '%s\n' CHANGELOG.md LICENSE README.md > "$docs_members"
while IFS= read -r member; do cp "$member" "$stage/docs/$member"; done < "$docs_members"

archive() { directory="$1"; name="$2"; shift 2; tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner -C "$directory" -cf - "$@" | gzip -n > "$out/$name"; }
archive "$stage/binary" "agent-workbench-${tag}-linux-x86_64.tar.gz" agent-workbench
archive "$stage/skill" "agent-workbench-${tag}-skill.tar.gz" -T "$skill_members"
archive "$stage/docs" "agent-workbench-${tag}-docs.tar.gz" -T "$docs_members"
binary_name="agent-workbench-${tag}-linux-x86_64.tar.gz"
skill_name="agent-workbench-${tag}-skill.tar.gz"
docs_name="agent-workbench-${tag}-docs.tar.gz"
binary_sha="$(sha256sum "$out/$binary_name" | sed 's/[[:space:]].*//')"
skill_sha="$(sha256sum "$out/$skill_name" | sed 's/[[:space:]].*//')"
docs_sha="$(sha256sum "$out/$docs_name" | sed 's/[[:space:]].*//')"
metadata="agent-workbench-${tag}-release-metadata.json"
checksums="agent-workbench-${tag}-checksums.txt"
commit="$(git rev-parse HEAD)"
peeled_commit="$(git rev-list -n 1 "$tag")"
printf '{"artifacts":{"binary":{"name":"%s","sha256":"%s"},"docs":{"name":"%s","sha256":"%s"},"skill":{"name":"%s","sha256":"%s"}},"assets":["%s","%s","%s","%s","%s"],"commit":"%s","peeled_tag_commit":"%s","schema":{"source_profiles":["877adac85029da006fd293f5f943b0191dac22f45fab2b6f596f156626bfcf76","b2b5db94248639cc319345bbacf42972884903220c3eb30b42996bf1b6bdbc35","08ce59ebc53cc6b422e01b25c173e84cccee539d23175fd72317f12f0a436166","c1ec0110b62a963f692abc1cdfe2ce2774b95fba5c795aaff79436db62d2bd9e"],"source_version":13,"target_profile":"5ea4819df0978e86402a81a94f6f61e61b2e7f9b501e052d1e4455fb934243ee","target_version":14},"tag":"%s","version":"%s"}' \
  "$binary_name" "$binary_sha" "$docs_name" "$docs_sha" "$skill_name" "$skill_sha" \
  "$binary_name" "$checksums" "$docs_name" "$metadata" "$skill_name" \
  "$commit" "$peeled_commit" "$tag" "${tag#v}" > "$out/$metadata"
(cd "$out" && sha256sum "$binary_name" "$skill_name" "$docs_name" "$metadata" > "$checksums")
(cd "$out" && sha256sum -c "$checksums")
