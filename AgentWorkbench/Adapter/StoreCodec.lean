import AgentWorkbench.Domain.State

namespace AgentWorkbench.Store.Codec

private def fail (message : String) : IO α :=
  throw (IO.userError message)

private def fromExcept : Except String α → IO α
  | .ok value => pure value
  | .error message => fail message

def encode [Lean.ToJson α] (value : α) : String :=
  (Lean.toJson value).compress

def decode [Lean.FromJson α] (kind source : String) : IO α := do
  let json ← fromExcept (Lean.Json.parse source)
  match Lean.fromJson? json with
  | .ok value => pure value
  | .error error => fail s!"invalid persisted {kind}: {error}"

private structure LegacyDesignSource where
  target : String
  snapshot : String
  deriving Lean.FromJson

private structure LegacyAcceptanceCriterion where
  id : String
  statement : String
  target : String
  evidenceKind : String
  deriving Lean.FromJson

private structure LegacyLeanClaim where
  id : String
  input : ClaimInput
  deriving Lean.FromJson

private structure LegacyDesignRevision where
  id : String
  parent : Option String := none
  createdAfterEntryOrder : Nat := 0
  status : DesignStatus := .candidate
  producerAgentRun : String
  sourceDocuments : List LegacyDesignSource := []
  statements : List Statement
  acceptanceCriteria : List LegacyAcceptanceCriterion
  leanClaims : List LegacyLeanClaim := []
  deriving Lean.FromJson

private def LegacyDesignRevision.upgrade (legacy : LegacyDesignRevision) : DesignRevision :=
  { id := legacy.id
    parent := legacy.parent
    createdAfterEntryOrder := legacy.createdAfterEntryOrder
    status := legacy.status
    producerAgentRun := legacy.producerAgentRun
    changeRationale := "legacy source unavailable"
    sourceDocuments := legacy.sourceDocuments.map fun source =>
      { target := source.target, snapshot := source.snapshot }
    statements := legacy.statements
    acceptanceCriteria := legacy.acceptanceCriteria.map fun criterion =>
      { id := criterion.id, statement := criterion.statement, target := criterion.target,
        evidenceKind := criterion.evidenceKind }
    leanClaims := legacy.leanClaims.map fun claim => { id := claim.id, input := claim.input } }

def decodeDesign (source : String) : IO DesignRevision := do
  let json ← fromExcept (Lean.Json.parse source)
  match Lean.fromJson? json with
  | .ok value => pure value
  | .error currentError =>
      match Lean.fromJson? json with
      | .ok legacy => pure (LegacyDesignRevision.upgrade legacy)
      | .error legacyError =>
          fail s!"invalid persisted design revision: {currentError}; legacy: {legacyError}"

def designStatusName : DesignStatus → String
  | .candidate => "candidate"
  | .accepted => "accepted"
  | .superseded => "superseded"
  | .replaced => "replaced"
  | .rejected => "rejected"

def workStatusName : WorkStatus → String
  | .active => "active"
  | .suspended => "suspended"
  | .completed => "completed"
  | .withdrawn => "withdrawn"

def planStatusName : PlanStatus → String
  | .candidate => "candidate"
  | .current => "current"
  | .superseded => "superseded"

def parseNat (field value : String) : IO Nat :=
  match value.toNat? with
  | some parsed => pure parsed
  | none => fail s!"invalid persisted {field}: {value}"

def optionText (value : Option String) : String :=
  value.getD ""

def textOption (value : String) : Option String :=
  if value.isEmpty then none else some value

def payloadKind : EntryPayload → String
  | .task _ => "task"
  | .workDesignAdoption _ => "work-design-adoption"
  | .workHandoff _ => "work-handoff"
  | .workWithdrawal _ => "work-withdrawal"
  | .workResume _ => "work-resume"
  | .workCompletion _ => "work-completion"
  | .designRejection _ => "design-rejection"
  | .commandProfile _ => "command-profile"
  | .commandExecution _ => "command-execution"
  | .artifactObservation _ => "artifact-observation"
  | .review _ => "review"
  | .finding _ => "finding"
  | .reviewDisposition _ => "review-disposition"
  | .reviewVerification _ => "review-verification"
  | .reviewHandoff _ => "review-handoff"
  | .reviewConclusion _ => "review-conclusion"
  | .userCorrection _ => "user-correction"
  | .kpt _ => "kpt"
  | .leanProofReceipt _ => "lean-proof-receipt"

end AgentWorkbench.Store.Codec
