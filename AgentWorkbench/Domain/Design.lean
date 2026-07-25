import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Design

open AgentWorkbench.Domain

structure Requirement where
  key : String
  active : Bool
deriving DecidableEq, Repr

structure DesignVersion where
  id : DesignId
  revision : Revision
  predecessor : Option DesignId := none
  owner : String
  contentDigest : String
  requirements : List Requirement
  decisions : List String
  validationGates : List String
deriving DecidableEq, Repr

structure Approval where
  design : DesignId
  review : ReviewId
deriving DecidableEq, Repr

structure TraceItem where
  key : String
  requirements : List String
  implementationWork : List String
  tasks : List String
  completionChecks : List String
  checklists : List String
  validationGates : List String
deriving DecidableEq, Repr

structure Decomposition where
  key : String
  design : DesignId
  work : WorkId
  designRevision : Revision
  contentDigest : String
  items : List TraceItem
  reviewer : String
  adjudicator : String
  accepted : Bool
deriving DecidableEq, Repr

structure Correction where
  key : String
  scope : String
  statement : String
  resolved : Bool
  resolutionReason : Option String := none
  rejected : Bool := false
  authorityTransition : Option String := none
  work : Option WorkId := none
  design : Option DesignId := none
deriving DecidableEq, Repr

inductive AuthorityOperation
  | create
  | amend
  | retire
deriving DecidableEq, Repr, BEq

inductive AuthorityLifetime
  | finite
  | persistent
deriving DecidableEq, Repr, BEq

inductive AuthorityKind
  | designArtifact
  | rule
  | instruction
  | workObligation
deriving DecidableEq, Repr, BEq

structure AuthorityTransition where
  key : String
  correction : String
  target : String
  operation : AuthorityOperation
  kind : AuthorityKind
  scope : String
  work : Option WorkId := none
  design : Option DesignId := none
  lifetime : AuthorityLifetime
  statement : String
  reason : String
deriving DecidableEq, Repr

def versionWellFormed (version : DesignVersion) : Bool :=
  !version.owner.isEmpty && !version.contentDigest.isEmpty &&
  version.predecessor != some version.id &&
  !version.requirements.isEmpty && !version.decisions.isEmpty &&
  !version.validationGates.isEmpty &&
  version.requirements.all (fun requirement => !requirement.key.isEmpty) &&
  version.decisions.all (fun decision => !decision.isEmpty) &&
  version.validationGates.all (fun gate => !gate.isEmpty) &&
  (version.requirements.map (·.key)).Nodup

def requirementsActive (version : DesignVersion) (keys : List String) : Bool :=
  !keys.isEmpty && keys.all fun key =>
    version.requirements.any fun requirement =>
      requirement.key == key && requirement.active

def versionCurrent (versions : List DesignVersion) (version : DesignVersion) : Bool :=
  !versions.any (·.predecessor == some version.id)

def decompositionWellFormed (decomposition : Decomposition) : Bool :=
  !decomposition.key.isEmpty && !decomposition.contentDigest.isEmpty &&
  !decomposition.items.isEmpty &&
  !decomposition.reviewer.isEmpty && !decomposition.adjudicator.isEmpty &&
  decomposition.items.all fun item =>
    !item.key.isEmpty && !item.requirements.isEmpty &&
    !item.implementationWork.isEmpty && !item.tasks.isEmpty &&
    !item.completionChecks.isEmpty && !item.checklists.isEmpty &&
    !item.validationGates.isEmpty &&
    (item.implementationWork ++ item.tasks ++ item.completionChecks ++
      item.checklists ++ item.validationGates).all (fun value => !value.isEmpty)

def traceItemCovers (item : TraceItem) (requirement : String) : Bool :=
  item.requirements.contains requirement &&
  !item.implementationWork.isEmpty && !item.tasks.isEmpty &&
  !item.completionChecks.isEmpty && !item.checklists.isEmpty &&
  !item.validationGates.isEmpty

