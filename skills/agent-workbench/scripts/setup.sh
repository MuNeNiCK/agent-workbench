#!/bin/sh
set -eu

project_root=${1:-"$(pwd)"}
local_archive=${2:-}
local_checksum=${3:-}
repository=https://github.com/MuNeNiCK/agent-workbench
skill_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
release_version=$(sed -n '1p' "$skill_dir/release-version")
if ! printf '%s\n' "$release_version" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "invalid Agent Workbench release version" >&2
  exit 1
fi

case "$(uname -s):$(uname -m)" in
  Linux:x86_64) target=linux-x86_64 ;;
  Linux:aarch64|Linux:arm64) target=linux-aarch64 ;;
  Darwin:x86_64) target=macos-x86_64 ;;
  Darwin:arm64) target=macos-aarch64 ;;
  *) echo "unsupported Agent Workbench platform" >&2; exit 1 ;;
esac

archive=agent-workbench-${target}.tar.gz
temporary=$(mktemp -d "${TMPDIR:-/tmp}/agent-workbench-setup.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

if [ -n "$local_archive" ] || [ -n "$local_checksum" ]; then
  test -n "$local_archive" && test -n "$local_checksum"
  cp "$local_archive" "$temporary/$archive"
  cp "$local_checksum" "$temporary/$archive.sha256"
else
  curl -fL --retry 3 -o "$temporary/$archive" \
    "$repository/releases/download/$release_version/$archive"
  curl -fL --retry 3 -o "$temporary/$archive.sha256" \
    "$repository/releases/download/$release_version/$archive.sha256"
  gh attestation verify "$temporary/$archive" \
    --repo MuNeNiCK/agent-workbench \
    --signer-workflow MuNeNiCK/agent-workbench/.github/workflows/release.yml \
    --deny-self-hosted-runners \
    --source-ref "refs/tags/$release_version" >/dev/null
fi

expected=$(sed 's/[[:space:]].*$//' "$temporary/$archive.sha256")
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$temporary/$archive" | sed 's/[[:space:]].*$//')
else
  actual=$(shasum -a 256 "$temporary/$archive" | sed 's/[[:space:]].*$//')
fi
test "$actual" = "$expected"

mkdir -p "$project_root/.agent-workbench/bin"
tar -xzf "$temporary/$archive" -C "$project_root/.agent-workbench/bin"
chmod +x "$project_root/.agent-workbench/bin/agent-workbench" \
  "$project_root/.agent-workbench/bin/elan"
if [ -f "$project_root/.agent-workbench/state.db" ]; then
  context_error="$temporary/context-error"
  if context=$("$project_root/.agent-workbench/bin/agent-workbench" \
      --project "$project_root" context 2>"$context_error"); then
    printf '%s\n' "$context"
  elif grep -Fq 'unsupported schema revision 1; expected 2' "$context_error"; then
    "$project_root/.agent-workbench/bin/agent-workbench" --project "$project_root" init
  else
    cat "$context_error" >&2
    exit 1
  fi
else
  "$project_root/.agent-workbench/bin/agent-workbench" --project "$project_root" init
fi
