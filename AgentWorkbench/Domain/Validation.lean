import AgentWorkbench.Domain.Lookup

namespace AgentWorkbench

private def ensure (condition : Bool) (message : String) : Except String Unit :=
  if condition then pure () else throw message

private def requireSome (value : Option α) (message : String) : Except String α :=
  match value with
  | some item => pure item
  | none => throw message

private def uniqueStrings (values : List String) : Bool :=
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

private def validContentDigest (value : String) : Bool :=
  value.startsWith "blake3:" || value.startsWith "sha3-256:"

private def validateCommand (command : CommandSpec) : Except String Unit := do
  ensure (!command.executable.isEmpty) "command executable is empty"
  ensure (command.environment.toList.map (·.1) |> uniqueStrings)
    "command environment contains duplicate keys"

private def validateDesign (design : DesignRevision) : Except String Unit := do
  ensure (!design.id.isEmpty) "design id is empty"
  ensure (!design.producerAgentRun.isEmpty) s!"design {design.id} has no producer"
  ensure (uniqueStrings (design.sourceDocuments.map (·.target)))
    s!"design {design.id} has duplicate source documents"
  for source in design.sourceDocuments do
    ensure (source.target.startsWith "file:" && validContentDigest source.snapshot)
      s!"design {design.id} has an invalid source document"
  ensure (uniqueStrings (design.statements.map (·.id)))
    s!"design {design.id} has duplicate statement ids"
  ensure (uniqueStrings (design.acceptanceCriteria.map (·.id)))
    s!"design {design.id} has duplicate criterion ids"
  ensure (uniqueStrings (design.leanClaims.map (·.id)))
    s!"design {design.id} has duplicate claim ids"
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
    ensure (!claim.input.mapping.isEmpty && validLeanName claim.input.proposition &&
      validLeanName claim.input.witness && !claim.input.proofRoot.isEmpty)
      s!"claim {claim.id} has incomplete proof input"
    ensure (claim.input.toolchain == ProofToolchain.identifier)
      s!"claim {claim.id} does not use the product's pinned Lean toolchain"
    ensure (claim.input.assumptions.all validLeanName)
      s!"claim {claim.id} has an assumption that is not a Lean declaration name"
    ensure (!claim.input.declaredSources.isEmpty)
      s!"claim {claim.id} has no declared source"
    ensure (claim.input.declaredSources.all validLeanSource)
      s!"claim {claim.id} has a source that cannot be imported as a Lean module"
    validateCommand claim.input.check

private def entryDesign? (state : ProjectState) (entry : LedgerEntry) : Option DesignRevision :=
  entry.designRevision.bind state.design?

private def validateEntryReferences
    (state : ProjectState) (entry : LedgerEntry) : Except String Unit := do
  if let some workId := entry.workId then
    let _ ← requireSome (state.work? workId) s!"entry {entry.id} references missing work {workId}"
  if let some designId := entry.designRevision then
    let _ ← requireSome (state.design? designId)
      s!"entry {entry.id} references missing design {designId}"
  for replacedId in entry.supersedes do
    ensure (replacedId != entry.id) s!"entry {entry.id} supersedes itself"
    let replaced ← requireSome (state.entry? replacedId)
      s!"entry {entry.id} supersedes missing entry {replacedId}"
    ensure (replaced.order < entry.order) s!"entry {entry.id} supersedes a non-earlier entry"
    ensure (replaced.scope == entry.scope && replaced.workId == entry.workId &&
      replaced.designRevision == entry.designRevision &&
      replaced.payload.tag == entry.payload.tag)
      s!"entry {entry.id} crosses a supersession boundary"
  match entry.payload with
  | .kpt _ => ensure entry.supersedes.isEmpty "KPT cannot supersede another entry"
  | _ => pure ()

