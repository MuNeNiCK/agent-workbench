import AgentWorkbench.Kernel.Replay
import AgentWorkbench.Policy.Traceability
import AgentWorkbench.Policy.Authority
import AgentWorkbench.Policy.Completion
import AgentWorkbench.Policy.Update

namespace AgentWorkbench.Kernel.Decide

open AgentWorkbench.Domain
open AgentWorkbench.Kernel.Replay

inductive Command
  | initializeWork (expectedRevision : Revision)
      (work : Work.WorkUnit) (activation : Work.Activation)
  | registerWork (expectedRevision : Revision) (work : Work.WorkUnit)
  | registerSuspendedActivation (expectedRevision : Revision)
      (activation : Work.Activation)
  | suspendWork (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId) (context : Work.SuspensionContext)
  | confirmResumeReadiness (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId) (basis : Work.ReadinessBasis)
  | reviseSuspension (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId) (context : Work.SuspensionContext)
  | resumeWork (expectedRevision : Revision) (work : WorkId)
      (activation : ActivationId)
  | importDesign (expectedRevision : Revision) (version : Design.DesignVersion)
  | approveDesign (expectedRevision : Revision) (design : DesignId)
  | recordDecomposition (expectedRevision : Revision)
      (decomposition : Design.Decomposition)
  | recordAuthorityException (expectedRevision : Revision)
      (exception : Review.AuthorityException)
  | recordReviewPlan (expectedRevision : Revision) (plan : Review.Plan)
  | planCompletion (expectedRevision : Revision) (plan : Lifecycle.CompletionPlan)
  | acknowledgeRelatedWorkTerminal (expectedRevision : Revision) (owner related : WorkId)
  | completePhase (expectedRevision : Revision) (work : WorkId) (key : String)
  | completeTask (expectedRevision : Revision) (work : WorkId) (key : String)
  | completeChecklist (expectedRevision : Revision) (work : WorkId) (key : String)
  | resolveFinding (expectedRevision : Revision) (work : WorkId) (key : String)
  | passValidation (expectedRevision : Revision) (work : WorkId)
      (key artifactDigest : String)
  | classifyRepository (expectedRevision : Revision) (work : WorkId)
      (key snapshotDigest : String)
  | resolveCorrection (expectedRevision : Revision) (work : WorkId) (key : String)
  | linkWorkRecord (expectedRevision : Revision) (work : WorkId)
      (key reference : String)
  | recordReviewClaim (expectedRevision : Revision) (claim : Review.Claim)
  | recordReviewAdjudication (expectedRevision : Revision)
      (adjudication : Review.Adjudication)
  | recordReviewFinding (expectedRevision : Revision) (finding : Review.Finding)
  | adjudicateReviewFinding (expectedRevision : Revision)
      (key principal reason : String) (accepted : Bool)
  | closeReviewFinding (expectedRevision : Revision) (key : String)
      (attempt : Review.ClosureAttempt)
  | verifyReviewFinding (expectedRevision : Revision)
      (verification : Review.Verification)
  | adjudicateFindingVerification (expectedRevision : Revision)
      (finding : String) (attempt : Nat) (adjudicator : String)
  | recordUserCorrection (expectedRevision : Revision)
      (correction : Design.Correction)
  | resolveUserCorrection (expectedRevision : Revision) (key reason : String)
  | rejectUserProposal (expectedRevision : Revision) (key reason : String)
  | recordAuthorityTransition (expectedRevision : Revision)
      (transition : Design.AuthorityTransition)
  | recordEvidence (expectedRevision : Revision) (evidence : Evidence.Evidence)
  | recordExternalOperation (expectedRevision : Revision)
      (attempt : ExternalOperation.Attempt)
  | advanceExternalOperation (expectedRevision : Revision)
      (attempt : ExternalOperation.Attempt)
  | recordObligation (expectedRevision : Revision) (obligation : Evidence.Obligation)
  | completeWork (expectedRevision : Revision) (target : WorkId)
deriving DecidableEq, Repr