def decompositionCovers (version : DesignVersion) (approval : Approval)
    (decomposition : Decomposition) : Bool :=
  let active := (version.requirements.filter (·.active)).map (·.key)
  approval.design == version.id && !version.owner.isEmpty &&
  !version.contentDigest.isEmpty && decomposition.design == version.id &&
  decomposition.designRevision == version.revision &&
  decomposition.accepted && decompositionWellFormed decomposition &&
  decomposition.reviewer != version.owner &&
  decomposition.reviewer != decomposition.adjudicator &&
  active.all fun requirement =>
    decomposition.items.any (traceItemCovers · requirement)

def correctionApplies (correction : Correction) (work : WorkId)
    (design : Option DesignId) : Bool :=
  (correction.work.isNone || correction.work == some work) &&
  (correction.design.isNone || correction.design == design)

def correctionWellFormed (correction : Correction) : Bool :=
  !correction.key.isEmpty && !correction.scope.isEmpty &&
  !correction.statement.isEmpty &&
  (correction.resolved == correction.resolutionReason.isSome) &&
  (!correction.rejected || correction.resolved) &&
  (correction.authorityTransition.isNone || correction.resolved) &&
  !(correction.rejected && correction.authorityTransition.isSome)

def authorityTransitionWellFormed (transition : AuthorityTransition) : Bool :=
  !transition.key.isEmpty && !transition.correction.isEmpty &&
  !transition.target.isEmpty && !transition.scope.isEmpty &&
  !transition.reason.isEmpty &&
  (transition.kind != .workObligation || transition.lifetime == .finite) &&
  (transition.operation == .retire || !transition.statement.isEmpty)

def latestAuthorityFor? (target scope : String) (work : Option WorkId)
    (design : Option DesignId)
    (transitions : List AuthorityTransition) : Option AuthorityTransition :=
  transitions.reverse.find? fun transition =>
    transition.target == target && transition.scope == scope &&
    transition.work == work && transition.design == design

def authorityCurrentFor (target : String) (work : WorkId)
    (design : Option DesignId) (transitions : List AuthorityTransition) : Bool :=
  transitions.any fun transition =>
    transition.target == target &&
    (transition.work.isNone || transition.work == some work) &&
    (transition.design.isNone || transition.design == design) &&
    (latestAuthorityFor? transition.target transition.scope transition.work
      transition.design transitions).any fun current =>
        current.key == transition.key && current.operation != .retire

end AgentWorkbench.Domain.Design

-- Completion lifecycle declarations share the normative Domain.Design module;
-- the namespace remains stable without creating a sibling L1 import edge.
namespace AgentWorkbench.Domain.Lifecycle

open AgentWorkbench.Domain

inductive RelatedWorkKind
  | child
  | dependency
deriving DecidableEq, Repr, BEq

inductive ItemStatus
  | pending
  | complete
  | accepted
deriving DecidableEq, Repr, BEq

inductive FindingStatus
  | open
  | resolved
deriving DecidableEq, Repr, BEq

inductive ValidationStatus
  | pending
  | passed
  | stale
deriving DecidableEq, Repr, BEq

inductive RepositoryStatus
  | unclassified
  | classified
deriving DecidableEq, Repr, BEq

inductive CorrectionStatus
  | open
  | resolved
deriving DecidableEq, Repr, BEq

inductive WorkRecordStatus
  | unlinked
  | linked
deriving DecidableEq, Repr, BEq

structure RelatedWorkRequirement where
  work : WorkId
  kind : RelatedWorkKind
deriving DecidableEq, Repr

structure CompletionPlan where
  work : WorkId
  relatedWork : List RelatedWorkRequirement
  phases : List String
  tasks : List String
  checklists : List String
  reviews : List ReviewPlanId
  findings : List String
  validations : List String
  repositories : List String
  corrections : List String
  workRecords : List String
deriving DecidableEq, Repr

structure PhaseRecord where
  key : String
  status : ItemStatus
deriving DecidableEq, Repr

structure TaskRecord where
  key : String
  status : ItemStatus
deriving DecidableEq, Repr

structure ChecklistRecord where
  key : String
  status : ItemStatus
deriving DecidableEq, Repr

structure FindingRecord where
  key : String
  status : FindingStatus
deriving DecidableEq, Repr

structure ValidationRecord where
  key : String
  status : ValidationStatus
  epoch : CompletionEpoch
  artifactDigest : String