private def validateTask
    (state : ProjectState) (entry : LedgerEntry) (task : TaskRecord) : Except String Unit := do
  ensure entry.workId.isSome s!"task {entry.id} is not work-bound"
  ensure entry.designRevision.isSome s!"task {entry.id} is not design-bound"
  if task.required then
    let criterionId ← requireSome task.criterionId s!"required task {entry.id} has no criterion"
    let design ← requireSome (entryDesign? state entry) s!"task {entry.id} has no design"
    let _ ← requireSome (design.criterion? criterionId)
      s!"task {entry.id} references missing criterion {criterionId}"

private def validateEvidenceBinding
    (state : ProjectState) (entry : LedgerEntry)
    (criterionId target evidenceKind : String) : Except String Unit := do
  ensure entry.workId.isSome s!"evidence {entry.id} is not work-bound"
  let design ← requireSome (entryDesign? state entry) s!"evidence {entry.id} has no design"
  let criterion ← requireSome (design.criterion? criterionId)
    s!"evidence {entry.id} references missing criterion {criterionId}"
  ensure (criterion.target == target) s!"evidence {entry.id} target differs from its criterion"
  ensure (criterion.evidenceKind == evidenceKind)
    s!"evidence {entry.id} kind differs from its criterion"

private def priorReview? (state : ProjectState) (entryId : String) : Option (LedgerEntry × ReviewRecord) := do
  let entry ← state.entry? entryId
  match entry.payload with
  | .review review => some (entry, review)
  | _ => none

private def validateReview
    (state : ProjectState) (entry : LedgerEntry) (review : ReviewRecord) : Except String Unit := do
  ensure entry.designRevision.isSome s!"review {entry.id} is not design-bound"
  ensure (!review.reviewId.isEmpty && !review.targetSourceId.isEmpty &&
    !review.target.isEmpty && !review.targetSnapshot.isEmpty)
    s!"review {entry.id} has an incomplete fixed target"
  match review.purpose with
  | .design =>
      let design ← requireSome (state.design? review.targetSourceId)
        s!"review {entry.id} has no target Design"
      ensure (review.target == s!"design:{design.id}" &&
        review.producerAgentRun == design.producerAgentRun &&
        entry.designRevision == some design.id)
        s!"review {entry.id} is not bound to its target Design provenance"
  | .implementation =>
      let source ← requireSome (state.entry? review.targetSourceId)
        s!"review {entry.id} has no target evidence"
      ensure (source.order < entry.order && source.scope == entry.scope &&
        source.workId == entry.workId && source.designRevision == entry.designRevision)
        s!"review {entry.id} target evidence crosses its binding"
      let provenanceMatches := match source.payload with
        | .artifactObservation evidence => evidence.target == review.target &&
            (review.context == .resume || evidence.snapshot == review.targetSnapshot) &&
            evidence.producerAgentRun == review.producerAgentRun
        | .commandExecution evidence => evidence.target == some review.target &&
            (review.context == .resume || evidence.snapshot == some review.targetSnapshot) &&
            evidence.producerAgentRun == review.producerAgentRun
        | _ => false
      ensure provenanceMatches s!"review {entry.id} differs from its target evidence provenance"
  ensure (review.producerAgentRun != review.reviewerAgentRun)
    s!"review {entry.id} uses its target producer as reviewer"
  match review.context with
  | .fresh =>
      ensure review.continuesEntryId.isNone s!"fresh review {entry.id} inherits prior context"
      let sameFresh := state.ledgerEntries.filter (fun candidate =>
        match candidate.payload with
        | .review value => value.reviewId == review.reviewId && value.context == .fresh
        | _ => false)
      ensure (sameFresh.length == 1) s!"review id {review.reviewId} has multiple fresh roots"
  | .resume =>
      let priorId ← requireSome review.continuesEntryId
        s!"resumed review {entry.id} has no prior review entry"
      let (priorEntry, prior) ← requireSome (priorReview? state priorId)
        s!"resumed review {entry.id} references a non-review entry"
      ensure (priorEntry.order < entry.order && prior.reviewId == review.reviewId &&
        prior.purpose == review.purpose && prior.targetSourceId == review.targetSourceId &&
        prior.target == review.target && prior.producerAgentRun == review.producerAgentRun &&
        prior.reviewerAgentRun == review.reviewerAgentRun &&
        priorEntry.scope == entry.scope && priorEntry.workId == entry.workId &&
        priorEntry.designRevision == entry.designRevision)
        s!"resumed review {entry.id} changes review lineage"

