import AgentWorkbench.Domain.Validation.Ledger
import AgentWorkbench.Domain.Validation.Plan

namespace AgentWorkbench

open Validation

def authorityIdsUnique (state : ProjectState) : Bool :=
  uniqueStrings (state.designRevisions.map (·.id)) &&
  uniqueStrings (state.works.map (·.id)) &&
  uniqueStrings (state.implementationPlans.map (·.id)) &&
  uniqueStrings (state.ledgerEntries.map (·.id))

def ledgerOrderUnique (state : ProjectState) : Bool :=
  let orders := state.ledgerEntries.map (·.order)
  orders.all (fun order => orders.count order == 1)

def acceptedDesignSelectorValid (state : ProjectState) : Bool :=
  let accepted := state.designRevisions.filter (·.status == .accepted)
  match state.acceptedDesignId with
  | none => accepted.isEmpty
  | some id => accepted.length == 1 && accepted.head?.map (·.id) == some id

def focusedWorkSelectorValid (state : ProjectState) : Bool :=
  match state.focusedWorkId with
  | none => true
  | some id => (state.work? id).any (·.status == .active)

def currentPlanAuthorityUnique (state : ProjectState) : Bool :=
  state.works.all fun work =>
    (state.implementationPlans.filter fun plan =>
      plan.workId == work.id && plan.status == .current).length <= 1

def workDesignBindingsExist (state : ProjectState) : Bool :=
  state.works.all fun work =>
    work.designRevision.all (state.design? · |>.isSome) &&
    work.baselineDesignRevision.all (state.design? · |>.isSome)

structure NamedProjectInvariants (state : ProjectState) : Prop where
  authorityIds : authorityIdsUnique state = true
  ledgerOrder : ledgerOrderUnique state = true
  acceptedDesignSelector : acceptedDesignSelectorValid state = true
  focusedWorkSelector : focusedWorkSelectorValid state = true
  currentPlanAuthority : currentPlanAuthorityUnique state = true
  workDesignBindings : workDesignBindingsExist state = true

def namedInvariantCheck (state : ProjectState) : Bool :=
  authorityIdsUnique state && ledgerOrderUnique state &&
  acceptedDesignSelectorValid state && focusedWorkSelectorValid state &&
  currentPlanAuthorityUnique state && workDesignBindingsExist state

def validateNamedProjectInvariants (state : ProjectState) : Except String Unit :=
  if namedInvariantCheck state then .ok () else .error "named project invariant failed"

theorem namedProjectInvariants_of_validation
    (state : ProjectState) (success : validateNamedProjectInvariants state = .ok ()) :
    NamedProjectInvariants state := by
  simp [validateNamedProjectInvariants, namedInvariantCheck] at success
  constructor <;> simp_all

def validateDesignHistoryInvariant (state : ProjectState) : Except String Unit := do
  for design in state.designRevisions do
    validateDesign design
    validateDesignRelations state design

def validateWorkLifecycleInvariant (state : ProjectState) : Except String Unit := do
  let accepted := state.designRevisions.filter (·.status == .accepted)
  match state.acceptedDesignId with
  | none => ensure accepted.isEmpty "accepted design exists without accepted selector"
  | some id =>
      ensure (accepted.length == 1 && accepted.head?.map (·.id) == some id)
        "accepted selector does not identify the unique accepted design"
  match state.focusedWorkId with
  | none => pure ()
  | some id =>
      let focused ← requireSome (state.work? id) "focused selector identifies no Work"
      ensure (focused.status == .active) "focused selector identifies a non-active Work"
  for work in state.works do
    if let some designId := work.designRevision then
      let _ ← requireSome (state.design? designId) s!"work {work.id} references missing design"
    if let some baselineId := work.baselineDesignRevision then
      let _ ← requireSome (state.design? baselineId) s!"work {work.id} references missing baseline"
    ensure (!work.id.isEmpty && !work.outcome.isEmpty && !work.scope.isEmpty &&
      !work.responsibleAgentRun.isEmpty)
      s!"work {work.id} is incomplete"
    let completions := state.ledgerEntries.filter fun entry =>
      entry.workId == some work.id && match entry.payload with
      | .workCompletion value => value.workId == work.id
      | .workResume _ => false
      | _ => false
    let prospectivelyInvalidated := completions.length == 1 && completions.any fun completion =>
      state.ledgerEntries.any fun dispositionEntry =>
        dispositionEntry.order > completion.order &&
          dispositionEntry.workId == some work.id && match dispositionEntry.payload with
          | .reviewDisposition disposition =>
              disposition.decision == .accepted &&
                disposition.impact == .implementationDefect &&
                (state.entry? disposition.findingEntryId).any fun findingEntry =>
                  findingEntry.order > completion.order && findingEntry.workId == some work.id
          | _ => false
    if work.status == .completed then
      let legacyUnavailable := work.designRevision.any fun designId =>
        (state.design? designId).any fun design => !design.sourceArchiveAvailable
      ensure (completions.length == 1 || (completions.isEmpty && legacyUnavailable))
        s!"completed Work {work.id} does not have exactly one completion authority"
    else
      ensure (completions.isEmpty ||
        ((work.status == .suspended || work.status == .active) && prospectivelyInvalidated))
        s!"non-completed Work {work.id} has completion authority"

