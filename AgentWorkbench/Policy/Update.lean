import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Policy.Update

open AgentWorkbench.Domain

structure Receipt where
  operation : OperationId
  payloadDigest : String
  resultDigest : String
deriving DecidableEq, Repr

inductive RetryResolution
  | exact (receipt : Receipt)
  | payloadConflict
  | unseen
deriving DecidableEq, Repr

def lookupReceipt (operation : OperationId) (receipts : List Receipt) : Option Receipt :=
  receipts.find? fun receipt => receipt.operation == operation

def resolveRetry (operation : OperationId) (payloadDigest : String)
    (_expectedRevision _currentRevision : Revision) (receipts : List Receipt) : RetryResolution :=
  match lookupReceipt operation receipts with
  | some receipt =>
      if receipt.payloadDigest == payloadDigest then .exact receipt else .payloadConflict
  | none => .unseen

theorem exact_retry_returns_same_receipt
    (operation : OperationId) (payloadDigest : String)
    (expectedRevision currentRevision : Revision) (receipts : List Receipt)
    (receipt : Receipt)
    (found : lookupReceipt operation receipts = some receipt)
    (samePayload : receipt.payloadDigest = payloadDigest) :
    resolveRetry operation payloadDigest expectedRevision currentRevision receipts = .exact receipt := by
  unfold resolveRetry
  rw [found]
  subst payloadDigest
  simp

end AgentWorkbench.Policy.Update
