import Lake
open Lake DSL System

package «agent-workbench» where
  version := v!"0.0.0"

require leansqlite from git
  "https://github.com/leanprover/leansqlite.git" @ "v4.30.0"

require Blake3 from git
  "https://github.com/argumentcomputer/Blake3.lean.git" @
    "d815da486b97afae97f8c1f76f17260e3db5b075"

lean_lib AgentWorkbench
lean_lib AgentWorkbenchTest

@[default_target]
lean_exe «agent-workbench» where
  root := `Main

lean_exe «agent-workbench-tests» where
  root := `AgentWorkbenchTests

lean_exe «agent-workbench-proof-tests» where
  root := `AgentWorkbenchProofTests
