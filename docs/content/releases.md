# Releases

Each release publishes:

- a Linux x86_64 persistence/runtime executable;
- its SHA-256 checksum.

The versioned repository supplies the Agent Skill, documentation, and source.
GitHub's tag is the release identity; separate copies of those same materials
are not additional product assets.

The Skill downloads the runtime on first use and acquires the pinned official
Lean distribution during `init`.

Release validation uses the pinned official Lean toolchain, runs the Domain,
Kernel, SQLite, and CLI tests, then checks the installed-Skill and release
boundaries.

The Git tag, package version, Skill version, executable version, and asset names
must agree.
