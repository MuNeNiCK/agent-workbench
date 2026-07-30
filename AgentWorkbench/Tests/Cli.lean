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
  match AgentWorkbench.Cli.parseCommandDisposition "required" with
  | .ok .required => pure ()
  | _ => throw <| IO.userError "required Command Profile did not parse"
  match AgentWorkbench.Cli.parseKPTCategory "problem" with
  | .ok .problem => pure ()
  | _ => throw <| IO.userError "KPT Problem category did not parse"
  match
      AgentWorkbench.Cli.parseKPTRelation
        "review-observation" "fresh-review" "missing-boundary" with
  | .ok (some (.reviewObservation "fresh-review" "missing-boundary")) =>
      pure ()
  | _ => throw <| IO.userError "tagged KPT Review relation did not parse"
  match
      AgentWorkbench.Cli.parseKPTRelation
        "command-profile" "shared-check" "work" with
  | .ok (some (.commandProfile "shared-check" .focusedWork)) => pure ()
  | _ =>
      throw <| IO.userError
        "scoped KPT Command Profile relation did not parse"
  match
      AgentWorkbench.Cli.parseKPTRelation
        "evidence-result" "shared-evidence" "design:selected-design" with
  | .ok (some (.evidenceResult "shared-evidence" (.design "selected-design"))) =>
      pure ()
  | _ =>
      throw <| IO.userError "Design-basis KPT Evidence relation did not parse"
  match
      AgentWorkbench.Cli.parseKPTRelation
        "evidence-result" "shared-evidence" "selected-design" with
  | .error _ => pure ()
  | _ =>
      throw <| IO.userError
        "KPT Evidence relation accepted a non-injective bare basis"
  match AgentWorkbench.Cli.parseKPTRelation "design" "selected-design" "extra" with
  | .error _ => pure ()
  | _ =>
      throw <| IO.userError
        "KPT relation accepted an invalid extra member"
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

