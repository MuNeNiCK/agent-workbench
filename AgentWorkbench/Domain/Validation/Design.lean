import AgentWorkbench.Domain.Lookup
import AgentWorkbench.Decision.PlanCoverage

namespace AgentWorkbench
namespace Validation

def ensure (condition : Bool) (message : String) : Except String Unit :=
  if condition then pure () else throw message

def requireSome (value : Option α) (message : String) : Except String α :=
  match value with
  | some item => pure item
  | none => throw message

def uniqueStrings (values : List String) : Bool :=
  values.all (fun value => values.count value == 1)

private def validLeanNamePart (value : String) : Bool :=
  match value.toList with
  | [] => false
  | first :: rest =>
      (first.isAlpha || first == '_') &&
      rest.all (fun char => char.isAlphanum || char == '_' || char == '\'')

private def validLeanName (value : String) : Bool :=
  !value.isEmpty && (value.splitOn ".").all validLeanNamePart

private def validLeanSource (source : SourceInput) : Bool :=
  let normalized := source.path.replace "\\" "/"
  !normalized.startsWith "/" && normalized.endsWith ".lean" &&
    !(normalized.splitOn "/").any (· == "..") &&
    validLeanName (normalized.dropEnd 5 |>.toString |>.replace "/" ".")

private def validProofRoot (value : String) : Bool :=
  let normalized := value.replace "\\" "/"
  !normalized.startsWith "/" && !(normalized.splitOn "/").any (· == "..") &&
    (normalized == ".agent-workbench/design/proofs" ||
      normalized.startsWith ".agent-workbench/design/proofs/")

private def claimSourceTarget (claim : LeanClaim) (source : SourceInput) : String :=
  let root := claim.input.proofRoot.replace "\\" "/" |>.dropEndWhile (· == '/') |>.toString
  "file:" ++ root ++ "/" ++ source.path.replace "\\" "/"

def validContentDigest (value : String) : Bool :=
  value.startsWith "blake3:"

private def validArchivedOrLegacyDigest (design : DesignRevision) (value : String) : Bool :=
  validContentDigest value ||
    (!design.sourceArchiveAvailable && value.startsWith "sha3-256:")

def validateCommand (command : CommandSpec) : Except String Unit := do
  ensure (!command.executable.isEmpty) "command executable is empty"
  ensure (command.environment.toList |> uniqueStrings)
    "command environment contains duplicate keys"

