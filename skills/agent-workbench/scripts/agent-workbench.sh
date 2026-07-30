#!/bin/sh
set -eu

repository="MuNeNiCK/agent-workbench"
script_dir="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)"
skill_dir="$(dirname -- "$script_dir")"

case "$(uname -s):$(uname -m)" in
  Linux:x86_64|Linux:amd64) platform="linux-x86_64-static" ;;
  *)
    echo "agent-workbench: unsupported platform: $(uname -s) $(uname -m)" >&2
    exit 1
    ;;
esac

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "agent-workbench: required command not found: $1" >&2
    exit 1
  }
}

project_root() {
  candidate="$(pwd -P)"
  while test "$candidate" != "/"; do
    if test -f "$candidate/.agent-workbench/state.sqlite3"; then
      printf '%s\n' "$candidate"
      return
    fi
    candidate="$(dirname -- "$candidate")"
  done
  if command -v git >/dev/null 2>&1 &&
      candidate="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    CDPATH='' cd -- "$candidate" && pwd -P
    return
  fi
  pwd -P
}

operation() {
  executor="$1"
  shift
  case "$executor" in
    */formal-check.sh) state_runtime="$1" ;;
    *) state_runtime="$executor" ;;
  esac
  root="$(project_root)"
  private_dir="$root/.agent-workbench"
  state="$private_dir/state.sqlite3"
  pending="$private_dir/${AGENT_WORKBENCH_PENDING_FILE:-pending-operation}"
  mkdir -p "$private_dir"

  intent="$(
    {
      printf '%s\000' "$executor"
      for argument in "$@"; do
        printf '%s\000' "$argument"
      done
    } | sha256sum | sed -n 's/[[:space:]].*//p'
  )"
  if test -s "$pending"; then
    pending_intent="$(sed -n '1p' "$pending")"
    token="$(sed -n '2p' "$pending")"
    issued_revision="$(sed -n '3p' "$pending")"
    issued_instance="$(sed -n '4p' "$pending")"
    if test "$pending_intent" != "$intent"; then
      echo "agent-workbench: the prior project action has an uncertain result; retry it unchanged" >&2
      exit 1
    fi
  else
    token="$(dd if=/dev/urandom bs=32 count=1 2>/dev/null |
      sha256sum | sed -n 's/[[:space:]].*//p')"
    case "${1:-}" in
      init)
        issued_revision="-"
        issued_instance="-"
        ;;
      *)
        state_context="$(
          AGENT_WORKBENCH_STATE_PATH="$state" \
            "$state_runtime" state-context
        )"
        issued_revision="${state_context%%	*}"
        issued_instance="${state_context#*	}"
        ;;
    esac
    printf '%s\n%s\n%s\n%s\n' \
      "$intent" "$token" "$issued_revision" "$issued_instance" > "$pending"
  fi

  invoke() {
    if test "$issued_revision" = "-"; then
      AGENT_WORKBENCH_STATE_PATH="$state" \
        AGENT_WORKBENCH_PRIVATE_TOKEN="$token" \
        AGENT_WORKBENCH_SOURCE_CONTEXT="${AGENT_WORKBENCH_SOURCE_CONTEXT:-$token}" \
        "$executor" "$@"
    else
      AGENT_WORKBENCH_STATE_PATH="$state" \
        AGENT_WORKBENCH_PRIVATE_TOKEN="$token" \
        AGENT_WORKBENCH_SOURCE_CONTEXT="${AGENT_WORKBENCH_SOURCE_CONTEXT:-$token}" \
        AGENT_WORKBENCH_EXPECTED_REVISION="$issued_revision" \
        AGENT_WORKBENCH_EXPECTED_INSTANCE="$issued_instance" \
        "$executor" "$@"
    fi
  }
  if invoke "$@"; then
    rm -f "$pending"
  else
    code=$?
    if test "$code" -ne 75 && test "$code" -lt 128; then
      rm -f "$pending"
    fi
    exit "$code"
  fi
}