def Command.expectedRevision : Command → Revision
  | .initializeWork revision _ _
  | .registerWork revision _
  | .registerSuspendedActivation revision _
  | .suspendWork revision _ _ _
  | .confirmResumeReadiness revision _ _ _
  | .reviseSuspension revision _ _ _
  | .resumeWork revision _ _
  | .importDesign revision _
  | .approveDesign revision _
  | .recordDecomposition revision _
  | .recordAuthorityException revision _
  | .recordReviewPlan revision _
  | .planCompletion revision _
  | .acknowledgeRelatedWorkTerminal revision _ _
  | .completePhase revision _ _
  | .completeTask revision _ _
  | .completeChecklist revision _ _
  | .resolveFinding revision _ _
  | .passValidation revision _ _ _
  | .classifyRepository revision _ _ _
  | .resolveCorrection revision _ _
  | .linkWorkRecord revision _ _ _
  | .recordReviewClaim revision _
  | .recordReviewAdjudication revision _
  | .recordReviewFinding revision _
  | .adjudicateReviewFinding revision _ _ _ _
  | .closeReviewFinding revision _ _
  | .verifyReviewFinding revision _
  | .adjudicateFindingVerification revision _ _ _
  | .recordUserCorrection revision _
  | .resolveUserCorrection revision _ _
  | .rejectUserProposal revision _ _
  | .recordAuthorityTransition revision _
  | .recordEvidence revision _
  | .recordExternalOperation revision _
  | .advanceExternalOperation revision _
  | .recordObligation revision _
  | .completeWork revision _ => revision

structure DerivedEvents where
  events : List Event
  eventsNonempty : events ≠ []

def completionReady (target : WorkId) (state : State) : Bool :=
  Policy.Completion.closeable target state.work state.activations
    state.claims state.adjudications state.reviewPlans
    state.reviewFindings state.findingVerifications state.lifecycle
    state.evidence state.obligations state.designs
    state.designApprovals state.decompositions state.corrections

