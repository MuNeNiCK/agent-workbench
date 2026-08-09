#!/usr/bin/env bash
set -Eeuo pipefail

trap 'status=$?; printf "installed-route failed at line %s (status %s)\n" "$LINENO" "$status" >&2; exit "$status"' ERR

staging=${1:?staged release directory is required}
skill_source=${2:?distributed Skill directory is required}
provided_archive=${3:-}
provided_checksum=${4:-}
project=$(mktemp -d "${TMPDIR:-/tmp}/agent-workbench-route.XXXXXX")
package_dir=$(mktemp -d "${TMPDIR:-/tmp}/agent-workbench-package.XXXXXX")
trap 'rm -rf "$project" "$package_dir"' EXIT

for license in LICENSE-agent-workbench LICENSE-leansqlite LICENSE-Blake3-lean \
    LICENSE-BLAKE3-APACHE-2.0 LICENSE-BLAKE3-APACHE-2.0-LLVM LICENSE-BLAKE3-CC0-1.0 \
    LICENSE-lean4 LICENSES-lean4 LICENSE-elan-APACHE LICENSE-elan-MIT; do
  test -f "$staging/$license"
done
for required in SKILL.md release-version scripts/setup.sh scripts/setup.ps1; do
  test -f "$staging/skill/agent-workbench/$required"
done
diff -r "$staging/skill/agent-workbench" "$skill_source"
test -f "$staging/README.md"
for document in assurance concepts getting-started index installation operation-reference recovery \
    releases reviews state-reference workflow; do
  test -f "$staging/docs/$document.md"
done

git -C "$project" init -q
git -C "$project" config user.email fixture@example.invalid
git -C "$project" config user.name fixture
mkdir -p "$project/src"
mkdir -p "$project/runtime" "$project/dist"
printf '%s\n' 'product source independent of Workbench' > "$project/src/Product.txt"
printf '%s\n' 'product build input independent of Workbench' > "$project/product.build"
printf '%s\n' 'product persisted state independent of Workbench' > "$project/runtime/product.state"
printf '%s\n' 'product artifact independent of Workbench' > "$project/dist/product.artifact"
printf '%s\n' '#!/bin/sh' "printf '%s\\n' 'product behavior independent of Workbench'" \
  > "$project/product-command.sh"
chmod +x "$project/product-command.sh"
git -C "$project" add src/Product.txt product.build runtime/product.state \
  dist/product.artifact product-command.sh
git -C "$project" commit -qm 'product fixture before Workbench'
product_behavior_before=$(sh "$project/product-command.sh")
git_config_before=$(git -C "$project" config --local --list)

assert_product_invariant() {
  git -C "$project" diff --quiet HEAD -- src/Product.txt product.build runtime/product.state \
    dist/product.artifact product-command.sh
  git -C "$project" diff --cached --quiet
  [[ "$(sh "$project/product-command.sh")" == "$product_behavior_before" ]]
  [[ "$(git -C "$project" config --local --list)" == "$git_config_before" ]]
}

runtime_snapshot() {
  python3 - "$project/.agent-workbench/bin" <<'PY'
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import stat
import sys

root = Path(sys.argv[1])
snapshot = []
for path in sorted([root, *root.rglob("*")], key=lambda value: value.as_posix()):
    metadata = path.lstat()
    relative = "." if path == root else path.relative_to(root).as_posix()
    entry = {
        "path": relative,
        "mode": stat.S_IMODE(metadata.st_mode),
        "mtime_ns": metadata.st_mtime_ns,
        "type": "directory" if path.is_dir() else "file",
    }
    if path.is_file():
        entry["digest"] = hashlib.sha256(path.read_bytes()).hexdigest()
    snapshot.append(entry)
print(json.dumps(snapshot, sort_keys=True, separators=(",", ":")))
PY
}
if [[ -n "${AGENT_WORKBENCH_SKILL_REPOSITORY:-}" ]]; then
  [[ -n "${AGENT_WORKBENCH_SKILL_REF:-}" ]]
  (cd "$project" && gh skill install "$AGENT_WORKBENCH_SKILL_REPOSITORY" agent-workbench \
    --agent codex --scope project --pin "$AGENT_WORKBENCH_SKILL_REF")