private def findReviewRootById?
    (state : ProjectState) (reviewId : String) : Option (LedgerEntry × ReviewRecord) :=
  state.ledgerEntries.findSome? (fun entry =>
    match entry.payload with
    | .review review =>
        if review.reviewId == reviewId && review.context == .fresh then some (entry, review) else none
    | _ => none)

private def validateFinding
    (state : ProjectState) (entry : LedgerEntry) (finding : FindingRecord) : Except String Unit := do
  let (reviewEntry, _) ← requireSome (findReviewRootById? state finding.reviewId)
    s!"finding {entry.id} references missing review {finding.reviewId}"
  ensure (reviewEntry.scope == entry.scope && reviewEntry.workId == entry.workId &&
    reviewEntry.designRevision == entry.designRevision && reviewEntry.order < entry.order)
    s!"finding {entry.id} crosses its Review binding"
  let evidence ← requireSome (state.entry? finding.mismatchEvidenceId)
    s!"finding {entry.id} references missing mismatch evidence"
  ensure (evidence.order < entry.order && evidence.scope == entry.scope &&
    evidence.workId == entry.workId && evidence.designRevision == entry.designRevision)
    s!"finding {entry.id} references future or differently-bound evidence"
  match evidence.payload with
  | .artifactObservation _ | .commandExecution _ => pure ()
  | _ => throw s!"finding {entry.id} cites a non-evidence mismatch entry"
  let design ← requireSome (entryDesign? state entry) s!"finding {entry.id} has no design"
  match finding.subject.kind with
  | .criterion =>
      let criterion ← requireSome (design.criterion? finding.subject.id)
        s!"finding {entry.id} references missing criterion"
      ensure (criterion.statement == finding.subject.exactQuote)
        s!"finding {entry.id} does not quote its exact current criterion"
  | .statement =>
      let statement ← requireSome (design.statement? finding.subject.id)
        s!"finding {entry.id} references missing statement"
      ensure (statement.text == finding.subject.exactQuote)
        s!"finding {entry.id} does not quote its exact current statement"
  | .assumption =>
      let statement ← requireSome (design.statement? finding.subject.id)
        s!"finding {entry.id} references a missing statement assumption"
      ensure (statement.assumptions.contains finding.subject.exactQuote)
        s!"finding {entry.id} does not quote an exact current assumption"

private def validateDisposition
    (state : ProjectState) (entry : LedgerEntry)
    (disposition : ReviewDispositionRecord) : Except String Unit := do
  let finding ← requireSome (state.entry? disposition.findingEntryId)
    s!"disposition {entry.id} references missing finding"
  match finding.payload with
  | .finding _ => pure ()
  | _ => throw s!"disposition {entry.id} references a non-finding entry"
  ensure (finding.order < entry.order && finding.scope == entry.scope &&
    finding.workId == entry.workId && finding.designRevision == entry.designRevision)
    s!"disposition {entry.id} crosses its Finding binding"
  let workId ← requireSome entry.workId s!"disposition {entry.id} is not work-bound"
  let work ← requireSome (state.work? workId) s!"disposition {entry.id} has missing work"
  ensure (!disposition.decidedByRun.isEmpty)
    s!"disposition {entry.id} has no deciding agent run"
  ensure (work.delegatedReviewDecisions.contains disposition.decision)
    s!"disposition {entry.id} is outside delegated Review decisions"
  ensure (!disposition.reason.isEmpty) s!"disposition {entry.id} has no grounded reason"

