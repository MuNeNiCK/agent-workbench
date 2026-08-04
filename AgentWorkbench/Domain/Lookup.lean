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

def ProjectState.currentDesign? (state : ProjectState) : Option DesignRevision := do
  let id ← state.acceptedDesignId
  let design ← state.design? id
  if design.status == .accepted then some design else none

def ProjectState.currentWork? (state : ProjectState) : Option Work := do
  let id ← state.focusedWorkId
  let work ← state.work? id
  if work.status == .focused then some work else none

def DesignRevision.criterion? (design : DesignRevision) (id : String) : Option AcceptanceCriterion :=
  uniqueBy? design.acceptanceCriteria (·.id) id

def DesignRevision.statement? (design : DesignRevision) (id : String) : Option Statement :=
  uniqueBy? design.statements (·.id) id

def DesignRevision.claim? (design : DesignRevision) (id : String) : Option LeanClaim :=
  uniqueBy? design.leanClaims (·.id) id

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
  | .commandProfile _ => "command_profile"
  | .commandExecution _ => "command_execution"
  | .artifactObservation _ => "artifact_observation"
  | .review _ => "review"
  | .finding _ => "finding"
  | .reviewDisposition _ => "review_disposition"
  | .reviewVerification _ => "review_verification"
  | .userCorrection _ => "user_correction"
  | .kpt _ => "kpt"
  | .leanProofReceipt _ => "lean_proof_receipt"

end AgentWorkbench
