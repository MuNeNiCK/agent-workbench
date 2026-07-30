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
  let firstIntent :=
    AgentWorkbench.Cli.formalResultMutationIntentArguments
      "rule" "design" "0" "tool" "oracle" "pass" "preview:digest"
      ["Rule.Proof"] ["Rule.Proof=sha256:abc"] "semantic meaning"
  let retriedIntent :=
    AgentWorkbench.Cli.formalResultMutationIntentArguments
      "rule" "design" "0" "tool" "oracle" "pass" "preview:digest"
      ["Rule.Proof"] ["Rule.Proof=sha256:abc"] "semantic meaning"
  let changedIntent :=
    AgentWorkbench.Cli.formalResultMutationIntentArguments
      "rule" "design" "0" "tool" "oracle" "pass" "preview:other"
      ["Rule.Proof"] ["Rule.Proof=sha256:abc"] "changed meaning"
  expect (firstIntent == retriedIntent && firstIntent != changedIntent)
    "formal mutation intent did not follow stable semantic content"
  let separator := String.singleton (Char.ofNat 31)
  expect
    (AgentWorkbench.Cli.mutationIntent
        ["alpha" ++ separator ++ "beta", "gamma"] !=
      AgentWorkbench.Cli.mutationIntent
        ["alpha", "beta" ++ separator ++ "gamma"])
    "mutation intent collapsed distinct argument vectors containing the old delimiter"

def testRendering : IO Unit := do
  expect (AgentWorkbench.Cli.roleName .projectStructure == "Project structure")
    "project structure role did not render in project language"
  expect (AgentWorkbench.Cli.reviewPurposeName .designMeaning == "design meaning")
    "Review purpose did not render in project language"
  expect (AgentWorkbench.Cli.reviewDecisionName .deferred == "deferred")
    "non-final Review disposition did not render"
  let selectedSource := source "render-assurance"
  let item : Design.Item :=
    { ref := { key := "checkout", version := 0 }
      predecessor := none
      statement := "Observe checkout."
      role := .functionalRequirement
      source := selectedSource
      dependencies := []
      assurance :=
        { kind := .evidence
          obligations :=
            [{ key := "shared"
               method := .evidence
               description := "Observe the shared rule." }] }
      authority :=
        .acceptedByCaller
          { source := selectedSource, reason := "Caller selected checkout." } }
  let accepted ← match item.acceptedRef? with
    | some accepted => pure accepted
    | none => throw <| IO.userError "rendering Design is not accepted"
  let state : AgentWorkbench.Kernel.State :=
    { initialState with
      design :=
        { effects := [{ source := item.source, content := .design item }] } }
  let member : Work.CompletionMember :=
    { target := .assurance "shared", basis := .design [accepted] }
  expect
    (AgentWorkbench.Cli.describeMember state member ==
      "run add-evidence shared ... checkout, then record-evidence shared ... checkout for: Observe the shared rule.")
    "next did not render the exact Evidence Design selector"