def testProjectMemoryRendering : IO Unit := do
  IO.FS.withTempDir fun root => do
    let path := root / "project-memory.sqlite3"
    let projectDecision :=
      decision "project-profile" "Select the project route."
    let projectProfile ← unwrap
      (Kernel.recordCommandProfile initialState projectDecision.source
        (some projectDecision) "shared-check" "verify the release"
        .project ["tool", "one argument"] none .required)
      "project-memory rendering profile fixture failed"
    let workDecision :=
      decision "work-profile" "Select the exact Work route."
    let workProfile ← unwrap
      (Kernel.recordCommandProfile projectProfile workDecision.source
        (some workDecision) "shared-check" "verify the release"
        (.work projectProfile.focus.work.key)
        ["tool", "one argument", "", "line\nbreak", "\"quoted\""]
        (some "path with space") .required)
      "project-memory rendering Work profile fixture failed"
    let diagnosticDecision :=
      decision "diagnostic-profile" "Accept the diagnostic route."
    let diagnosticProfile ← unwrap
      (Kernel.recordCommandProfile workProfile diagnosticDecision.source
        (some diagnosticDecision) "diagnostic-check" "diagnose the release"
        .project ["tool", "recommended"] none .recommended)
      "project-memory rendering diagnostic profile fixture failed"
    let deviated ← unwrap
      (Kernel.recordCommandDeviation diagnosticProfile "diagnostic-check"
        ["tool", "alternate argument", "", "line\nbreak"]
        (some "diagnostic path") "Use the bounded diagnostic alternate."
        (source "diagnostic-deviation" .agent))
      "project-memory rendering deviation fixture failed"
    let callerKPTDecision :=
      decision "caller-kpt" "Retain the caller-owned lesson."
    let callerKPT ← unwrap
      (Kernel.recordKPT deviated callerKPTDecision.source "caller"
        (some callerKPTDecision) "review-context" .problem
        (.work workProfile.focus.work.key)
        "A resumed reviewer retains implementation context."
        (some (.task "implement the selected change")))
      "project-memory rendering caller KPT fixture failed"
    let proposal ← unwrap
      (Kernel.recordKPT callerKPT (source "proposal-action" .agent) "codex"
        none "review-context" .try (.work callerKPT.focus.work.key)
        "Use a fresh reviewer execution." none)
      "project-memory rendering KPT proposal fixture failed"
    let adopted ← unwrap
      (Kernel.acceptKPT proposal "review-context"
        (.work proposal.focus.work.key) "codex"
        (decision "adopt-kpt" "Adopt the exact correction."))
      "project-memory rendering KPT adoption fixture failed"
    expect adopted.wellFormed
      "project-memory rendering fixture is not a valid Kernel state"
    let _ ← match ← AgentWorkbench.Adapter.SQLite.initializeStore
        path "memory-store" "initialize" adopted with
      | .ok snapshot => pure snapshot
      | .error error =>
          throw <| IO.userError
            s!"project-memory CLI fixture initialization failed: {repr error}"
    let delegated ← runCliChild path
      #["add-evidence", "render-evidence",
        "Observe the exact rendered route.", "run exact argv",
        "supported host", "-", "passes", "ordinary process",
        "sha256:render", "-", "shared-check", "work",
        "Select the exact Work route."]
      #[("AGENT_WORKBENCH_PRIVATE_TOKEN", some "cli-project-memory"),
        ("AGENT_WORKBENCH_SOURCE_CONTEXT", some "cli-project-memory")]
    expect (delegated.exitCode == 0)
      s!"project-memory CLI delegation failed: {delegated.stderr}"
    let delegatedState ← match ← AgentWorkbench.Adapter.SQLite.inspect path with
      | .ok snapshot => pure snapshot
      | .error error =>
          throw <| IO.userError
            s!"project-memory delegation inspection failed: {repr error}"
    expect (delegatedState.revision == 1)
      "project-memory CLI delegation produced more than one durable effect"
    let status ← runCliChild path #["status"]
    let exactArgv :=
      (Lean.toJson
        ["tool", "one argument", "", "line\nbreak", "\"quoted\""]).compress
    expect (status.exitCode == 0 &&
        status.stdout.contains s!"argv: {exactArgv}" &&
        status.stdout.contains "Scope: Work: deliver the selected change" &&
        status.stdout.contains
          "Profile selection: caller-owned (Select the exact Work route.)" &&
        status.stdout.contains
          "actual argv: [\"tool\",\"alternate argument\",\"\",\"line\\nbreak\"]" &&
        status.stdout.contains "actual cwd: \"diagnostic path\"" &&
        status.stdout.contains
          "Reason: Use the bounded diagnostic alternate." &&
        status.stdout.contains "Source: agent (diagnostic-deviation)" &&
        status.stdout.contains "Author: codex" &&
        !status.stdout.contains "work:")
      "status did not render exact argv or current KPT in project language"
    let pendingNext ← runCliChild path #["next"]
    expect (pendingNext.exitCode == 0 &&
        pendingNext.stdout.contains
          s!"Command Profile shared-check@1 ({exactArgv} from \"path with space\")")
      "next did not render the exact structured argv selected by Evidence"
    let history ← runCliChild path #["kpt-history", "review-context", "work"]
    expect (history.exitCode == 0 &&
        history.stdout.contains
          "KPT history [kpt:review-context] for Work: deliver the selected change:" &&
        history.stdout.contains "[Problem:review-context@0]" &&
        history.stdout.contains "[Try:review-context@1]" &&
        history.stdout.contains "[Try:review-context@2]" &&
        history.stdout.contains "Predecessor: review-context@1" &&
        history.stdout.contains
          "Relation: Task implement the selected change [task@0]" &&
        history.stdout.contains "Source: agent")
      "KPT history did not expose immutable succession and provenance"

def run : IO Unit := do
  testParsing
  testRendering
  testJsonValidation
  testRepresentativeDelegation
  testProjectMemoryRendering
  IO.println "cli tests: pass"

end AgentWorkbench.Tests.Cli
