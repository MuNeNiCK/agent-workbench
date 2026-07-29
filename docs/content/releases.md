# Releases

Each release publishes:

- a static Linux x86_64 persistence/runtime executable;
- a portable pinned Lean formal-tool archive;
- SHA-256 checksums for both archives.

The versioned repository supplies the Agent Skill, documentation, and source.
GitHub's tag is the release identity; separate copies of those same materials
are not additional product assets.

The Skill downloads the runtime on first use and the formal tool only on first
formal checking. Both archives are verified before installation and their
cached content is revalidated.

Release validation builds from the pinned Lean source commit, runs the Domain,
Kernel, SQLite, and CLI tests, then checks the distinct installed-Skill and
release boundaries. It verifies lazy acquisition, a multi-module
contract/proof/oracle package, one real product-boundary adapter comparison,
the static runtime, and the installed formal tool without host glibc in clean
Alpine.

The Git tag, package version, Skill version, executable version, and asset names
must agree.
