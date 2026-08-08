import AgentWorkbench.Application.Mutation
import AgentWorkbench.Adapter.StoreSchemaInventory
import AgentWorkbench.Adapter.ManagedOutput
import AgentWorkbench.Adapter.ProofBuild
import AgentWorkbenchProof.InvariantFamily

namespace AgentWorkbenchProof.PersistedFields

open AgentWorkbench

private def cover (owner : InvariantFamily) (fields : List String) : List FieldCoverage :=
  fields.map fun field => { field, owner }

/-- Positional inventories are intentionally over production constructors. A persisted field
addition, removal, or reorder makes this private release artifact fail to compile. -/
def statement : Statement → List FieldCoverage
  | .mk _ _ _ => cover .designHistory ["id", "text", "assumptions"]

def designSource : DesignSource → List FieldCoverage
  | .mk _ _ _ => cover .designHistory ["target", "mediaKind", "snapshot"]

def acceptanceCriterion : AcceptanceCriterion → List FieldCoverage
  | .mk _ _ _ _ _ => cover .designHistory
      ["id", "statementId", "statement", "target", "evidenceKind"]

def sourceInput : SourceInput → List FieldCoverage
  | .mk _ _ => cover .designHistory ["path", "expectedDigest"]

def commandSpec : CommandSpec → List FieldCoverage
  | .mk _ _ _ _ => cover .ledgerAuthority
      ["executable", "arguments", "workingDirectory", "environment"]

def claimInput : ClaimInput → List FieldCoverage
  | .mk _ _ _ _ _ _ _ _ _ _ =>
      cover .designHistory ["statementId", "statementText", "mapping", "proposition", "witness", "assumptions",
       "proofRoot", "declaredSources", "check", "toolchain"]

def leanClaim : LeanClaim → List FieldCoverage
  | .mk _ _ _ _ =>
      cover .designHistory ["id", "input", "elaboratedPropositionDigest", "propositionDependencies"]

def sourceUnit : DesignSourceUnit → List FieldCoverage
  | .mk _ _ _ _ _ _ _ =>
      cover .designHistory ["id", "target", "path", "kind", "headingAncestry", "text", "digest"]

def sourceDisposition : SourceUnitDisposition → List FieldCoverage
  | .mk _ _ _ => cover .designHistory ["unitId", "role", "reason"]

def designAssumption : DesignAssumption → List FieldCoverage
  | .mk _ _ _ => cover .designHistory ["id", "text", "sourceUnitIds"]

def selectionChoice : SelectionChoice → List FieldCoverage
  | .mk _ _ => cover .designHistory ["selectedIds", "noSelectionReason"]

def statementCoverage : StatementCoverage → List FieldCoverage
  | .mk _ _ _ _ _ _ => cover .designHistory ["statementId", "sourceUnitIds", "leanClaims",
      "acceptanceCriteria", "implementationRequired", "noImplementationReason"]

def removedStatement : RemovedStatementTombstone → List FieldCoverage
  | .mk _ _ _ _ =>
      cover .designHistory ["statementId", "statementText", "implementationRequired", "noImplementationReason"]

def designRevision : DesignRevision → List FieldCoverage
  | .mk _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ =>
      cover .designHistory ["id", "workId", "parent", "amendsCandidate", "createdAfterEntryOrder", "status",
       "producerAgentRun", "changeRationale", "changeBasisEntryIds", "revisionContentDigest",
       "sourceArchiveAvailable", "sourceDocuments", "sourceUnits", "sourceUnitDispositions",
       "assumptions", "statements", "statementCoverage", "removedStatements",
       "acceptanceCriteria", "leanClaims"]

def work : Work → List FieldCoverage
  | .mk _ _ _ _ _ _ _ _ _ => cover .workLifecycle ["id", "outcome", "scope", "baselineDesignRevision",
      "designRevision", "status", "responsibleAgentRun", "resumeCondition", "migrationDiagnostic"]

def planSource : PlanSource → List FieldCoverage
  | .mk _ _ => cover .planTask ["target", "digest"]

def planSourceDisposition : PlanSourceUnitDisposition → List FieldCoverage
  | .mk _ _ _ => cover .planTask ["unitId", "stepId", "noStepReason"]

def planStatementDisposition : PlanStatementDisposition → List FieldCoverage
  | .mk _ _ _ _ _ =>
      cover .planTask ["statementId", "statementText", "deltaKind", "stepIds", "noActionReason"]

