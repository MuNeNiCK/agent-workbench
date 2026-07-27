# Releases

## Release contents

Each supported release publishes:

- an Agent Skill archive containing the runtime bootstrap and pinned version;
- a static Linux x86_64 native runtime archive acquired internally by the Skill;
- a documentation archive;
- a source archive;
- release metadata containing the exact commit and implementation;
- SHA-256 checksums.

The Git tag, `lakefile.lean` version, Skill `CLI_VERSION`, native
`--version`, metadata, and asset names must agree.

## Release validation

The tag workflow builds the project with the pinned Lean toolchain and runs:

```bash
.lake/build/bin/kernel-laws
.lake/build/bin/storage-laws
.lake/build/bin/workflow-laws
.lake/build/bin/cli-laws
```

It builds the release executable against musl, checks that the packaged
artifact is statically linked, and runs the exact binary in clean Alpine and
Debian environments before creating the GitHub Release.

## Updating

Update the installed Skill to the new tag. Its wrapper downloads and verifies
the matching runtime on first use. Existing state must be opened only by a
release that supports its stored schema.