def deriveEvents (command : Command) (state : State) : Except DomainError DerivedEvents :=
  match command with
  | .initializeWork _ work activation =>
      if state.work.isEmpty && state.activations.isEmpty &&
          work.status == .open && work.wellFormed &&
          activation.status == .active &&
          !activation.readyToResume && activation.work == work.id then
        .ok ⟨[.workInitialized work activation], by simp⟩
      else
        .error (.invalidTransition "work initialization requires an empty state and one matching open active frame")
  | .registerWork _ work =>
      if work.status == .open && work.wellFormed &&
          !state.work.any (·.id == work.id) then
        .ok ⟨[.workRegistered work], by simp⟩
      else
        .error (.invalidTransition "registered work must be a new open work unit")
  | .registerSuspendedActivation _ activation =>
      if activation.status == .suspended &&
          !activation.readyToResume && activation.confirmedBasis.isNone &&
          activation.suspension.any (fun context =>
            context.wellFormed &&
            context.readinessWellFormed) &&
          state.work.any (fun work => work.id == activation.work && work.status == .open) &&
          !state.activations.any (·.id == activation.id) &&
          (activation.suspension.any fun context =>
            context.basis.any fun basis =>
              Replay.traceReadyFor basis.design activation.work
                basis.decompositionKey basis.decompositionDigest state) &&
          (activation.parent.isNone || activation.parent.any fun parent =>
            state.activations.any (fun current =>
              current.id == parent && current.status == .active)) then
        .ok ⟨[.suspendedActivationRegistered activation], by simp⟩
      else
        .error (.invalidTransition "registered parent activation must be new, suspended with durable context, and reference open work")
  | .suspendWork _ work activation context =>
      if context.wellFormed && state.activations.any (fun current =>
          current.id == activation && current.work == work &&
            current.status == .active) then
        .ok ⟨[.workSuspended work activation context], by simp⟩
      else
        .error (.invalidTransition "suspension requires the exact active frame and a complete durable context")
  | .confirmResumeReadiness _ work activation basis =>
      if Replay.readinessCurrent work activation basis state then
        .ok ⟨[.resumeReadinessConfirmed work activation basis], by simp⟩
      else
        .error (.invalidTransition "resume readiness requires current assumptions, no active frame, and no unresolved correction")
  | .reviseSuspension _ work activation context =>
      if context.readinessWellFormed && state.activations.any (fun current =>
          current.id == activation && current.work == work &&
          current.status == .suspended) then
        .ok ⟨[.suspensionRevised work activation context], by simp⟩
      else
        .error (.invalidTransition "suspension revision requires an exact suspended frame and complete current basis")
  | .resumeWork _ work activation =>
      if Work.workIsOpen state.work work &&
          state.activations.any (fun candidate =>
            candidate.id == activation && candidate.work == work) &&
          Work.resumable state.activations activation &&
          Replay.resumeCurrent work activation state then
        .ok ⟨[.workResumed work activation], by simp⟩
      else
        .error (.invalidTransition "resumed activation must be ready, suspended, inactive, and reference the exact open work")
  | .importDesign _ version =>
      if Design.versionWellFormed version &&
          (match version.predecessor with
          | none => true
          | some predecessor =>
              state.designs.any fun current =>
                current.id == predecessor &&
                  Design.versionCurrent state.designs current) &&
          !state.designs.any (·.id == version.id) then
        .ok ⟨[.designImported version], by simp⟩
      else
        .error (.invalidTransition "design import requires a new immutable, well-formed, unapproved version")
  | .approveDesign _ design =>
      match state.designs.find? (·.id == design) with
      | none => .error (.invalidTransition "design version is missing")
      | some version =>
          match state.reviewPlans.find? (fun plan =>
              plan.scope.design == some design && plan.scope.purpose == .design &&
              plan.scope.artifactDigest == version.contentDigest &&
              plan.owner == version.owner &&
              Review.isLatestPlan plan state.reviewPlans &&
              Replay.reviewScopeReady plan.id state) with
          | none =>
              .error (.invalidTransition "design approval requires an exact adjudicated fresh clean independent review and closed findings")
          | some plan =>
              match state.claims.find? (fun claim =>
                  claim.plan == plan.id && claim.claim == .clean &&
                  state.adjudications.any (fun decision =>
                    decision.review == claim.id &&
                    decision.decision == .accepted)) with
              | none => .error (.invalidTransition "accepted design review claim is missing")
              | some claim =>
                  let approval : Design.Approval := { design, review := claim.id }
                  if !state.designApprovals.any (·.design == design) &&
                      Replay.designApprovalLineageReady state version &&
                      !state.corrections.any (fun correction =>
                        !correction.resolved &&
                        (correction.design == some design ||
                          (correction.design.isNone && correction.work.isNone))) then
                    .ok ⟨[.designApproved approval], by simp⟩
                  else
                    .error (.invalidTransition "design is already approved or has an applicable correction")
  | .recordDecomposition _ decomposition =>
      if Replay.decompositionRecordable decomposition state then
        .ok ⟨[.decompositionRecorded decomposition], by simp⟩
      else
        .error (.invalidTransition "decomposition must be new, complete, and bind an exact approved design version")
  | .recordAuthorityException _ exception =>
      if !exception.key.isEmpty && !exception.reason.isEmpty &&
          exception.authorizedBy == "user" &&
          !state.authorityExceptions.any (·.key == exception.key) then
        .ok ⟨[.authorityExceptionRecorded exception], by simp⟩
      else
        .error (.invalidTransition "review separation bypass requires a new explicit scoped user authority event")
  | .recordReviewPlan _ plan =>
      if Policy.Authority.mayInvoke plan state.authorityExceptions &&
          Review.planWellFormed plan && !state.reviewPlans.any (·.id == plan.id) &&
          state.work.any (·.id == plan.scope.work) &&
          (match plan.scope.design with
          | some design => state.designs.any (fun version =>
              version.id == design && version.owner == plan.owner) &&
              state.work.any (fun work =>
                work.id == plan.scope.work && work.owner == plan.owner)
          | none => state.work.any (fun work =>
              work.id == plan.scope.work && work.owner == plan.owner)) then
        .ok ⟨[.reviewPlanRecorded plan], by simp⟩
      else
        .error (.invalidTransition "review plan must freeze an exact scope and satisfy independent authority")
  | .planCompletion _ plan =>
      if state.lifecycle.any (fun completion => completion.plan.work == plan.work) then
        .error (.invalidTransition "completion plan already exists")
      else if Lifecycle.ValidPlan (state.work.map (·.id)) plan &&
          plan.reviews.all (fun review =>
            state.reviewPlans.any (fun existing =>
              existing.id == review && existing.scope.work == plan.work)) then
        .ok ⟨[.completionPlanned plan], by simp⟩
      else
        .error (.invalidTransition "completion plan is not authoritative or well formed")
  | .acknowledgeRelatedWorkTerminal _ owner related =>
      match Lifecycle.forWork state.lifecycle owner with
      | none => .error (.invalidTransition "completion plan is missing")
      | some completion =>
          if completion.plan.relatedWork.any (·.work == related) &&
              state.work.any (fun work => work.id == related &&
                (work.status == .closed || work.status == .abandoned)) then
            .ok ⟨[.relatedWorkTerminalAcknowledged owner related], by simp⟩
          else
            .error (.invalidTransition "related work has not reached its own terminal state")
  | .completePhase _ work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if completion.phases.any (fun record =>
              record.key == key && record.status == .pending) then
            .ok ⟨[.phaseCompleted work key], by simp⟩
          else .error (.invalidTransition "phase is not pending")
      | none => .error (.invalidTransition "completion plan is missing")
  | .completeTask _ work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if completion.tasks.any (fun record =>
              record.key == key && record.status == .pending) then
            .ok ⟨[.taskCompleted work key], by simp⟩
          else .error (.invalidTransition "task is not pending")
      | none => .error (.invalidTransition "completion plan is missing")
  | .completeChecklist _ work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if completion.checklists.any (fun record =>
              record.key == key && record.status == .pending) then
            .ok ⟨[.checklistCompleted work key], by simp⟩
          else .error (.invalidTransition "checklist is not pending")
      | none => .error (.invalidTransition "completion plan is missing")
  | .resolveFinding _ work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if completion.findings.any (fun record =>
              record.key == key && record.status == .open) then
            .ok ⟨[.findingResolved work key], by simp⟩
          else .error (.invalidTransition "finding is not open")
      | none => .error (.invalidTransition "completion plan is missing")
  | .passValidation _ work key artifactDigest =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if !artifactDigest.isEmpty && completion.validations.any (·.key == key) then
            .ok ⟨[.validationPassed work key artifactDigest], by simp⟩
          else .error (.invalidTransition "validation observation is not a required nonempty artifact")
      | none => .error (.invalidTransition "completion plan is missing")
  | .classifyRepository _ work key snapshotDigest =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if !snapshotDigest.isEmpty && completion.repositories.any (·.key == key) then
            .ok ⟨[.repositoryClassified work key snapshotDigest], by simp⟩
          else .error (.invalidTransition "repository classification is not bound to a required snapshot")
      | none => .error (.invalidTransition "completion plan is missing")
  | .resolveCorrection _ work key =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if completion.corrections.any (fun record =>
              record.key == key && record.status == .open) then
            .ok ⟨[.correctionResolved work key], by simp⟩
          else .error (.invalidTransition "correction is not open")
      | none => .error (.invalidTransition "completion plan is missing")
  | .linkWorkRecord _ work key reference =>
      match Lifecycle.forWork state.lifecycle work with
      | some completion =>
          if !reference.isEmpty && completion.workRecords.any (fun record =>
              record.key == key && record.status == .unlinked) then
            .ok ⟨[.workRecordLinked work key reference], by simp⟩
          else .error (.invalidTransition "work record link is not a required unlinked record")
      | none => .error (.invalidTransition "completion plan is missing")
  | .recordReviewClaim _ claim =>
      if Review.claimWellFormed claim &&
          !state.claims.any (·.id == claim.id) &&
          state.reviewPlans.any (fun plan => Review.scopeExact plan claim) then
        .ok ⟨[.reviewClaimed claim], by simp⟩
      else
        .error (.invalidTransition "review claim is not bound to the exact frozen scope")
  | .recordReviewAdjudication _ adjudication =>
      if Replay.reviewAdjudicationApplicable adjudication state then
        .ok ⟨[.reviewAdjudicated adjudication], by simp⟩
      else
        .error (.invalidTransition
          "review adjudication requires a known claim, a reason, and dispositions for its observations")
  | .recordReviewFinding _ finding =>
      if Review.findingWellFormed finding &&
          !finding.adjudicated && finding.closureAttempts.isEmpty &&
          state.claims.any (fun claim =>
            claim.id == finding.review &&
              Review.claimAcceptsFindings claim state.reviewPlans state.claims &&
              claim.scope.any
                (Replay.reviewAuthorityCurrent finding.authority · state)) &&
          !state.reviewFindings.any (·.key == finding.key) then
        .ok ⟨[.reviewFindingRecorded finding], by simp⟩
      else
        .error (.invalidTransition "finding must be new, scoped, and belong to a findings review")
  | .adjudicateReviewFinding _ key principal reason accepted =>
      if state.reviewFindings.any (fun finding =>
          finding.key == key && !finding.adjudicated &&
          finding.closureAttempts.isEmpty && !reason.isEmpty &&
          state.claims.any (fun claim =>
            claim.id == finding.review &&
            state.reviewPlans.any (fun plan =>
              plan.id == claim.plan && plan.adjudicator == principal &&
              principal != claim.reviewer))) then
        .ok ⟨[.reviewFindingAdjudicated key principal reason accepted], by simp⟩
      else
        .error (.invalidTransition
          "finding adjudication requires the frozen plan adjudicator, a reason, and separation from the reviewer")
  | .closeReviewFinding _ key attempt =>
      if Review.attemptWellFormed attempt &&
          state.reviewFindings.any (fun finding =>
            finding.key == key && finding.accepted &&
            attempt.attempt == finding.closureAttempts.length + 1 &&
            Review.mayStartAttempt finding state.findingVerifications &&
            state.claims.any (fun claim =>
              claim.id == finding.review && claim.scope.any (fun scope =>
                scope.repositorySnapshot != attempt.repositorySnapshot))) then
        .ok ⟨[.reviewFindingClosureAttempted key attempt], by simp⟩
      else
        .error (.invalidTransition "closure attempt must be new, exact, and follow an adjudicated failed verification")
  | .verifyReviewFinding _ verification =>
      if state.reviewFindings.any (fun finding =>
          finding.key == verification.finding &&
          finding.closureAttempts.getLast?.any (fun attempt =>
            attempt.attempt == verification.attempt &&
            attempt.evidenceDigest == verification.evidenceDigest &&
            attempt.repositorySnapshot ==
              verification.scope.repositorySnapshot) &&
          state.claims.any (fun claim =>
            claim.id == finding.review &&
            !verification.adjudicated && !verification.accepted &&
            claim.scope.any (Review.sameContext verification.scope) &&
            verification.verifier != claim.reviewer)) &&
          !state.findingVerifications.any (fun existing =>
            existing.finding == verification.finding &&
            existing.attempt == verification.attempt) then
        .ok ⟨[.findingVerified verification], by simp⟩
      else
        .error (.invalidTransition "finding verification must be independent and preserve the frozen scope")
  | .adjudicateFindingVerification _ finding attempt adjudicator =>
      if state.findingVerifications.any (fun verification =>
          verification.finding == finding && verification.attempt == attempt &&
          !verification.adjudicated && !verification.accepted &&
          state.reviewFindings.any (fun record =>
            record.key == finding &&
            state.claims.any (fun claim =>
              claim.id == record.review &&
              state.reviewPlans.any (fun plan =>
                plan.id == claim.plan && plan.adjudicator == adjudicator &&
                adjudicator != verification.verifier)))) then
        .ok ⟨[.findingVerificationAdjudicated finding attempt adjudicator], by simp⟩
      else
        .error (.invalidTransition "finding verification requires separate owner adjudication")
  | .recordUserCorrection _ correction =>
      if Design.correctionWellFormed correction && !correction.resolved &&
          !state.corrections.any (·.key == correction.key) then
        .ok ⟨[.correctionRecorded correction], by simp⟩
      else
        .error (.invalidTransition "correction must be new, durable, scoped, and unresolved")
  | .resolveUserCorrection _ key reason =>
      if state.corrections.any (fun correction =>
          correction.key == key && !correction.resolved) &&
          !reason.isEmpty then
        .ok ⟨[.userCorrectionResolved key reason false], by simp⟩
      else
        .error (.invalidTransition
          "user statement resolution requires an open statement and a reason")
  | .rejectUserProposal _ key reason =>
      if state.corrections.any (fun correction =>
          correction.key == key && !correction.resolved) &&
          !reason.isEmpty then
        .ok ⟨[.userCorrectionResolved key reason true], by simp⟩
      else
        .error (.invalidTransition
          "proposal rejection requires an open statement and a reason")
  | .recordAuthorityTransition _ transition =>
      if Design.authorityTransitionWellFormed transition &&
          state.corrections.any (fun correction =>
            correction.key == transition.correction && !correction.resolved &&
            correction.scope == transition.scope &&
            correction.work == transition.work &&
            correction.design == transition.design) &&
          !state.authorityTransitions.any (·.key == transition.key) &&
          (match Design.latestAuthorityFor? transition.target transition.scope
              transition.work transition.design state.authorityTransitions with
          | none => transition.operation == .create
          | some current =>
              transition.operation != .create &&
              current.operation != .retire &&
              current.scope == transition.scope &&
              current.kind == transition.kind &&
              current.lifetime == transition.lifetime) then
        .ok ⟨[.authorityTransitionRecorded transition], by simp⟩
      else
        .error (.invalidTransition
          "authority transition requires an open source statement, a reason, and a valid create/amend/retire target")
  | .recordEvidence _ evidence =>
      if Evidence.traceable evidence &&
          state.obligations.any (fun obligation =>
            obligation.work == evidence.work &&
            obligation.key == evidence.obligation &&
            Evidence.exactFor evidence obligation) then
        .ok ⟨[.evidenceRecorded evidence], by simp⟩
      else
        .error (.invalidTransition "evidence requires requirement links, producer, and observation time")
  | .recordExternalOperation _ attempt =>
      if attempt.state == .prepared && attempt.wellFormed &&
          !state.externalOperations.any (·.operation == attempt.operation) then
        .ok ⟨[.externalOperationRecorded attempt], by simp⟩
      else
        .error (.invalidTransition
          "external operation must begin as one new well-formed prepared intent")
  | .advanceExternalOperation _ attempt =>
      match state.externalOperations.find? (·.operation == attempt.operation) with
      | some current =>
          if ExternalOperation.transitionAllowed current attempt then
            .ok ⟨[.externalOperationAdvanced attempt], by simp⟩
          else
            .error (.invalidTransition
              "external operation transition is not allowed")
      | none =>
          .error (.invalidTransition
            "external operation intent must be committed before dispatch")
  | .recordObligation _ obligation =>
      if !obligation.requirements.isEmpty &&
          !obligation.expectedProducer.isEmpty &&
          !obligation.expectedObservation.isEmpty &&
          Evidence.negativeBoundaryAdmissible obligation &&
          obligation.revision == state.revision &&
          state.designs.any (fun version =>
            version.id == obligation.design &&
            version.revision == obligation.designRevision &&
            Replay.approvedDesignCurrent state version &&
            Design.requirementsActive version obligation.requirements) then
        .ok ⟨[.obligationRecorded obligation], by simp⟩
      else
        .error (.invalidTransition
          "obligations require active requirements from the exact current design version")
  | .completeWork _ target =>
      match Work.activeFor state.activations target with
      | none => .error (.invalidTransition "target work is not active")
      | some activation =>
          if completionReady target state then
            .ok ⟨[.workCompleted target activation.id], by simp⟩
          else
            .error (.invalidTransition "completion obligations remain")

