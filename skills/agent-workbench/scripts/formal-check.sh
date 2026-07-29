#!/bin/sh
set -eu

if test "$#" -ne 2; then
  echo "usage: $0 <runtime> <assurance>" >&2
  exit 2
fi

runtime="$1"
assurance="$2"
oracle="$("$runtime" formal-plan "$assurance" oracle)"
modules="$("$runtime" formal-plan "$assurance" modules)"
surfaces="$("$runtime" formal-plan "$assurance" surfaces)"
adapter="$("$runtime" formal-plan "$assurance" adapter)"
cases="$("$runtime" formal-plan "$assurance" cases)"
design_statement="$("$runtime" formal-plan "$assurance" statement)"
script_dir="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)"
formal_tool="$script_dir/formal-tool.sh"
tool_root="$("$formal_tool" root)"
lake="$tool_root/bin/lake"

workspace="$(mktemp -d)"
cleanup() {
  rm -rf "$workspace"
}
trap cleanup EXIT HUP INT TERM

module_file="$workspace/modules"
processed_file="$workspace/processed"
closure_file="$workspace/closure"
artifact_file="$workspace/artifacts"
preview_file="$workspace/semantic-preview"
observation_file="$workspace/oracle-observations"
case_file="$workspace/cases"
printf '%s\n' "$modules" | tr ',' '\n' > "$module_file"
if test "$oracle" != "-"; then
  printf '%s\n' "$oracle" >> "$module_file"
fi
sort -u "$module_file" -o "$module_file"
: > "$processed_file"
: > "$closure_file"
: > "$artifact_file"
: > "$preview_file"
: > "$observation_file"
: > "$case_file"

while :; do
  module="$(
    while IFS= read -r candidate; do
      if ! grep -Fx "$candidate" "$processed_file" >/dev/null 2>&1; then
        printf '%s\n' "$candidate"
        break
      fi
    done < "$module_file"
  )"
  test -n "$module" || break
  printf '%s\n' "$module" >> "$processed_file"
  source_path="$(printf '%s' "$module" | tr . /).lean"
  test -f "$source_path"
  "$lake" build "+$module:olean"
  printf '%s\n' "$module" >> "$closure_file"
  dependency_file="$workspace/dependencies"
  raw_dependency_file="$workspace/dependency-paths"
  "$lake" env lean --deps "$source_path" > "$raw_dependency_file"
  sed -n \
      -e 's#^.*/lib/lean/##; s#\.olean$##; p' \
      -e 's#^\.lake/build/lib/lean/##; s#\.olean$##; p' \
      "$raw_dependency_file" > "$dependency_file"
  while IFS= read -r dependency_path; do
    test -f "$dependency_path" || continue
    dependency_digest="$(sha256sum "$dependency_path" |
      sed -n 's/[[:space:]].*//p')"
    case "$dependency_path" in
      "$PWD"/.lake/packages/*)
        relative_dependency="${dependency_path#"$PWD"/}"
        printf '%s=sha256:%s\n' \
          "$relative_dependency" "$dependency_digest" >> "$artifact_file"
        ;;
      */lib/lean/*.olean)
        logical_dependency="${dependency_path##*/lib/lean/}"
        printf '@formal-tool/%s=sha256:%s\n' \
          "$logical_dependency" "$dependency_digest" >> "$artifact_file"
        ;;
    esac
  done < "$raw_dependency_file"
  while IFS= read -r dependency; do
    test -n "$dependency" || continue
    dependency_module="$(printf '%s' "$dependency" | tr / .)"
    printf '%s\n' "$dependency_module" >> "$closure_file"
    if test -f "$(printf '%s' "$dependency_module" | tr . /).lean"; then
      printf '%s\n' "$dependency_module" >> "$module_file"
    fi
  done < "$dependency_file"
  sort -u "$module_file" -o "$module_file"
  source_digest="$(sha256sum "$source_path" | sed -n 's/[[:space:]].*//p')"
  printf '%s=sha256:%s\n' "$source_path" "$source_digest" >> "$artifact_file"
  olean=".lake/build/lib/lean/$(printf '%s' "$module" | tr . /).olean"
  test -f "$olean"
  olean_digest="$(sha256sum "$olean" | sed -n 's/[[:space:]].*//p')"
  printf '%s=sha256:%s\n' "$olean" "$olean_digest" >> "$artifact_file"
done

