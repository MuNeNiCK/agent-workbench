# Release contents and validation

This reference is for users and maintainers auditing what setup downloads and how published assets
are validated. It is not required reading for ordinary Workbench use.

## Published assets

Each GitHub Release publishes one native archive and one SHA-256 checksum for each supported target:

- Linux x86_64
- Linux aarch64
- macOS x86_64
- macOS aarch64
- Windows x86_64

An archive contains the native Workbench executable, the matching official Elan executable,
redistribution licenses for Agent Workbench, LeanSQLite, Blake3.lean, BLAKE3, Lean, and Elan, the
release-matched Skill and setup helpers below `skill/agent-workbench/`, and the public README and
reference pages. The Lean toolchain is acquired project-locally by native `init`; setup does not
assume a global installation.

## Setup boundary

The installed Skill carries its release version. Its setup helper selects the platform archive,
downloads that exact version's archive and checksum, verifies the archive's GitHub build-provenance
attestation and SHA-256 checksum, validates a complete bundle in a fresh sibling of
`.agent-workbench/bin`, replaces the old bundle through a recoverable directory swap, and invokes
native `init`. A retry recovers an interrupted swap, and an upgrade cannot retain obsolete files
from the prior bundle. It never resolves `latest`; a Skill installed from one release cannot
silently acquire a runtime from another release. POSIX and PowerShell helpers own acquisition only;
after initialization the Skill uses the native executable directly.

## Release validation

Release CI uses GitHub-hosted native runners without Docker or QEMU. For each platform it:

1. builds the runtime, domain/integration tests, and isolated proof tests;
2. runs both test executables;
3. stages the runtime, Elan, licenses, release-matched Skill, and public documentation;
4. installs the candidate Skill through `gh skill install`;
5. exercises Skill installation, first initialization, idempotent setup, the operation index, and
   the first public Work transition; the full semantic and concurrent routes remain in the native
   product test executable;
6. packages the archive and checksum; and
7. uploads its assets.

The publish job runs only after all platform jobs succeed. Publication uses two representations of
one authorization: a record generated directly from the ready Workbench state and a signed
annotated Git tag whose message must be byte-for-byte identical to that record. Its form is:

```text
agent-workbench release authorization v1
work-id: <ready Work ID>
target-commit: <tagged commit SHA>
ready-state-revision: <revision used for ready>
ready-digest: blake3:<digest of the canonical ready result>
design-review-entry-id: <fresh Design Review root entry>
design-review-conclusion-entry-id: <zero-Finding conclusion entry>
design-review-target-snapshot: blake3:<immutable Design Review target digest>
design-review-clean: true
implementation-review-entry-id: <fresh Implementation Review root entry>
implementation-review-conclusion-entry-id: <zero-Finding conclusion entry>
implementation-review-target-snapshot: blake3:<exact release candidate target digest>
implementation-review-clean: true
```

After the candidate commit is fixed, the maintainer generates the record with the native runtime:

```sh
commit=$(git rev-parse HEAD)
authorization_file=$(mktemp)
python3 .github/verify-release-authorization.py prepare \
  --workbench .agent-workbench/bin/agent-workbench \
  --project . \
  --target-commit "$commit" \
  --design-review-entry-id <fresh-clean-design-review> \
  --implementation-review-entry-id <fresh-clean-implementation-review> \
  > "$authorization_file"
git notes --ref=refs/notes/agent-workbench-release add \
  -F "$authorization_file" "$commit"
git push origin refs/notes/agent-workbench-release
git tag -s -F "$authorization_file" <version>
git push origin <version>
```

`prepare` reads `ready` and both exact Review records itself. It rejects a non-ready Work, any
remaining gap, the wrong Work or Design target, a non-fresh or non-independent Review, a Review with
a Finding, or a non-clean/missing conclusion. The Git note transports that checked record without
changing the already reviewed commit; it is not an alternative source of release facts.

Before emitting the record, `prepare` also derives the exact source/build target set from the fixed
Implementation Review manifest. For each declared target it rejects tracked, staged, untracked, or
ignored checkout content that differs from the authorized commit. Paths outside that reviewed set
do not become release inputs merely because they exist in the same repository.

Release CI imports the repository-pinned public key and requires exactly one GnuPG `VALIDSIG`
record whose signing-key fingerprint is that pinned primary key. Signing subkeys are intentionally
rejected; when GnuPG also emits a primary-key fingerprint, it must name the same key. CI fetches the
remote annotated-tag object into a dedicated verification ref before checking its type and
signature. The checkout-selected commit and any checkout-local tag name are not signature
authority. CI then loads the separately transported record from
`refs/notes/agent-workbench-release`, requires the signed tag message to match it exactly, requires
both to name the workflow commit, and rejects
missing, duplicate, extra, or malformed authorization fields. Thus a passing build from an arbitrary
`v*` tag cannot publish a release. A Finding disposition can resolve Work
authority but cannot make a finding-bearing Review clean. The signed digests bind both immutable
targets without publishing `.agent-workbench` state.

These checks establish the tested distribution boundary. They do not turn external platform,
network, filesystem, or natural-language properties into Lean theorems; see
[Lean and assurance](assurance.md).