structure AcceptedTransaction where
  command : Command
  events : List Event
  eventsNonempty : events ≠ []
  result : VerifiedState

def decide (command : Command) (state : State) : Except DomainError AcceptedTransaction :=
  if command.expectedRevision = state.revision then
    match deriveEvents command state with
    | .error error => .error error
    | .ok derived =>
        match replay derived.events state with
        | .ok result => .ok ⟨command, derived.events, derived.eventsNonempty, result⟩
        | .error error => .error error
  else
    .error .staleRevision

theorem decide_preserves_valid (command : Command) (state : State)
    {transaction : AcceptedTransaction}
    (_accepted : decide command state = .ok transaction) :
    ValidState transaction.result.state :=
  transaction.result.valid

theorem decide_emits_only_derived_events (command : Command) (state : State)
    {transaction : AcceptedTransaction}
    (accepted : decide command state = .ok transaction) :
    ∃ derived, deriveEvents command state = .ok derived ∧
      transaction.events = derived.events := by
  unfold decide at accepted
  split at accepted
  · split at accepted
    · contradiction
    · split at accepted
      · cases accepted
        exact ⟨_, by assumption, rfl⟩
      · contradiction
  · contradiction

def committedEvents (result : Except DomainError AcceptedTransaction) : List Event :=
  match result with
  | .ok transaction => transaction.events
  | .error _ => []

