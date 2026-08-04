import Lake
open Lake DSL System

package «agent-workbench» where
  version := v!"0.0.0"

require leansqlite from git
  "https://github.com/leanprover/leansqlite.git" @ "v4.30.0"

require cryptography from git
  "https://github.com/gdncc/cryptography.git" @
    "883139dc0cd152a0f6f219b23aae35cbf6d67223"

lean_lib AgentWorkbench
lean_lib AgentWorkbenchTest

@[default_target]
lean_exe «agent-workbench» where
  root := `Main

lean_exe «agent-workbench-tests» where
  root := `AgentWorkbenchTests

lean_exe «agent-workbench-proof-tests» where
  root := `AgentWorkbenchProofTests
