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
    LICENSE-lean4 LICENSES-lean4 \
    LICENSE-elan-APACHE LICENSE-elan-MIT; do
  test -f "$staging/$license"
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
test -f "$project/.agents/skills/agent-workbench/SKILL.md"
test -f "$project/.agents/skills/agent-workbench/release-version"
test -f "$project/.agents/skills/agent-workbench/scripts/setup.sh"
if [[ -n "${AGENT_WORKBENCH_SKILL_REF:-}" ]]; then
  [[ "$(<"$project/.agents/skills/agent-workbench/release-version")" == "$AGENT_WORKBENCH_SKILL_REF" ]]
fi
if grep -R "releases/latest" "$project/.agents/skills/agent-workbench"; then
  echo "installed Skill retained a moving release route" >&2
  exit 1
fi

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
    f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
PY
fi

if [[ -f "$staging/agent-workbench.exe" ]]; then
  setup_result=$(powershell -NoProfile -ExecutionPolicy Bypass -File \
    "$project/.agents/skills/agent-workbench/scripts/setup.ps1" \
    -ProjectRoot "$project" -LocalArchive "$archive" -LocalChecksum "$checksum")
  awb="$project/.agent-workbench/bin/agent-workbench.exe"
else
  setup_result=$(sh "$project/.agents/skills/agent-workbench/scripts/setup.sh" \
    "$project" "$archive" "$checksum")
  awb="$project/.agent-workbench/bin/agent-workbench"
fi

for license in LICENSE-agent-workbench LICENSE-leansqlite LICENSE-Blake3-lean \
    LICENSE-BLAKE3-APACHE-2.0 LICENSE-BLAKE3-APACHE-2.0-LLVM LICENSE-BLAKE3-CC0-1.0 \
    LICENSE-lean4 LICENSES-lean4 \
    LICENSE-elan-APACHE LICENSE-elan-MIT; do
  test -f "$project/.agent-workbench/bin/$license"
done

invoke() { "$awb" --project "$project" "$@"; }
example() {
  invoke describe "$@" |
    python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["inputExample"]))'
}
revision() {
  invoke context | python3 -c 'import json,sys; print(json.load(sys.stdin)["stateRevision"])'
}

printf '%s\n' "$setup_result" | python3 -c '
import json,sys
x=json.load(sys.stdin); assert "context" in x and x["context"] is None
'
second_init=$(invoke init)
printf '%s\n' "$second_init" | python3 -c '
import json,sys
x=json.load(sys.stdin); assert "context" in x and x["context"] is None
'
operation_index=$(invoke describe)
[[ "$operation_index" == *'"task add"'* ]]
[[ "$operation_index" == *'"review resume"'* ]]
[[ "$operation_index" != *'entry append'* ]]
printf '%s\n' "$operation_index" | python3 -c '
import json,sys
x=json.load(sys.stdin)
assert "design propose" in x["applicableOperations"]
assert "task add" not in x["applicableOperations"]
'
invoke describe design propose | python3 -c 'import json,sys; assert json.load(sys.stdin)["applicable"] is True'
invoke describe task add | python3 -c 'import json,sys; assert json.load(sys.stdin)["applicable"] is False'
example design propose | python3 -c '
import json,sys
x=json.load(sys.stdin)
assert "id" not in x and "parent" not in x and "status" not in x
assert "createdAfterEntryOrder" not in x and "sourceDocuments" not in x
assert x["leanClaims"][0]["input"]["declaredSources"]
'
example task add | python3 -c '
import json,sys
x=json.load(sys.stdin)
for forbidden in ("order", "scope", "workId", "designRevision", "supersedes"):
    assert forbidden not in x
'

mkdir -p "$project/proof/Proof"
printf '%s\n' 'name = "proof"' 'version = "0.0.0"' 'defaultTargets = ["Proof"]' \
  '' '[[lean_lib]]' 'name = "Proof"' > "$project/proof/lakefile.toml"