def validatePlanTaskInvariant (state : ProjectState) : Except String Unit := do
  for plan in state.implementationPlans do validatePlan state plan
  for work in state.works do
    ensure ((state.implementationPlans.filter fun plan =>
      plan.workId == work.id && plan.status == .current).length <= 1)
      s!"Work {work.id} has multiple current Plans"
  for work in state.works do
    if let some plan := state.currentPlanFor? work.id then
      let currentTasks := state.ledgerEntries.filter fun entry =>
        entry.workId == some work.id &&
        !(state.ledgerEntries.any fun successor => successor.supersedes.contains entry.id) &&
        match entry.payload with
        | .task task => !task.retired && task.planId == some plan.id
        | _ => false
      ensure (currentTasks.length == plan.steps.length)
        s!"Work {work.id} current Task graph differs from current Plan {plan.id}"
      for step in plan.steps do
        let lineage := s!"{work.id}:{step.id}"
        ensure ((currentTasks.countP fun entry => match entry.payload with
          | .task task => task.planId == some plan.id && task.lineageId == some lineage
          | _ => false) == 1)
          s!"Plan step {step.id} does not have exactly one current Task"

def validateLedgerAuthorityInvariant (state : ProjectState) : Except String Unit := do
  for entry in state.ledgerEntries do validateEntry state entry

def validateState (state : ProjectState) : Except String Unit :=
  match validateNamedProjectInvariants state with
  | .error message => .error message
  | .ok _ => match validateDesignHistoryInvariant state with
    | .error message => .error message
    | .ok _ => match validateWorkLifecycleInvariant state with
      | .error message => .error message
      | .ok _ => match validatePlanTaskInvariant state with
        | .error message => .error message
        | .ok _ => validateLedgerAuthorityInvariant state

structure ValidProjectState (state : ProjectState) : Prop where
  named : NamedProjectInvariants state
  designHistory : validateDesignHistoryInvariant state = .ok ()
  workLifecycle : validateWorkLifecycleInvariant state = .ok ()
  planTask : validatePlanTaskInvariant state = .ok ()
  ledgerAuthority : validateLedgerAuthorityInvariant state = .ok ()

theorem validProjectState_of_validation
    (state : ProjectState) (success : validateState state = .ok ()) :
    ValidProjectState state := by
  unfold validateState at success
  cases namedCheck : validateNamedProjectInvariants state with
  | error message => simp [namedCheck] at success
  | ok namedValue =>
      cases namedValue
      cases designCheck : validateDesignHistoryInvariant state with
      | error message => simp [namedCheck, designCheck] at success
      | ok designValue =>
          cases designValue
          cases workCheck : validateWorkLifecycleInvariant state with
          | error message => simp [namedCheck, designCheck, workCheck] at success
          | ok workValue =>
              cases workValue
              cases planCheck : validatePlanTaskInvariant state with
              | error message => simp [namedCheck, designCheck, workCheck, planCheck] at success
              | ok planValue =>
                  cases planValue
                  have ledgerCheck : validateLedgerAuthorityInvariant state = .ok () := by
                    simpa [namedCheck, designCheck, workCheck, planCheck] using success
                  exact {
                    named := namedProjectInvariants_of_validation state namedCheck
                    designHistory := designCheck
                    workLifecycle := workCheck
                    planTask := planCheck
                    ledgerAuthority := ledgerCheck }

end AgentWorkbench
