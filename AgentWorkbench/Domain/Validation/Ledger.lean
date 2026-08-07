import AgentWorkbench.Domain.Validation.Design
import AgentWorkbench.Decision.Finding
import AgentWorkbench.Decision.ReviewInput

namespace AgentWorkbench
namespace Validation

private def entryDesign? (state : ProjectState) (entry : LedgerEntry) : Option DesignRevision :=
  entry.designRevision.bind state.design?

private def isSuperseded (state : ProjectState) (entry : LedgerEntry) : Bool :=
  state.ledgerEntries.any fun replacement =>
    replacement.order > entry.order && replacement.supersedes.contains entry.id

def validateEntryReferences
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
    let sameDesign := replaced.designRevision == entry.designRevision
    let successorTask := match replaced.payload, entry.payload,
        replaced.designRevision, entry.designRevision with
      | .task _, .task _, some predecessor, some successor =>
          replaced.workId == entry.workId && state.designDescendsFrom predecessor successor
      | _, _, _, _ => false
    ensure (replaced.scope == entry.scope && replaced.workId == entry.workId &&
      (sameDesign || successorTask) && replaced.payload.tag == entry.payload.tag)
      s!"entry {entry.id} crosses a supersession boundary"
  match entry.payload with
  | .kpt _ => ensure entry.supersedes.isEmpty "KPT cannot supersede another entry"
  | _ => pure ()

private def validateTask
    (state : ProjectState) (entry : LedgerEntry) (task : TaskRecord) : Except String Unit := do
  ensure entry.workId.isSome s!"task {entry.id} is not work-bound"
  ensure entry.designRevision.isSome s!"task {entry.id} is not design-bound"
  match task.planId, task.planStepId, task.lineageId with
  | some planId, some stepId, some lineageId =>
      let plan ← requireSome (state.plan? planId) s!"task {entry.id} references missing Plan"
      let step ← requireSome (uniqueBy? plan.steps (·.id) stepId)
        s!"task {entry.id} references missing Plan step"
      ensure (entry.workId == some plan.workId && entry.designRevision == some plan.designRevision &&
        lineageId == s!"{plan.workId}:{step.id}" &&
        task.dependencyLineageIds == step.dependsOnStepIds.map (fun id => s!"{plan.workId}:{id}") &&
        task.outputScopes == step.outputScopes &&
        task.verificationCriterionIds == step.verificationCriterionIds &&
        task.criterionId.isNone && (task.retired || task.required))
        s!"task {entry.id} differs from its materialized Plan step"
  | none, none, none =>
      if task.required then
        let criterionId ← requireSome task.criterionId s!"legacy required task {entry.id} has no criterion"
        let design ← requireSome (entryDesign? state entry) s!"task {entry.id} has no design"
        let _ ← requireSome (design.criterion? criterionId)
          s!"task {entry.id} references missing criterion {criterionId}"
  | _, _, _ => throw s!"task {entry.id} has a partial Plan binding"
  if task.planId.isSome && task.required && !task.retired then
    if task.closed then
      let sourceTaskId ← requireSome task.verificationTaskEntryId
        s!"closed task {entry.id} has no verification source Task"
      ensure (task.verificationEvidenceEntryIds.length == task.verificationCriterionIds.length &&
        uniqueStrings task.verificationEvidenceEntryIds)
        s!"closed task {entry.id} does not bind one exact evidence entry per criterion"
      for evidenceId in task.verificationEvidenceEntryIds do
        let evidenceEntry ← requireSome (state.entry? evidenceId)
          s!"closed task {entry.id} references missing evidence {evidenceId}"
        ensure (evidenceEntry.order < entry.order && evidenceEntry.workId == entry.workId &&
          evidenceEntry.designRevision == entry.designRevision &&
          match evidenceEntry.payload with
          | .artifactObservation evidence => evidence.taskEntryId == some sourceTaskId
          | .commandExecution evidence => evidence.taskEntryId == some sourceTaskId
          | _ => false)
          s!"closed task {entry.id} has evidence from another Task or binding"
    else
      ensure (task.verificationEvidenceEntryIds.isEmpty && task.verificationTaskEntryId.isNone)
        s!"open task {entry.id} already carries closing evidence"

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

