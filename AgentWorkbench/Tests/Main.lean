import AgentWorkbench.Tests.Domain
import AgentWorkbench.Tests.Kernel
import AgentWorkbench.Tests.SQLite
import AgentWorkbench.Tests.Cli

def main (arguments : List String) : IO Unit :=
  match arguments with
  | "cli-child" :: childArguments =>
      AgentWorkbench.Cli.run childArguments
  | [] => do
      AgentWorkbench.Tests.Domain.run
      AgentWorkbench.Tests.Kernel.run
      AgentWorkbench.Tests.SQLite.run
      AgentWorkbench.Tests.Cli.run
  | _ =>
      throw <| IO.userError "unknown test process mode"
