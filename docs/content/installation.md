# Installation

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench@v0.2.3 \
  --scope user --agent <target-agent>
```

Ask the coding agent to use `$agent-workbench` for the project. The Skill:

1. resolves the project root;
2. downloads the matching static Linux x86_64 runtime on first use;
3. verifies the published SHA-256 checksum; and
4. creates project memory under `.agent-workbench` after `init`.

If formal assurance is selected, the first `formal-check` separately downloads
and verifies the pinned portable Lean 4.30.0 tool. Ordinary work does not incur
that download. Neither runtime requires host glibc, and the user does not
operate Lean directly.

The Skill does not change `.gitignore`, Git configuration, or the index.

## Source build

```bash
lake build
.lake/build/bin/agent-workbench --version
lake test
scripts/test-skill.sh .lake/build/bin/agent-workbench
```
