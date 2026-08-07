import AgentWorkbench.Domain.State

namespace AgentWorkbench

def uniqueBy? (items : List α) (key : α → String) (id : String) : Option α :=
  match items.filter (fun item => key item == id) with
  | [item] => some item
  | _ => none

def ProjectState.design? (state : ProjectState) (id : String) : Option DesignRevision :=
  uniqueBy? state.designRevisions (·.id) id

def ProjectState.work? (state : ProjectState) (id : String) : Option Work :=
  uniqueBy? state.works (·.id) id

def ProjectState.entry? (state : ProjectState) (id : String) : Option LedgerEntry :=
  uniqueBy? state.ledgerEntries (·.id) id

/-- The latest responsible-agent disposition is the current authority for one Finding. -/
def findingDispositionIn?
    (entries : List LedgerEntry) (findingId workId : String) : Option LedgerEntry :=
  entries.foldl (fun latest entry =>
    if entry.workId != some workId then latest else
    match entry.payload with
    | .reviewDisposition disposition =>
        if disposition.findingEntryId != findingId then latest
        else match latest with
          | some prior => if prior.order < entry.order then some entry else latest
          | none => some entry
    | _ => latest) none

def ProjectState.findingDisposition?
    (state : ProjectState) (findingId workId : String) : Option LedgerEntry :=
  findingDispositionIn? state.ledgerEntries findingId workId

def ProjectState.findingAccepted (state : ProjectState) (findingId workId : String) : Bool :=
  state.findingDisposition? findingId workId |>.any fun entry =>
    match entry.payload with
    | .reviewDisposition disposition => disposition.decision == .accepted
    | _ => false

def ProjectState.findingAcceptedAt
    (state : ProjectState) (findingId workId : String) (maximumOrder : Nat) : Bool :=
  findingDispositionIn? (state.ledgerEntries.filter (·.order <= maximumOrder)) findingId workId
    |>.any fun entry => match entry.payload with
      | .reviewDisposition disposition => disposition.decision == .accepted
      | _ => false

def ProjectState.plan? (state : ProjectState) (id : String) : Option ImplementationPlan :=
  uniqueBy? state.implementationPlans (·.id) id

def ProjectState.currentPlanFor? (state : ProjectState) (workId : String) : Option ImplementationPlan :=
  let designId := (state.work? workId).bind (·.designRevision)
  match state.implementationPlans.filter (fun plan =>
      plan.workId == workId && plan.status == .current && some plan.designRevision == designId) with
  | [plan] => some plan
  | _ => none

/-- The last materialized Plan remains the replacement predecessor after Design adoption, but is
not current authority until a Plan for the adopted Design is materialized. -/
def ProjectState.materializedPlanFor? (state : ProjectState) (workId : String) : Option ImplementationPlan :=
  match state.implementationPlans.filter (fun plan =>
      plan.workId == workId && plan.status == .current) with
  | [plan] => some plan
  | _ => none

def ProjectState.currentDesign? (state : ProjectState) : Option DesignRevision := do
  let id ← state.acceptedDesignId
  let design ← state.design? id
  if design.status == .accepted then some design else none

def ProjectState.currentWork? (state : ProjectState) : Option Work := do
  let id ← state.focusedWorkId
  let work ← state.work? id
  if work.status == .active then some work else none

def DesignRevision.criterion? (design : DesignRevision) (id : String) : Option AcceptanceCriterion :=
  uniqueBy? design.acceptanceCriteria (·.id) id

def DesignRevision.statement? (design : DesignRevision) (id : String) : Option Statement :=
  uniqueBy? design.statements (·.id) id

def DesignRevision.claim? (design : DesignRevision) (id : String) : Option LeanClaim :=
  uniqueBy? design.leanClaims (·.id) id

def DesignRevision.assumption? (design : DesignRevision) (id : String) : Option DesignAssumption :=
  uniqueBy? design.assumptions (·.id) id

def ProjectState.designDescendsFrom
    (state : ProjectState) (ancestor descendant : String) : Bool :=
  let rec follow : Nat → String → Bool
    | 0, _ => false
    | fuel + 1, current =>
        if current == ancestor then true else
        match (state.design? current).bind (·.parent) with
        | some parent => follow fuel parent
        | none => false
  ancestor != descendant && follow (state.designRevisions.length + 1) descendant

def EntryPayload.tag : EntryPayload → String
  | .task _ => "task"
  | .workDesignAdoption _ => "work_design_adoption"
  | .workHandoff _ => "work_handoff"
  | .workWithdrawal _ => "work_withdrawal"
  | .workResume _ => "work_resume"
  | .workCompletion _ => "work_completion"
  | .designRejection _ => "design_rejection"
  | .commandProfile _ => "command_profile"
  | .commandExecution _ => "command_execution"
  | .artifactObservation _ => "artifact_observation"
  | .review _ => "review"
  | .finding _ => "finding"
  | .reviewDisposition _ => "review_disposition"
  | .reviewVerification _ => "review_verification"
  | .reviewHandoff _ => "review_handoff"
  | .reviewConclusion _ => "review_conclusion"
  | .userCorrection _ => "user_correction"
  | .kpt _ => "kpt"
  | .leanProofReceipt _ => "lean_proof_receipt"

end AgentWorkbench
