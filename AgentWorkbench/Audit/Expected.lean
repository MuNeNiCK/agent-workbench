import AgentWorkbench.Application.Service

namespace AgentWorkbench.Audit.Expected

open AgentWorkbench.Domain
open AgentWorkbench.Kernel
open AgentWorkbench.Kernel.Replay

def cliWithoutMutationFixture : IO Unit :=
  pure ()

def expectedExecuteBootstrap :=
  Application.Service.execute Application.Service.bootstrapCommand
    Application.Service.initialStore

def expectedCliRun : IO Unit := do
  match expectedExecuteBootstrap with
  | .error error => throw <| IO.userError s!"verified mutation rejected: {repr error}"
  | .ok transaction =>
      match (Application.Service.queryValidity transaction.result).value,
          (Application.Service.resolve transaction.result).value with
      | .pass, .action action =>
          match Application.Service.executeRequest (.action action) transaction.result with
          | .ok response => IO.println s!"agent-workbench verified core: {response.output}"
          | .error error => throw <| IO.userError s!"resolver action rejected: {error}"
      | .blocked reason, _ => throw <| IO.userError reason
      | _, .blocked blocker => throw <| IO.userError s!"resolver blocked: {repr blocker}"

def cliConditionalBypassFixture : IO Unit := do
  let takeMutation ← pure false
  if takeMutation then
    match Application.Service.execute Application.Service.bootstrapCommand
        Application.Service.initialStore with
    | .ok _ => pure ()
    | .error _ => pure ()
  else
    pure ()

axiom replay_deterministic (events : List Event) (initial : State)
    {left right : VerifiedState}
    (leftResult : replay events initial = .ok left)
    (rightResult : replay events initial = .ok right) :
    left.state = right.state

axiom replay_preserves_valid (events : List Event) (initial : State)
    {result : VerifiedState} (_accepted : replay events initial = .ok result) :
    ValidState result.state

axiom work_completed_event_exact (verified : VerifiedState)
    (work : WorkId) (activation : ActivationId) {completed : VerifiedState}
    (accepted : applyEvent (.workCompleted work activation) verified = .ok completed) :
    completed.state.work = Work.closeWork verified.state.work work ∧
    completed.state.activations = Work.closeActivation verified.state.activations activation ∧
    completed.state.revision = verified.state.revision.next

axiom decide_preserves_valid (command : Decide.Command) (state : State)
    {transaction : Decide.AcceptedTransaction}
    (_accepted : Decide.decide command state = .ok transaction) :
    ValidState transaction.result.state

axiom decide_emits_only_derived_events (command : Decide.Command) (state : State)
    {transaction : Decide.AcceptedTransaction}
    (accepted : Decide.decide command state = .ok transaction) :
    ∃ derived, Decide.deriveEvents command state = .ok derived ∧
      transaction.events = derived.events

axiom decide_rejection_has_no_effect (command : Decide.Command) (state : State)
    (error : DomainError) (rejected : Decide.decide command state = .error error) :
    (¬ ∃ events result, Decide.Commits command state events result) ∧
    Decide.committedEvents (Decide.decide command state) = [] ∧
    Decide.committedState (Decide.decide command state) state = state ∧
    (Decide.committedState (Decide.decide command state) state).revision = state.revision

axiom close_work_preserves_valid (expectedRevision : Revision) (target : WorkId) (state : State)
    {transaction : Decide.CompletionTransaction}
    (_accepted : Decide.closeWork expectedRevision target state = .ok transaction) :
    ValidState transaction.result.state

axiom close_work_emits_atomic_event (expectedRevision : Revision) (target : WorkId) (state : State)
    {transaction : Decide.CompletionTransaction}
    (accepted : Decide.closeWork expectedRevision target state = .ok transaction) :
    transaction.events = [.workCompleted transaction.target transaction.activation]

axiom decide_complete_requires_closeable (target : WorkId) (state : State)
    {transaction : Decide.AcceptedTransaction}
    (accepted : Decide.decide (.completeWork state.revision target) state = .ok transaction) :
    Decide.completionReady target state = true

axiom single_active_activation {activations : List Work.Activation}
    (valid : Work.AtMostOneActive activations) :
    (Work.activeActivations activations).length ≤ 1

axiom resume_requires_readiness {activations : List Work.Activation} {id : ActivationId}
    {resumed : List Work.Activation} (accepted : Work.resume activations id = some resumed) :
    Work.resumable activations id = true