deriving DecidableEq, Repr

structure RepositoryRecord where
  key : String
  status : RepositoryStatus
  epoch : CompletionEpoch
  snapshotDigest : String
deriving DecidableEq, Repr

structure CorrectionRecord where
  key : String
  status : CorrectionStatus
deriving DecidableEq, Repr

structure WorkRecordLink where
  key : String
  status : WorkRecordStatus
  reference : String
deriving DecidableEq, Repr

structure CompletionState where
  plan : CompletionPlan
  epoch : CompletionEpoch
  phases : List PhaseRecord
  tasks : List TaskRecord
  checklists : List ChecklistRecord
  findings : List FindingRecord
  validations : List ValidationRecord
  repositories : List RepositoryRecord
  corrections : List CorrectionRecord
  workRecords : List WorkRecordLink
deriving DecidableEq, Repr

def initializeState (plan : CompletionPlan) : CompletionState :=
  { plan
    epoch := ⟨0⟩
    phases := plan.phases.map fun key => { key, status := .pending }
    tasks := plan.tasks.map fun key => { key, status := .pending }
    checklists := plan.checklists.map fun key => { key, status := .pending }
    findings := plan.findings.map fun key => { key, status := .open }
    validations := plan.validations.map fun key =>
      { key, status := .pending, epoch := ⟨0⟩, artifactDigest := "" }
    repositories := plan.repositories.map fun key =>
      { key, status := .unclassified, epoch := ⟨0⟩, snapshotDigest := "" }
    corrections := plan.corrections.map fun key => { key, status := .open }
    workRecords := plan.workRecords.map fun key =>
      { key, status := .unlinked, reference := "" } }

def nonemptyKeys (keys : List String) : Prop :=
  (keys.all fun key => !key.isEmpty) = true

def ValidPlan (work : List WorkId) (plan : CompletionPlan) : Prop :=
  work.contains plan.work = true ∧
  (plan.relatedWork.map (·.work)).Nodup ∧
  (plan.relatedWork.all fun requirement =>
    requirement.work != plan.work && work.contains requirement.work) = true ∧
  plan.phases.Nodup ∧ nonemptyKeys plan.phases ∧
  plan.tasks.Nodup ∧ nonemptyKeys plan.tasks ∧
  plan.checklists.Nodup ∧ nonemptyKeys plan.checklists ∧
  plan.reviews.Nodup ∧
  plan.findings.Nodup ∧ nonemptyKeys plan.findings ∧
  plan.validations.Nodup ∧ nonemptyKeys plan.validations ∧
  plan.repositories.Nodup ∧ nonemptyKeys plan.repositories ∧
  plan.corrections.Nodup ∧ nonemptyKeys plan.corrections ∧
  plan.workRecords.Nodup ∧ nonemptyKeys plan.workRecords

def MatchesPlan (state : CompletionState) : Prop :=
  state.phases.map (·.key) = state.plan.phases ∧
  state.tasks.map (·.key) = state.plan.tasks ∧
  state.checklists.map (·.key) = state.plan.checklists ∧
  state.findings.map (·.key) = state.plan.findings ∧
  state.validations.map (·.key) = state.plan.validations ∧
  state.repositories.map (·.key) = state.plan.repositories ∧
  state.corrections.map (·.key) = state.plan.corrections ∧
  state.workRecords.map (·.key) = state.plan.workRecords

def RecordsWellFormed (state : CompletionState) : Prop :=
  (state.validations.all fun record =>
    record.status != .passed || !record.artifactDigest.isEmpty) = true ∧
  (state.repositories.all fun record =>
    record.status != .classified || !record.snapshotDigest.isEmpty) = true ∧
  (state.workRecords.all fun record =>
    record.status != .linked || !record.reference.isEmpty) = true

instance (work : List WorkId) (plan : CompletionPlan) :
    Decidable (ValidPlan work plan) := by
  unfold ValidPlan nonemptyKeys
  infer_instance

instance (state : CompletionState) : Decidable (MatchesPlan state) := by
  unfold MatchesPlan
  infer_instance

instance (state : CompletionState) : Decidable (RecordsWellFormed state) := by
  unfold RecordsWellFormed
  infer_instance