else
  skill_repository=$(cd "$skill_source/../.." && pwd)
  (cd "$project" && gh skill install "$skill_repository" agent-workbench \
    --from-local --agent codex --scope project)
fi
installed_skill="$project/.agents/skills/agent-workbench"
for required in SKILL.md release-version scripts/setup.sh scripts/setup.ps1; do
  test -f "$installed_skill/$required"
done
if [[ -n "${AGENT_WORKBENCH_SKILL_REF:-}" ]]; then
  [[ "$(<"$installed_skill/release-version")" == "$AGENT_WORKBENCH_SKILL_REF" ]]
fi
! grep -R "releases/latest" "$installed_skill"

if [[ -n "$provided_archive" || -n "$provided_checksum" ]]; then
  [[ -n "$provided_archive" && -n "$provided_checksum" ]]
  archive=$provided_archive
  checksum=$provided_checksum
elif [[ -f "$staging/agent-workbench.exe" ]]; then
  archive="$package_dir/agent-workbench-windows-x86_64.zip"
  checksum="$archive.sha256"
  (cd "$staging" && 7z a -tzip "$archive" . >/dev/null)
else
  archive="$package_dir/agent-workbench-local.tar.gz"
  checksum="$archive.sha256"
  tar -czf "$archive" -C "$staging" .
fi
if [[ -z "$provided_archive" ]]; then
  python3 - "$archive" <<'PY'
import hashlib, pathlib, sys
path = pathlib.Path(sys.argv[1])
path.with_name(path.name + ".sha256").write_text(
    f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n",
    encoding="utf-8", newline="\n")
PY
fi

if [[ -f "$staging/agent-workbench.exe" ]]; then
  setup_archive_for() {
    powershell -NoProfile -ExecutionPolicy Bypass -File "$installed_skill/scripts/setup.ps1" \
      -ProjectRoot "$1" -LocalArchive "$2" -LocalChecksum "$3"
  }
  awb="$project/.agent-workbench/bin/agent-workbench.exe"
else
  setup_archive_for() {
    sh "$installed_skill/scripts/setup.sh" "$1" "$2" "$3"
  }
  awb="$project/.agent-workbench/bin/agent-workbench"
fi
setup_archive() {
  setup_archive_for "$project" "$1" "$2"
}
setup() {
  setup_archive "$archive" "$checksum"
}

first_setup=$(setup)
for license in LICENSE-agent-workbench LICENSE-leansqlite LICENSE-Blake3-lean \
    LICENSE-BLAKE3-APACHE-2.0 LICENSE-BLAKE3-APACHE-2.0-LLVM LICENSE-BLAKE3-CC0-1.0 \
    LICENSE-lean4 LICENSES-lean4 LICENSE-elan-APACHE LICENSE-elan-MIT; do
  test -f "$project/.agent-workbench/bin/$license"
done
for required in SKILL.md release-version scripts/setup.sh scripts/setup.ps1; do
  test -f "$project/.agent-workbench/bin/skill/agent-workbench/$required"
done
test -f "$project/.agent-workbench/bin/README.md"
test -f "$project/.agent-workbench/bin/docs/getting-started.md"
test -x "$awb" || [[ "$awb" == *.exe ]]

printf '%s\n' "$first_setup" | python3 -c '
import json,sys
value=json.load(sys.stdin)
assert value["stateRevision"] == 1
assert value["acceptedDesignId"] is None
assert value["focusedWorkId"] is None
'
for directory in product implementation plans proofs; do
  test -d "$project/.agent-workbench/design/$directory"
done

before=$("$awb" --project "$project" context)
runtime_before_same_version=$(runtime_snapshot)
second_setup=$(setup)
[[ "$second_setup" == "$before" ]]
after=$("$awb" --project "$project" context)
[[ "$after" == "$before" ]]
[[ "$(runtime_snapshot)" == "$runtime_before_same_version" ]]
runtime_release="$project/.agent-workbench/bin/skill/agent-workbench/release-version"
expected_release=$(sed -n '1p' "$installed_skill/release-version")

