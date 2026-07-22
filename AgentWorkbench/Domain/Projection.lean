import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Projection

open AgentWorkbench.Domain

structure LedgerPoint where
  ledger : LedgerId
  revision : Revision
  historyDigest : Digest
deriving DecidableEq, Repr

structure ProjectionFingerprint where
  id : ProjectionId
  rawDigest : Digest
deriving DecidableEq, Repr

structure ProjectionRef where
  fingerprint : ProjectionFingerprint
  ledger : LedgerId
  revision : Revision
  historyDigest : Digest
  stateDigest : Digest
deriving DecidableEq, Repr

inductive DecodeFault
  | unreadable
  | unsupportedSchema
deriving DecidableEq, Repr

inductive LedgerFault
  | replayRejected (error : DomainError)
  | headRevisionMismatch (replayed stored : Revision)
  | historyDigestMismatch (replayed stored : Digest)
deriving DecidableEq, Repr

inductive ProjectionFault
  | undecodable (fault : DecodeFault)
  | wrongLedger (observed expected : LedgerId)
  | aheadOfLedger (observed expected : Revision)
  | historyDigestMismatch
  | stateDigestMismatch
  | replayMismatch
deriving DecidableEq, Repr

structure RepairBinding where
  head : LedgerPoint
  observed : Option ProjectionFingerprint
deriving DecidableEq, Repr

structure RepairCommand where
  binding : RepairBinding
deriving DecidableEq, Repr

end AgentWorkbench.Domain.Projection
