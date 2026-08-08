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
runtime_parent="$project_root/.agent-workbench"
destination="$runtime_parent/bin"
candidate="$runtime_parent/.bin.next"
previous="$runtime_parent/.bin.previous"
runtime="$destination/agent-workbench"
temporary=
context_error=

required_files() {
  cat <<'EOF'
./LICENSE-BLAKE3-APACHE-2.0
./LICENSE-BLAKE3-APACHE-2.0-LLVM
./LICENSE-BLAKE3-CC0-1.0
./LICENSE-Blake3-lean
./LICENSE-agent-workbench
./LICENSE-elan-APACHE
./LICENSE-elan-MIT
./LICENSE-lean4
./LICENSE-leansqlite
./LICENSES-lean4
./README.md
./agent-workbench
./docs/assurance.md
./docs/concepts.md
./docs/getting-started.md
./docs/index.md
./docs/installation.md
./docs/operation-reference.md
./docs/recovery.md
./docs/releases.md
./docs/reviews.md
./docs/state-reference.md
./docs/workflow.md
./elan
./skill/agent-workbench/SKILL.md
./skill/agent-workbench/agents/openai.yaml
./skill/agent-workbench/release-version
./skill/agent-workbench/scripts/setup.ps1
./skill/agent-workbench/scripts/setup.sh
EOF
}

required_directories() {
  cat <<'EOF'
.
./docs
./skill
./skill/agent-workbench
./skill/agent-workbench/agents
./skill/agent-workbench/scripts
EOF
}

runtime_complete() {
  bundle_root=$1
  [ -d "$bundle_root" ] || return 1
  [ -x "$bundle_root/agent-workbench" ] || return 1
  [ -x "$bundle_root/elan" ] || return 1
  [ -f "$bundle_root/skill/agent-workbench/release-version" ] || return 1
  [ "$(cat "$bundle_root/skill/agent-workbench/release-version")" = "$release_version" ] ||
    return 1
  actual_files=$(CDPATH='' cd -- "$bundle_root" &&
    find . -type f -print | LC_ALL=C sort)
  [ "$actual_files" = "$(required_files)" ] || return 1
  actual_directories=$(CDPATH='' cd -- "$bundle_root" &&
    find . -type d -print | LC_ALL=C sort)
  [ "$actual_directories" = "$(required_directories)" ] || return 1
  ! (CDPATH='' cd -- "$bundle_root" &&
    find . ! -type f ! -type d -print | grep -q .)
}

path_exists() {
  [ -e "$1" ] || [ -L "$1" ]
}

recover_runtime_swap() {
  if path_exists "$previous"; then
    if path_exists "$destination" && runtime_complete "$destination"; then
      rm -rf "$previous"
    else
      rm -rf "$destination"
      mv "$previous" "$destination"
    fi
  fi
  if path_exists "$candidate"; then
    rm -rf "$candidate"
  fi
}

cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  if ! path_exists "$destination" && path_exists "$previous"; then
    mv "$previous" "$destination" || true
  fi
  if path_exists "$candidate"; then
    rm -rf "$candidate"
  fi
  if path_exists "$destination" && path_exists "$previous"; then
    rm -rf "$previous"
  fi
  if [ -n "$temporary" ] && [ -d "$temporary" ]; then
    rm -rf "$temporary"
  fi
  if [ -n "$context_error" ] && [ -f "$context_error" ]; then
    rm -f "$context_error"
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

if path_exists "$previous" || path_exists "$candidate"; then
  mkdir -p "$runtime_parent"
  recover_runtime_swap
fi

needs_install=true
if runtime_complete "$destination"; then
  needs_install=false
fi

if [ "$needs_install" = true ]; then
  temporary=$(mktemp -d "${TMPDIR:-/tmp}/agent-workbench-setup.XXXXXX")
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

  mkdir -p "$runtime_parent"
  rm -rf "$candidate"
  mkdir "$candidate"
  tar -xzf "$temporary/$archive" -C "$candidate"
  chmod +x "$candidate/agent-workbench" "$candidate/elan"
  if ! runtime_complete "$candidate"; then
    echo "downloaded Agent Workbench archive is not a complete release bundle" >&2
    exit 1
  fi

  rm -rf "$previous"
  if path_exists "$destination"; then
    mv "$destination" "$previous"
  fi
  if ! mv "$candidate" "$destination"; then
    if ! path_exists "$destination" && path_exists "$previous"; then
      mv "$previous" "$destination"
    fi
    echo "failed to replace the Agent Workbench runtime bundle" >&2
    exit 1
  fi
  rm -rf "$previous"
fi
if [ -f "$project_root/.agent-workbench/state.db" ]; then
  context_error=$(mktemp "${TMPDIR:-/tmp}/agent-workbench-context.XXXXXX")
  if context=$("$runtime" \
      --project "$project_root" context 2>"$context_error"); then
    printf '%s\n' "$context"
  elif grep -Fq 'unsupported schema revision 1; expected 2' "$context_error"; then
    "$runtime" --project "$project_root" init
  else
    cat "$context_error" >&2
    exit 1
  fi
else
  "$runtime" --project "$project_root" init
fi