private def validateVerification
    (state : ProjectState) (entry : LedgerEntry)
    (verification : ReviewVerificationRecord) : Except String Unit := do
  let reviewEntry ← requireSome (state.entry? verification.reviewEntryId)
    s!"verification {entry.id} references missing review entry"
  let review ← match reviewEntry.payload with
    | .review value => pure value
    | _ => throw s!"verification {entry.id} references a non-review entry"
  ensure (review.context == .resume && review.reviewId == verification.reviewId &&
    review.reviewerAgentRun == verification.verifiedByRun &&
    review.target == verification.target && review.targetSnapshot == verification.snapshot)
    s!"verification {entry.id} is not a resumed review of the same target"
  ensure (reviewEntry.order < entry.order && reviewEntry.scope == entry.scope &&
    reviewEntry.workId == entry.workId && reviewEntry.designRevision == entry.designRevision)
    s!"verification {entry.id} crosses its resumed Review binding"
  let findingEntry ← requireSome (state.entry? verification.findingEntryId)
    s!"verification {entry.id} references missing finding"
  let finding ← match findingEntry.payload with
    | .finding value => pure value
    | _ => throw s!"verification {entry.id} references a non-finding entry"
  ensure (finding.reviewId == verification.reviewId && findingEntry.order < entry.order &&
    findingEntry.scope == entry.scope && findingEntry.workId == entry.workId &&
    findingEntry.designRevision == entry.designRevision)
    s!"verification {entry.id} crosses its Finding binding"
  let evidenceEntry ← requireSome (state.entry? verification.evidenceEntryId)
    s!"verification {entry.id} references missing evidence"
  ensure (evidenceEntry.order < entry.order && evidenceEntry.scope == entry.scope &&
    evidenceEntry.workId == entry.workId && evidenceEntry.designRevision == entry.designRevision)
    s!"verification {entry.id} crosses its evidence binding"
  let evidenceProducer := match evidenceEntry.payload with
    | .artifactObservation evidence => some evidence.producerAgentRun
    | .commandExecution evidence => some evidence.producerAgentRun
    | _ => none
  ensure (evidenceProducer.isSome && evidenceProducer != some verification.verifiedByRun)
    s!"verification {entry.id} was performed by its remediation evidence producer"
  let matchesSuccessfulEvidence :=
    match evidenceEntry.payload with
    | .artifactObservation evidence =>
        evidence.successful && evidence.target == verification.target &&
          evidence.snapshot == verification.snapshot
    | .commandExecution evidence =>
        evidence.successful && evidence.target == some verification.target &&
          evidence.snapshot == some verification.snapshot
    | _ => false
  ensure matchesSuccessfulEvidence
    s!"verification {entry.id} does not cite successful evidence for its exact target snapshot"

