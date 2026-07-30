import AgentWorkbench.Domain.Evidence

namespace AgentWorkbench.Tests.Domain

open AgentWorkbench
open AgentWorkbench.Domain

def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do
    throw <| IO.userError message

def source (id : String) (kind : SourceKind := .caller) : Source :=
  { id := ⟨id⟩, kind, description := id }

def decision (id reason : String) : CallerDecision :=
  { source := source id, reason }

def workRef : WorkRef := { key := "work", version := 0 }

def taskRef : TaskRef := { key := "task", version := 0 }

def work : Work.Unit :=
  { ref := workRef
    outcome := "deliver the selected change"
    completionBoundary :=
      [{ target := .taskSatisfied taskRef
         basis := .workBoundary workRef }]
    authority := decision "start" "Start the selected outcome." }

def task : Work.Task :=
  { ref := taskRef
    work := workRef
    description := "implement the selected change"
    basis := .workBoundary workRef
    designScope := []
    phase := none
    state := .pending }

def testAuthorityAndClassification : IO Unit := do
  let callerSource := source "caller-message"
  let agentSource := source "agent-proposal" .agent
  let accepted : Design.Item :=
    { ref := { key := "language", version := 0 }
      predecessor := none
      statement := "Use Lean for the selected state rules."
      role := .decision
      source := callerSource
      dependencies := []
      assurance := { kind := .none, obligations := [] }
      authority := .acceptedByCaller (decision "caller-message" "Selected by caller.") }
  let proposal : Design.Item :=
    { ref := { key := "cache", version := 0 }
      predecessor := none
      statement := "Add a cache."
      role := .decision
      source := agentSource
      dependencies := []
      assurance := { kind := .none, obligations := [] }
      addsComplexity := true
      authority := .unaccepted }
  let instruction : Design.OperatingInstruction :=
    { source := callerSource
      statement := "Do not add unselected complexity."
      authority := decision "caller-message" "Binding caller instruction." }
  let rejectionEffect : Design.Effect :=
    { source := callerSource
      content := .nonAuthoritative
        { kind := .rejection
          statement := "The cache is not selected."
          target := some "cache" } }
  let package : Design.Package :=
    { effects :=
        [({ source := callerSource, content := .design accepted } : Design.Effect),
         ({ source := agentSource, content := .design proposal } : Design.Effect),
         ({ source := callerSource, content := .instruction instruction } : Design.Effect),
         rejectionEffect] }
  expect package.wellFormed "classified package is not well formed"
  expect (package.designItems.length == 2)
    "design effects were not retained independently"
  expect (accepted.acceptedRef?.isSome)
    "caller-accepted design did not expose an accepted reference"
  expect proposal.acceptedRef?.isNone
    "agent proposal acquired caller authority"
  expect (package.instructions == [instruction])
    "caller instruction was not retained as its own effect"

def testWorkFactsAndOptionalPhase : IO Unit := do
  expect work.wellFormed "Work facts are not well formed"
  expect task.wellFormed "Task facts are not well formed"
  expect task.phase.isNone "a flat task unexpectedly requires a Phase"
  expect (task.designScope.isEmpty && task.state == .pending)
    "task facts do not preserve their selected scope and satisfaction"
  let invalid := { task with description := "" }
  expect (!invalid.wellFormed)
    "a task without a project-language description was accepted"

def testEvidenceFacts : IO Unit := do
  let spec : Evidence.Spec :=
    { ref := { key := "latency", version := 0 }
      observation := "The command completes within 100 ms."
      method := "measure one selected invocation"
      environment := "release build on the supported host"
      inputs := ["command=check"]
      acceptanceCondition := "elapsed <= 100 ms"
      trustedBoundary := "monotonic host clock"
      artifactIdentity := "sha256:release"
      basis := .workBoundary workRef }
  let result : Evidence.Result :=
    { spec, observedValue := "42 ms", passed := true }
  expect spec.wellFormed "Evidence specification lost required observation facts"
  expect result.wellFormed "Evidence result lost the selected observed value"

