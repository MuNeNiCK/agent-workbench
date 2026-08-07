#!/usr/bin/env bash
set -euo pipefail

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
  setup() {
    powershell -NoProfile -ExecutionPolicy Bypass -File "$installed_skill/scripts/setup.ps1" \
      -ProjectRoot "$project" -LocalArchive "$archive" -LocalChecksum "$checksum"
  }
  awb="$project/.agent-workbench/bin/agent-workbench.exe"
else
  setup() {
    sh "$installed_skill/scripts/setup.sh" "$project" "$archive" "$checksum"
  }
  awb="$project/.agent-workbench/bin/agent-workbench"
fi

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
second_setup=$(setup)
[[ "$second_setup" == "$before" ]]
after=$("$awb" --project "$project" context)
[[ "$after" == "$before" ]]

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
