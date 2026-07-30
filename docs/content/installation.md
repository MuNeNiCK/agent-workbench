# Installation

```bash
gh skill install MuNeNiCK/agent-workbench agent-workbench@v0.2.4 \
  --scope user --agent <target-agent>
```

Ask the coding agent to use `$agent-workbench` for the project. The Skill:

1. resolves the project root;
2. downloads the matching Linux x86_64 runtime on first use;
3. verifies the published SHA-256 checksum; and
4. acquires the pinned official Lean distribution during `init`.

The user does not install or operate Lean directly.

The Skill does not change `.gitignore`, Git configuration, or the index.

## Source build

```bash
lake build
.lake/build/bin/agent-workbench --version
lake test
scripts/test-skill.sh .lake/build/bin/agent-workbench
```
