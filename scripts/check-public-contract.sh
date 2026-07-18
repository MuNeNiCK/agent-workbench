#!/bin/sh
set -eu

root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)"
cd "$root"

version="$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n 1)"
skill_version="$(sed -n '1{s/[[:space:]]//g;p;}' skills/agent-workbench/CLI_VERSION)"
test "$skill_version" = "v$version"

git ls-files | while IFS= read -r path; do
  case "$path" in
    .agent-workbench/*|.agents/*)
      echo "private workbench material is tracked: $path" >&2
      exit 1
      ;;
  esac
done

if grep -Eq 'contents:[[:space:]]*write|workflow_dispatch:|gh release (create|upload|delete)' .github/workflows/release.yml; then
  echo "release workflow is not observer-only" >&2
  exit 1
fi

assert_commands() {
  path="$1"
  expected="$2"
  actual="$(target/debug/agent-workbench $path --help | awk '/^Commands:/{collect=1;next} collect && /^$/{exit} collect{printf "%s ",$1}' | sed 's/[[:space:]]*$//')"
  if test "$actual" != "$expected"; then
    echo "public command inventory mismatch at '$path'" >&2
    echo "expected: $expected" >&2
    echo "actual:   $actual" >&2
    exit 1
  fi
}

assert_commands "" "init update status next doctor work resume-check gate correction command rules record repository task phase design requirement design-decision gate-template trace evidence coverage review finding closure acceptance kpt decompose checklist stale review-context export help"
assert_commands doctor "integrity help"
assert_commands update "restore help"
assert_commands work "start activate block unblock suspend resume close abandon reopen follow-up dependency help"
assert_commands "work dependency" "add list satisfy accept help"
assert_commands task "add list block unblock close accept-out-of-scope help"
assert_commands phase "create list show assign dependency close-ready close accept-out-of-scope help"
assert_commands "phase dependency" "add list satisfy accept help"
assert_commands review "decide policy plan run help"
assert_commands "review policy" "add list help"
assert_commands "review plan" "add list waive help"
assert_commands "review run" "add list help"
assert_commands finding "decide add list verify remediate accept-out-of-scope help"
assert_commands closure "add ready supersede help"
assert_commands correction "add list link-requirement link-validation resolve except help"
assert_commands kpt "start close item help"
assert_commands "kpt item" "add list convert dismiss help"
assert_commands command "add prefer fix deprecate usage deviation list help"
assert_commands "command usage" "add list help"
assert_commands "command deviation" "add help"
assert_commands rules "applicable help"
assert_commands record "create list export command commit file help"
assert_commands "record command" "add help"
assert_commands "record commit" "add help"
assert_commands "record file" "add help"
assert_commands repository "add list snapshot classify commit file compare help"
assert_commands "repository snapshot" "add list finalize help"
assert_commands "repository classify" "add help"
assert_commands "repository commit" "add help"
assert_commands "repository file" "add help"
assert_commands "repository compare" "add help"
assert_commands design "init import refresh approve help"
assert_commands requirement "list help"
assert_commands design-decision "list help"
assert_commands gate-template "list help"
assert_commands trace "derive-task help"
assert_commands evidence "add list help"
assert_commands coverage "add list help"
assert_commands decompose "design help"
assert_commands checklist "list close item help"
assert_commands "checklist item" "list close help"
assert_commands stale "list accept close help"
assert_commands acceptance "add revoke help"
assert_commands gate "close-ready phase-close-ready resume-ready design-ready implementation-ready help"
assert_commands export "design plan help"

for command in \
  "update" \
  "doctor integrity" \
  "review run add" \
  "review decide" \
  "review plan waive" \
  "finding decide" \
  "finding verify" \
  "correction resolve" \
  "kpt item dismiss" \
  "repository snapshot finalize"
do
  target/debug/agent-workbench $command --help >/dev/null
done

for retired in \
  "migration" \
  "authority" \
  "verification" \
  "decision" \
  "git" \
  "doctor validation-links" \
  "authority assertion" \
  "authority provider" \
  "authority grant" \
  "owner grant" \
  "principal resolve" \
  "review provenance" \
  "review invocation" \
  "decision capability"
do
  if target/debug/agent-workbench $retired --help >/dev/null 2>&1; then
    echo "retired authority command remains public: $retired" >&2
    exit 1
  fi
done

echo "public contract: pass"