def testComplexityDecisionFacts : IO Unit := do
  let rationale : Design.ComplexityRationale :=
    { necessity := "The accepted lookup volume requires bounded indexing."
      simplerAlternativeInsufficient :=
        "The measured direct lookup exceeds the accepted latency."
      boundedScope := "One index for the selected lookup."
      maintenanceCost := "Maintain the index with that lookup." }
  let selectedSource := source "complexity-proposal" .agent
  let item : Design.Item :=
    { ref := { key := "lookup-index", version := 0 }
      predecessor := none
      statement := "Use one bounded lookup index."
      role := .decision
      source := selectedSource
      dependencies := []
      assurance := { kind := .none, obligations := [] }
      addsComplexity := true
      complexityRationale := some rationale
      authority :=
        .acceptedByCaller
          (decision "complexity-decision" "Caller adopted the bounded index.") }
  expect item.wellFormed
    "complexity adoption did not retain its four caller-owned facts"

def testDesignPackageRoles : IO Unit := do
  let shared := source "design-package"
  let item (key statement : String) (role : Design.Role)
      (assurance : Design.AssuranceSelection) : Design.Item :=
    { ref := { key, version := 0 }
      predecessor := none
      statement
      role
      source := shared
      dependencies := []
      assurance
      authority := .unaccepted }
  let layout :=
    item "layout" "Contracts remain under Inventory." .projectStructure
      { kind := .none, obligations := [] }
  let invariant :=
    item "inventory" "Reservations do not exceed stock."
      .functionalRequirement
      { kind := .formal
        obligations :=
          [{ key := "inventory"
             method := .formal
             description := "Prove the selected inventory rule." }] }
  let latency :=
    item "latency" "The selected command completes within 100 ms."
      .nonFunctionalRequirement
      { kind := .evidence
        obligations :=
          [{ key := "latency"
             method := .evidence
             description := "Observe command latency." }] }
  let question : Design.Effect :=
    { source := shared
      content := .nonAuthoritative
        { kind := .question
          statement := "Which deployment host supplies measurements?" } }
  let package : Design.Package :=
    { effects :=
        [layout, invariant, latency].map
          (fun selected =>
            { source := shared
              content := Design.EffectContent.design selected }) ++
          [question] }
  expect package.wellFormed
    "Design Package did not retain structure, formal, Evidence, and question roles"
  expect (package.designItems.map (·.role) ==
      [.projectStructure, .functionalRequirement, .nonFunctionalRequirement])
    "Design Package roles were collapsed"

def testCommandProfileAndKPTFacts : IO Unit := do
  let acceptedDecision :=
    decision "command-profile" "Caller selected the exact argv."
  let profile : CommandProfile.Profile :=
    { ref := { key := "check", version := 0 }
      predecessor := none
      purpose := "verify the selected implementation"
      scope := .project
      argv := ["lake", "test"]
      cwd := some "."
      disposition := .required
      source := acceptedDecision.source
      authority := .acceptedByCaller acceptedDecision }
  expect profile.wellFormed
    "structured accepted Command Profile is not well formed"
  expect (!({ profile with argv := [] }).wellFormed)
    "a Command Profile without argv was accepted"
  expect (!({ profile with cwd := some "../outside" }).wellFormed)
    "a Command Profile cwd escaped the project boundary"
  let proposed :=
    { profile with
      ref := { key := "proposal", version := 0 }
      source := source "profile-agent" .agent
      authority := .proposed }
  expect proposed.wellFormed
    "non-authoritative repository or agent profile could not be represented"
  expect
    (!({ proposed with source := source "profile-caller" .caller }).wellFormed)
    "a caller profile silently became a non-authoritative proposal"
  let entry : KPT.Entry :=
    { ref := { key := "fresh-review", version := 0 }
      predecessor := none
      category := .keep
      scope := .work workRef.key
      statement := "Use a context-free reviewer for a fresh Review."
      source := acceptedDecision.source
      relation := some "review-boundary"
      authority := .callerOwned acceptedDecision }
  expect entry.wellFormed "caller-owned KPT is not well formed"
  expect (!({ entry with statement := "" }).wellFormed)
    "an empty KPT statement was accepted"

def run : IO Unit := do
  testAuthorityAndClassification
  testWorkFactsAndOptionalPhase
  testEvidenceFacts
  testComplexityDecisionFacts
  testDesignPackageRoles
  testCommandProfileAndKPTFacts
  IO.println "domain tests: pass"

end AgentWorkbench.Tests.Domain