printf '%s\n' 'leanprover/lean4:v4.30.0' > "$project/proof/lean-toolchain"
printf '%s\n' 'theorem supportClaim : True := by trivial' > "$project/proof/Proof/Support.lean"
printf '%s\n' 'import Proof.Support' 'theorem designClaim : True := supportClaim' > "$project/proof/Proof.lean"
printf '%s\n' 'current accepted design source' > "$project/design-source.md"

before_missing_source=$(revision)
missing_source_design=$(example design propose | python3 -c '
import json,sys
x=json.load(sys.stdin); x["sourceDocumentTargets"]=["file:does-not-exist.md"]
print(json.dumps(x))')
if printf '%s\n' "$missing_source_design" | invoke design propose >/dev/null 2>&1; then
  echo "Design proposal accepted a missing normative source" >&2
  exit 1
fi
[[ "$(revision)" == "$before_missing_source" ]]

initial_design=$(example design propose | python3 -c '
import json,sys
x=json.load(sys.stdin); x["sourceDocumentTargets"]=["file:design-source.md"]
print(json.dumps(x))' | invoke design propose)
printf '%s\n' "$initial_design" | python3 -c '
import json,sys
x=json.load(sys.stdin)
assert x["id"] == "design-1" and x["parent"] is None
assert x["sourceDocuments"][0]["target"] == "file:design-source.md"
'
printf '%s\n' '{"id":"design-1"}' | invoke design get | python3 -c '
import json,sys
x=json.load(sys.stdin); assert x["parent"] is None and x["status"] == "candidate"
'
example design accept | invoke design accept >/dev/null
start_result=$(example work start | invoke work start)
printf '%s\n' "$start_result" | python3 -c '
import json,sys
x=json.load(sys.stdin); assert x["context"]["work"]["id"] == "work-1"
'
printf '%s\n' '{"id":"work-1"}' | invoke work get | python3 -c '
import json,sys
x=json.load(sys.stdin); assert x["designRevision"] == "design-1" and x["status"] == "focused"
'
invoke describe task add | python3 -c 'import json,sys; assert json.load(sys.stdin)["applicable"] is True'
example task add | invoke task add >/dev/null

before_inapplicable=$(revision)
invoke describe design propose | python3 -c 'import json,sys; assert json.load(sys.stdin)["applicable"] is False'
if example design propose | invoke design propose >/dev/null 2>&1; then
  echo "mutation boundary committed an inapplicable Design proposal" >&2
  exit 1
fi
[[ "$(revision)" == "$before_inapplicable" ]]

profile_template=$(example profile define)
profile=$(python3 - "$project" "$profile_template" <<'PY'
import json, pathlib, sys
p=json.loads(sys.argv[2])
root=pathlib.Path(sys.argv[1])
suffix=".exe" if (root/".agent-workbench/bin/elan.exe").exists() else ""
p["command"]["executable"]=str(root/f".agent-workbench/bin/elan{suffix}")
p["command"]["arguments"]=["run", "leanprover/lean4:v4.30.0", "lean", "--version"]
p["command"]["environment"]=[["ELAN_HOME", str(root/".agent-workbench/toolchains")]]
print(json.dumps(p))
PY
)
printf '%s\n' "$profile" | invoke profile define >/dev/null
example kpt record | invoke kpt record >/dev/null

context=$(invoke context)
[[ "$context" == *'"relevantKpt":[{'* ]]
printf '%s\n' '{"workId":"work-1","resumeCondition":"continue the same route"}' \
  | invoke work suspend >/dev/null
resume_result=$(printf '%s\n' '{"id":"work-1"}' | invoke work resume)
printf '%s\n' "$resume_result" | python3 -c '
import json,sys
x=json.load(sys.stdin); assert x["context"]["work"]["id"] == "work-1"
'
handoff_result=$(printf '%s\n' \
  '{"workId":"work-1","entryId":"handoff-1","successorRun":"agent-run-2","reason":"continue the same Work across an agent run"}' \
  | invoke work handoff)
printf '%s\n' "$handoff_result" | python3 -c '
import json,sys
x=json.load(sys.stdin); assert x["context"]["work"]["id"] == "work-1"
'

before_invalid=$(revision)
if printf '%s\n' '{}' | invoke entry append >/dev/null 2>&1; then
  echo "removed generic entry append remained public" >&2
  exit 1
fi
[[ "$(revision)" == "$before_invalid" ]]
invalid_system_field=$(example task add | python3 -c '
import json,sys
x=json.load(sys.stdin); x["entryId"]="task-forbidden"; x["order"]=999
print(json.dumps(x))')
if printf '%s\n' "$invalid_system_field" | invoke task add >/dev/null 2>&1; then
  echo "semantic command accepted a request-selected ledger order" >&2
  exit 1
fi
[[ "$(revision)" == "$before_invalid" ]]
invalid_design_id=$(example design propose | python3 -c '
import json,sys
x=json.load(sys.stdin); x["id"]="design-forbidden"
print(json.dumps(x))')
if printf '%s\n' "$invalid_design_id" | invoke design propose >/dev/null 2>&1; then
  echo "Design proposal accepted a request-selected identity" >&2
  exit 1
fi
[[ "$(revision)" == "$before_invalid" ]]
invalid_nested_field=$(example design propose | python3 -c '
import json,sys
x=json.load(sys.stdin); x["statements"][0]["status"]="accepted"
print(json.dumps(x))')
if printf '%s\n' "$invalid_nested_field" | invoke design propose >/dev/null 2>&1; then
  echo "semantic command ignored an unknown nested system field" >&2
  exit 1
fi
[[ "$(revision)" == "$before_invalid" ]]

printf '%s\n' 'initial artifact' > "$project/artifact.txt"
printf '%s\n' 'initial observation target' > "$project/observed.txt"
example correction record | invoke correction record >/dev/null
[[ "$(invoke ready)" == *'"ready":false'* ]]
example artifact observe | invoke artifact observe >/dev/null
example command run | invoke command run >/dev/null
example correction resolve | invoke correction resolve >/dev/null
invoke context | python3 -c '
import json,sys
assert json.load(sys.stdin)["context"]["effectiveUserCorrections"] == []
'
mkdir -p "$project/proof/.lake/build"
printf '%s\n' 'pre-existing user build output' > "$project/proof/.lake/build/preserved"
proof_one=$(example proof run | invoke proof run)
[[ "$proof_one" == *'"kernelAccepted":true'* ]]
python3 - "$project/proof/.lake/build" <<'PY'
import pathlib, sys
root = pathlib.Path(sys.argv[1])
assert sorted(path.name for path in root.iterdir()) == ["preserved"]
assert (root / "preserved").read_text() == "pre-existing user build output\n"
PY

proof_concurrent_a=$(example proof run | python3 -c '
import json,sys
x=json.load(sys.stdin); x["entryId"]="proof-concurrent-a"; print(json.dumps(x))')
proof_concurrent_b=$(example proof run | python3 -c '
import json,sys
x=json.load(sys.stdin); x["entryId"]="proof-concurrent-b"; print(json.dumps(x))')
printf '%s\n' "$proof_concurrent_a" | invoke proof run > "$project/proof-concurrent-a.json" &
proof_concurrent_a_pid=$!
printf '%s\n' "$proof_concurrent_b" | invoke proof run > "$project/proof-concurrent-b.json" &
proof_concurrent_b_pid=$!
wait "$proof_concurrent_a_pid"
wait "$proof_concurrent_b_pid"
python3 - "$project/proof-concurrent-a.json" "$project/proof-concurrent-b.json" \
  "$project/proof/.lake/build" <<'PY'
import json, pathlib, sys
for result in sys.argv[1:3]:
    receipt = json.loads(pathlib.Path(result).read_text())
    assert receipt["entry"]["payload"]["leanProofReceipt"]["value"]["kernelAccepted"] is True
root = pathlib.Path(sys.argv[3])
assert sorted(path.name for path in root.iterdir()) == ["preserved"]
assert (root / "preserved").read_text() == "pre-existing user build output\n"
PY
rm -f "$project/proof-concurrent-a.json" "$project/proof-concurrent-b.json"

before_forged_review=$(revision)
forged_review=$(example review start | python3 -c '
import json,sys
x=json.load(sys.stdin); x["producerAgentRun"]="forged-producer"; print(json.dumps(x))')
if printf '%s\n' "$forged_review" | invoke review start >/dev/null 2>&1; then
  echo "Review accepted request-selected producer provenance" >&2
  exit 1
fi
[[ "$(revision)" == "$before_forged_review" ]]
example review start | invoke review start >/dev/null
fresh_input=$(example review context | invoke review context)
[[ "$fresh_input" == *'"lineage":[]'* ]]
example review finding | invoke review finding >/dev/null
example review disposition | invoke review disposition >/dev/null
[[ "$(invoke ready)" == *'"ready":false'* ]]

printf '%s\n' 'changed artifact' > "$project/artifact.txt"
printf '%s\n' 'changed observation target' > "$project/observed.txt"
[[ "$(invoke ready)" == *'"ready":false'* ]]
example artifact observe | python3 -c '
import json,sys
x=json.load(sys.stdin); x["entryId"]="evidence-2"; print(json.dumps(x))' \
  | invoke artifact observe >/dev/null
example command run | python3 -c '
import json,sys
x=json.load(sys.stdin); x["entryId"]="command-2"; print(json.dumps(x))' \
  | invoke command run >/dev/null
example review resume | invoke review resume >/dev/null
resume_input=$(printf '%s\n' '{"id":"review-resume"}' | invoke review context)
[[ "$resume_input" != *'"lineage":[]'* ]]
example review verify | python3 -c '
import json,sys
x=json.load(sys.stdin); x["evidenceEntryId"]="command-2"; print(json.dumps(x))' \
  | invoke review verify >/dev/null
example kpt apply | python3 -c '
import json,sys
x=json.load(sys.stdin); x["actionEntryId"]="command-2"; print(json.dumps(x))' \
  | invoke kpt apply >/dev/null
example task close | invoke task close >/dev/null
[[ "$(invoke ready)" == *'"ready":true'* ]]

printf '%s\n' 'changed without a successor Design' > "$project/design-source.md"
[[ "$(invoke ready)" == *'"ready":false'* ]]
printf '%s\n' 'current accepted design source' > "$project/design-source.md"
[[ "$(invoke ready)" == *'"ready":true'* ]]

printf '%s\n' 'theorem supportClaim : True := by exact True.intro' > "$project/proof/Proof/Support.lean"
[[ "$(invoke ready)" == *'"ready":false'* ]]
example proof run | python3 -c '
import json,sys
x=json.load(sys.stdin); x["entryId"]="proof-2"; print(json.dumps(x))' \
  | invoke proof run >/dev/null
python3 - "$project/proof/.lake/build" <<'PY'
import pathlib, sys
root = pathlib.Path(sys.argv[1])
assert sorted(path.name for path in root.iterdir()) == ["preserved"]
assert (root / "preserved").read_text() == "pre-existing user build output\n"
PY
[[ "$(invoke ready)" == *'"ready":true'* ]]
invoke work complete >/dev/null

printf '%s\n' '{"afterOrder":0,"limit":2}' | invoke history |
  python3 -c 'import json,sys; assert len(json.load(sys.stdin)) == 2'
printf '%s\n' '{"id":"task-1"}' | invoke entry get >/dev/null