def committedState (result : Except DomainError AcceptedTransaction) (original : State) : State :=
  match result with
  | .ok transaction => transaction.result.state
  | .error _ => original

inductive Commits (command : Command) (initial : State) : List Event → State → Prop
  | accepted (transaction : AcceptedTransaction)
      (accepted : decide command initial = .ok transaction) :
      Commits command initial transaction.events transaction.result.state

theorem decide_rejection_has_no_effect (command : Command) (state : State)
    (error : DomainError) (rejected : decide command state = .error error) :
    (¬ ∃ events result, Commits command state events result) ∧
    committedEvents (decide command state) = [] ∧
    committedState (decide command state) state = state ∧
    (committedState (decide command state) state).revision = state.revision := by
  constructor
  · rintro ⟨_, _, commit⟩
    cases commit with
    | accepted transaction accepted =>
        rw [rejected] at accepted
        contradiction
  · simp [committedEvents, committedState, rejected]

structure CompletionTransaction extends AcceptedTransaction where
  target : WorkId
  activation : ActivationId

def closeWork (expectedRevision : Revision) (target : WorkId) (state : State) :
    Except DomainError CompletionTransaction :=
  match Work.activeFor state.activations target with
  | none => .error (.invalidTransition "target work is not active")
  | some activation =>
      match decide (.completeWork expectedRevision target) state with
      | .error error => .error error
      | .ok transaction => .ok { transaction with target, activation := activation.id }