private def validateEntry (state : ProjectState) (entry : LedgerEntry) : Except String Unit := do
  ensure (!entry.id.isEmpty && !entry.scope.isEmpty) "ledger entry identity or scope is empty"
  validateEntryReferences state entry
  match entry.payload with
  | .task value => validateTask state entry value
  | .workDesignAdoption value =>
      let workId ← requireSome entry.workId s!"design adoption {entry.id} is not work-bound"
      let _ ← requireSome (state.work? workId) s!"design adoption {entry.id} has missing work"
      let _ ← requireSome (state.design? value.successor)
        s!"design adoption {entry.id} has missing successor"
      ensure (entry.designRevision == some value.successor &&
        state.designDescendsFrom value.predecessor value.successor &&
        !value.adoptedByRun.isEmpty && !value.impactDisposition.isEmpty)
        s!"design adoption {entry.id} has invalid immutable transition evidence"
  | .workHandoff value =>
      let workId ← requireSome entry.workId s!"Work handoff {entry.id} is not work-bound"
      let _ ← requireSome (state.work? workId) s!"Work handoff {entry.id} has missing Work"
      ensure (!value.predecessorRun.isEmpty && !value.successorRun.isEmpty &&
        value.predecessorRun != value.successorRun && !value.reason.isEmpty)
        s!"Work handoff {entry.id} has invalid immutable transition evidence"
  | .commandProfile value => validateCommand value.command
  | .commandExecution value =>
      ensure (!value.producerAgentRun.isEmpty)
        s!"command execution {entry.id} has no producer"
      let profileEntry ← requireSome (state.entry? value.profileEntryId)
        s!"command execution {entry.id} references missing profile"
      match profileEntry.payload with
      | .commandProfile profile =>
          ensure (profile.command.executable == value.command.executable &&
            profile.command.arguments == value.command.arguments &&
            profile.command.environment == value.command.environment &&
            value.command.workingDirectory.isSome && profile.target == value.target)
            s!"command execution {entry.id} differs from the resolved profile"
      | _ => throw s!"command execution {entry.id} references a non-profile entry"
      if let some criterionId := value.criterionId then
        let target ← requireSome value.target s!"command evidence {entry.id} has no target"
        validateEvidenceBinding state entry criterionId target "command"
  | .artifactObservation value =>
      ensure (!value.producerAgentRun.isEmpty)
        s!"artifact observation {entry.id} has no producer"
      validateEvidenceBinding state entry value.criterionId value.target "artifact"
  | .review value => validateReview state entry value
  | .finding value => validateFinding state entry value
  | .reviewDisposition value => validateDisposition state entry value
  | .reviewVerification value => validateVerification state entry value
  | .userCorrection value =>
      ensure (!value.content.isEmpty) s!"correction {entry.id} is empty"
      ensure (value.resolvedByEntryId.isSome == value.resolutionReason.isSome)
        s!"correction {entry.id} has an incomplete action resolution"
      ensure (!(value.resolvedByEntryId.isSome && value.incorporatedIn.isSome))
        s!"correction {entry.id} has two resolution modes"
      if value.resolvedByEntryId.isSome || value.incorporatedIn.isSome then
        ensure (entry.supersedes.length == 1)
          s!"resolved correction {entry.id} does not supersede exactly one open correction"
      if let some actionId := value.resolvedByEntryId then
        let action ← requireSome (state.entry? actionId)
          s!"correction {entry.id} references missing resolution action"
        let prior ← requireSome (state.entry? entry.supersedes.head!)
          s!"correction {entry.id} has no prior correction"
        ensure (prior.order < action.order && action.order < entry.order &&
          prior.scope == action.scope && prior.workId == action.workId &&
          prior.designRevision == action.designRevision &&
          !value.resolutionReason.get!.isEmpty)
          s!"correction {entry.id} resolution action is not later and same-bound"
        match prior.payload with
        | .userCorrection priorValue =>
            ensure (priorValue.resolvedByEntryId.isNone && priorValue.incorporatedIn.isNone &&
              priorValue.content == value.content)
              s!"correction {entry.id} does not resolve the same open correction"
        | _ => throw s!"correction {entry.id} supersedes a non-correction entry"
      if let some designId := value.incorporatedIn then
        let incorporatedDesign ← requireSome (state.design? designId)
          s!"correction {entry.id} references missing incorporated design"
        let boundDesign ← requireSome entry.designRevision
          s!"incorporated correction {entry.id} is not design-bound"
        ensure (state.designDescendsFrom boundDesign designId)
          s!"correction {entry.id} is not incorporated by a strict successor"
        ensure (entry.supersedes.length == 1)
          s!"correction {entry.id} records incorporation without superseding its open record"
        let prior ← requireSome (state.entry? entry.supersedes.head!)
          s!"correction {entry.id} has no prior correction"
        match prior.payload with
        | .userCorrection priorValue =>
            ensure (priorValue.resolvedByEntryId.isNone && priorValue.incorporatedIn.isNone &&
              priorValue.content == value.content)
              s!"correction {entry.id} does not close the same open correction"
            ensure (incorporatedDesign.createdAfterEntryOrder >= prior.order)
              s!"correction {entry.id} names a Design that predates the open correction"
        | _ => throw s!"correction {entry.id} supersedes a non-correction entry"
      if value.resolvedByEntryId.isNone && value.incorporatedIn.isNone &&
          !entry.supersedes.isEmpty then
        ensure (entry.supersedes.length == 1)
          s!"correction {entry.id} supersedes more than one correction"
        let prior ← requireSome (state.entry? entry.supersedes.head!)
          s!"correction {entry.id} has no superseded correction"
        match prior.payload with
        | .userCorrection priorValue =>
            ensure (priorValue.resolvedByEntryId.isNone && priorValue.incorporatedIn.isNone)
              s!"correction {entry.id} does not supersede an open correction"
        | _ => throw s!"correction {entry.id} supersedes a non-correction entry"
  | .kpt value =>
      ensure (value.keep.isSome || value.problem.isSome || value.tryNext.isSome)
        s!"KPT {entry.id} is empty"
      ensure (value.appliesKptEntryId.isSome == value.appliedByEntryId.isSome)
        s!"KPT {entry.id} has only one side of an application witness"
      if let some sourceId := value.appliesKptEntryId then
        let source ← requireSome (state.entry? sourceId)
          s!"KPT {entry.id} references a missing source Try"
        match source.payload with
        | .kpt sourceValue =>
            ensure (sourceValue.tryNext.isSome && source.order < entry.order &&
              source.scope == entry.scope && source.workId == entry.workId &&
              source.designRevision == entry.designRevision)
              s!"KPT {entry.id} does not apply an earlier same-bound Try"
        | _ => throw s!"KPT {entry.id} application source is not KPT"
      if let some appliedId := value.appliedByEntryId then
        let applied ← requireSome (state.entry? appliedId)
          s!"KPT {entry.id} references a missing application entry"
        ensure (applied.order < entry.order && applied.scope == entry.scope &&
          applied.workId == entry.workId && applied.designRevision == entry.designRevision)
          s!"KPT {entry.id} application crosses its binding"
        let sourceId ← requireSome value.appliesKptEntryId
          s!"KPT {entry.id} has no source Try"
        let source ← requireSome (state.entry? sourceId)
          s!"KPT {entry.id} has no source Try entry"
        ensure (source.order < applied.order)
          s!"KPT {entry.id} action does not occur after its Try"
  | .leanProofReceipt value =>
      let design ← requireSome (entryDesign? state entry) s!"proof receipt {entry.id} has no design"
      let claim ← requireSome (design.claim? value.claimId)
        s!"proof receipt {entry.id} references missing claim"
      ensure (!value.inputDigest.isEmpty && !value.outputDigest.isEmpty &&
        !value.sourceDigests.isEmpty && value.claimInput == claim.input &&
        value.kernelAccepted == (value.exitCode == 0))
        s!"proof receipt {entry.id} has incomplete identity"