def validateDesign (design : DesignRevision) : Except String Unit := do
  ensure (!design.id.isEmpty) "design id is empty"
  ensure (!design.producerAgentRun.isEmpty) s!"design {design.id} has no producer"
  ensure (uniqueStrings (design.sourceDocuments.map (·.target)))
    s!"design {design.id} has duplicate source documents"
  for source in design.sourceDocuments do
    ensure (source.target.startsWith "file:" &&
      (source.mediaKind == "markdown" || source.mediaKind == "lean") &&
      validArchivedOrLegacyDigest design source.snapshot)
      s!"design {design.id} has an invalid source document"
  ensure (uniqueStrings (design.statements.map (·.id)))
    s!"design {design.id} has duplicate statement ids"
  ensure (uniqueStrings (design.acceptanceCriteria.map (·.id)))
    s!"design {design.id} has duplicate criterion ids"
  ensure (uniqueStrings (design.leanClaims.map (·.id)))
    s!"design {design.id} has duplicate claim ids"
  if design.sourceArchiveAvailable then
    ensure (design.workId.isSome && !design.changeRationale.isEmpty &&
      validContentDigest design.revisionContentDigest && !design.sourceDocuments.isEmpty)
      s!"design {design.id} has incomplete immutable history identity"
    ensure (uniqueStrings (design.sourceUnits.map (·.id)))
      s!"design {design.id} has duplicate source-unit ids"
    ensure (uniqueStrings (design.sourceUnitDispositions.map (·.unitId)))
      s!"design {design.id} has duplicate source-unit dispositions"
    ensure (uniqueStrings (design.assumptions.map (·.id)))
      s!"design {design.id} has duplicate assumptions"
    ensure (design.sourceUnitDispositions.length == design.sourceUnits.length)
      s!"design {design.id} does not classify every content-bearing source unit"
    for unit in design.sourceUnits do
      ensure (design.sourceDocuments.any (·.target == unit.target) &&
        design.sourceUnitDispositions.any (·.unitId == unit.id))
        s!"design {design.id} has an unbound source unit {unit.id}"
    for disposition in design.sourceUnitDispositions do
      let _ ← requireSome (uniqueBy? design.sourceUnits (·.id) disposition.unitId)
        s!"design {design.id} classifies an unknown source unit {disposition.unitId}"
      match disposition.role with
      | .rationale | .example | .reference =>
          ensure (disposition.reason.any fun reason => !reason.isEmpty)
            s!"design {design.id} has an unreasoned non-authoritative source unit"
      | .requirement | .assumption => pure ()
    for assumption in design.assumptions do
      ensure (!assumption.id.isEmpty && !assumption.text.isEmpty &&
        !assumption.sourceUnitIds.isEmpty && uniqueStrings assumption.sourceUnitIds)
        s!"assumption {assumption.id} is incomplete"
      for unitId in assumption.sourceUnitIds do
        let disposition ← requireSome
          (uniqueBy? design.sourceUnitDispositions (·.unitId) unitId)
          s!"assumption {assumption.id} grounds an unknown source unit"
        ensure (disposition.role == .assumption)
          s!"assumption {assumption.id} is grounded by a non-assumption source unit"
    ensure (uniqueStrings (design.statementCoverage.map (·.statementId)))
      s!"design {design.id} has duplicate Statement coverage"
    ensure (design.statementCoverage.length == design.statements.length)
      s!"design {design.id} does not cover every Statement"
    for coverage in design.statementCoverage do
      let statement ← requireSome (design.statement? coverage.statementId)
        s!"design {design.id} covers an unknown Statement {coverage.statementId}"
      ensure (!coverage.sourceUnitIds.isEmpty && uniqueStrings coverage.sourceUnitIds)
        s!"Statement {statement.id} has incomplete source grounding"
      for unitId in coverage.sourceUnitIds do
        let disposition ← requireSome
          (uniqueBy? design.sourceUnitDispositions (·.unitId) unitId)
          s!"Statement {statement.id} grounds an unknown source unit"
        ensure (disposition.role == .requirement)
          s!"Statement {statement.id} is grounded by a non-requirement source unit"
      let claimChoiceValid :=
        (!coverage.leanClaims.selectedIds.isEmpty && coverage.leanClaims.noSelectionReason.isNone) ||
        (coverage.leanClaims.selectedIds.isEmpty &&
          coverage.leanClaims.noSelectionReason.any fun reason => !reason.isEmpty)
      let criterionChoiceValid :=
        (!coverage.acceptanceCriteria.selectedIds.isEmpty &&
          coverage.acceptanceCriteria.noSelectionReason.isNone) ||
        (coverage.acceptanceCriteria.selectedIds.isEmpty &&
          coverage.acceptanceCriteria.noSelectionReason.any fun reason => !reason.isEmpty)
      ensure claimChoiceValid s!"Statement {statement.id} has no explicit Lean Claim choice"
      ensure criterionChoiceValid
        s!"Statement {statement.id} has no explicit Acceptance Criterion choice"
      ensure (coverage.implementationRequired != coverage.noImplementationReason.isSome &&
        (coverage.implementationRequired ||
          coverage.noImplementationReason.any fun reason => !reason.isEmpty))
        s!"Statement {statement.id} has no explicit implementation choice"
      for claimId in coverage.leanClaims.selectedIds do
        let claim ← requireSome (design.claim? claimId)
          s!"Statement {statement.id} selects missing Claim {claimId}"
        ensure (claim.input.statementId == statement.id)
          s!"Statement {statement.id} selects another Statement's Claim"
      for criterionId in coverage.acceptanceCriteria.selectedIds do
        let criterion ← requireSome (design.criterion? criterionId)
          s!"Statement {statement.id} selects missing Criterion {criterionId}"
        ensure (criterion.statementId == some statement.id)
          s!"Statement {statement.id} selects another Statement's Criterion"
      for assumptionId in statement.assumptions do
        let _ ← requireSome (design.assumption? assumptionId)
          s!"Statement {statement.id} references missing assumption {assumptionId}"
    for claim in design.leanClaims do
      ensure (design.statementCoverage.countP (fun coverage =>
        coverage.statementId == claim.input.statementId &&
          coverage.leanClaims.selectedIds.contains claim.id) == 1)
        s!"claim {claim.id} is declared but not selected exactly once by its Statement"
    for criterion in design.acceptanceCriteria do
      let statementId ← requireSome criterion.statementId
        s!"criterion {criterion.id} is not bound to a Statement"
      ensure (design.statementCoverage.countP (fun coverage =>
        coverage.statementId == statementId &&
          coverage.acceptanceCriteria.selectedIds.contains criterion.id) == 1)
        s!"criterion {criterion.id} is declared but not selected exactly once by its Statement"
    for disposition in design.sourceUnitDispositions do
      if disposition.role == .requirement then
        ensure (design.statementCoverage.any (·.sourceUnitIds.contains disposition.unitId))
          s!"requirement source unit {disposition.unitId} grounds no Statement"
      if disposition.role == .assumption then
        ensure (design.assumptions.any (·.sourceUnitIds.contains disposition.unitId))
          s!"assumption source unit {disposition.unitId} grounds no structured assumption"
    ensure (uniqueStrings (design.removedStatements.map (·.statementId)))
      s!"design {design.id} has duplicate removed-Statement tombstones"
    for removed in design.removedStatements do
      ensure (!removed.statementId.isEmpty && !removed.statementText.isEmpty &&
        removed.implementationRequired != removed.noImplementationReason.isSome &&
        (removed.implementationRequired ||
          removed.noImplementationReason.any fun reason => !reason.isEmpty))
        s!"removed Statement {removed.statementId} has no explicit implementation choice"
      ensure ((design.statement? removed.statementId).isNone)
        s!"removed Statement {removed.statementId} still exists in Design {design.id}"
  for criterion in design.acceptanceCriteria do
    ensure (!criterion.id.isEmpty && !criterion.statement.isEmpty && !criterion.target.isEmpty)
      s!"design {design.id} has an incomplete acceptance criterion"
    ensure (criterion.evidenceKind == "artifact" || criterion.evidenceKind == "command")
      s!"criterion {criterion.id} uses an unsupported evidence kind"
  for claim in design.leanClaims do
    let statement ← requireSome (design.statement? claim.input.statementId)
      s!"claim {claim.id} references a missing statement"
    ensure (statement.text == claim.input.statementText)
      s!"claim {claim.id} is not bound to the exact statement text"
    if design.sourceArchiveAvailable then
      ensure (!claim.input.mapping.isEmpty && validLeanName claim.input.proposition &&
        validLeanName claim.input.witness && validProofRoot claim.input.proofRoot)
        s!"claim {claim.id} has incomplete proof input"
      ensure (validContentDigest claim.elaboratedPropositionDigest &&
        uniqueStrings claim.propositionDependencies)
        s!"claim {claim.id} has no valid pinned proposition elaboration"
      ensure (claim.input.toolchain == ProofToolchain.identifier)
        s!"claim {claim.id} does not use the product's pinned Lean toolchain"
      ensure (claim.input.assumptions.all validLeanName)
        s!"claim {claim.id} has an assumption that is not a Lean declaration name"
      ensure (!claim.input.declaredSources.isEmpty)
        s!"claim {claim.id} has no declared source"
      ensure (claim.input.declaredSources.all validLeanSource)
        s!"claim {claim.id} has a source that cannot be imported as a Lean module"
      for source in claim.input.declaredSources do
        let digest ← requireSome source.expectedDigest
          s!"claim {claim.id} has an unbound Lean source digest"
        ensure (design.sourceDocuments.any fun archived =>
          archived.target == claimSourceTarget claim source &&
          archived.mediaKind == "lean" && archived.snapshot == digest)
          s!"claim {claim.id} source is not in the immutable Design archive: {source.path}"
      validateCommand claim.input.check
  if design.sourceArchiveAvailable then
    for archived in design.sourceDocuments do
      if archived.mediaKind == "lean" then
        ensure (design.leanClaims.any fun claim =>
          claim.input.declaredSources.any fun source =>
            claimSourceTarget claim source == archived.target &&
              source.expectedDigest == some archived.snapshot)
          s!"Design {design.id} contains an unbound Lean source archive"