private def validateTaskEvidenceBinding
    (state : ProjectState) (entry : LedgerEntry) (taskEntryId outputScope : Option String)
    (criterionIds : List String) : Except String Unit := do
  let workId ← requireSome entry.workId s!"evidence {entry.id} is not Work-bound"
  if (state.currentPlanFor? workId).any fun plan =>
      entry.designRevision == some plan.designRevision then
    let taskId ← requireSome taskEntryId s!"evidence {entry.id} has no Task binding"
    let scope ← requireSome outputScope s!"evidence {entry.id} has no output scope"
    let taskEntry ← requireSome (state.entry? taskId)
      s!"evidence {entry.id} references missing Task {taskId}"
    ensure (!(state.ledgerEntries.any fun successor =>
      successor.order < entry.order && successor.supersedes.contains taskEntry.id))
      s!"evidence {entry.id} references a Task superseded before observation"
    let task ← match taskEntry.payload with
      | .task value => pure value
      | _ => throw s!"evidence {entry.id} references a non-Task entry"
    ensure (task.planId.isSome && !task.retired && taskEntry.order <= entry.order &&
      entry.order > task.materializedAtOrder && task.outputScopes.contains scope &&
      !criterionIds.isEmpty && criterionIds.all task.verificationCriterionIds.contains)
      s!"evidence {entry.id} differs from its current materialized Task"

private def priorReview? (state : ProjectState) (entryId : String) : Option (LedgerEntry × ReviewRecord) := do
  let entry ← state.entry? entryId
  match entry.payload with
  | .review review => some (entry, review)
  | _ => none

private def activeReviewerBefore
    (state : ProjectState) (reviewId initial : String) (beforeOrder : Nat) : String :=
  (state.ledgerEntries.filter (·.order < beforeOrder) |>.mergeSort (fun left right => left.order < right.order))
    |>.foldl (fun active entry => match entry.payload with
      | .reviewHandoff value =>
          if value.reviewId == reviewId && value.predecessorReviewerRun == active then
            value.successorReviewerRun
          else active
      | _ => active) initial

