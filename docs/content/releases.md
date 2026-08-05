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
redistribution licenses for Agent Workbench, LeanSQLite, cryptography, Lean, and Elan. The Lean
toolchain is acquired project-locally by native `init`; setup does not assume a global installation.

## Setup boundary

The installed setup helper selects the platform archive, downloads the archive and checksum from the
latest GitHub Release, verifies the archive, extracts it below `.agent-workbench/bin`, and invokes
native `init`. POSIX and PowerShell helpers own acquisition only; after initialization the Skill uses
the native executable directly.

## Release validation

Release CI uses GitHub-hosted native runners without Docker or QEMU. For each platform it:

1. builds the runtime, domain/integration tests, and isolated proof tests;
2. runs both test executables;
3. stages the runtime, Elan, and licenses;
4. installs the candidate Skill through `gh skill install`;
5. exercises setup and the public semantic-operation route, including concurrent proof execution;
6. packages the archive and checksum; and
7. uploads its assets.

The publish job runs only after all platform jobs succeed. The signed Git tag is the release identity.

These checks establish the tested distribution boundary. They do not turn external platform,
network, filesystem, or natural-language properties into Lean theorems; see
[Lean and assurance](assurance.md).
