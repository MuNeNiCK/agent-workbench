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

An archive contains the native Workbench executable, the matching official Elan executable, and
redistribution licenses for Agent Workbench, LeanSQLite, Blake3.lean, BLAKE3, Lean, and Elan. The Lean
toolchain is acquired project-locally by native `init`; setup does not assume a global installation.

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
3. stages the runtime, Elan, and licenses;
4. installs the candidate Skill through `gh skill install`;
5. exercises Skill installation, first initialization, idempotent setup, the operation index, and
   the first public Work transition; the full semantic and concurrent routes remain in the native
   product test executable;
6. packages the archive and checksum; and
7. uploads its assets.

The publish job runs only after all platform jobs succeed. The signed Git tag is the release identity.

These checks establish the tested distribution boundary. They do not turn external platform,
network, filesystem, or natural-language properties into Lean theorems; see
[Lean and assurance](assurance.md).
