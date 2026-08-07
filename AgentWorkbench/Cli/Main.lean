import AgentWorkbench.Adapter.MutationStore
import AgentWorkbench.Application.Design
import AgentWorkbench.Application.Work
import AgentWorkbench.Application.Ledger
import AgentWorkbench.Application.Completion
import AgentWorkbench.Application.Command
import AgentWorkbench.Application.Proof
import AgentWorkbench.Application.Current
import AgentWorkbench.Application.Task
import AgentWorkbench.Application.Profile
import AgentWorkbench.Application.Artifact
import AgentWorkbench.Application.Guidance
import AgentWorkbench.Application.Review
import AgentWorkbench.Application.Query
import AgentWorkbench.Adapter.Runtime
import AgentWorkbench.Adapter.DesignArchive
import AgentWorkbench.Adapter.PlanArchive
import AgentWorkbench.Cli.Protocol
import AgentWorkbench.Cli.Describe
import AgentWorkbench.Cli.Decode
import AgentWorkbench.Cli.Query

namespace AgentWorkbench.Cli

private def fail (message : String) : IO α :=
  throw (IO.userError message)


private def writeJson [Lean.ToJson α] (value : α) : IO Unit :=
  IO.println (Lean.toJson value).compress

private structure Invocation where
  projectRoot : System.FilePath
  command : List String

private def parseInvocation (arguments : List String) : IO Invocation := do
  match arguments with
  | "--project" :: root :: command =>
      if command.isEmpty then fail "missing command after --project"
      pure { projectRoot := ← IO.FS.realPath root, command }
  | command =>
      if command.isEmpty then fail "missing command"
      pure { projectRoot := ← IO.currentDir, command }

private def databasePath (root : System.FilePath) : System.FilePath :=
  root / ".agent-workbench" / "state.db"


private def emitState (state : ProjectState) : IO Unit :=
  writeJson (StateResult.ofState state)

private def emitCurrentState
    (projectRoot : System.FilePath) (state : ProjectState) : IO Unit := do
  let inputs ← evaluateCurrentInputs projectRoot state
  writeJson (ContextResult.mk state.revision
    (projectContext? state inputs.observations inputs.claimDigests))

private def emitMutationResult (projectRoot : System.FilePath) : MutationResult → IO Unit
  | .state state => emitState state
  | .context state => emitCurrentState projectRoot state
  | .design design => writeJson design
  | .plan plan => writeJson plan
  | .command result => writeJson result
  | .proof result => writeJson result


private def runCommand (invocation : Invocation) : IO Unit := do
  match ← decodeMutation? invocation.command with
  | some mutation => do
      let result ← Store.executeMutation invocation.projectRoot
        (databasePath invocation.projectRoot) mutation
      emitMutationResult invocation.projectRoot result
  | none =>
      match ← decodeQuery? invocation.command with
      | some query => runQuery invocation.projectRoot query
      | none => fail s!"unknown command: {String.intercalate " " invocation.command}"

def main (arguments : List String) : IO Unit := do
  runCommand (← parseInvocation arguments)

end AgentWorkbench.Cli