def planStep : PlanStep → List FieldCoverage
  | .mk _ _ _ _ _ _ _ => cover .planTask ["id", "description", "dependsOnStepIds", "outputScopes",
      "requiredClaimIds", "verificationCriterionIds", "acceptedFindingEntryIds"]

def implementationPlan : ImplementationPlan → List FieldCoverage
  | .mk _ _ _ _ _ _ _ _ _ _ _ _ _ _ _ => cover .planTask ["id", "workId", "designRevision",
      "predecessorPlanId", "status", "producerAgentRun", "reason", "changeBasisEntryIds",
      "contentDigest", "sourceArchiveAvailable", "sourceDocuments", "sourceUnits",
      "sourceUnitDispositions", "statementDispositions", "steps"]

def reviewTargetComponent : ReviewTargetComponent → List FieldCoverage
  | .mk _ _ _ _ => cover .ledgerAuthority ["kind", "id", "snapshot", "producerAgentRuns"]

def findingSubject : FindingSubject → List FieldCoverage
  | .mk _ _ _ => cover .ledgerAuthority ["kind", "id", "exactQuote"]

def proofSourceDigest : ProofSourceDigest → List FieldCoverage
  | .mk _ _ => cover .ledgerAuthority ["path", "digest"]

def ledgerEntry : LedgerEntry → List FieldCoverage
  | .mk _ _ _ _ _ _ _ =>
      cover .ledgerAuthority ["id", "order", "scope", "workId", "designRevision", "supersedes", "payload"]

def projectState : ProjectState → List FieldCoverage
  | .mk _ _ _ _ _ _ _ =>
      [{ field := "revision", owner := .ledgerAuthority },
       { field := "acceptedDesignId", owner := .designHistory },
       { field := "focusedWorkId", owner := .workLifecycle },
       { field := "designRevisions", owner := .designHistory },
       { field := "works", owner := .workLifecycle },
       { field := "implementationPlans", owner := .planTask },
       { field := "ledgerEntries", owner := .ledgerAuthority }]

/-- Recovery manifests are serialized inside one SQLite column, but their authority-bearing
structure is still covered positionally. A field change must update this release proof. -/
def managedOutputNode : ManagedOutput.Node → List FieldCoverage
  | .mk _ _ _ => cover .ledgerAuthority ["relativePath", "directory", "contentBytes"]

def managedOutputBaseline : ManagedOutput.Baseline → List FieldCoverage
  | .mk _ _ _ _ _ => cover .ledgerAuthority ["identity", "kind", "existed", "nodes", "digest"]

def proofOutputLayout : ProofBuild.OutputLayout → List FieldCoverage
  | .mk _ _ _ _ _ _ => cover .ledgerAuthority
      ["original", "existed", "parentExisted", "backup", "isolated", "baselineDigest"]

def proofManagedOutputManifest : ProofBuild.ManagedOutputManifest → List FieldCoverage
  | .mk _ => cover .ledgerAuthority ["layouts"]

/-- Exhaustive ownership for every authoritative SQLite column. The actual schema is checked
against `PersistedColumn.all` by the product test suite. -/
def sqliteColumnOwner : StoreSchema.PersistedColumn → InvariantFamily
  | .metadataAcceptedDesign | .designId | .designAcceptedParent | .designAmendsCandidate
  | .designStatus | .designProducer | .designRationale | .designDigest | .designDocument
  | .designBasisDesign | .designBasisOrdinal | .designBasisLedgerEntry
  | .designSourceDesign | .designSourceOrdinal | .designSourceTarget | .designSourceMediaKind
  | .designSourceDigest | .designSourceContent => .designHistory
  | .metadataFocusedWork | .workId | .workStatus | .workScope | .workOutcome
  | .workBaselineDesign | .workDesign | .workResponsible | .workResumeCondition
  | .workMigrationDiagnostic
  | .workDocument => .workLifecycle
  | .planId | .planWork | .planDesign | .planPredecessor | .planStatus | .planProducer
  | .planReason | .planDigest | .planDocument | .planBasisPlan | .planBasisOrdinal
  | .planBasisLedgerEntry | .planSourcePlan | .planSourceOrdinal | .planSourceTarget
  | .planSourceDigest | .planSourceContent => .planTask
  | .metadataSingleton | .metadataSchemaRevision | .metadataStateRevision
  | .ledgerId | .ledgerOrder | .ledgerScope | .ledgerWork | .ledgerDesign
  | .ledgerPayloadKind | .ledgerDocument | .managedOperationId | .managedExpectedRevision
  | .managedRecoveryPolicy | .managedManifest | .managedCommittedRevision => .ledgerAuthority

end AgentWorkbenchProof.PersistedFields