preview_formal() {
  runtime="$1"
  shift
  root="$(project_root)"
  private_dir="$root/.agent-workbench"
  progress="$private_dir/pending-preview-formal"
  progress_tmp="$private_dir/.pending-preview-formal.$$"
  mkdir -p "$private_dir"
  public_intent="$(
    {
      printf '%s\000' "preview-formal"
      for argument in "$@"; do
        printf '%s\000' "$argument"
      done
    } | sha256sum | sed -n 's/[[:space:]].*//p'
  )"
  stage="select"
  if test -s "$progress"; then
    recorded_intent="$(sed -n '1p' "$progress")"
    stage="$(sed -n '2p' "$progress")"
    if test "$recorded_intent" != "$public_intent"; then
      echo "agent-workbench: the prior formal preview has an uncertain result; retry it unchanged" >&2
      return 1
    fi
  else
    printf '%s\n%s\n' "$public_intent" "$stage" > "$progress_tmp"
    mv "$progress_tmp" "$progress"
  fi
  assurance="$1"
  design="$2"
  shift 2
  if test "$stage" = "select"; then
    if (cd "$root" &&
        AGENT_WORKBENCH_PENDING_FILE="pending-preview-select" \
          with_stale_formal_result_identities "$runtime" "$root" \
            operation "$runtime" select-formal "$assurance" "$design" "$@"); then
      printf '%s\n%s\n' "$public_intent" "check" > "$progress_tmp"
      mv "$progress_tmp" "$progress"
    else
      code=$?
      if test "$code" -ne 75; then rm -f "$progress"; fi
      return "$code"
    fi
  fi
  if (cd "$root" &&
      AGENT_WORKBENCH_PENDING_FILE="pending-preview-check" \
        with_stale_formal_result_identities "$runtime" "$root" \
          operation "$script_dir/formal-check.sh" "$runtime" "$assurance" \
            "$design" "preview"); then
    rm -f "$progress"
  else
    code=$?
    if test "$code" -ne 75; then rm -f "$progress"; fi
    return "$code"
  fi
}