private def validateReview
    (state : ProjectState) (entry : LedgerEntry) (review : ReviewRecord) : Except String Unit := do
  let producers := review.producerAgentRuns
  ensure (review.targetManifestVersion <= 2)
    s!"review {entry.id} has an unsupported target manifest version"
  ensure entry.designRevision.isSome s!"review {entry.id} is not design-bound"
  ensure (!review.reviewId.isEmpty && !review.targetSourceId.isEmpty &&
    !review.target.isEmpty && !review.targetSnapshot.isEmpty && !producers.isEmpty)
    s!"review {entry.id} has an incomplete fixed target"
  match review.purpose with
  | .design =>
      let design ← requireSome (state.design? review.targetSourceId)
        s!"review {entry.id} has no target Design"
      ensure (review.target == s!"design:{design.id}" &&
        producers.contains design.producerAgentRun &&
        entry.designRevision == some design.id)
        s!"review {entry.id} is not bound to its target Design provenance"
  | .implementation =>
      if review.targetManifest.isEmpty then
        let source ← requireSome (state.entry? review.targetSourceId)
          s!"legacy review {entry.id} has no target evidence"
        ensure (source.order < entry.order && source.scope == entry.scope &&
          source.workId == entry.workId && source.designRevision == entry.designRevision)
          s!"legacy review {entry.id} target evidence crosses its binding"
      else
        let work ← requireSome (state.work? review.targetSourceId)
          s!"review {entry.id} has no target Work"
        ensure (review.target == s!"work:{work.id}" && entry.workId == some work.id &&
          work.scope == entry.scope)
          s!"review {entry.id} target Work crosses its binding"
        let designId ← requireSome entry.designRevision s!"review {entry.id} has no Design"
        let design ← requireSome (state.design? designId)
          s!"review {entry.id} has no fixed Design"
        let designComponents := review.targetManifest.filter (·.kind == "design")
        let designComponent ← match designComponents with
          | [value] => pure value
          | _ => throw s!"review {entry.id} omits or duplicates its fixed Design"
        ensure (designComponent.id == designId)
          s!"review {entry.id} changes its fixed Design identity"
        ensure (designComponent.snapshot == design.revisionContentDigest)
          s!"review {entry.id} changes its fixed Design digest"
        let planComponents := review.targetManifest.filter (·.kind == "plan")
        let planComponent ← match planComponents with
          | [value] => pure value
          | _ => throw s!"review {entry.id} omits or duplicates its fixed Plan"
        let plan ← requireSome (state.plan? planComponent.id)
          s!"review {entry.id} has no fixed implementation Plan"
        ensure (plan.workId == work.id && plan.designRevision == designId &&
          planComponent.snapshot == plan.contentDigest)
          s!"review {entry.id} changes its fixed Plan identity or digest"
        let coverageOrder := match review.context with
          | .fresh => entry.order
          | .resume => (state.ledgerEntries.find? fun candidate =>
              match candidate.payload with
              | .review value => value.reviewId == review.reviewId && value.context == .fresh
              | _ => false) |>.map (·.order) |>.getD entry.order
        let taskIds := state.ledgerEntries.filterMap fun candidate =>
          if candidate.scope != entry.scope || candidate.workId != entry.workId ||
              candidate.designRevision != entry.designRevision || candidate.order >= coverageOrder ||
              isSuperseded state candidate then none
          else match candidate.payload with
            | .task task => if task.planId == some plan.id && !task.retired then some candidate.id else none
            | _ => none
        ensure (taskIds.all fun id => review.targetManifest.any fun value =>
          value.kind == "task" && value.id == id)
          s!"review {entry.id} omits part of the current Task graph"
        let priorLedger := state.ledgerEntries.filter (·.order < coverageOrder)
        let supersededBeforeReview (candidate : LedgerEntry) := priorLedger.any fun replacement =>
          replacement.order > candidate.order && replacement.supersedes.contains candidate.id
        let currentEntries := priorLedger.filter fun candidate =>
          entryAppliesTo state design work candidate && !supersededBeforeReview candidate
        let expectedLedgerComponents := normalizeReviewTargetComponents <| match
            review.targetManifestVersion with
          | 2 =>
              (implementationReviewLedgerEntries currentEntries design plan work.id).map
                (reviewLedgerComponent state work)
          | _ =>
              let historyEntries := implementationReviewHistoryEntriesV1
                (priorLedger.filter fun candidate => !supersededBeforeReview candidate) work.id
              (implementationReviewLedgerEntriesV1 currentEntries plan work.id ++ historyEntries).map
                (reviewLedgerComponentAt state work coverageOrder)
        ensure (review.targetSnapshot ==
          ContentDigest.string (Lean.toJson review.targetManifest).compress)
          s!"review {entry.id} changes its fixed manifest digest"
        ensure (review.targetManifest == normalizeReviewTargetComponents review.targetManifest)
          s!"review {entry.id} changes canonical manifest ordering"
        let workComponents := review.targetManifest.filter (·.kind == "work")
        let workComponent ← match workComponents with
          | [value] => pure value
          | _ => throw s!"review {entry.id} omits or duplicates its fixed Work"
        ensure (workComponent.id == work.id)
          s!"review {entry.id} changes its fixed Work identity"
        if review.targetManifestVersion >= 1 then
          let taskPlanIds := currentEntries.foldl (fun found candidate =>
            match candidate.payload with
            | .task task => match task.planId with
              | some id => if task.retired || found.contains id then found else found ++ [id]
              | none => found
            | _ => found) []
          ensure (taskPlanIds == [plan.id])
            s!"review {entry.id} Plan is not the authoritative Plan at target capture"
          ensure (designComponent.producerAgentRuns == [design.producerAgentRun])
            s!"review {entry.id} changes its fixed Design producer provenance"
          ensure (planComponent.producerAgentRuns == [plan.producerAgentRun])
            s!"review {entry.id} changes its fixed Plan producer provenance"
          ensure (workComponent.snapshot == reviewWorkIdentitySnapshot work)
            s!"review {entry.id} changes its immutable Work identity"
          ensure (workComponent.producerAgentRuns ==
              reviewWorkProducerRunsAt state work coverageOrder)
            s!"review {entry.id} changes its Work producer provenance"
        let expectedTargetIds := implementationReviewOutputTargetIds currentEntries plan
          |>.mergeSort (· < ·)
        let recordedImplementationTargets :=
          review.targetManifest.filter (·.kind == "implementation_target")
        let implementationTargets := if review.targetManifestVersion >= 1 then
          recordedImplementationTargets
        else
          deduplicateReviewTargetComponents recordedImplementationTargets
        let actualTargetIds := implementationTargets.map (·.id) |>.mergeSort (· < ·)
        ensure (actualTargetIds == expectedTargetIds && uniqueStrings actualTargetIds)
          s!"review {entry.id} changes its exact implementation output targets"
        ensure (review.targetManifestVersion == 0 || implementationTargets.all fun component =>
          component.producerAgentRuns == [responsibleWorkAgentRunAt state work coverageOrder])
          s!"review {entry.id} changes implementation target producer provenance"
        let structuralKinds := ["design", "plan", "work", "implementation_target"]
        let actualLedgerComponents := normalizeReviewTargetComponents <|
          review.targetManifest.filter fun component => !structuralKinds.contains component.kind
        ensure (actualLedgerComponents == expectedLedgerComponents)
          (s!"review {entry.id} changes the exact current implementation component projection: " ++
            s!"recorded={actualLedgerComponents.map (·.id)}, expected={expectedLedgerComponents.map (·.id)}")
        ensure (review.targetManifest.all fun value =>
          value.producerAgentRuns.all producers.contains)
          s!"review {entry.id} manifest producer closure is incomplete"
        let expectedProducers := review.targetManifest.foldl (fun found component =>
          component.producerAgentRuns.foldl (fun runs producer =>
            if producer.isEmpty || runs.contains producer then runs else runs ++ [producer]) found) []
        ensure (producers == expectedProducers)
          s!"review {entry.id} manifest producer closure is not exact"
  ensure (!producers.contains review.reviewerAgentRun)
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
        prior.target == review.target && prior.targetSnapshot == review.targetSnapshot &&
        prior.targetManifestVersion == review.targetManifestVersion &&
        prior.targetManifest == review.targetManifest &&
        prior.producerAgentRuns == review.producerAgentRuns &&
        activeReviewerBefore state prior.reviewId prior.reviewerAgentRun entry.order ==
          review.reviewerAgentRun &&
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
  let (reviewEntry, review) ← requireSome (findReviewRootById? state finding.reviewId)
    s!"finding {entry.id} references missing review {finding.reviewId}"
  ensure (reviewEntry.scope == entry.scope && reviewEntry.workId == entry.workId &&
    reviewEntry.designRevision == entry.designRevision && reviewEntry.order < entry.order)
    s!"finding {entry.id} crosses its Review binding"
  ensure (finding.targetSourceId == review.targetSourceId && finding.target == review.target &&
    finding.targetSnapshot == review.targetSnapshot &&
    finding.producerAgentRuns == review.producerAgentRuns)
    s!"finding {entry.id} differs from its fixed Review target provenance"
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
  | .implementationComponent =>
      ensure (review.purpose == .implementation && review.targetManifest.any (fun component =>
        component.id == finding.subject.id && component.snapshot == finding.subject.exactQuote))
        s!"finding {entry.id} does not identify an exact fixed implementation component"

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
  ensure (disposition.decidedByRun == responsibleWorkAgentRunAt state work entry.order)
    s!"disposition {entry.id} was not made by the responsible Work agent"
  ensure (!disposition.reason.isEmpty) s!"disposition {entry.id} has no grounded reason"

