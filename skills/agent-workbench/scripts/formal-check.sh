#!/bin/sh
set -eu

if test "$#" -lt 2 || test "$#" -gt 4; then
  echo "usage: $0 <runtime> <assurance> [design] [selection-mode]" >&2
  exit 2
fi

runtime="$1"
assurance="$2"
design="${3:-}"
selection_mode="${4:-completion}"
case "$selection_mode" in
  completion|preview) ;;
  *) echo "agent-workbench: invalid formal selection mode" >&2; exit 2 ;;
esac
formal_plan() {
  field="$1"
  if test -n "$design"; then
    "$runtime" formal-plan "$assurance" "$design" "$selection_mode" "$field"
  else
    "$runtime" formal-plan "$assurance" "$selection_mode" "$field"
  fi
}
oracle="$(formal_plan oracle)"
modules="$(formal_plan modules)"
surfaces="$(formal_plan surfaces)"
adapter="$(formal_plan adapter)"
cases="$(formal_plan cases)"
design_statement="$(formal_plan statement)"
design_key="$(formal_plan design-key)"
design_version="$(formal_plan design-version)"
script_dir="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)"
formal_tool="$script_dir/formal-tool.sh"
tool_root="$("$formal_tool" root)"
lake="$tool_root/bin/lake"
command -v timeout >/dev/null 2>&1
command -v wc >/dev/null 2>&1

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
  "$lake" build --wfail "+$module:olean"
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
formal_meaning_available=true
max_output_bytes=1048576
max_output_blocks=2048
run_bounded() (
  ulimit -f "$max_output_blocks"
  exec "$@"
)
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
  counterexample_found=false
  adapter_execution_failed=false
  while IFS= read -r case_path; do
    test -f "$case_path"
    case_digest="$(sha256sum "$case_path" | sed -n 's/[[:space:]].*//p')"
    printf '%s=sha256:%s\n' "$case_path" "$case_digest" >> "$artifact_file"
    expected="$workspace/expected-$(basename -- "$case_path")"
    actual="$workspace/actual-$(basename -- "$case_path")"
    oracle_error="$expected.stderr"
    adapter_error="$actual.stderr"
    : > "$actual"
    : > "$adapter_error"
    set +e
    run_bounded timeout 30 "$lake" env lean --run "$oracle_path" \
      < "$case_path" > "$expected" 2>"$oracle_error"
    oracle_status=$?
    set -e
    execution_failure=""
    if test "$oracle_status" -eq 124; then
      execution_failure="Lean oracle timed out."
      formal_meaning_available=false
    elif test "$oracle_status" -ne 0 &&
        { test "$(wc -c < "$expected")" -ge "$max_output_bytes" ||
          test "$(wc -c < "$oracle_error")" -ge "$max_output_bytes"; }; then
      execution_failure="Lean oracle output exceeded $max_output_bytes bytes."
      formal_meaning_available=false
    elif test "$oracle_status" -ne 0; then
      execution_failure="Lean oracle exited with status $oracle_status."
      formal_meaning_available=false
    elif test "$(wc -c < "$expected")" -gt "$max_output_bytes"; then
      execution_failure="Lean oracle output exceeded $max_output_bytes bytes."
      formal_meaning_available=false
    elif test "$(wc -c < "$oracle_error")" -gt "$max_output_bytes"; then
      execution_failure="Lean oracle error output exceeded $max_output_bytes bytes."
      formal_meaning_available=false
    elif ! "$runtime" validate-json-file "$expected" >/dev/null 2>&1; then
      execution_failure="Lean oracle returned malformed JSON."
      formal_meaning_available=false
    fi
    if test -z "$execution_failure"; then
      set +e
      run_bounded timeout 30 "$adapter" \
        < "$case_path" > "$actual" 2>"$adapter_error"
      adapter_status=$?
      set -e
      if test "$adapter_status" -eq 124; then
        execution_failure="Product adapter timed out."
      elif test "$adapter_status" -ne 0 &&
          { test "$(wc -c < "$actual")" -ge "$max_output_bytes" ||
            test "$(wc -c < "$adapter_error")" -ge "$max_output_bytes"; }; then
        execution_failure="Product adapter output exceeded $max_output_bytes bytes."
      elif test "$adapter_status" -ne 0; then
        execution_failure="Product adapter exited with status $adapter_status."
      elif test "$(wc -c < "$actual")" -gt "$max_output_bytes"; then
        execution_failure="Product adapter output exceeded $max_output_bytes bytes."
      elif test "$(wc -c < "$adapter_error")" -gt "$max_output_bytes"; then
        execution_failure="Product adapter error output exceeded $max_output_bytes bytes."
      elif ! "$runtime" validate-json-file "$actual" >/dev/null 2>&1; then
        execution_failure="Product adapter returned malformed JSON."
      fi
    fi
    if test -n "$execution_failure"; then
      case_conformance="execution-failure"
      if test "$formal_meaning_available" = true; then
        adapter_execution_failed=true
      fi
    elif "$runtime" compare-json-files "$expected" "$actual" >/dev/null 2>&1; then
      case_conformance="pass"
    else
      case_conformance="fail"
      counterexample_found=true
    fi
    {
      printf 'Example input (%s):\n' "$case_path"
      sed 's/^/  /' "$case_path"
      printf 'Lean oracle observation:\n'
      sed 's/^/  /' "$expected"
      printf 'Product observation:\n'
      sed 's/^/  /' "$actual"
      printf 'Conformance: %s\n' "$case_conformance"
      if test -n "$execution_failure"; then
        printf 'Execution failure: %s\n' "$execution_failure"
        if test -s "$oracle_error"; then
          printf 'Lean oracle error output:\n'
          sed -n '1,20{s/^/  /;p;}' "$oracle_error"
        fi
        if test -s "$adapter_error"; then
          printf 'Product adapter error output:\n'
          sed -n '1,20{s/^/  /;p;}' "$adapter_error"
        fi
      fi
    } >> "$observation_file"
    if test "$(wc -c < "$observation_file")" -gt "$max_output_bytes"; then
      echo "agent-workbench: the aggregate formal preview exceeded its output bound" >&2
      exit 1
    fi
  done < "$case_file"
  if test "$adapter_execution_failed" = true; then
    conformance="execution-failure"
  elif test "$counterexample_found" = true; then
    conformance="fail"
  fi