inspect_stale_formal_result_identities() {
  root="$1"
  tab="$(printf '\t')"
  while IFS="$tab" read -r identity artifact; do
    path="${artifact%=sha256:*}"
    expected="${artifact##*=sha256:}"
    case "$path" in
      @formal-tool/*) continue ;;
    esac
    if test -z "$identity" || test "$path" = "$artifact" ||
        ! printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' ||
        ! test -f "$root/$path"; then
      printf '%s\n' "$identity"
      continue
    fi
    actual_line="$(sha256sum "$root/$path")" || return $?
    actual="${actual_line%% *}"
    if test "$actual" != "$expected"; then
      printf '%s\n' "$identity"
    fi
  done
}

stale_formal_result_identities() {
  runtime="$1"
  root="$2"
  output="$3"
  state="$root/.agent-workbench/state.sqlite3"
  temporary="$(mktemp -d)"
  artifacts="$temporary/formal-artifacts"
  inspected="$temporary/inspected"
  sorted="$temporary/sorted"
  complete="$temporary/complete"
  if AGENT_WORKBENCH_STATE_PATH="$state" "$runtime" formal-artifacts \
      > "$artifacts" &&
      inspect_stale_formal_result_identities "$root" \
        < "$artifacts" > "$inspected" &&
      sort -u "$inspected" > "$sorted" &&
      sed '/^$/d' "$sorted" > "$complete" &&
      mv "$complete" "$output"; then
    code=0
  else
    code=$?
  fi
  rm -f "$artifacts" "$inspected" "$sorted" "$complete"
  rmdir "$temporary"
  return "$code"
}

with_stale_formal_result_identities() {
  runtime="$1"
  root="$2"
  shift 2
  stale_file="$(mktemp)"
  state="$root/.agent-workbench/state.sqlite3"
  if test -f "$state"; then
    if stale_formal_result_identities "$runtime" "$root" "$stale_file"; then
      :
    else
      code=$?
      rm -f "$stale_file"
      return "$code"
    fi
  else
    : > "$stale_file"
  fi
  if test "${AGENT_WORKBENCH_WARN_STALE:-false}" = true &&
      test -s "$stale_file"; then
    echo "agent-workbench: formal assurance is stale; the affected claim requires formal-check" >&2
  fi
  if AGENT_WORKBENCH_STALE_FORMAL_RESULT_IDENTITIES_FILE="$stale_file" "$@"; then
    code=0
  else
    code=$?
  fi
  rm -f "$stale_file"
  return "$code"
}

run() {
  runtime="$1"
  shift
  case "${1:-}" in
    --version|-h|--help)
      exec "$runtime" "$@"
      ;;
    status|next|complete)
      root="$(project_root)"
      AGENT_WORKBENCH_STATE_PATH="$root/.agent-workbench/state.sqlite3" \
        AGENT_WORKBENCH_WARN_STALE=true \
        with_stale_formal_result_identities "$runtime" "$root" "$runtime" "$@"
      ;;
    request-design-review|accept-design|accept-complex-design|adopt-review-proposal|adopt-complex-review-proposal)
      root="$(project_root)"
      with_stale_formal_result_identities "$runtime" "$root" \
        operation "$runtime" "$@"
      ;;
    select-formal|record-formal-result|record-formal-result-files|formal-plan|formal-artifacts|remaining-stale-formal-identities|state-revision|state-context|compare-json-files|validate-json-file)
      echo "agent-workbench: this action is private to the verified formal route" >&2
      exit 1
      ;;
    formal-check)
      shift
      test "$#" -ge 1 && test "$#" -le 2
      root="$(project_root)"
      (cd "$root" &&
        with_stale_formal_result_identities "$runtime" "$root" \
          operation "$script_dir/formal-check.sh" "$runtime" "$@")
      ;;
    preview-formal)
      shift
      test "$#" -eq 7
      preview_formal "$runtime" "$@"
      ;;
    *)
      root="$(project_root)"
      with_stale_formal_result_identities "$runtime" "$root" \
        operation "$runtime" "$@"
      ;;
  esac
}

source_root="$(CDPATH='' cd -- "$skill_dir/../.." 2>/dev/null && pwd -P)" ||
  source_root=""
if test -n "$source_root" && test -f "$source_root/lakefile.lean" &&
    test -x "$source_root/.lake/build/bin/agent-workbench"; then
  run "$source_root/.lake/build/bin/agent-workbench" "$@"
  exit $?
fi

need curl
need grep
need sed
need sha256sum
need tar
need flock

version="$(sed -n '1{s/[[:space:]]//g;p;}' "$skill_dir/CLI_VERSION")"
if ! printf '%s\n' "$version" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "agent-workbench: invalid CLI_VERSION: $version" >&2
  exit 1
fi

cache_base="${XDG_CACHE_HOME:-${HOME}/.cache}"
cache_dir="$cache_base/agent-workbench/releases/$version/$platform"
runtime="$cache_dir/agent-workbench"
marker="$cache_dir/agent-workbench.sha256"
cache_parent="$(dirname -- "$cache_dir")"
cache_lock="$cache_parent/.runtime-install-lock"
mkdir -p "$cache_parent"
exec 9>"$cache_lock"
until flock -n 9; do
  sleep 0.05
done
release_cache_lock() {
  flock -u 9 2>/dev/null || true
  exec 9>&-
}
abort_cache_install() {
  code="$1"
  trap - EXIT HUP INT TERM
  release_cache_lock
  exit "$code"
}
trap release_cache_lock EXIT
trap 'abort_cache_install 129' HUP
trap 'abort_cache_install 130' INT
trap 'abort_cache_install 143' TERM

cache_valid=false
if test -x "$runtime" && test -s "$marker"; then
  expected="$(exec 9>&-; sed -n '1{s/[[:space:]]//g;p;}' "$marker")"
  actual="$(exec 9>&-; sha256sum "$runtime" |
    sed -n 's/[[:space:]].*//p' 9>&-)"
  if printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' 9>&- &&
      test "$actual" = "$expected"; then
    cache_valid=true
  fi
fi

if test "$cache_valid" != true; then
  download_dir="$(exec 9>&-; mktemp -d)"
  staging_dir="$(exec 9>&-; mktemp -d "$cache_parent/.runtime.XXXXXX")"
  cleanup() {
    rm -rf "$download_dir" "$staging_dir" 9>&-
  }
  abort_download() {
    code="$1"
    trap - EXIT HUP INT TERM
    cleanup
    release_cache_lock
    exit "$code"
  }
  trap 'cleanup; release_cache_lock' EXIT
  trap 'abort_download 129' HUP
  trap 'abort_download 130' INT
  trap 'abort_download 143' TERM

  asset="agent-workbench-$version-$platform.tar.gz"
  checksums="agent-workbench-$version-checksums.txt"
  base_url="https://github.com/$repository/releases/download/$version"
  curl -fsSL --connect-timeout 30 --speed-limit 1024 --speed-time 60 \
    "$base_url/$asset" -o "$download_dir/$asset" 9>&-
  curl -fsSL --connect-timeout 30 --speed-limit 1024 --speed-time 60 \
    "$base_url/$checksums" -o "$download_dir/$checksums" 9>&-
  grep "  $asset\$" "$download_dir/$checksums" 9>&- \
    > "$download_dir/$asset.sha256"
  (exec 9>&-; cd "$download_dir" && sha256sum -c "$asset.sha256" >/dev/null)
  tar -xzf "$download_dir/$asset" -C "$staging_dir" 9>&-
  chmod +x "$staging_dir/agent-workbench" 9>&-
  test "$(exec 9>&-; "$staging_dir/agent-workbench" --version)" = \
    "agent-workbench ${version#v}"
  sha256sum "$staging_dir/agent-workbench" 9>&- |
    sed -n 's/[[:space:]].*//p' 9>&- \
      > "$staging_dir/agent-workbench.sha256"

  rm -rf "$cache_dir" 9>&-
  mv "$staging_dir" "$cache_dir" 9>&-
  staging_dir=""
  cleanup
fi

release_cache_lock
trap - EXIT HUP INT TERM
run "$runtime" "$@"