incomplete_stage="$package_dir/incomplete-stage"
mkdir "$incomplete_stage"
cp -a "$staging/." "$incomplete_stage/"
rm "$incomplete_stage/README.md"
if [[ -f "$staging/agent-workbench.exe" ]]; then
  incomplete_archive="$package_dir/incomplete-agent-workbench.zip"
  (cd "$incomplete_stage" && 7z a -tzip "$incomplete_archive" . >/dev/null)
else
  incomplete_archive="$package_dir/incomplete-agent-workbench.tar.gz"
  tar -czf "$incomplete_archive" -C "$incomplete_stage" .
fi
incomplete_checksum="$incomplete_archive.sha256"
python3 - "$incomplete_archive" <<'PY'
import hashlib, pathlib, sys
path = pathlib.Path(sys.argv[1])
path.with_name(path.name + ".sha256").write_text(
    f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n",
    encoding="utf-8", newline="\n")
PY
printf '%s\n' 'v0.2.8' > "$runtime_release"
runtime_before_incomplete=$(runtime_snapshot)
if setup_archive "$incomplete_archive" "$incomplete_checksum" >/dev/null 2>&1; then
  echo "an incomplete runtime bundle unexpectedly replaced the installed runtime" >&2
  exit 1
fi
[[ "$(runtime_snapshot)" == "$runtime_before_incomplete" ]]

activation_failure_stage="$package_dir/activation-failure-stage"
mkdir "$activation_failure_stage"
cp -a "$staging/." "$activation_failure_stage/"
if [[ -f "$staging/agent-workbench.exe" ]]; then
  printf '%s\n' 'invalid executable used to inject activation failure' > \
    "$activation_failure_stage/agent-workbench.exe"
  activation_failure_archive="$package_dir/activation-failure-agent-workbench.zip"
  (cd "$activation_failure_stage" && 7z a -tzip "$activation_failure_archive" . >/dev/null)
else
  printf '%s\n' '#!/bin/sh' 'echo "injected runtime activation failure" >&2' 'exit 71' > \
    "$activation_failure_stage/agent-workbench"
  activation_failure_archive="$package_dir/activation-failure-agent-workbench.tar.gz"
  tar -czf "$activation_failure_archive" -C "$activation_failure_stage" .
fi
activation_failure_checksum="$activation_failure_archive.sha256"
python3 - "$activation_failure_archive" <<'PY'
import hashlib, pathlib, sys
path = pathlib.Path(sys.argv[1])
path.with_name(path.name + ".sha256").write_text(
    f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n",
    encoding="utf-8", newline="\n")
PY

# A structurally complete replacement is not current until its native context activation succeeds.
# Failure restores the exact old bundle and leaves setup able to retry the pinned archive.
runtime_before_context_activation_failure=$(runtime_snapshot)
if setup_archive "$activation_failure_archive" "$activation_failure_checksum" \
    >/dev/null 2>&1; then
  echo "a context-failing runtime bundle unexpectedly remained installed" >&2
  exit 1
fi
[[ "$(runtime_snapshot)" == "$runtime_before_context_activation_failure" ]]
[[ ! -e "$project/.agent-workbench/.bin.activation-pending" ]]
[[ ! -e "$project/.agent-workbench/.bin.activation-committed" ]]
[[ ! -e "$project/.agent-workbench/.bin.previous" ]]
[[ ! -e "$project/.agent-workbench/.bin.next" ]]
[[ "$("$awb" --project "$project" context)" == "$before" ]]
assert_product_invariant
context_activation_retry=$(setup)
[[ "$context_activation_retry" == "$before" ]]
[[ "$(sed -n '1p' "$runtime_release")" == "$expected_release" ]]

# Exercise the same rollback boundary for first-use init, where no prior runtime exists. The failed
# candidate must disappear completely so the exact pinned archive can initialize on the next try.
fresh_activation_project="$package_dir/fresh-activation-project"
mkdir "$fresh_activation_project"
printf '%s\n' 'fresh product content independent of Workbench' > \
  "$fresh_activation_project/product.txt"
