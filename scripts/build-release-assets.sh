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
build_target="${CARGO_TARGET_DIR:-target}"
case "$build_target" in
  /*) ;;
  *) build_target="$root/$build_target" ;;
esac
test "$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n 1)" = "${tag#v}"
test "$(sed -n '1{s/[[:space:]]//g;p;}' skills/agent-workbench/CLI_VERSION)" = "$tag"

cargo build --release --locked
epoch="$(git show -s --format=%ct HEAD)"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT HUP INT TERM
mkdir -p "$out" "$stage/binary" "$stage/skill" "$stage/docs"
cp "$build_target/release/agent-workbench" "$stage/binary/agent-workbench"

skill_members="$stage/skill-members.txt"
printf '%s\n' agent-workbench/CLI_VERSION agent-workbench/LICENSE agent-workbench/SKILL.md \
  agent-workbench/agents/openai.yaml agent-workbench/references/cli-workflow.md \
  agent-workbench/references/close-ready-troubleshooting.md agent-workbench/references/interruption-recovery.md \
  agent-workbench/references/quickstart.md agent-workbench/references/repository-validation.md \
  agent-workbench/references/review-recipes.md agent-workbench/references/state-recovery.md \
  agent-workbench/scripts/agent-workbench.sh > "$skill_members"
while IFS= read -r member; do mkdir -p "$stage/skill/$(dirname "$member")"; if test "$member" = agent-workbench/LICENSE; then cp LICENSE "$stage/skill/$member"; else cp "skills/$member" "$stage/skill/$member"; fi; done < "$skill_members"

docs_members="$stage/docs-members.txt"
printf '%s\n' CHANGELOG.md LICENSE README.md mkdocs.yml requirements.txt content/agent-skills.md content/concepts.md \
  content/design-packages.md content/index.md content/operations.md content/quick-start.md \
  content/reference.md content/workflows.md > "$docs_members"
while IFS= read -r member; do mkdir -p "$stage/docs/$(dirname "$member")"; if test "$member" = LICENSE || test "$member" = CHANGELOG.md; then cp "$member" "$stage/docs/$member"; else cp "docs/$member" "$stage/docs/$member"; fi; done < "$docs_members"

archive() { directory="$1"; name="$2"; shift 2; tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner -C "$directory" -cf - "$@" | gzip -n > "$out/$name"; }
archive "$stage/binary" "agent-workbench-${tag}-linux-x86_64.tar.gz" agent-workbench
archive "$stage/skill" "agent-workbench-${tag}-skill.tar.gz" -T "$skill_members"
archive "$stage/docs" "agent-workbench-${tag}-docs.tar.gz" -T "$docs_members"

mkdir -p "$stage/verify/binary" "$stage/verify/skill" "$stage/verify/docs"
tar -xzf "$out/agent-workbench-${tag}-linux-x86_64.tar.gz" -C "$stage/verify/binary"
tar -xzf "$out/agent-workbench-${tag}-skill.tar.gz" -C "$stage/verify/skill"
tar -xzf "$out/agent-workbench-${tag}-docs.tar.gz" -C "$stage/verify/docs"
test "$(AGENT_WORKBENCH_BIN="$stage/verify/binary/agent-workbench" \
  "$stage/verify/skill/agent-workbench/scripts/agent-workbench.sh" --version)" = "agent-workbench ${tag#v}"
AGENT_WORKBENCH_UNDER_TEST="$stage/verify/binary/agent-workbench" \
  AGENT_WORKBENCH_SKILL_UNDER_TEST="$stage/verify/skill/agent-workbench" \
  AGENT_WORKBENCH_DOCS_UNDER_TEST="$stage/verify/docs" \
  cargo test --locked --test command_line 'cli::registry::'

source_archive="$out/agent-workbench-${tag}-source.tar.gz"
git archive --format=tar HEAD -- . ':(exclude).agent-workbench' ':(exclude).agent-workbench/**' | gzip -n > "$source_archive"
source_inventory="$stage/source-members.txt"
tar -tzf "$source_archive" > "$source_inventory"
if grep -Eq '^\.agent-workbench(/|$)' "$source_inventory"; then
  echo "source archive contains managed project state" >&2
  exit 1
fi
grep -Fx Cargo.toml "$source_inventory" >/dev/null
grep -Fx src/lib.rs "$source_inventory" >/dev/null
printf 'version=%s\ncommit=%s\ntarget=linux-x86_64\n' "$tag" "$(git rev-parse HEAD)" > "$out/agent-workbench-${tag}-release-metadata.txt"
(cd "$out" && sha256sum "agent-workbench-${tag}-linux-x86_64.tar.gz" "agent-workbench-${tag}-skill.tar.gz" "agent-workbench-${tag}-docs.tar.gz" "agent-workbench-${tag}-source.tar.gz" "agent-workbench-${tag}-release-metadata.txt" > "agent-workbench-${tag}-checksums.txt")
(cd "$out" && sha256sum -c "agent-workbench-${tag}-checksums.txt")
