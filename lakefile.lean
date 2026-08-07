import Lake
open Lake DSL System

package «agent-workbench» where
  version := v!"0.0.0"

require leansqlite from git
  "https://github.com/leanprover/leansqlite.git" @ "v4.32.0"

require Blake3 from git
  "https://github.com/MuNeNiCK/Blake3.lean.git" @ "main"

require MD4Lean from git
  "https://github.com/acmepjz/md4lean.git" @ "main"

lean_lib AgentWorkbench
lean_lib AgentWorkbenchTest
lean_lib AgentWorkbenchProof

@[default_target]
lean_exe «agent-workbench» where
  root := `Main

lean_exe «agent-workbench-tests» where
  root := `AgentWorkbenchTests

lean_exe «agent-workbench-proof-tests» where
  root := `AgentWorkbenchProofTests
