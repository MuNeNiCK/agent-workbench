import AgentWorkbench.Application.Ledger
import AgentWorkbench.Decision.Projection

namespace AgentWorkbench

structure CorrectionRecordRequest where
  entryId : String
  content : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CorrectionSupersedeRequest where
  entryId : String
  correctionEntryId : String
  content : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CorrectionResolveRequest where
  entryId : String
  correctionEntryId : String
  actionEntryId : String
  reason : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CorrectionIncorporateRequest where
  entryId : String
  correctionEntryId : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure KptRecordRequest where
  entryId : String
  keep : Option String := none
  problem : Option String := none
  tryNext : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure KptApplyRequest where
  entryId : String
  kptEntryId : String
  actionEntryId : String
  outcome : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def recordCorrection
    (state : ProjectState) (request : CorrectionRecordRequest) : Except String ProjectState :=
  appendCurrentEntry state request.entryId (.userCorrection { content := request.content })

private def currentCorrection
    (state : ProjectState) (correctionEntryId : String) : Except String (LedgerEntry × UserCorrectionRecord) := do
  let (_, work) ← currentBinding state
  let prior ← match state.entry? correctionEntryId with
    | some value => pure value
    | none => throw s!"no User Correction {correctionEntryId}"
  if entryIsSuperseded state prior then throw s!"User Correction {correctionEntryId} is not current"
  if prior.scope != work.scope || prior.workId != some work.id then
    throw s!"User Correction {correctionEntryId} belongs to another Work"
  let correction ← match prior.payload with
    | .userCorrection value => pure value
    | _ => throw s!"entry {correctionEntryId} is not a User Correction"
  if correction.resolvedByEntryId.isSome || correction.incorporatedIn.isSome then
    throw s!"User Correction {correctionEntryId} is already resolved"
  pure (prior, correction)

def supersedeCorrection
    (state : ProjectState) (request : CorrectionSupersedeRequest) : Except String ProjectState := do
  let (prior, _) ← currentCorrection state request.correctionEntryId
  appendCurrentEntry state request.entryId (.userCorrection { content := request.content }) [prior.id]

def resolveCorrection
    (state : ProjectState) (request : CorrectionResolveRequest) : Except String ProjectState := do
  let (prior, correction) ← currentCorrection state request.correctionEntryId
  let action ← match state.entry? request.actionEntryId with
    | some value => pure value
    | none => throw s!"no correction action {request.actionEntryId}"
  if action.order <= prior.order || action.scope != prior.scope || action.workId != prior.workId ||
      action.designRevision != prior.designRevision then
    throw "correction resolution action is not a later same-bound entry"
  if request.reason.isEmpty then throw "correction resolution requires a reason"
  appendCurrentEntry state request.entryId (.userCorrection {
    content := correction.content, resolvedByEntryId := some action.id
    resolutionReason := some request.reason }) [prior.id]

def incorporateCorrection
    (state : ProjectState) (request : CorrectionIncorporateRequest) : Except String ProjectState := do
  let (currentDesign, _) ← currentBinding state
  let (prior, correction) ← currentCorrection state request.correctionEntryId
  let priorDesignId ← match prior.designRevision with
    | some value => pure value
    | none => throw s!"User Correction {request.correctionEntryId} has no Design binding"
  if !state.designDescendsFrom priorDesignId currentDesign.id then
    throw "current accepted Design is not a strict successor of the correction Design"
  appendEntry state {
    id := request.entryId, order := nextEntryOrder state, scope := prior.scope
    workId := prior.workId, designRevision := prior.designRevision
    supersedes := [prior.id]
    payload := .userCorrection {
      content := correction.content, incorporatedIn := some currentDesign.id } }

def recordKpt (state : ProjectState) (request : KptRecordRequest) : Except String ProjectState :=
  appendCurrentEntry state request.entryId (.kpt {
    keep := request.keep, problem := request.problem, tryNext := request.tryNext })

def applyKpt (state : ProjectState) (request : KptApplyRequest) : Except String ProjectState := do
  let (design, work) ← currentBinding state
  let source ← match state.entry? request.kptEntryId with
    | some value => pure value
    | none => throw s!"no KPT {request.kptEntryId}"
  let action ← match state.entry? request.actionEntryId with
    | some value => pure value
    | none => throw s!"no application action {request.actionEntryId}"
  if source.scope != work.scope || source.workId != some work.id ||
      source.designRevision != some design.id || action.scope != source.scope ||
      action.workId != source.workId || action.designRevision != source.designRevision then
    throw "KPT Try and application action are not bound to current Work and Design"
  let sourceKpt ← match source.payload with
    | .kpt value => pure value
    | _ => throw s!"entry {request.kptEntryId} is not KPT"
  if sourceKpt.tryNext.isNone then throw s!"KPT {request.kptEntryId} has no Try"
  if source.order >= action.order then throw "KPT action does not follow its Try"
  appendCurrentEntry state request.entryId (.kpt {
    keep := some request.outcome, appliesKptEntryId := some source.id
    appliedByEntryId := some action.id })

end AgentWorkbench
