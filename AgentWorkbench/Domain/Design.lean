import Lean.Data.Json
import AgentWorkbench.Domain.ProofToolchain
import AgentWorkbench.Domain.DesignSourceGraph

namespace AgentWorkbench

inductive DesignStatus where
  | candidate
  | accepted
  | superseded
  /-- `replaced` is legacy; `rejected` is an explicit non-authoritative terminal status. -/
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
  mediaKind : String := "markdown"
  snapshot : String
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure AcceptanceCriterion where
  id : String
  statementId : Option String := none
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
  elaboratedPropositionDigest : String := ""
  propositionDependencies : List String := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

structure DesignRevision where
  id : String
  workId : Option String := none
  parent : Option String := none
  amendsCandidate : Option String := none
  createdAfterEntryOrder : Nat := 0
  status : DesignStatus := .candidate
  producerAgentRun : String
  changeRationale : String := "legacy source unavailable"
  changeBasisEntryIds : List String := []
  revisionContentDigest : String := ""
  sourceArchiveAvailable : Bool := false
  sourceDocuments : List DesignSource := []
  sourceUnits : List DesignSourceUnit := []
  sourceUnitDispositions : List SourceUnitDisposition := []
  assumptions : List DesignAssumption := []
  statements : List Statement
  statementCoverage : List StatementCoverage := []
  removedStatements : List RemovedStatementTombstone := []
  acceptanceCriteria : List AcceptanceCriterion
  leanClaims : List LeanClaim := []
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

end AgentWorkbench