sort -u "$closure_file" -o "$closure_file"
if test "$surfaces" != "-"; then
  printf '%s\n' "$surfaces" | tr ',' '\n' | while IFS= read -r surface; do
    test -n "$surface"
    test -e "$surface"
    surface_digest="$(sha256sum "$surface" | sed -n 's/[[:space:]].*//p')"
    printf '%s=sha256:%s\n' "$surface" "$surface_digest" >> "$artifact_file"
  done
fi

tool_identity="$("$formal_tool" identity)"
conformance="none"
oracle_artifact="-"
if test "$oracle" = "-"; then
  echo "agent-workbench: formal assurance requires a project-domain meaning oracle" >&2
  exit 1
elif test "$adapter" != "-"; then
  oracle_path="$(printf '%s' "$oracle" | tr . /).lean"
  oracle_digest="$(sha256sum "$oracle_path" | sed -n 's/[[:space:]].*//p')"
  oracle_artifact="$oracle=sha256:$oracle_digest"
  test -x "$adapter"
  test "$cases" != "-"
  adapter_digest="$(sha256sum "$adapter" | sed -n 's/[[:space:]].*//p')"
  printf '%s=sha256:%s\n' "$adapter" "$adapter_digest" >> "$artifact_file"
  printf '%s\n' "$cases" | tr ',' '\n' > "$case_file"
  conformance="pass"
  while IFS= read -r case_path; do
    test -f "$case_path"
    case_digest="$(sha256sum "$case_path" | sed -n 's/[[:space:]].*//p')"
    printf '%s=sha256:%s\n' "$case_path" "$case_digest" >> "$artifact_file"
    expected="$workspace/expected-$(basename -- "$case_path")"
    actual="$workspace/actual-$(basename -- "$case_path")"
    "$lake" env lean --run "$oracle_path" \
      < "$case_path" > "$expected"
    "$adapter" < "$case_path" > "$actual"
    if "$runtime" compare-json-files "$expected" "$actual" >/dev/null 2>&1; then
      case_conformance="pass"
    else
      case_conformance="fail"
      conformance="fail"
    fi
    {
      printf 'Example input (%s):\n' "$case_path"
      sed 's/^/  /' "$case_path"
      printf 'Lean oracle observation:\n'
      sed 's/^/  /' "$expected"
      printf 'Product observation:\n'
      sed 's/^/  /' "$actual"
      printf 'Conformance: %s\n' "$case_conformance"
    } >> "$observation_file"
  done < "$case_file"
elif test "$cases" != "-"; then
  echo "agent-workbench: cases require an adapter" >&2
  exit 1
else
  oracle_path="$(printf '%s' "$oracle" | tr . /).lean"
  oracle_digest="$(sha256sum "$oracle_path" | sed -n 's/[[:space:]].*//p')"
  oracle_artifact="$oracle=sha256:$oracle_digest"
  oracle_output="$("$lake" env lean --run "$oracle_path")"
  test -n "$oracle_output"
  {
    printf 'Lean oracle observations:\n'
    printf '%s\n' "$oracle_output" | sed 's/^/  /'
  } >> "$observation_file"
fi

sort -u "$artifact_file" -o "$artifact_file"
checked_closure="$(paste -sd, "$closure_file")"
checked_artifacts="$(paste -sd, "$artifact_file")"
{
  printf 'Design contract: %s\n' "$design_statement"
  printf 'Contract modules: %s\n' "$modules"
  cat "$observation_file"
  printf 'Checked module closure: %s\n' "$checked_closure"
  printf 'Lean tool: %s\n' "$tool_identity"
  printf 'Oracle artifact: %s\n' "$oracle_artifact"
  printf 'Product conformance: %s\n' "$conformance"
  printf 'Checked artifacts:\n'
  sed 's/^/  /' "$artifact_file"
} >> "$preview_file"
preview_digest="$(sha256sum "$preview_file" | sed -n 's/[[:space:]].*//p')"
preview_identity="formal-preview:sha256:$preview_digest"
printf 'Preview identity: %s\n' "$preview_identity" >> "$preview_file"
semantic_preview="$(cat "$preview_file")"
printf '%s\n' "$semantic_preview"

"$runtime" record-formal-result "$assurance" "$tool_identity" \
  "$oracle_artifact" "$checked_closure" "$checked_artifacts" \
  "$conformance" "$semantic_preview" "$preview_identity"
