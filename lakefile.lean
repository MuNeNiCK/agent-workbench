import Lake

open System Lake DSL

package «agent-workbench» where
  version := v!"0.1.0"

require leansqlite from git
  "https://github.com/leanprover/leansqlite" @ "v4.30.0"

extern_lib durableFilesystem pkg := do
  let source ← inputTextFile <| pkg.dir / "bindings" / "durable_filesystem.c"
  let object := pkg.buildDir / "durable_filesystem.o"
  let object ← buildO object source #["-I", (← getLeanIncludeDir).toString]
    (traceArgs := #["-fPIC", "-std=c11", "-D_GNU_SOURCE", "-Wall", "-Wextra", "-Werror"])
    (extraDepTrace := getLeanTrace)
  buildStaticLib (pkg.staticLibDir / nameToStaticLib "durable_filesystem") #[object]

@[default_target]
lean_lib AgentWorkbench where
  needs := #[durableFilesystem]

@[default_target]
lean_exe «agent-workbench» where
  root := `Main

@[default_target]
lean_exe «kernel-laws» where
  root := `AgentWorkbench.Tests.KernelLaws

@[default_target]
lean_exe «storage-laws» where
  root := `AgentWorkbench.Tests.StorageLaws

@[default_target]
lean_exe «workflow-laws» where
  root := `AgentWorkbench.Tests.WorkflowLaws

@[default_target]
lean_exe «verified-core-audit» where
  root := `AgentWorkbench.Audit.Main
