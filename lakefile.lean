import Lake
open Lake DSL System

package «agent-workbench» where
  version := v!"0.0.0"

require leansqlite from git
  "https://github.com/leanprover/leansqlite.git" @ "v4.32.0"

require Blake3 from git
  "https://github.com/MuNeNiCK/Blake3.lean.git" @
    "4d4ec7d21b43fcc0cf89a93e3344b10fbd2e0754"

lean_lib AgentWorkbench
lean_lib AgentWorkbenchTest

@[default_target]
lean_exe «agent-workbench» where
  root := `Main

lean_exe «agent-workbench-tests» where
  root := `AgentWorkbenchTests

lean_exe «agent-workbench-proof-tests» where
  root := `AgentWorkbenchProofTests
