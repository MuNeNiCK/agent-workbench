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
attestation and SHA-256 checksum, extracts it below `.agent-workbench/bin`, and invokes native
`init`. It never resolves `latest`; a Skill installed from one release cannot silently acquire a
runtime from another release. POSIX and PowerShell helpers own acquisition only; after initialization
the Skill uses the native executable directly.

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

The publish job runs only after all platform jobs succeed. The signed annotated Git tag is both the
release identity and the immutable release authorization. Its message has this exact form:

```text
agent-workbench release authorization v1
work-id: <completed Work ID>
target-commit: <tagged commit SHA>
target-snapshot: blake3:<fresh Review target digest>
ready-state-revision: <revision used for ready>
ready-digest: blake3:<digest of the canonical ready result>
fresh-review-entry-id: <fresh Review root entry>
fresh-review-conclusion-entry-id: <clean conclusion entry>
fresh-review-clean: true
```

Release CI imports the repository-pinned public key, verifies the tag signature against its exact
fingerprint, requires the tag object and authorization to name the workflow commit, and rejects
missing, duplicate, extra, or malformed authorization fields. Thus a passing build from an arbitrary
`v*` tag cannot publish a release. The signer creates this tag only from the current `ready` result
and the clean fresh Review of that exact candidate; the signed digests make those private Workbench
records tamper-evident without publishing `.agent-workbench` state.

These checks establish the tested distribution boundary. They do not turn external platform,
network, filesystem, or natural-language properties into Lean theorems; see
[Lean and assurance](assurance.md).