fresh_product_before=$(cat "$fresh_activation_project/product.txt")
if setup_archive_for "$fresh_activation_project" \
    "$activation_failure_archive" "$activation_failure_checksum" \
    >/dev/null 2>&1; then
  echo "an init-failing runtime bundle unexpectedly remained installed" >&2
  exit 1
fi
[[ ! -e "$fresh_activation_project/.agent-workbench/bin" ]]
[[ ! -e "$fresh_activation_project/.agent-workbench/.bin.activation-pending" ]]
[[ ! -e "$fresh_activation_project/.agent-workbench/.bin.activation-committed" ]]
[[ ! -e "$fresh_activation_project/.agent-workbench/.bin.previous" ]]
[[ ! -e "$fresh_activation_project/.agent-workbench/.bin.next" ]]
[[ "$(cat "$fresh_activation_project/product.txt")" == "$fresh_product_before" ]]
init_activation_retry=$(setup_archive_for "$fresh_activation_project" "$archive" "$checksum")
printf '%s\n' "$init_activation_retry" | python3 -c '
import json,sys
value=json.load(sys.stdin)
assert value["stateRevision"] == 1
assert value["acceptedDesignId"] is None
assert value["focusedWorkId"] is None
'
[[ "$(cat "$fresh_activation_project/product.txt")" == "$fresh_product_before" ]]

# A migrated database and its compatible runtime commit together. Interrupt setup after native init
# returns but before swap cleanup, then prove that the public runtime path still names the new
# bundle and that retry only finishes cleanup. This fixture selects setup.sh on POSIX and setup.ps1
# on Windows CI.
migration_activation_project="$package_dir/migration-activation-project"
mkdir -p "$migration_activation_project/.agent-workbench"
printf '%s\n' 'migration product content independent of Workbench' > \
  "$migration_activation_project/product.txt"
migration_product_before=$(cat "$migration_activation_project/product.txt")
python3 - "$migration_activation_project/.agent-workbench/state.db" <<'PY'
import sqlite3
import sys

database = sqlite3.connect(sys.argv[1])
database.executescript("""
PRAGMA foreign_keys = ON;
CREATE TABLE project_metadata(
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  schema_revision INTEGER NOT NULL,
  state_revision INTEGER NOT NULL,
  accepted_design_id TEXT,
  focused_work_id TEXT
) STRICT;
CREATE TABLE design_revisions(id TEXT PRIMARY KEY, document TEXT NOT NULL) STRICT;
CREATE TABLE works(
  id TEXT PRIMARY KEY,
  design_revision TEXT NOT NULL,
  status TEXT NOT NULL,
  scope TEXT NOT NULL,
  document TEXT NOT NULL
) STRICT;
CREATE INDEX works_by_design ON works(design_revision);
CREATE INDEX works_by_scope_status ON works(scope, status);
CREATE TABLE ledger_entries(
  id TEXT PRIMARY KEY,
  entry_order INTEGER NOT NULL UNIQUE,
  scope TEXT NOT NULL,
  work_id TEXT,
  design_revision TEXT,
  payload_kind TEXT NOT NULL,
  document TEXT NOT NULL
) STRICT;
CREATE INDEX ledger_by_context
  ON ledger_entries(scope, work_id, design_revision, entry_order);
CREATE INDEX ledger_by_kind ON ledger_entries(payload_kind, entry_order);
INSERT INTO project_metadata VALUES (1, 1, 7, NULL, NULL);
""")
database.close()
PY
cp -a "$project/.agent-workbench/bin" "$migration_activation_project/.agent-workbench/bin"
migration_release="$migration_activation_project/.agent-workbench/bin/skill/agent-workbench/release-version"
printf '%s\n' 'v0.2.8' > "$migration_release"
printf '%s\n' 'old runtime must remain quarantined after migration' > \
  "$migration_activation_project/.agent-workbench/bin/old-runtime-sentinel"
if AGENT_WORKBENCH_SETUP_FAULT_POINT=after-native-activation \
    setup_archive_for "$migration_activation_project" "$archive" "$checksum" \
    >/dev/null 2>&1; then
  echo "post-migration activation fault did not interrupt setup" >&2
  exit 1
