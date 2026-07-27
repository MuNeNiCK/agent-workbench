# Agent Workbench

Agent Workbench gives coding agents durable state for work that spans design,
implementation, review, interruption, and recovery. The product is a native
CLI backed by a Lean transition kernel and a project-local SQLite state file.

It does not replace the managed project's source tree, issue tracker, build
system, or tests. It records the accepted work boundary and the positive
evidence needed to decide whether that work is complete.

## What the Lean version changes

Starting with `v0.2.0`, Lean is the default implementation. Acceptance-critical
transitions are decided by one typed kernel. The SQLite adapter persists the
accepted events, and read-only commands recover the same state in a fresh
process.

Reviewers provide observations. The caller remains responsible for accepting
or rejecting them and records the reason. A review comment does not
automatically become a product requirement.

## Supported release

Prebuilt releases from `v0.2.2` target Linux x86_64 as a static executable and
do not require a host-provided glibc. Source builds use the toolchain pinned in
`lean-toolchain`.

Continue with [Installation](installation.md) and [Workflow](workflow.md).
