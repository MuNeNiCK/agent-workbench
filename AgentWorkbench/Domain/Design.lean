import Lean.Data.Json
import AgentWorkbench.Domain.ProofToolchain

namespace AgentWorkbench

inductive DesignStatus where
  | candidate
  | accepted
  | replaced
  | rejected
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure Statement where
  id : String
  text : String
  assumptions : List String := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignSource where
  target : String
  snapshot : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure AcceptanceCriterion where
  id : String
  statement : String
  target : String
  evidenceKind : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure SourceInput where
  path : String
  expectedDigest : Option String := none
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure CommandSpec where
  executable : String
  arguments : Array String := #[]
  workingDirectory : Option String := none
  environment : Array (String × String) := #[]
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure ClaimInput where
  statementId : String
  statementText : String
  mapping : String
  proposition : String
  witness : String
  assumptions : List String := []
  proofRoot : String
  declaredSources : List SourceInput
  check : CommandSpec
  toolchain : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure LeanClaim where
  id : String
  input : ClaimInput
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignRevision where
  id : String
  parent : Option String := none
  createdAfterEntryOrder : Nat := 0
  status : DesignStatus := .candidate
  producerAgentRun : String
  sourceDocuments : List DesignSource := []
  statements : List Statement
  acceptanceCriteria : List AcceptanceCriterion
  leanClaims : List LeanClaim := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

end AgentWorkbench
