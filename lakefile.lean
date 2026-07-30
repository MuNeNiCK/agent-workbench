import Lake

open Lake DSL

package «agent-workbench» where
  version := v!"0.2.4"
  leanOptions := #[⟨`warningAsError, true⟩]
  moreLinkArgs :=
    match get_config? staticRelease with
    | some "true" => #["-static"]
    | _ => #[]

require leansqlite from git
  "https://github.com/leanprover/leansqlite" @ "v4.30.0"

@[default_target]
lean_lib AgentWorkbench

@[default_target]
lean_exe «agent-workbench» where
  root := `Main

@[test_driver]
lean_exe tests where
  root := `AgentWorkbench.Tests.Main
