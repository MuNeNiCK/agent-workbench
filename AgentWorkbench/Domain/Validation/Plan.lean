import AgentWorkbench.Domain.Validation.Design

namespace AgentWorkbench
namespace Validation

def validatePlan
    (state : ProjectState) (plan : ImplementationPlan) : Except String Unit := do
  let work ← requireSome (state.work? plan.workId)
    s!"Plan {plan.id} references missing Work {plan.workId}"
  let design ← requireSome (state.design? plan.designRevision)
    s!"Plan {plan.id} references missing Design {plan.designRevision}"
  let designBelongsToWork := work.designRevision.any fun current =>
    current == design.id || state.designDescendsFrom design.id current
  ensure (designBelongsToWork && !plan.id.isEmpty &&
    !plan.producerAgentRun.isEmpty && !plan.reason.isEmpty &&
    validContentDigest plan.contentDigest && plan.sourceArchiveAvailable &&
    !plan.sourceDocuments.isEmpty)
    s!"Plan {plan.id} has incomplete immutable identity"
  ensure (uniqueStrings (plan.sourceDocuments.map (·.target)) &&
    uniqueStrings (plan.sourceUnits.map (·.id)) &&
    uniqueStrings (plan.sourceUnitDispositions.map (·.unitId)) &&
    plan.sourceUnits.length == plan.sourceUnitDispositions.length)
    s!"Plan {plan.id} has incomplete source-unit classification"
  ensure (uniqueStrings (plan.steps.map (·.id)) &&
    uniqueStrings (plan.statementDispositions.map (·.statementId)))
    s!"Plan {plan.id} has duplicate step or Statement disposition IDs"
  let acceptedFindingIds := acceptedImplementationFindingIds state work.id design.id
  for disposition in plan.sourceUnitDispositions do
    let _ ← requireSome (uniqueBy? plan.sourceUnits (·.id) disposition.unitId)
      s!"Plan {plan.id} classifies an unknown source unit {disposition.unitId}"
    ensure (disposition.stepId.isSome != disposition.noStepReason.isSome &&
      (disposition.stepId.isSome ||
        disposition.noStepReason.any fun reason => !reason.isEmpty))
      s!"Plan source unit {disposition.unitId} has no explicit step choice"
    if let some stepId := disposition.stepId then
      let _ ← requireSome (uniqueBy? plan.steps (·.id) stepId)
        s!"Plan source unit {disposition.unitId} references missing step {stepId}"
  for step in plan.steps do
    ensure (!step.id.isEmpty && !step.description.isEmpty &&
      !step.outputScopes.isEmpty && uniqueStrings step.outputScopes &&
      !step.verificationCriterionIds.isEmpty &&
      uniqueStrings step.dependsOnStepIds && uniqueStrings step.requiredClaimIds &&
      uniqueStrings step.verificationCriterionIds &&
      uniqueStrings step.acceptedFindingEntryIds)
      s!"Plan step {step.id} is incomplete"
    ensure (plan.sourceUnitDispositions.any (·.stepId == some step.id))
      s!"Plan step {step.id} is not grounded in its Markdown source"
    for dependency in step.dependsOnStepIds do
      let _ ← requireSome (uniqueBy? plan.steps (·.id) dependency)
        s!"Plan step {step.id} has missing dependency {dependency}"
      ensure (dependency != step.id) s!"Plan step {step.id} depends on itself"
    let rec reaches : Nat → String → Bool
      | 0, _ => false
      | fuel + 1, current =>
          match uniqueBy? plan.steps (·.id) current with
          | none => false
          | some candidate => candidate.dependsOnStepIds.any fun dependency =>
              dependency == step.id || reaches fuel dependency
    ensure (!step.dependsOnStepIds.any (reaches (plan.steps.length + 1)))
      s!"Plan step {step.id} participates in a dependency cycle"
    for claimId in step.requiredClaimIds do
      let _ ← requireSome (design.claim? claimId)
        s!"Plan step {step.id} references missing Claim {claimId}"
    for criterionId in step.verificationCriterionIds do
      let _ ← requireSome (design.criterion? criterionId)
        s!"Plan step {step.id} references missing Criterion {criterionId}"
    for findingId in step.acceptedFindingEntryIds do
      ensure (acceptedFindingIds.contains findingId)
        s!"Plan step {step.id} references a Finding outside the accepted Implementation Review"
  let expected ← expectedStatementDeltas state work design
  ensure (expected.length == plan.statementDispositions.length)
    s!"Plan {plan.id} does not cover the complete Work baseline delta"
  for delta in expected do
    let disposition ← requireSome
      (uniqueBy? plan.statementDispositions (·.statementId) delta.statementId)
      s!"Plan {plan.id} omits Statement delta {delta.statementId}"
    ensure (disposition.statementText == delta.statementText &&
      disposition.deltaKind == delta.kind)
      s!"Plan {plan.id} changes Statement delta identity {delta.statementId}"
    if delta.implementationRequired then
      ensure (!disposition.stepIds.isEmpty && disposition.noActionReason.isNone &&
        uniqueStrings disposition.stepIds)
        s!"Plan {plan.id} omits required implementation for {delta.statementId}"
      for stepId in disposition.stepIds do
        let _ ← requireSome (uniqueBy? plan.steps (·.id) stepId)
          s!"Plan delta {delta.statementId} references missing step {stepId}"
    else
      ensure (disposition.stepIds.isEmpty && disposition.noActionReason == delta.noActionReason)
        s!"Plan {plan.id} changes the accepted no-action choice for {delta.statementId}"
  for step in plan.steps do
    let coversStatementDelta := plan.statementDispositions.any fun disposition =>
      disposition.stepIds.contains step.id
    let coversAcceptedFinding := step.acceptedFindingEntryIds.any acceptedFindingIds.contains
    ensure (coversStatementDelta || coversAcceptedFinding)
      s!"Plan step {step.id} covers no Statement delta or accepted Implementation Finding"
  if let some predecessorId := plan.predecessorPlanId then
    let predecessor ← requireSome (state.plan? predecessorId)
      s!"Plan {plan.id} references missing predecessor {predecessorId}"
    ensure (predecessor.workId == plan.workId &&
      (if plan.status == .current then predecessor.status == .superseded
       else predecessor.status == .superseded || predecessor.status == .current))
      s!"Plan {plan.id} has invalid predecessor"
  ensure (uniqueStrings plan.changeBasisEntryIds &&
    plan.changeBasisEntryIds.all fun basis =>
      match state.entry? basis with
      | some entry => entry.workId == some work.id
      | none => false)
    s!"Plan {plan.id} has invalid change bases"



end Validation
end AgentWorkbench