theorem close_work_preserves_valid (expectedRevision : Revision) (target : WorkId) (state : State)
    {transaction : CompletionTransaction}
    (_accepted : closeWork expectedRevision target state = .ok transaction) :
    ValidState transaction.result.state :=
  transaction.result.valid

theorem decide_complete_emits (expectedRevision : Revision) (target : WorkId) (state : State)
    (activation : Work.Activation) (transaction : AcceptedTransaction)
    (active : Work.activeFor state.activations target = some activation)
    (accepted : decide (.completeWork expectedRevision target) state = .ok transaction) :
    transaction.events = [.workCompleted target activation.id] := by
  unfold decide at accepted
  simp only [Command.expectedRevision] at accepted
  split at accepted
  · by_cases ready : completionReady target state = true
    · simp only [deriveEvents, active, ready, if_true] at accepted
      split at accepted
      · cases accepted
        rfl
      · contradiction
    · have notReady : completionReady target state = false := by
        cases result : completionReady target state with
        | false => rfl
        | true => exact (ready result).elim
      simp [deriveEvents, active, notReady] at accepted
  · contradiction

theorem decide_complete_requires_closeable (target : WorkId) (state : State)
    {transaction : AcceptedTransaction}
    (accepted : decide (.completeWork state.revision target) state = .ok transaction) :
    completionReady target state = true := by
  obtain ⟨derived, derivedBy, _⟩ :=
    decide_emits_only_derived_events (.completeWork state.revision target) state accepted
  simp only [deriveEvents] at derivedBy
  split at derivedBy
  · contradiction
  · split at derivedBy
    · assumption
    · contradiction