def findingAccepted (state : ProjectState) (findingId workId : String) : Bool :=
  state.findingAccepted findingId workId

def changeBasisValid
    (state : ProjectState) (design : DesignRevision) (basisId : String) : Bool :=
  match state.entry? basisId with
  | none => false
  | some basis =>
      if basis.order > design.createdAfterEntryOrder || basis.workId != design.workId then false
      else match basis.payload with
      | .userCorrection _ => true
      | .finding _ => design.workId.any fun workId =>
          state.findingAcceptedAt basis.id workId design.createdAfterEntryOrder
      | _ => false

def validateDesignRelations
    (state : ProjectState) (design : DesignRevision) : Except String Unit := do
  if let some parentId := design.parent then
    let parent ← requireSome (state.design? parentId)
      s!"Design {design.id} references missing accepted parent {parentId}"
    ensure (parent.id != design.id && !state.designDescendsFrom design.id parent.id)
      s!"Design {design.id} has cyclic accepted ancestry"
    if design.sourceArchiveAvailable then
      ensure (design.workId.isSome &&
        (parent.workId.isNone || design.workId == parent.workId))
        s!"Design {design.id} crosses its Work ancestry"
      let expectedRemoved := parent.statements.filter fun statement =>
        (design.statement? statement.id).isNone
      ensure (expectedRemoved.length == design.removedStatements.length &&
        expectedRemoved.all fun statement => design.removedStatements.any fun removed =>
          removed.statementId == statement.id && removed.statementText == statement.text)
        s!"Design {design.id} does not preserve the exact removed-Statement delta"
  else if design.sourceArchiveAvailable then
    ensure design.removedStatements.isEmpty
      s!"initial Design {design.id} has removed-Statement tombstones"
  if let some predecessorId := design.amendsCandidate then
    let predecessor ← requireSome (state.design? predecessorId)
      s!"Design {design.id} amends missing candidate {predecessorId}"
    ensure (predecessor.id != design.id && predecessor.parent == design.parent &&
      predecessor.workId == design.workId && predecessor.status == .superseded)
      s!"Design {design.id} has invalid candidate amendment ancestry"
    let rec followsAmendment : Nat → DesignRevision → Bool
      | 0, _ => false
      | fuel + 1, current =>
          match current.amendsCandidate.bind state.design? with
          | none => false
          | some prior => prior.id == design.id || followsAmendment fuel prior
    ensure (!followsAmendment (state.designRevisions.length + 1) predecessor)
      s!"Design {design.id} has cyclic candidate amendment ancestry"
    let acceptedFindingBases := state.ledgerEntries.filterMap fun entry =>
      if entry.designRevision == some predecessorId && entry.workId == design.workId &&
          entry.order <= design.createdAfterEntryOrder &&
          state.findingAcceptedAt entry.id (design.workId.getD "") design.createdAfterEntryOrder then
        match entry.payload with | .finding _ => some entry.id | _ => none
      else none
    ensure (acceptedFindingBases.all design.changeBasisEntryIds.contains)
      s!"Design {design.id} omits an accepted Finding that caused its candidate amendment"
  let activeAmendments := state.designRevisions.filter fun candidate =>
    candidate.amendsCandidate == some design.id && candidate.status == .candidate
  ensure (activeAmendments.length <= 1)
    s!"Design {design.id} has multiple current candidate amendments"
  if !activeAmendments.isEmpty then
    ensure (design.status == .superseded)
      s!"amended candidate {design.id} was not superseded atomically"
  ensure (uniqueStrings design.changeBasisEntryIds &&
    design.changeBasisEntryIds.all (changeBasisValid state design))
    s!"Design {design.id} has invalid causal change bases"



end Validation
end AgentWorkbench