elif test "$cases" != "-"; then
  echo "agent-workbench: cases require an adapter" >&2
  exit 1
else
  oracle_path="$(printf '%s' "$oracle" | tr . /).lean"
  oracle_digest="$(sha256sum "$oracle_path" | sed -n 's/[[:space:]].*//p')"
  oracle_artifact="$oracle=sha256:$oracle_digest"
  oracle_output="$workspace/oracle-output"
  oracle_error="$workspace/oracle-error"
  set +e
  run_bounded timeout 30 "$lake" env lean --run "$oracle_path" \
    >"$oracle_output" 2>"$oracle_error"
  oracle_status=$?
  set -e
  if test "$oracle_status" -eq 124; then
    echo "agent-workbench: the Lean oracle timed out" >&2
    exit 1
  elif test "$oracle_status" -ne 0 &&
      { test "$(wc -c < "$oracle_output")" -ge "$max_output_bytes" ||
        test "$(wc -c < "$oracle_error")" -ge "$max_output_bytes"; }; then
    echo "agent-workbench: the Lean oracle exceeded its output bound" >&2
    exit 1
  elif test "$oracle_status" -ne 0; then
    echo "agent-workbench: the Lean oracle exited with status $oracle_status" >&2
    sed -n '1,20p' "$oracle_error" >&2
    exit 1
  elif test "$(wc -c < "$oracle_output")" -gt "$max_output_bytes" ||
      test "$(wc -c < "$oracle_error")" -gt "$max_output_bytes"; then
    echo "agent-workbench: the Lean oracle exceeded its output bound" >&2
    exit 1
  elif test ! -s "$oracle_output"; then
    echo "agent-workbench: the Lean oracle produced no semantic observations" >&2
    exit 1
  fi
  {
    printf 'Lean oracle observations:\n'
    sed 's/^/  /' "$oracle_output"
  } >> "$observation_file"
  if test "$(wc -c < "$observation_file")" -gt "$max_output_bytes"; then
    echo "agent-workbench: the aggregate formal preview exceeded its output bound" >&2
    exit 1
  fi
fi

sort -u "$artifact_file" -o "$artifact_file"
checked_closure="$(paste -sd, "$closure_file")"
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
if test "$(wc -c < "$preview_file")" -gt "$max_output_bytes"; then
  echo "agent-workbench: the aggregate formal preview exceeded its output bound" >&2
  exit 1
fi
preview_digest="$(sha256sum "$preview_file" | sed -n 's/[[:space:]].*//p')"
preview_identity="formal-preview:sha256:$preview_digest"
printf 'Preview identity: %s\n' "$preview_identity" >> "$preview_file"
if test "$(wc -c < "$preview_file")" -gt "$max_output_bytes"; then
  echo "agent-workbench: the aggregate formal preview exceeded its output bound" >&2
  exit 1
fi
cat "$preview_file"

if test "$formal_meaning_available" != true; then
  echo "agent-workbench: the Lean oracle did not produce reviewable formal meaning" >&2
  exit 1
fi

remaining_stale_file="$workspace/remaining-stale-formal-result-identities"
"$runtime" remaining-stale-formal-identities \
  "$assurance" "$design_key" "$design_version" > "$remaining_stale_file"
AGENT_WORKBENCH_STALE_FORMAL_RESULT_IDENTITIES_FILE="$remaining_stale_file" \
  "$runtime" record-formal-result-files "$assurance" "$design_key" \
  "$design_version" \
  "$tool_identity" "$oracle_artifact" "$closure_file" "$artifact_file" \
  "$conformance" "$preview_file" "$preview_identity"