def validateState (state : ProjectState) : Except String Unit := do
  ensure (uniqueStrings (state.designRevisions.map (·.id))) "duplicate design ids"
  ensure (uniqueStrings (state.works.map (·.id))) "duplicate work ids"
  ensure (uniqueStrings (state.ledgerEntries.map (·.id))) "duplicate ledger entry ids"
  let orders := state.ledgerEntries.map (·.order)
  ensure (orders.all (fun order => orders.count order == 1)) "duplicate ledger order"
  for design in state.designRevisions do validateDesign design
  let accepted := state.designRevisions.filter (·.status == .accepted)
  match state.acceptedDesignId with
  | none => ensure accepted.isEmpty "accepted design exists without accepted selector"
  | some id =>
      ensure (accepted.length == 1 && accepted.head?.map (·.id) == some id)
        "accepted selector does not identify the unique accepted design"
  let focused := state.works.filter (·.status == .focused)
  match state.focusedWorkId with
  | none => ensure focused.isEmpty "focused work exists without focused selector"
  | some id =>
      ensure (focused.length == 1 && focused.head?.map (·.id) == some id)
        "focused selector does not identify the unique focused work"
  for work in state.works do
    let _ ← requireSome (state.design? work.designRevision)
      s!"work {work.id} references missing design"
    ensure (!work.id.isEmpty && !work.outcome.isEmpty && !work.scope.isEmpty &&
      !work.responsibleAgentRun.isEmpty)
      s!"work {work.id} is incomplete"
    ensure (work.delegatedReviewDecisions.all (fun decision =>
      work.delegatedReviewDecisions.count decision == 1))
      s!"work {work.id} has duplicate delegated Review decisions"
  for entry in state.ledgerEntries do validateEntry state entry

end AgentWorkbench