theorem replay_completion_applicability_matches_policy (target : WorkId)
    (state : State) :
    completionApplicable target state =
      completionReady target state := by
  unfold completionApplicable completionObligationsReady
    completionObligationSatisfied completionRelatedWorkTerminal
    completionReviewsReady latestCompletionReview
    completionRequiredReviewsReady completionPurposeReviewReady
    completionRequiredReviewPurposes
    completionReady Policy.Completion.closeable Policy.Completion.obligationsReady
    Policy.Completion.obligationSatisfied Policy.Completion.authoritativeReady
    Policy.Completion.relatedWorkTerminal Policy.Completion.reviewsReady
    Policy.Completion.traceReady Policy.Completion.requiredReviewsReady
    Policy.Completion.purposeReviewReady Policy.Completion.requiredReviewPurposes
    Policy.Completion.completionBinding?
    Policy.Completion.completionBindingReady Policy.Completion.correctionsReady
    completionBinding? completionBindingReady
    Policy.Traceability.ready
    Review.scopeReady
    Review.scopeFindingsClosed
    Policy.Completion.latestReviewClaim
  rfl

theorem close_work_emits_atomic_event (expectedRevision : Revision) (target : WorkId) (state : State)
    {transaction : CompletionTransaction}
    (accepted : closeWork expectedRevision target state = .ok transaction) :
    transaction.events = [.workCompleted transaction.target transaction.activation] := by
  unfold closeWork at accepted
  split at accepted
  · contradiction
  · split at accepted
    · contradiction
    · cases accepted
      exact decide_complete_emits expectedRevision target state _ _ (by assumption) (by assumption)

end AgentWorkbench.Kernel.Decide