axiom review_claim_has_no_authority (state : Policy.Authority.ReviewState)
    (claim : Review.Claim) :
    Policy.Authority.authority (Policy.Authority.recordClaim state claim) =
      Policy.Authority.authority state

axiom gate_is_read_only (gate : Projection.Store → GateResult) (store : Projection.Store) :
    (Gates.observeGate gate store).1 = store

axiom next_is_allowed (inspection : Projection.Inspection) :
    match Resolver.next inspection with
    | .action action => action.executable inspection = true
    | .blocked blocker => blocker.exact inspection

axiom status_is_read_only (store : Projection.Store) :
    (Application.Service.status store).store = store

axiom next_is_read_only (store : Projection.Store) :
    (Application.Service.resolve store).store = store

axiom gates_all_read_only (request : Gates.Request) (store : Projection.Store) :
    (Gates.observeGate (Gates.run request) store).1 = store

axiom every_gate_is_read_only (request : Gates.Request) (store : Projection.Store) :
    (Application.Service.queryGate request store).store = store

axiom verified_stage_matches_replay (verified : Projection.VerifiedStage) :
    verified.candidateState = verified.ledger.head.state ∧
    Projection.projectionMatchesHead verified.ledger verified.stage.candidate = true

axiom adoption_is_atomic (transaction : Projection.AdoptionTransaction) :
    transaction.result.ledger = transaction.sourceLedger ∧
    transaction.result.active = some transaction.candidate

axiom completion_requires_current_obligations (target : WorkId)
    (work : List Work.WorkUnit) (activations : List Work.Activation)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (reviewPlans : List Review.Plan) (findings : List Review.Finding)
    (verifications : List Review.Verification)
    (lifecycle : List Lifecycle.CompletionState)
    (evidence : List Evidence.Evidence) (obligations : List Evidence.Obligation)
    (designs : List Design.DesignVersion) (approvals : List Design.Approval)
    (decompositions : List Design.Decomposition) (corrections : List Design.Correction)
    (accepted : Policy.Completion.closeable target work activations claims adjudications
      reviewPlans findings verifications lifecycle evidence obligations designs approvals
      decompositions corrections = true) :
    Policy.Completion.obligationsReady target evidence obligations = true

axiom completion_requires_authoritative_lifecycle (target : WorkId)
    (work : List Work.WorkUnit) (activations : List Work.Activation)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (reviewPlans : List Review.Plan) (findings : List Review.Finding)
    (verifications : List Review.Verification)
    (lifecycle : List Lifecycle.CompletionState)
    (evidence : List Evidence.Evidence) (obligations : List Evidence.Obligation)
    (designs : List Design.DesignVersion) (approvals : List Design.Approval)
    (decompositions : List Design.Decomposition) (corrections : List Design.Correction)
    (accepted : Policy.Completion.closeable target work activations claims adjudications
      reviewPlans findings verifications lifecycle evidence obligations designs approvals
      decompositions corrections = true) :
    Policy.Completion.authoritativeReady target work claims adjudications lifecycle = true

axiom completion_requires_active_target (target : WorkId)
    (work : List Work.WorkUnit) (activations : List Work.Activation)
    (claims : List Review.Claim) (adjudications : List Review.Adjudication)
    (reviewPlans : List Review.Plan) (findings : List Review.Finding)
    (verifications : List Review.Verification)
    (lifecycle : List Lifecycle.CompletionState)
    (evidence : List Evidence.Evidence) (obligations : List Evidence.Obligation)
    (designs : List Design.DesignVersion) (approvals : List Design.Approval)
    (decompositions : List Design.Decomposition) (corrections : List Design.Correction)
    (accepted : Policy.Completion.closeable target work activations claims adjudications
      reviewPlans findings verifications lifecycle evidence obligations designs approvals
      decompositions corrections = true) :
    (Work.activeFor activations target).isSome = true

axiom replay_completion_applicability_matches_policy (target : WorkId)
    (state : Replay.State) :
    Replay.completionApplicable target state =
      Decide.completionReady target state

axiom exact_retry_returns_same_receipt
    (operation : OperationId) (payloadDigest : String)
    (expectedRevision currentRevision : Revision) (receipts : List Policy.Update.Receipt)
    (receipt : Policy.Update.Receipt)
    (found : Policy.Update.lookupReceipt operation receipts = some receipt)
    (samePayload : receipt.payloadDigest = payloadDigest) :
    Policy.Update.resolveRetry operation payloadDigest expectedRevision currentRevision receipts =
      .exact receipt

end AgentWorkbench.Audit.Expected
