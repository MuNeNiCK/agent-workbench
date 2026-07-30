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

def testProjectMemoryDelegation : IO Unit := do
  IO.FS.withTempDir fun root => do
    let path := root / "project-memory.sqlite3"
    let _ ← match ← AgentWorkbench.Adapter.SQLite.initializeStore
        path "memory-store" "initialize" initialState with
      | .ok snapshot => pure snapshot
      | .error error =>
          throw <| IO.userError
            s!"project-memory CLI fixture initialization failed: {repr error}"
    let environment token :=
      #[("AGENT_WORKBENCH_PRIVATE_TOKEN", some token),
        ("AGENT_WORKBENCH_SOURCE_CONTEXT", some token)]
    let profile ← runCliChild path
      #["record-command-profile", "release-check", "verify the release",
        "project", "required", "-", "Caller selected the exact check.",
        "lake", "test"]
      (environment "cli-profile")
    expect (profile.exitCode == 0)
      s!"Command Profile CLI delegation failed: {profile.stderr}"
    let evidence ← runCliChild path
      #["add-evidence", "release-observation", "Observe the release check.",
        "run exact argv", "supported host", "-", "passes",
        "ordinary process", "sha256:release", "-", "release-check"]
      (environment "cli-profile-evidence")
    expect (evidence.exitCode == 0)
      s!"Evidence Command Profile selection failed: {evidence.stderr}"
    let pendingNext ← runCliChild path #["next"]
    expect (pendingNext.exitCode == 0 &&
        pendingNext.stdout.contains
          "Command Profile release-check@0 (lake test)")
      "next did not name the exact Command Profile frozen by Evidence"
    let recorded ← runCliChild path
      #["record-evidence", "release-observation", "passed", "pass"]
      (environment "cli-profile-result")
    expect (recorded.exitCode == 0)
      s!"Command Profile Evidence result failed: {recorded.stderr}"
    let rejectedDeviation ← runCliChild path
      #["record-command-deviation", "release-check", "release-observation", "-",
        "Use another command.", "lake", "build"]
      (environment "cli-required-deviation")
    expect (rejectedDeviation.exitCode != 0 &&
        rejectedDeviation.stderr.contains
          "Only a recommended Command Profile")
      "public CLI allowed an agent-reasoned required-profile deviation"
    let kpt ← runCliChild path
      #["record-kpt", "review-context", "problem", "work",
        "A resumed reviewer retains implementation context.", "-"]
      (environment "cli-kpt")
    expect (kpt.exitCode == 0)
      s!"KPT CLI delegation failed: {kpt.stderr}"
    let proposedKPT ← runCliChild path
      #["propose-kpt", "review-context", "try", "work",
        "Reuse the reviewer context.", "review-context"]
      (environment "cli-kpt-proposal")
    expect (proposedKPT.exitCode == 0)
      s!"KPT proposal CLI delegation failed: {proposedKPT.stderr}"
    let atomic ← runCliChild path
      #["record-kpt-command-profile", "stable-check", "keep", "project",
        "The release check is stable.", "-", "diagnostic-check",
        "diagnose the release", "recommended", "-", "lake", "build"]
      (environment "cli-atomic-memory")
    expect (atomic.exitCode == 0)
      s!"atomic KPT and Command Profile CLI delegation failed: {atomic.stderr}"
    let design ← runCliChild path
      #["record-design", "learning-design", "decision", "none",
        "Accept reviewed learning with its KPT."]
      (environment "cli-learning-design")
    expect (design.exitCode == 0)
      s!"KPT acceptance Design recording failed: {design.stderr}"
    let requested ← runCliChild path
      #["request-design-review", "learning-design-review", "learning-design"]
      (environment "cli-learning-review")
    expect (requested.exitCode == 0)
      s!"KPT acceptance Design Review request failed: {requested.stderr}"
    let clean ← runCliChild path
      #["record-clean-review", "learning-design-review", "fresh-reviewer"]
      (environment "cli-learning-clean")
    expect (clean.exitCode == 0)
      s!"KPT acceptance clean Review failed: {clean.stderr}"
    let accepted ← runCliChild path
      #["accept-design-with-kpt", "learning-design",
        "Caller accepted the reviewed learning.", "learning-lesson", "keep",
        "project", "The reviewed learning is useful.", "-"]
      (environment "cli-learning-accept")
    expect (accepted.exitCode == 0)
      s!"Design acceptance with KPT CLI delegation failed: {accepted.stderr}"
    let status ← runCliChild path #["status"]
    expect (status.exitCode == 0 &&
        status.stdout.contains "Accepted Command Profiles:" &&
        status.stdout.contains
          "Command Profile: release-check@0" &&
        status.stdout.contains
          "A resumed reviewer retains implementation context." &&
        status.stdout.contains "Agent-authored KPT candidates:")
      "status lost durable Command Profile or KPT project language"

def run : IO Unit := do
  testParsing
  testRendering
  testJsonValidation
  testRepresentativeDelegation
  testProjectMemoryDelegation
  IO.println "cli tests: pass"

end AgentWorkbench.Tests.Cli