fi
[[ -e "$migration_activation_project/.agent-workbench/.bin.activation-pending" ]]
[[ ! -e "$migration_activation_project/.agent-workbench/.bin.activation-committed" ]]
[[ -e "$migration_activation_project/.agent-workbench/.bin.previous" ]]
[[ -e "$migration_activation_project/.agent-workbench/.bin.previous/old-runtime-sentinel" ]]
[[ "$(sed -n '1p' "$migration_release")" == "$expected_release" ]]
[[ ! -e "$migration_activation_project/.agent-workbench/bin/old-runtime-sentinel" ]]
migration_awb="$migration_activation_project/.agent-workbench/bin/agent-workbench"
if [[ -f "$staging/agent-workbench.exe" ]]; then
  migration_awb="$migration_activation_project/.agent-workbench/bin/agent-workbench.exe"
fi
"$migration_awb" --project "$migration_activation_project" context | python3 -c '
import json,sys
value=json.load(sys.stdin)
assert value["stateRevision"] == 8
'
[[ "$(python3 - "$migration_activation_project/.agent-workbench/state.db" <<'PY'
import sqlite3
import sys
database = sqlite3.connect(sys.argv[1])
print(database.execute(
    "SELECT schema_revision FROM project_metadata WHERE singleton = 1").fetchone()[0])
database.close()
PY
)" == "2" ]]
migration_activation_retry=$(setup_archive_for \
  "$migration_activation_project" "$archive" "$checksum")
printf '%s\n' "$migration_activation_retry" | python3 -c '
import json,sys
value=json.load(sys.stdin)
assert value["stateRevision"] == 8
'
[[ ! -e "$migration_activation_project/.agent-workbench/.bin.activation-pending" ]]
[[ ! -e "$migration_activation_project/.agent-workbench/.bin.activation-committed" ]]
[[ ! -e "$migration_activation_project/.agent-workbench/.bin.previous" ]]
[[ ! -e "$migration_activation_project/.agent-workbench/.bin.next" ]]
[[ "$(cat "$migration_activation_project/product.txt")" == "$migration_product_before" ]]

mkdir "$project/.agent-workbench/bin/obsolete-runtime-directory"
printf '%s\n' 'must not survive a bundle replacement' > \
  "$project/.agent-workbench/bin/obsolete-runtime-directory/stale-file"
printf '%s\n' 'v0.2.8' > "$runtime_release"
printf '%s\n' 'stale runtime fixture' > "$awb"
upgrade_setup=$(setup)
[[ "$upgrade_setup" == "$before" ]]
[[ "$(sed -n '1p' "$runtime_release")" == "$expected_release" ]]
[[ ! -e "$project/.agent-workbench/bin/obsolete-runtime-directory" ]]
if [[ "$awb" == *.exe ]]; then
  cmp "$awb" "$staging/agent-workbench.exe"
else
  cmp "$awb" "$staging/agent-workbench"
fi

# Model an interruption before native activation commits. The swapped candidate cannot load current
# state, so the pending marker makes the next setup restore the prior runtime and retry the full
# replacement. This route runs through setup.sh on POSIX and setup.ps1 on Windows CI.
printf '%s\n' 'v0.2.8' > "$runtime_release"
printf '%s\n' 'obsolete after interrupted replacement' > \
  "$project/.agent-workbench/bin/stale-after-interruption"
mv "$project/.agent-workbench/bin" "$project/.agent-workbench/.bin.previous"
cp -a "$activation_failure_stage/." "$project/.agent-workbench/bin/"
touch "$project/.agent-workbench/.bin.activation-pending"
retry_setup=$(setup)
[[ "$retry_setup" == "$before" ]]
[[ ! -e "$project/.agent-workbench/.bin.previous" ]]
[[ ! -e "$project/.agent-workbench/.bin.next" ]]
[[ ! -e "$project/.agent-workbench/.bin.activation-pending" ]]
[[ ! -e "$project/.agent-workbench/.bin.activation-committed" ]]
[[ ! -e "$project/.agent-workbench/bin/stale-after-interruption" ]]
[[ "$(sed -n '1p' "$runtime_release")" == "$expected_release" ]]
if [[ "$awb" == *.exe ]]; then
  cmp "$awb" "$staging/agent-workbench.exe"
else
  cmp "$awb" "$staging/agent-workbench"
fi

runtime_before_final_same_version=$(runtime_snapshot)
same_version_setup=$(setup)
[[ "$same_version_setup" == "$before" ]]
[[ "$(runtime_snapshot)" == "$runtime_before_final_same_version" ]]
outside_workbench_status=$(git -C "$project" status --porcelain --untracked-files=all -- . \
  ':(exclude).agents/**' ':(exclude).agent-workbench/**')
[[ -z "$outside_workbench_status" ]]
assert_product_invariant

"$awb" --project "$project" describe | python3 -c '
import json,sys
value=json.load(sys.stdin)
operations=value["operations"]
assert "work start" in operations
assert "design inspect-sources" in operations
assert "plan propose" in operations
assert "task close" in operations, {"check":"operation-index","operations":operations}
assert "task add" not in operations
assert "formal-check" not in operations
'
printf '%s\n' '{"id":"work-route","outcome":"verify installed release route","scope":"project","responsibleAgentRun":"release-route"}' \
  | "$awb" --project "$project" work start >/dev/null
"$awb" --project "$project" context | python3 -c '
import json,sys
value=json.load(sys.stdin)
assert value["stateRevision"] == 2
assert value["context"]["focused"] is None
assert [work["id"] for work in value["context"]["openWorks"]] == ["work-route"]
'

python3 - "$project" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1]) / ".agent-workbench" / "design"
(root / "product").mkdir(parents=True, exist_ok=True)
(root / "proofs" / "example" / "ExampleDesign").mkdir(parents=True, exist_ok=True)
(root / "product" / "design.md").write_text(
    "The selected property is true.\n", encoding="utf-8", newline="\n")
