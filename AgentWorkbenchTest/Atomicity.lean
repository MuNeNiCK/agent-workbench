import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.Store
import AgentWorkbench.Adapter.SQLite
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.PlanSource

namespace AgentWorkbenchTest.Atomicity

open AgentWorkbench AgentWorkbenchTest

private def expectMutationFailure
    (root database : System.FilePath) (mutation : Mutation) : IO Unit := do
  let failed ← try
      let _ ← Store.executeMutation root database mutation
      pure false
    catch _ => pure true
  expect failed "injected SQLite write fault did not reject the mutation"

private def installFault
    (connection : AgentWorkbench.SQLite.Connection) (table event : String) : IO Unit := do
  AgentWorkbench.SQLite.runScript connection s!"
    DROP TRIGGER IF EXISTS injected_write_fault;
    CREATE TRIGGER injected_write_fault BEFORE {event} ON {table}
    BEGIN SELECT RAISE(ABORT, 'injected write fault'); END;"

private def clearFault (connection : AgentWorkbench.SQLite.Connection) : IO Unit :=
  AgentWorkbench.SQLite.runScript connection "DROP TRIGGER IF EXISTS injected_write_fault;"

private def installAfterPlanDemotionFault
    (connection : AgentWorkbench.SQLite.Connection) : IO Unit :=
  AgentWorkbench.SQLite.runScript connection "
    DROP TRIGGER IF EXISTS injected_write_fault;
    CREATE TRIGGER injected_write_fault
    AFTER UPDATE ON implementation_plans
    WHEN OLD.status = 'current' AND NEW.status <> 'current'
    BEGIN SELECT RAISE(ABORT, 'injected write fault after Plan demotion'); END;"

private def designProposal
    (target : String) (units : List DesignSourceUnit) : DesignProposalRequest :=
  let statement : Statement := {
    id := "statement-atomic-design", text := "Design source persistence is atomic." }
  let criterion : AcceptanceCriterion := {
    id := "criterion-atomic-design", statementId := some statement.id
    statement := "the atomic artifact is observable"
    target := "file:atomic-artifact.txt", evidenceKind := "artifact" }
  { producerAgentRun := "agent-a"
    changeRationale := "exercise every Design archive write boundary"
    sourceDocumentTargets := [target]
    sourceUnitDispositions := units.map fun unit =>
      { unitId := unit.id, role := .requirement }
    statements := [statement]
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := units.map (·.id)
      leanClaims := { noSelectionReason := some "this storage property is externally observed" }
      acceptanceCriteria := { selectedIds := [criterion.id] }
      implementationRequired := true }]
    acceptanceCriteria := [criterion] }

private def planProposal
    (target : String) (units : List DesignSourceUnit)
    (predecessor : Option String := none) : PlanProposalRequest :=
  let step : PlanStep := {
    id := "atomic-step", description := "produce the atomic artifact"
    outputScopes := ["file:atomic-artifact.txt"]
    verificationCriterionIds := ["criterion-atomic-design"] }
  { predecessorPlanId := predecessor
    producerAgentRun := "agent-a"
    reason := "exercise every Plan and Task transaction boundary"
    sourceDocumentTargets := [target]
    sourceUnitDispositions := units.map fun unit =>
      { unitId := unit.id, stepId := some step.id }
    statementDispositions := [{
      statementId := "statement-atomic-design"
      statementText := "Design source persistence is atomic."
      deltaKind := .added, stepIds := [step.id] }]
    steps := [step] }

private def designResult (result : MutationResult) : IO DesignRevision :=
  match result with
  | .design value => pure value
  | _ => throw (IO.userError "Design proposal returned the wrong result")

private def planResult (result : MutationResult) : IO ImplementationPlan :=
  match result with
  | .plan value => pure value
  | _ => throw (IO.userError "Plan proposal returned the wrong result")