def ValidLifecycleState (work : List WorkId)
    (states : List CompletionState) : Prop :=
  (states.map fun state => state.plan.work).Nodup ∧
  (states.all fun state => decide (ValidPlan work state.plan)) = true ∧
  (states.all fun state => decide (MatchesPlan state)) = true ∧
  (states.all fun state => decide (RecordsWellFormed state)) = true

instance (work : List WorkId) (states : List CompletionState) :
    Decidable (ValidLifecycleState work states) := by
  unfold ValidLifecycleState ValidPlan MatchesPlan nonemptyKeys
  unfold RecordsWellFormed
  infer_instance

def forWork (states : List CompletionState) (work : WorkId) : Option CompletionState :=
  states.find? fun state => state.plan.work == work

def replace (states : List CompletionState) (updated : CompletionState) :
    List CompletionState :=
  states.map fun state =>
    if state.plan.work == updated.plan.work then updated else state

def itemTerminal (status : ItemStatus) : Bool :=
  status == .complete || status == .accepted

def phasesReady (state : CompletionState) : Bool :=
  state.phases.all fun record => itemTerminal record.status

def tasksReady (state : CompletionState) : Bool :=
  state.tasks.all fun record => itemTerminal record.status

def checklistsReady (state : CompletionState) : Bool :=
  state.checklists.all fun record => itemTerminal record.status

def findingsReady (state : CompletionState) : Bool :=
  state.findings.all fun record => record.status == .resolved

def validationsReady (state : CompletionState) : Bool :=
  state.validations.all fun record =>
    record.status == .passed && record.epoch == state.epoch &&
      !record.artifactDigest.isEmpty

def repositoriesReady (state : CompletionState) : Bool :=
  state.repositories.all fun record =>
    record.status == .classified && record.epoch == state.epoch &&
      !record.snapshotDigest.isEmpty

def correctionsReady (state : CompletionState) : Bool :=
  state.corrections.all fun record => record.status == .resolved

def workRecordsReady (state : CompletionState) : Bool :=
  state.workRecords.all fun record =>
    record.status == .linked && !record.reference.isEmpty

def recordsReady (state : CompletionState) : Bool :=
  phasesReady state && tasksReady state && checklistsReady state &&
  findingsReady state && validationsReady state && repositoriesReady state &&
  correctionsReady state && workRecordsReady state

def advance (state : CompletionState) : CompletionState :=
  { state with epoch := state.epoch.next }

def completePhase (state : CompletionState) (key : String) : CompletionState :=
  advance { state with phases := state.phases.map fun record =>
    if record.key == key then { record with status := .complete } else record }

def completeTask (state : CompletionState) (key : String) : CompletionState :=
  advance { state with tasks := state.tasks.map fun record =>
    if record.key == key then { record with status := .complete } else record }

def completeChecklist (state : CompletionState) (key : String) : CompletionState :=
  advance { state with checklists := state.checklists.map fun record =>
    if record.key == key then { record with status := .complete } else record }

def resolveFinding (state : CompletionState) (key : String) : CompletionState :=
  advance { state with findings := state.findings.map fun record =>
    if record.key == key then { record with status := .resolved } else record }

def passValidation (state : CompletionState) (key artifactDigest : String) :
    CompletionState :=
  { state with validations := state.validations.map fun record =>
    if record.key == key then
      { record with status := .passed, epoch := state.epoch, artifactDigest }
    else record }

def classifyRepository (state : CompletionState) (key snapshotDigest : String) :
    CompletionState :=
  { state with repositories := state.repositories.map fun record =>
    if record.key == key then
      { record with status := .classified, epoch := state.epoch, snapshotDigest }
    else record }

def resolveCorrection (state : CompletionState) (key : String) : CompletionState :=
  advance { state with corrections := state.corrections.map fun record =>
    if record.key == key then { record with status := .resolved } else record }

def linkWorkRecord (state : CompletionState) (key reference : String) :
    CompletionState :=
  advance { state with workRecords := state.workRecords.map fun record =>
    if record.key == key then { record with status := .linked, reference } else record }

end AgentWorkbench.Domain.Lifecycle