private def validateVerification
    (state : ProjectState) (entry : LedgerEntry)
    (verification : ReviewVerificationRecord) : Except String Unit := do
  let reviewEntry ← requireSome (state.entry? verification.reviewEntryId)
    s!"verification {entry.id} references missing review entry"
  let review ← match reviewEntry.payload with
    | .review value => pure value
    | _ => throw s!"verification {entry.id} references a non-review entry"
  ensure (review.context == .resume && review.purpose == .implementation &&
    review.reviewId == verification.reviewId &&
    activeReviewerBefore state review.reviewId review.reviewerAgentRun entry.order ==
      verification.verifiedByRun &&
    review.targetManifest.any (fun component =>
      component.kind == "implementation_target" && component.id == verification.target))
    s!"verification {entry.id} is not derived from the resumed Review manifest"
  ensure (reviewEntry.order < entry.order && reviewEntry.scope == entry.scope &&
    reviewEntry.workId == entry.workId && reviewEntry.designRevision == entry.designRevision)
    s!"verification {entry.id} crosses its resumed Review binding"
  let findingEntry ← requireSome (state.entry? verification.findingEntryId)
    s!"verification {entry.id} references missing finding"
  let finding ← match findingEntry.payload with
    | .finding value => pure value
    | _ => throw s!"verification {entry.id} references a non-finding entry"
  let evidenceEntry ← requireSome (state.entry? verification.evidenceEntryId)
    s!"verification {entry.id} references missing evidence"
  ensure (finding.reviewId == verification.reviewId && findingEntry.order < evidenceEntry.order &&
    evidenceEntry.order < reviewEntry.order && reviewEntry.order < entry.order &&
    findingEntry.scope == entry.scope && findingEntry.workId == entry.workId &&
    findingEntry.designRevision == entry.designRevision)
    s!"verification {entry.id} crosses its Finding binding"
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
  ensure (findingRemediationBindingBefore state findingEntry evidenceEntry finding
    verification.target entry.order)
    s!"verification {entry.id} bypasses its Finding-bound replacement Plan Task"