def runCliChild (path : System.FilePath) (arguments : Array String)
    (extraEnv : Array (String × Option String) := #[]) : IO IO.Process.Output := do
  let executable ← IO.appPath
  IO.Process.output
    { cmd := executable.toString
      args := #["cli-child"] ++ arguments
      env := #[("AGENT_WORKBENCH_STATE_PATH", some path.toString)] ++ extraEnv }

def testJsonValidation : IO Unit := do
  IO.FS.withTempDir fun root => do
    let valid := root / "valid.json"
    let malformed := root / "malformed.json"
    IO.FS.writeFile valid "{\"result\":\"observed\"}\n"
    IO.FS.writeFile malformed "{not-json\n"
    let accepted ← runCliChild (root / "unused.sqlite3")
      #["validate-json-file", valid.toString]
    expect (accepted.exitCode == 0)
      "valid project observation was rejected"
    let rejected ← runCliChild (root / "unused.sqlite3")
      #["validate-json-file", malformed.toString]
    expect (rejected.exitCode != 0)
      "malformed project observation reached structural comparison"

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
    let staleFile := root / "stale-formal-identities"
    let encodedIdentity :=
      "{\"assurance\":\"shared\",\"design\":\"rule\",\"version\":0,\"result\":\"preview:digest\"}"
    IO.FS.writeFile staleFile <|
      String.intercalate "\n" (List.replicate 225000 encodedIdentity)
    let largeStaleRead ← runCliChild path #["status"]
      #[("AGENT_WORKBENCH_STALE_FORMAL_RESULT_IDENTITIES_FILE",
          some staleFile.toString)]
    expect (largeStaleRead.exitCode == 0)
      s!"file-backed stale identities disabled a public read: {largeStaleRead.stderr}"
    let afterRead ← match ← AgentWorkbench.Adapter.SQLite.inspect path with
      | .ok snapshot => pure snapshot
      | .error error =>
          throw <| IO.userError s!"CLI read inspection failed: {repr error}"
    expect (afterRead == initialized)
      "representative CLI read delegated to a mutation"
    let incomplete ← runCliChild path #["complete"]
    expect (incomplete.exitCode != 0 &&
        incomplete.stdout.contains "Next:")
      "incomplete Work reported successful completion"
    let selectedEvidence ← runCliChild path
      #["add-evidence", "shared", "Observe the selected rule.", "observe",
        "supported host", "-", "observation passes", "ordinary process",
        "sha256:selected", "selected-design"]
      #[("AGENT_WORKBENCH_PRIVATE_TOKEN", some "cli-evidence-select"),
        ("AGENT_WORKBENCH_SOURCE_CONTEXT", some "cli-evidence-select"),
        ("AGENT_WORKBENCH_EXPECTED_REVISION",
          some (toString initialized.revision)),
        ("AGENT_WORKBENCH_EXPECTED_INSTANCE", some initialized.storeId)]
    expect (selectedEvidence.exitCode != 0 &&
        selectedEvidence.stderr.contains
          "No selected Evidence obligation matches that Design.")
      "optional Evidence Design selector did not reach the Kernel"
    let recordedEvidence ← runCliChild path
      #["record-evidence", "shared", "observed", "pass", "selected-design"]
      #[("AGENT_WORKBENCH_PRIVATE_TOKEN", some "cli-evidence-record"),
        ("AGENT_WORKBENCH_SOURCE_CONTEXT", some "cli-evidence-record"),
        ("AGENT_WORKBENCH_EXPECTED_REVISION",
          some (toString initialized.revision)),
        ("AGENT_WORKBENCH_EXPECTED_INSTANCE", some initialized.storeId)]
    expect (recordedEvidence.exitCode != 0 &&
        recordedEvidence.stderr.contains
          "No current evidence description has that name.")
      "optional Evidence result Design selector did not reach the Kernel"
    let mutation ← runCliChild path #["finish-task"]
      #[("AGENT_WORKBENCH_PRIVATE_TOKEN", some "cli-finish"),
        ("AGENT_WORKBENCH_SOURCE_CONTEXT", some "cli-finish"),
        ("AGENT_WORKBENCH_EXPECTED_REVISION",
          some (toString initialized.revision)),
        ("AGENT_WORKBENCH_EXPECTED_INSTANCE", some initialized.storeId),
        ("AGENT_WORKBENCH_STALE_FORMAL_RESULT_IDENTITIES_FILE",
          some staleFile.toString)]
    expect (mutation.exitCode == 0)
      s!"representative CLI mutation failed: {mutation.stderr}"
    let afterMutation ← match ← AgentWorkbench.Adapter.SQLite.inspect path with
      | .ok snapshot => pure snapshot
      | .error error =>
          throw <| IO.userError s!"CLI mutation inspection failed: {repr error}"
    expect (afterMutation.revision == initialized.revision + 1)
      "representative CLI mutation produced more than one durable effect"
    let complete ← runCliChild path #["complete"]
    expect (complete.exitCode == 0 &&
        complete.stdout.contains "The current outcome is complete.")
      "satisfied Work did not report successful completion"

def run : IO Unit := do
  testParsing
  testRendering
  testJsonValidation
  testRepresentativeDelegation
  IO.println "cli tests: pass"

end AgentWorkbench.Tests.Cli
