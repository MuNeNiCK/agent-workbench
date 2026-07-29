import AgentWorkbench.Cli.Program
import AgentWorkbench.Tests.Support

namespace AgentWorkbench.Tests.Cli

open AgentWorkbench
open AgentWorkbench.Domain
open AgentWorkbench.Tests

def testParsing : IO Unit := do
  match AgentWorkbench.Cli.parseRole "non-functional" with
  | .ok .nonFunctionalRequirement => pure ()
  | _ => throw <| IO.userError "non-functional role did not parse"
  match AgentWorkbench.Cli.parseReviewPurpose "reuse" with
  | .ok .reuseDecision => pure ()
  | _ => throw <| IO.userError "reuse Review purpose did not parse"
  match AgentWorkbench.Cli.parseReviewDecision "needs-evidence" with
  | .ok .needsEvidence => pure ()
  | _ => throw <| IO.userError "non-final Review disposition did not parse"
  match AgentWorkbench.Cli.parsePassed "pass" with
  | .ok true => pure ()
  | _ => throw <| IO.userError "Evidence result did not parse"
  expect (AgentWorkbench.Cli.commaSeparated "a,b" == ["a", "b"])
    "comma-separated project inputs did not parse"
  expect (AgentWorkbench.Cli.commaSeparated "-").isEmpty
    "optional empty list did not parse"

def testRendering : IO Unit := do
  expect (AgentWorkbench.Cli.roleName .projectStructure == "Project structure")
    "project structure role did not render in project language"
  expect (AgentWorkbench.Cli.reviewPurposeName .designMeaning == "design meaning")
    "Review purpose did not render in project language"
  expect (AgentWorkbench.Cli.reviewDecisionName .deferred == "deferred")
    "non-final Review disposition did not render"

def runCliChild (path : System.FilePath) (arguments : Array String)
    (extraEnv : Array (String × Option String) := #[]) : IO IO.Process.Output := do
  let executable ← IO.appPath
  IO.Process.output
    { cmd := executable.toString
      args := #["cli-child"] ++ arguments
      env := #[("AGENT_WORKBENCH_STATE_PATH", some path.toString)] ++ extraEnv }

def testRepresentativeDelegation : IO Unit := do
  IO.FS.withTempDir fun root => do
    let path := root / "state.sqlite3"
    let initialized ← match ← AgentWorkbench.Adapter.SQLite.initializeStore
        path "cli-store" "initialize" initialState with
      | .ok snapshot => pure snapshot
      | .error error =>
          throw <| IO.userError s!"CLI fixture initialization failed: {repr error}"
    let read ← runCliChild path #["status"]
    expect (read.exitCode == 0)
      s!"representative CLI read failed: {read.stderr}"
    let afterRead ← match ← AgentWorkbench.Adapter.SQLite.inspect path with
      | .ok snapshot => pure snapshot
      | .error error =>
          throw <| IO.userError s!"CLI read inspection failed: {repr error}"
    expect (afterRead == initialized)
      "representative CLI read delegated to a mutation"
    let mutation ← runCliChild path #["finish-task"]
      #[("AGENT_WORKBENCH_PRIVATE_TOKEN", some "cli-finish"),
        ("AGENT_WORKBENCH_SOURCE_CONTEXT", some "cli-finish"),
        ("AGENT_WORKBENCH_EXPECTED_REVISION",
          some (toString initialized.revision)),
        ("AGENT_WORKBENCH_EXPECTED_INSTANCE", some initialized.storeId)]
    expect (mutation.exitCode == 0)
      s!"representative CLI mutation failed: {mutation.stderr}"
    let afterMutation ← match ← AgentWorkbench.Adapter.SQLite.inspect path with
      | .ok snapshot => pure snapshot
      | .error error =>
          throw <| IO.userError s!"CLI mutation inspection failed: {repr error}"
    expect (afterMutation.revision == initialized.revision + 1)
      "representative CLI mutation produced more than one durable effect"

def run : IO Unit := do
  testParsing
  testRendering
  testRepresentativeDelegation
  IO.println "cli tests: pass"

end AgentWorkbench.Tests.Cli