def run : IO Unit := do
  IO.FS.withTempDir fun queryRoot => do
    let missing := queryRoot / "missing" / "state.db"
    let rejected ← try
        let _ ← Store.openReadOnly missing
        pure false
      catch _ => pure true
    expect (rejected && !(← missing.pathExists) && !(← (queryRoot / "missing").pathExists))
      "read-only query capability created a missing project or database"
  IO.FS.withTempDir fun missingRuntimeRoot => do
    let missingDatabase := missingRuntimeRoot / ".agent-workbench" / "state.db"
    expectMutationFailure missingRuntimeRoot missingDatabase .init
    expect (!(← missingDatabase.pathExists))
      "failed runtime initialization created authoritative project state"
  IO.FS.withTempDir fun root => do
    let workbenchRoot := root / ".agent-workbench"
    let database := workbenchRoot / "state.db"
    IO.FS.createDirAll workbenchRoot
    let _ ← Store.open database
    let start : WorkStartRequest :=
      { id := "work-atomic", outcome := "preserve one atomic authority"
        scope := "project", responsibleAgentRun := "agent-a" }
    let _ ← Store.executeMutation root database (.workStart start)
    let beforeRead ← IO.FS.readBinFile database
    let _ ← Store.loadState (← Store.openReadOnly database)
    let afterRead ← IO.FS.readBinFile database
    expect (beforeRead == afterRead)
      "read-only authoritative query changed SQLite bytes"
    let baseline ← Store.loadState (← Store.openReadOnly database)
    let handoff := Mutation.workHandoff
      "work-atomic" "handoff-atomic" "agent-b" "continue the same Work"
    let connection ← AgentWorkbench.SQLite.open database
    for (table, event) in
        [("works", "INSERT"), ("ledger_entries", "INSERT"), ("project_metadata", "UPDATE")] do
      installFault connection table event
      expectMutationFailure root database handoff
      clearFault connection
      let after ← Store.loadState (← Store.openReadOnly database)
      expect (after == baseline)
        s!"SQLite fault at {table} left a partial Work, history, selector, or revision update"

    let sourceRoot := workbenchRoot / "design" / "product"
    IO.FS.createDirAll sourceRoot
    IO.FS.createDirAll (workbenchRoot / "design" / "implementation")
    let target := "file:.agent-workbench/design/product/atomic.md"
    IO.FS.writeFile (sourceRoot / "atomic.md") "Design source persistence is atomic.\n"
    let units := (← AgentWorkbench.DesignSource.inspectAll root [target]).flatMap (·.units)
    let proposal := Mutation.designPropose (designProposal target units)
    for (table, event) in
        [("design_revisions", "INSERT"), ("design_sources", "INSERT"),
         ("project_metadata", "UPDATE")] do
      installFault connection table event
      expectMutationFailure root database proposal
      clearFault connection
      let after ← Store.loadState (← Store.openReadOnly database)
      let storedSources ← AgentWorkbench.SQLite.queryScalar connection
        "SELECT CAST(COUNT(*) AS TEXT) FROM design_sources" #[]
      expect (after == baseline && storedSources == "0")
        s!"SQLite fault at {table} left a partial Design, source BLOB, or revision update"

    let candidate ← designResult (← Store.executeMutation root database proposal)
    let _ ← Store.executeMutation root database (.designAccept candidate.id)
    let planRoot := workbenchRoot / "design" / "plans" / "work-atomic"
    IO.FS.createDirAll planRoot
    let planTarget := "file:.agent-workbench/design/plans/work-atomic/plan.md"
    IO.FS.writeFile (planRoot / "plan.md") "Produce the atomic artifact.\n"
    let planUnits := (← AgentWorkbench.PlanSource.inspectAll
      root "work-atomic" [planTarget]).flatMap (·.units)
    let planRequest := planProposal planTarget planUnits
    let beforePlan ← Store.loadState (← Store.openReadOnly database)
    for (table, event) in
        [("implementation_plans", "INSERT"),
         ("implementation_plan_sources", "INSERT"),
         ("project_metadata", "UPDATE")] do
      installFault connection table event
      expectMutationFailure root database (.planPropose planRequest)
      clearFault connection
      let after ← Store.loadState (← Store.openReadOnly database)
      let storedSources ← AgentWorkbench.SQLite.queryScalar connection
        "SELECT CAST(COUNT(*) AS TEXT) FROM implementation_plan_sources" #[]
      expect (after == beforePlan && storedSources == "0")
        s!"SQLite fault at {table} left a partial Plan, source BLOB, or revision update"

    let candidatePlan ← planResult (← Store.executeMutation root database (.planPropose planRequest))
    let beforeMaterialize ← Store.loadState (← Store.openReadOnly database)
    for (table, event) in
        [("implementation_plans", "UPDATE"), ("works", "INSERT"),
         ("ledger_entries", "INSERT"), ("project_metadata", "UPDATE")] do
      installFault connection table event
      expectMutationFailure root database (.planMaterialize candidatePlan.id)
      clearFault connection
      let after ← Store.loadState (← Store.openReadOnly database)
      expect (after == beforeMaterialize)
        s!"SQLite fault at {table} left a partial current Plan or Task graph"

    let _ ← Store.executeMutation root database (.planMaterialize candidatePlan.id)
    let mut currentPlanId := candidatePlan.id
    for _ in List.range 8 do
      let replacement ← planResult (← Store.executeMutation root database
        (.planReplace (planProposal planTarget planUnits (some currentPlanId))))
      let _ ← Store.executeMutation root database (.planMaterialize replacement.id)
      currentPlanId := replacement.id
    expect (currentPlanId == "plan-9")
      "Plan fixture did not reach the Plan 9 side of the lexicographic boundary"
    let plan10 ← planResult (← Store.executeMutation root database
      (.planReplace (planProposal planTarget planUnits (some currentPlanId))))
    expect (plan10.id == "plan-10")
      "Plan fixture did not create the lexicographically earlier successor"
    let beforePlan10Materialize ← Store.loadState (← Store.openReadOnly database)
    installAfterPlanDemotionFault connection
    expectMutationFailure root database (.planMaterialize plan10.id)
    clearFault connection
    let afterPlan10Failure ← Store.loadState (← Store.openReadOnly database)
    expect (afterPlan10Failure == beforePlan10Materialize)
      "fault after Plan 9 demotion did not roll the complete transaction back"
    let _ ← Store.executeMutation root database (.planMaterialize plan10.id)
    currentPlanId := plan10.id
    let taskId := s!"task-{currentPlanId}-atomic-step"
    IO.FS.writeFile (root / "atomic-artifact.txt") "atomic artifact\n"
    let _ ← Store.executeMutation root database (.artifactObserve {
      entryId := "atomic-evidence", taskEntryId := taskId
      criterionId := "criterion-atomic-design", operation := "inspect atomic artifact"
      result := "artifact exists", successful := true })
    let beforeClose ← Store.loadState (← Store.openReadOnly database)
    let closeRequest : TaskCloseRequest := {
      entryId := "atomic-task-closed", taskEntryId := taskId }
    for (table, event) in
        [("works", "INSERT"), ("ledger_entries", "INSERT"),
         ("project_metadata", "UPDATE")] do
      installFault connection table event
      expectMutationFailure root database (.taskClose closeRequest)
      clearFault connection
      let after ← Store.loadState (← Store.openReadOnly database)
      expect (after == beforeClose)
        s!"SQLite fault at {table} left a partially closed Task"

    let _ ← Store.executeMutation root database (.taskClose closeRequest)
    let beforeCompletion ← Store.loadState (← Store.openReadOnly database)
    for (table, event) in
        [("works", "INSERT"), ("ledger_entries", "INSERT"),
         ("project_metadata", "UPDATE")] do
      installFault connection table event
      expectMutationFailure root database .workComplete
      clearFault connection
      let after ← Store.loadState (← Store.openReadOnly database)
      expect (after == beforeCompletion)
        s!"SQLite fault at {table} left a completion status without complete authority"

    let _ ← Store.executeMutation root database .workComplete
    let completed ← Store.loadState (← Store.openReadOnly database)
    expect ((completed.work? "work-atomic").any (·.status == .completed) &&
      completed.ledgerEntries.any fun entry => match entry.payload with
        | .workCompletion value => value.workId == "work-atomic"
        | _ => false)
      "successful completion did not commit status and authority together"

end AgentWorkbenchTest.Atomicity
