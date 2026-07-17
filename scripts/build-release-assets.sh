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
  agent-workbench/agents/openai.yaml agent-workbench/references/cli-workflow.md \
  agent-workbench/references/close-ready-troubleshooting.md agent-workbench/references/interruption-recovery.md \
  agent-workbench/references/quickstart.md agent-workbench/references/repository-validation.md \
  agent-workbench/references/review-recipes.md agent-workbench/references/state-recovery.md \
  agent-workbench/scripts/agent-workbench.sh > "$skill_members"
while IFS= read -r member; do mkdir -p "$stage/skill/$(dirname "$member")"; if test "$member" = agent-workbench/LICENSE; then cp LICENSE "$stage/skill/$member"; else cp "skills/$member" "$stage/skill/$member"; fi; done < "$skill_members"

docs_members="$stage/docs-members.txt"
printf '%s\n' LICENSE README.md mkdocs.yml requirements.txt content/agent-skills.md content/concepts.md \
  content/design-packages.md content/index.md content/operations.md content/quick-start.md \
  content/reference.md content/workflows.md > "$docs_members"
while IFS= read -r member; do mkdir -p "$stage/docs/$(dirname "$member")"; if test "$member" = LICENSE; then cp LICENSE "$stage/docs/$member"; else cp "docs/$member" "$stage/docs/$member"; fi; done < "$docs_members"

archive() { directory="$1"; name="$2"; shift 2; tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner -C "$directory" -cf - "$@" | gzip -n > "$out/$name"; }
archive "$stage/binary" "agent-workbench-${tag}-linux-x86_64.tar.gz" agent-workbench
archive "$stage/skill" "agent-workbench-${tag}-skill.tar.gz" -T "$skill_members"
archive "$stage/docs" "agent-workbench-${tag}-docs.tar.gz" -T "$docs_members"
printf 'version=%s\ncommit=%s\ntarget=linux-x86_64\n' "$tag" "$(git rev-parse HEAD)" > "$out/agent-workbench-${tag}-release-metadata.txt"
(cd "$out" && sha256sum "agent-workbench-${tag}-linux-x86_64.tar.gz" "agent-workbench-${tag}-skill.tar.gz" "agent-workbench-${tag}-docs.tar.gz" "agent-workbench-${tag}-release-metadata.txt" > "agent-workbench-${tag}-checksums.txt")
(cd "$out" && sha256sum -c "agent-workbench-${tag}-checksums.txt")
