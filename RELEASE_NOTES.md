# Agent Workbench v0.2.1

`v0.2.1` makes the Linux x86_64 release independent of the host C runtime.

- Publish Agent Workbench as a static Linux x86_64 executable.
- Validate the packaged executable in clean musl and glibc environments.
- Keep ordinary source builds on the Lean toolchain selected by
  `lean-toolchain`.
- Preserve exact release commit, target, checksums, and version metadata.
