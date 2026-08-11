import AgentWorkbench

namespace AgentWorkbenchTest

open AgentWorkbench

def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw (IO.userError message)

def expectError (result : Except String α) (message : String) : IO Unit :=
  match result with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError message)

def fromExcept : Except String α → IO α
  | .ok value => pure value
  | .error message => throw (IO.userError message)

def statement : Statement :=
  { id := "statement-1", text := "the artifact exists" }

def criterion : AcceptanceCriterion :=
  { id := "criterion-1", statementId := some statement.id
    statement := "the artifact observation succeeds"
    target := "file:artifact.txt", evidenceKind := "artifact" }

/-- Authored assurance judgment used by general route fixtures.  Assurance-specific tests keep a
separate literal oracle so this convenience constructor cannot validate its own implementation. -/
def fixtureAssuranceInput
    (statement : Statement) (claims : List LeanClaim)
    (criteria : List AcceptanceCriterion) (implementationRequired : Bool := true) :
    AssuranceContractInput :=
  let claimWitnesses := claims.map fun claim => {
    id := claim.id
    independenceClass := s!"fixture-independent-kernel:{claim.id}"
    producerBoundary := s!"claim:{claim.id}:pinned-kernel:{claim.input.toolchain}" }
  let criterionWitnesses := criteria.map fun criterion => {
    id := criterion.id
    independenceClass := s!"fixture-independent-observer:{criterion.id}"
    producerBoundary := s!"criterion:{criterion.id}:current-task-evidence-producer" }
  let witnesses := claimWitnesses ++ criterionWitnesses
  { statementId := statement.id
    witnesses := witnesses
    counterexamples := if implementationRequired then
      AssuranceFailureClass.all.map fun failureClass => {
        failureClass
        rejectedCondition := failureClass.rejectedCondition
        positiveProperty := statement.text
        witnessIds := witnesses.map (·.id) }
      else [] }

def sourceUnit : DesignSourceUnit :=
  { id := "unit-1", target := "file:.agent-workbench/design/product/design.md"
    path := ".agent-workbench/design/product/design.md", kind := .paragraph
    text := statement.text, digest := "blake3:unit" }

def design : DesignRevision :=
  let base : DesignRevision :=
  { id := "design-1", workId := some "work-1", status := .accepted
    producerAgentRun := "designer-1", changeRationale := "initial accepted Design"
    revisionContentDigest := "blake3:design", sourceArchiveAvailable := true
    sourceDocuments := [{ target := sourceUnit.target, snapshot := "blake3:source" }]
    sourceUnits := [sourceUnit]
    sourceUnitDispositions := [{ unitId := sourceUnit.id, role := .requirement }]
    statements := [statement]
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := [sourceUnit.id]
      leanClaims := { noSelectionReason := some "no logical Claim is needed for this fixture" }
      acceptanceCriteria := { selectedIds := [criterion.id] }
      implementationRequired := true }]
    acceptanceCriteria := [criterion]
    assuranceSchemaVersion := 1 }
  { base with assuranceContracts := base.derivedAssuranceContracts }

def withCurrentAssurance (value : DesignRevision) : DesignRevision :=
  let base := { value with assuranceSchemaVersion := 1, assuranceContracts := [] }
  { base with assuranceContracts := base.derivedAssuranceContracts }

def work : Work :=
  { id := "work-1", outcome := "produce the accepted artifact", scope := "project"
    baselineDesignRevision := none, designRevision := some design.id, status := .active
    responsibleAgentRun := "agent-1" }

def planUnit : DesignSourceUnit :=
  { id := "plan-unit-1", target := "file:.agent-workbench/design/plans/work-1/plan.md"
    path := ".agent-workbench/design/plans/work-1/plan.md", kind := .paragraph
    text := "create the artifact", digest := "blake3:plan-unit" }

def step : PlanStep :=
  { id := "step-1", description := "create the artifact"
    outputScopes := [criterion.target], verificationCriterionIds := [criterion.id] }

def plan : ImplementationPlan :=
  { id := "plan-1", workId := work.id, designRevision := design.id, status := .current
    producerAgentRun := "planner-1", reason := "implement the complete baseline delta"
    contentDigest := "blake3:plan", sourceArchiveAvailable := true
    sourceDocuments := [{ target := planUnit.target, digest := "blake3:plan-source" }]
    sourceUnits := [planUnit]
    sourceUnitDispositions := [{ unitId := planUnit.id, stepId := some step.id }]
    statementDispositions := [{
      statementId := statement.id
      statementText := statement.text
      deltaKind := .added
      stepIds := [step.id] }]
    steps := [step] }

def taskEntry (closed : Bool := false) : LedgerEntry :=
  { id := (if closed then "task-closed" else "task-open")
    order := (if closed then 3 else 1), scope := work.scope
    workId := some work.id, designRevision := some design.id
    supersedes := if closed then ["task-open"] else []
    payload := .task {
      planId := some plan.id, planStepId := some step.id
      lineageId := some s!"{work.id}:{step.id}"
      outputScopes := step.outputScopes
      verificationCriterionIds := step.verificationCriterionIds
      materializedAtOrder := 1, description := step.description
      required := true, closed } }

def baseState : ProjectState :=
  { revision := 3, acceptedDesignId := some design.id, focusedWorkId := some work.id
    designRevisions := [design], works := [work], implementationPlans := [plan]
    ledgerEntries := [taskEntry] }

def evidenceEntry : LedgerEntry :=
  { id := "evidence-1", order := 2, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .artifactObservation {
      taskEntryId := some "task-open", outputScope := some criterion.target
      criterionId := criterion.id, target := criterion.target, snapshot := "blake3:artifact"
      operation := "inspect artifact", result := "artifact exists", successful := true
      producerAgentRun := work.responsibleAgentRun
      assuranceBinding := some <| design.assuranceBindingForCriterion
        work.responsibleAgentRun criterion.id } }

def evidencedState : ProjectState :=
  { baseState with revision := 4, ledgerEntries := [taskEntry, evidenceEntry] }

end AgentWorkbenchTest