def validateEntry (state : ProjectState) (entry : LedgerEntry) : Except String Unit := do
  ensure (!entry.id.isEmpty && !entry.scope.isEmpty) "ledger entry identity or scope is empty"
  validateEntryReferences state entry
  match entry.payload with
  | .task value => validateTask state entry value
  | .workDesignAdoption value =>
      let workId ← requireSome entry.workId s!"design adoption {entry.id} is not work-bound"
      let work ← requireSome (state.work? workId) s!"design adoption {entry.id} has missing work"
      let _ ← requireSome (state.design? value.successor)
        s!"design adoption {entry.id} has missing successor"
      ensure (entry.designRevision == some value.successor &&
        state.designDescendsFrom value.predecessor value.successor &&
        value.adoptedByRun == responsibleWorkAgentRunAt state work entry.order &&
        !value.impactDisposition.isEmpty)
        s!"design adoption {entry.id} has invalid immutable transition evidence"
  | .workHandoff value =>
      let workId ← requireSome entry.workId s!"Work handoff {entry.id} is not work-bound"
      let work ← requireSome (state.work? workId) s!"Work handoff {entry.id} has missing Work"
      ensure (value.predecessorRun == responsibleWorkAgentRunAt state work entry.order &&
        !value.successorRun.isEmpty &&
        value.predecessorRun != value.successorRun && !value.reason.isEmpty)
        s!"Work handoff {entry.id} has invalid immutable transition evidence"
  | .workWithdrawal value =>
      let workId ← requireSome entry.workId s!"Work withdrawal {entry.id} is not Work-bound"
      let work ← requireSome (state.work? workId) s!"Work withdrawal {entry.id} has missing Work"
      let correctionEntry ← requireSome (state.entry? value.correctionEntryId)
        s!"Work withdrawal {entry.id} has missing Correction"
      let correctionOpen := match correctionEntry.payload with
        | .userCorrection correction =>
            correction.resolvedByEntryId.isNone && correction.incorporatedIn.isNone
        | _ => false
      ensure (work.status == .withdrawn && correctionEntry.order < entry.order &&
        correctionEntry.workId == entry.workId && correctionOpen &&
        !value.reason.isEmpty &&
        value.withdrawnByRun == responsibleWorkAgentRunAt state work entry.order)
        s!"Work withdrawal {entry.id} lacks current Correction authority"
  | .workResume value =>
      let workId ← requireSome entry.workId s!"Work resume {entry.id} is not Work-bound"
      let work ← requireSome (state.work? workId) s!"Work resume {entry.id} has missing Work"
      ensure (!value.condition.isEmpty && !value.satisfaction.isEmpty &&
        !value.basisEntryIds.isEmpty && uniqueStrings value.basisEntryIds &&
        value.resumedByRun == responsibleWorkAgentRunAt state work entry.order)
        s!"Work resume {entry.id} has incomplete satisfaction evidence"
      for basisId in value.basisEntryIds do
        let basis ← requireSome (state.entry? basisId)
          s!"Work resume {entry.id} has missing basis {basisId}"
        ensure (basis.order < entry.order && basis.workId == entry.workId &&
          basis.designRevision == entry.designRevision)
          s!"Work resume {entry.id} crosses its Work or Design binding"
  | .workCompletion value =>
      let workId ← requireSome entry.workId s!"Work completion {entry.id} is not Work-bound"
      let work ← requireSome (state.work? workId) s!"Work completion {entry.id} has missing Work"
      let plan ← requireSome (state.plan? value.planId)
        s!"Work completion {entry.id} has missing Plan"
      ensure (work.status == .completed && value.workId == work.id &&
        entry.designRevision == some value.designRevision &&
        work.designRevision == some value.designRevision && plan.workId == work.id &&
        plan.designRevision == value.designRevision && plan.status == .current &&
        value.inputRevision < state.revision && validContentDigest value.inputDigest &&
        value.completedByRun == responsibleWorkAgentRunAt state work entry.order)
        s!"Work completion {entry.id} has invalid authority or input identity"
  | .designRejection value =>
      let design ← requireSome (state.design? value.designId)
        s!"Design rejection {entry.id} has missing Design"
      let workId ← requireSome entry.workId s!"Design rejection {entry.id} is not Work-bound"
      let work ← requireSome (state.work? workId) s!"Design rejection {entry.id} has missing Work"
      ensure (design.status == .rejected && entry.designRevision == some design.id &&
        design.workId == entry.workId && !value.reason.isEmpty &&
        value.rejectedByRun == responsibleWorkAgentRunAt state work entry.order)
        s!"Design rejection {entry.id} has invalid provenance"
  | .commandProfile value =>
      validateCommand value.command
      ensure (value.inputTargets.getD [] |> uniqueStrings)
        s!"Command Profile {entry.id} has duplicate input targets"
      validateTaskEvidenceBinding state entry value.taskEntryId value.outputScope
        (value.criterionIds.getD [])
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
            value.environmentSnapshots.any (fun snapshots =>
              snapshots.map (·.target) == profile.command.environment.toList.map ("env:" ++ ·) &&
              uniqueStrings (snapshots.map (·.target)) &&
              snapshots.all fun snapshot => validContentDigest snapshot.snapshot) &&
            value.command.workingDirectory.isSome && profile.target == value.target &&
            profile.taskEntryId == value.taskEntryId && profile.outputScope == value.outputScope &&
            value.inputSnapshots.all (fun snapshots =>
              snapshots.map (·.target) == profile.inputTargets.getD [] &&
              uniqueStrings (snapshots.map (·.target)) &&
              snapshots.all fun snapshot => !snapshot.snapshot.isEmpty) &&
            (profile.taskEntryId.isNone ||
              value.criterionId.all fun id => (profile.criterionIds.getD []).contains id))
            s!"command execution {entry.id} differs from the resolved profile"
      | _ => throw s!"command execution {entry.id} references a non-profile entry"
      if let some criterionId := value.criterionId then
        let target ← requireSome value.target s!"command evidence {entry.id} has no target"
        validateEvidenceBinding state entry criterionId target "command"
      validateTaskEvidenceBinding state entry value.taskEntryId value.outputScope
        value.criterionId.toList
  | .artifactObservation value =>
      ensure (!value.producerAgentRun.isEmpty)
        s!"artifact observation {entry.id} has no producer"
      validateEvidenceBinding state entry value.criterionId value.target "artifact"
      validateTaskEvidenceBinding state entry value.taskEntryId value.outputScope [value.criterionId]
  | .review value => validateReview state entry value
  | .finding value => validateFinding state entry value
  | .reviewDisposition value => validateDisposition state entry value
  | .reviewVerification value => validateVerification state entry value
  | .reviewHandoff value =>
      let reviewEntry ← requireSome (state.entry? value.reviewEntryId)
        s!"Review handoff {entry.id} has missing Review"
      let review ← match reviewEntry.payload with
        | .review review => pure review
        | _ => throw s!"Review handoff {entry.id} references a non-Review entry"
      let producers := review.producerAgentRuns
      ensure (review.reviewId == value.reviewId && reviewEntry.order < entry.order &&
        reviewEntry.scope == entry.scope && reviewEntry.workId == entry.workId &&
        reviewEntry.designRevision == entry.designRevision &&
        value.predecessorReviewerRun == activeReviewerBefore state review.reviewId
          review.reviewerAgentRun entry.order && !value.successorReviewerRun.isEmpty &&
        value.predecessorReviewerRun != value.successorReviewerRun &&
        !producers.contains value.successorReviewerRun && !value.reason.isEmpty)
        s!"Review handoff {entry.id} changes target binding or reviewer independence"
  | .reviewConclusion value =>
      let reviewEntry ← requireSome (state.entry? value.reviewEntryId)
        s!"Review conclusion {entry.id} has missing Review"
      let review ← match reviewEntry.payload with
        | .review review => pure review
        | _ => throw s!"Review conclusion {entry.id} references a non-Review entry"
      let hasFinding := state.ledgerEntries.any fun candidate =>
        candidate.order > reviewEntry.order && candidate.order < entry.order &&
        match candidate.payload with
        | .finding finding => finding.reviewId == review.reviewId
        | _ => false
      ensure (review.reviewId == value.reviewId && reviewEntry.order < entry.order &&
        reviewEntry.scope == entry.scope && reviewEntry.workId == entry.workId &&
        reviewEntry.designRevision == entry.designRevision &&
        value.reviewerAgentRun == activeReviewerBefore state review.reviewId
          review.reviewerAgentRun entry.order && !value.summary.isEmpty &&
        value.clean != hasFinding)
        s!"Review conclusion {entry.id} is inconsistent with its fixed Review lineage"
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
      let currentIdentity :=
        value.elaboratedPropositionDigest == claim.elaboratedPropositionDigest &&
        value.propositionDependencies == claim.propositionDependencies &&
        value.assumptionDependencies == claim.input.assumptions.mergeSort (· < ·)
      ensure (!value.inputDigest.isEmpty && !value.outputDigest.isEmpty &&
        !value.sourceDigests.isEmpty && value.claimInput == claim.input &&
        (!design.sourceArchiveAvailable || currentIdentity) &&
        value.kernelAccepted == (value.exitCode == 0))
        s!"proof receipt {entry.id} has incomplete identity"



end Validation
end AgentWorkbench