(root / "proofs" / "example" / "lean-toolchain").write_text(
    "leanprover/lean4:v4.32.2\n", encoding="utf-8", newline="\n")
(root / "proofs" / "example" / "lakefile.lean").write_text(
    "import Lake\nopen Lake DSL\npackage «installed-route-proof»\n"
    "@[default_target] lean_lib ExampleDesign\n",
    encoding="utf-8", newline="\n")
(root / "proofs" / "example" / "ExampleDesign.lean").write_text(
    "import ExampleDesign.Base\nnamespace ExampleDesign\n"
    "def Property : Prop := Base\ntheorem property : Property := by trivial\nend ExampleDesign\n",
    encoding="utf-8", newline="\n")
(root / "proofs" / "example" / "ExampleDesign" / "Base.lean").write_text(
    "namespace ExampleDesign\ndef Base : Prop := True\nend ExampleDesign\n",
    encoding="utf-8", newline="\n")
PY

inspection=$(printf '%s\n' \
  '{"sourceDocumentTargets":["file:.agent-workbench/design/product/design.md"]}' \
  | "$awb" --project "$project" design inspect-sources)
proposal=$(printf '%s\n' "$inspection" | python3 -c '
import json,sys
inspection=json.load(sys.stdin)
units=[unit for source in inspection for unit in source["units"]]
statement="The selected property is true."
value={
  "producerAgentRun":"release-route",
  "changeRationale":"verify immutable Design Claim installation route",
  "changeBasisEntryIds":[],
  "amendsCandidate":None,
  "sourceDocumentTargets":["file:.agent-workbench/design/product/design.md"],
  "sourceUnitDispositions":[{"unitId":unit["id"],"role":"requirement","reason":None} for unit in units],
  "statements":[{"id":"statement-route","text":statement,"assumptions":[]}],
  "statementCoverage":[{
    "statementId":"statement-route",
    "sourceUnitIds":[unit["id"] for unit in units],
    "leanClaims":{"selectedIds":["claim-route"],"noSelectionReason":None},
    "acceptanceCriteria":{"selectedIds":[],"noSelectionReason":"the route has no external criterion"},
    "implementationRequired":False,
    "noImplementationReason":"the installed route verifies the Design Claim itself"
  }],
  "assumptions":[],
  "removedStatements":[],
  "acceptanceCriteria":[],
  "leanClaims":[{
    "id":"claim-route",
    "elaboratedPropositionDigest":"",
    "propositionDependencies":[],
    "input":{
      "statementId":"statement-route","statementText":statement,
      "mapping":"ExampleDesign.Property is the selected Design property",
      "proposition":"ExampleDesign.Property","witness":"ExampleDesign.property",
      "assumptions":[],"proofRoot":".agent-workbench/design/proofs/example",
      "declaredSources":[{"path":"ExampleDesign.lean","expectedDigest":None},
                         {"path":"ExampleDesign/Base.lean","expectedDigest":None}],
      "check":{"executable":"lake","arguments":["build"],"workingDirectory":None,"environment":[]},
      "toolchain":"leanprover/lean4:v4.32.2"
    }
  }]
}
json.dump(value,sys.stdout,separators=(",",":"))
')
design=$(printf '%s\n' "$proposal" | "$awb" --project "$project" design propose)
design_id=$(printf '%s\n' "$design" | python3 -c '
import json,sys
value=json.load(sys.stdin)
assert value["leanClaims"][0]["elaboratedPropositionDigest"].startswith("blake3:")
assert len(value["leanClaims"][0]["propositionDependencies"]) > 0
assert sorted(source["mediaKind"] for source in value["sourceDocuments"]) == ["lean","lean","markdown"]
print(value["id"])
')
printf '{"designId":"%s","target":"file:.agent-workbench/design/proofs/example/ExampleDesign.lean"}\n' \
  "$design_id" | "$awb" --project "$project" design source | python3 -c '
import json,sys
value=json.load(sys.stdin)
expected=("import ExampleDesign.Base\nnamespace ExampleDesign\n"
          "def Property : Prop := Base\ntheorem property : Property := by trivial\nend ExampleDesign\n").encode()
assert value["mediaKind"] == "lean"
assert bytes(value["contentBytes"]) == expected, {
    "check":"archived-source-bytes","actual":bytes(value["contentBytes"]),"expected":expected}
'
printf '{"id":"%s"}\n' "$design_id" | "$awb" --project "$project" design accept >/dev/null
proof=$(printf '%s\n' '{"entryId":"proof-route","claimId":"claim-route"}' \
  | "$awb" --project "$project" proof run)
printf '%s\n' "$proof" | python3 -c '
import json,sys
value=json.load(sys.stdin)
receipt=value["entry"]["payload"]["leanProofReceipt"]["value"]
assert receipt["kernelAccepted"] is True
assert receipt["elaboratedPropositionDigest"].startswith("blake3:")
assert receipt["propositionDependencies"], {"check":"proof-dependencies","receipt":receipt}
assert receipt["assumptionDependencies"] == []
'
before_stale=$("$awb" --project "$project" context)
printf '%s\n' '-- stale source edit' >> \
  "$project/.agent-workbench/design/proofs/example/ExampleDesign.lean"
if printf '%s\n' '{"entryId":"proof-stale","claimId":"claim-route"}' \
    | "$awb" --project "$project" proof run >/dev/null 2>&1; then
  echo "stale installed Claim unexpectedly produced a receipt" >&2
  exit 1
fi
after_stale=$("$awb" --project "$project" context)
printf '%s\n%s\n' "$before_stale" "$after_stale" | python3 -c '
import json,sys
before=json.loads(sys.stdin.readline())
after=json.loads(sys.stdin.readline())
assert after["stateRevision"] == before["stateRevision"]
assert before["context"]["focused"]["claimGaps"] == []
assert after["context"]["focused"]["claimGaps"] == [
    {"claimId":"claim-route","kind":"missingInputDigest"}], {
        "check":"stale-claim-gap","before":before,"after":after}
'
printf '%s\n' '{"afterOrder":0,"limit":100}' \
  | "$awb" --project "$project" history | python3 -c '
import json,sys
assert all(entry["id"] != "proof-stale" for entry in json.load(sys.stdin))
'

assert_product_invariant
rm -rf "$project/.agents" "$project/.agent-workbench"
[[ ! -e "$project/.agents" && ! -e "$project/.agent-workbench" ]]
assert_product_invariant
[[ -z "$(git -C "$project" status --porcelain --untracked-files=all)" ]]
