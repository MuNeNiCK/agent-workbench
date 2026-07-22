import AgentWorkbench.Application.Service

namespace AgentWorkbench.Audit.Expected

open AgentWorkbench.Domain
open AgentWorkbench.Kernel
open AgentWorkbench.Kernel.Replay

axiom replay_deterministic (events : List Event) (initial : State)
    {left right : VerifiedState}
    (leftResult : replay events initial = .ok left)
    (rightResult : replay events initial = .ok right) :
    left.state = right.state

axiom replay_preserves_valid (events : List Event) (initial : State)
    {result : VerifiedState} (_accepted : replay events initial = .ok result) :
    ValidState result.state

axiom work_completed_event_exact (state : State) (work : WorkId) (activation : ActivationId) :
    let completed := applyUnchecked (.workCompleted work activation) state
    completed.work = Work.closeWork state.work work ∧
    completed.activations = Work.closeActivation state.activations activation ∧
    completed.revision = state.revision.next

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

axiom close_work_preserves_valid (target : WorkId) (state : State)
    {transaction : Decide.CompletionTransaction}
    (_accepted : Decide.closeWork target state = .ok transaction) :
    ValidState transaction.result.state

axiom close_work_emits_atomic_event (target : WorkId) (state : State)
    {transaction : Decide.CompletionTransaction}
    (accepted : Decide.closeWork target state = .ok transaction) :
    transaction.events = [.workCompleted transaction.target transaction.activation]

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

axiom gate_is_read_only (gate : State → GateResult) (state : State) :
    (Gates.observeGate gate state).1 = state

axiom next_is_allowed (state : State) :
    match Resolver.next state with
    | .action action => action.executable state = true
    | .blocked blocker => blocker.exact state

axiom completion_requires_current_obligations (target : WorkId)
    (work : List Work.WorkUnit) (activations : List Work.Activation)
    (facts : List Work.CompletionFacts) (obligations : List Evidence.Obligation)
    (accepted : Policy.Completion.closeable target work activations facts obligations = true) :
    Policy.Completion.obligationsReady obligations target = true

axiom completion_requires_active_target (target : WorkId)
    (work : List Work.WorkUnit) (activations : List Work.Activation)
    (facts : List Work.CompletionFacts) (obligations : List Evidence.Obligation)
    (accepted : Policy.Completion.closeable target work activations facts obligations = true) :
    (Work.activeFor activations target).isSome = true

axiom exact_retry_returns_same_receipt
    (operation : OperationId) (payloadDigest : String)
    (expectedRevision currentRevision : Revision) (receipts : List Policy.Update.Receipt)
    (receipt : Policy.Update.Receipt)
    (found : Policy.Update.lookupReceipt operation receipts = some receipt)
    (samePayload : receipt.payloadDigest = payloadDigest) :
    Policy.Update.resolveRetry operation payloadDigest expectedRevision currentRevision receipts =
      .exact receipt

end AgentWorkbench.Audit.Expected
