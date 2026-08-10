import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.Store
import AgentWorkbenchTest.RouteReceipt

namespace AgentWorkbenchTest.PublicDesignWorkRoute

open AgentWorkbench AgentWorkbenchTest

private def executablePath : System.FilePath :=
  if System.Platform.isWindows then ".lake/build/bin/agent-workbench.exe"
  else ".lake/build/bin/agent-workbench"

private def invoke
    (root : System.FilePath) (operation : Operation) (input : Option String := none) :
    IO String := do
  let output ← IO.Process.output {
    cmd := executablePath.toString
    args := #["--project", root.toString] ++ (operation.name.splitOn " ").toArray } input
  unless output.exitCode == 0 do
    throw (IO.userError s!"public {operation.name} route failed: {output.stderr}")
  RouteReceipt.recordSuccessful .publicDesignWorkRoute operation
  pure output.stdout

private def invokeJson [Lean.ToJson α]
    (root : System.FilePath) (operation : Operation) (input : α) : IO String :=
  invoke root operation (some (Lean.toJson input).compress)

private def invokeJsonRejected [Lean.ToJson α]
    (root : System.FilePath) (operation : Operation) (input : α) : IO Unit := do
  let output ← IO.Process.output {
    cmd := executablePath.toString
    args := #["--project", root.toString] ++ (operation.name.splitOn " ").toArray }
    (some (Lean.toJson input).compress)
  expect (output.exitCode != 0) s!"public {operation.name} route unexpectedly succeeded"

private def decode [Lean.FromJson α] (source : String) : IO α := do
  let json ← match Lean.Json.parse source with
    | .ok value => pure value
    | .error message => throw (IO.userError message)
  match Lean.fromJson? json with
  | .ok value => pure value
  | .error message => throw (IO.userError message)

private def prepareWorkspace (root : System.FilePath) : IO System.FilePath := do
  let designRoot := root / ".agent-workbench" / "design"
  IO.FS.createDirAll (designRoot / "product")
  IO.FS.createDirAll (designRoot / "implementation")
  pure (designRoot / "product" / "design.md")

private def proposal
    (target : String) (units : List DesignSourceUnit)
    (rationale : String) (amends : Option String := none)
    (bases : List String := []) : DesignProposalRequest :=
  let statement : Statement := {
    id := "statement-public-lifecycle", text := "The public lifecycle remains reachable." }
  { producerAgentRun := "agent-public-lifecycle"
    changeRationale := rationale, changeBasisEntryIds := bases, amendsCandidate := amends
    sourceDocumentTargets := [target]
    sourceUnitDispositions := units.map fun unit => { unitId := unit.id, role := .requirement }
    statements := [statement]
    statementCoverage := [{
      statementId := statement.id, sourceUnitIds := units.map (·.id)
      leanClaims := { noSelectionReason := some "the lifecycle is observed through the public route" }
      acceptanceCriteria := { noSelectionReason := some "this route is the direct observation" }
      implementationRequired := false
      noImplementationReason := some "the route itself is the completed behavior" }]
    acceptanceCriteria := []
    assuranceContracts := some [fixtureAssuranceInput statement [] [] false] }

private def proposedDesign (source : String) : IO DesignRevision :=
  decode source

private def exerciseAmendAndReject : IO Unit :=
  IO.FS.withTempDir fun root => do
    let sourcePath ← prepareWorkspace root
    let target := "file:.agent-workbench/design/product/design.md"
    let _ ← invokeJson root .workStart ({
      id := "work-amend", outcome := "exercise candidate amendment and rejection"
      scope := "project", responsibleAgentRun := "agent-public-lifecycle" } : WorkStartRequest)
    IO.FS.writeFile sourcePath "The public lifecycle remains reachable.\n"
    let firstUnits := (← DesignSource.inspectAll root [target]).flatMap (·.units)
    let first ← proposedDesign (← invokeJson root .designPropose
      (proposal target firstUnits "record the first public candidate"))
    IO.FS.writeFile sourcePath "# Lifecycle\n\nThe public lifecycle remains reachable.\n"
    let amendedUnits := (← DesignSource.inspectAll root [target]).flatMap (·.units)
    let amendment ← proposedDesign (← invokeJson root .designAmend
      (proposal target amendedUnits "replace the public candidate" (some first.id)))
    let _ ← invokeJson root .designReject ({
      designId := amendment.id, entryId := "design-rejection-public"
      reason := "exercise the explicit rejected-candidate route" } : DesignRejectRequest)
    let state ← Store.loadState (← Store.openReadOnly (root / ".agent-workbench" / "state.db"))
    expect ((state.design? first.id).any (·.status == .superseded) &&
      (state.design? amendment.id).any (·.status == .rejected) &&
      state.acceptedDesignId.isNone)
      "public amendment/rejection route changed accepted Design authority"

private def exerciseAdoptIncorporateAndWithdraw : IO Unit :=
  IO.FS.withTempDir fun root => do
    let sourcePath ← prepareWorkspace root
    let target := "file:.agent-workbench/design/product/design.md"
    let _ ← invokeJson root .workStart ({
      id := "work-successor", outcome := "exercise successor Work lifecycle"
      scope := "project", responsibleAgentRun := "agent-public-lifecycle" } : WorkStartRequest)
    IO.FS.writeFile sourcePath "The public lifecycle remains reachable.\n"
    let initialUnits := (← DesignSource.inspectAll root [target]).flatMap (·.units)
    let initial ← proposedDesign (← invokeJson root .designPropose
      (proposal target initialUnits "record the initial public Design"))
    let _ ← invokeJson root .designAccept ({ id := initial.id } : AgentWorkbench.Cli.IdInput)
    let _ ← invokeJson root .correctionRecord ({
      entryId := "correction-successor"
      content := "carry this intent into the strict successor" } : CorrectionRecordRequest)
    IO.FS.writeFile sourcePath "## Successor\n\nThe public lifecycle remains reachable.\n"
    let successorUnits := (← DesignSource.inspectAll root [target]).flatMap (·.units)
    let successor ← proposedDesign (← invokeJson root .designPropose
      (proposal target successorUnits "apply the current user intent in a strict successor"
        none ["correction-successor"]))
    let _ ← invokeJson root .workSuspend ({
      workId := "work-successor", resumeCondition := "adopt the accepted successor" } : AgentWorkbench.Cli.SuspendInput)
    let _ ← invokeJson root .designAccept ({ id := successor.id } : AgentWorkbench.Cli.IdInput)
    let _ ← invokeJson root .workAdoptDesign ({
      workId := "work-successor", entryId := "adoption-successor"
      agentRun := "agent-public-lifecycle" } : WorkAdoptDesignRequest)
    let _ ← invokeJson root .workResume ({
      workId := "work-successor", entryId := "resume-successor"
      satisfaction := "the accepted successor was adopted"
      basisEntryIds := ["adoption-successor"]
      agentRun := "agent-public-lifecycle" } : WorkResumeRequest)
    let _ ← invokeJson root .correctionIncorporate ({
      entryId := "correction-successor-incorporated"
      correctionEntryId := "correction-successor" } : CorrectionIncorporateRequest)
    let _ ← invokeJson root .correctionRecord ({
      entryId := "correction-withdraw-public"
      content := "withdraw this outcome without declaring completion" } : CorrectionRecordRequest)
    let _ ← invokeJson root .workWithdraw ({
      workId := "work-successor", entryId := "withdrawal-public"
      correctionEntryId := "correction-withdraw-public"
      reason := "the current user intent withdraws the outcome" } : WorkWithdrawRequest)
    let state ← Store.loadState (← Store.openReadOnly (root / ".agent-workbench" / "state.db"))
    expect (state.focusedWorkId.isNone &&
      (state.work? "work-successor").any fun work =>
        work.status == .withdrawn && work.designRevision == some successor.id)
      "public successor lifecycle lost Work identity or declared false completion"

private def exerciseSuccessorAcrossWorks : IO Unit :=
  IO.FS.withTempDir fun root => do
    let sourcePath ← prepareWorkspace root
    let target := "file:.agent-workbench/design/product/design.md"
    let _ ← invokeJson root .workStart ({
      id := "work-first-design", outcome := "establish the first Design"
      scope := "project", responsibleAgentRun := "agent-public-lifecycle" } : WorkStartRequest)
    IO.FS.writeFile sourcePath "The public lifecycle remains reachable.\n"
    let firstUnits := (← DesignSource.inspectAll root [target]).flatMap (·.units)
    let first ← proposedDesign (← invokeJson root .designPropose
      (proposal target firstUnits "establish the first accepted Design"))
    let _ ← invokeJson root .designAccept ({ id := first.id } : AgentWorkbench.Cli.IdInput)
    let _ ← invokeJson root .workSuspend ({
      workId := "work-first-design", resumeCondition := "a later Work owns the successor" }
      : AgentWorkbench.Cli.SuspendInput)
    let _ ← invokeJson root .workStart ({
      id := "work-second-design", outcome := "author a strict successor Design"
      scope := "project", responsibleAgentRun := "agent-public-lifecycle" } : WorkStartRequest)
    IO.FS.writeFile sourcePath "# Successor\n\nThe public lifecycle remains reachable.\n"
    let successorUnits := (← DesignSource.inspectAll root [target]).flatMap (·.units)
    let successor ← proposedDesign (← invokeJson root .designPropose
      (proposal target successorUnits "a later Work authors the strict successor"))
    let _ ← invokeJson root .workSuspend ({
      workId := "work-second-design", resumeCondition := "accept and adopt the successor" }
      : AgentWorkbench.Cli.SuspendInput)
    let _ ← invokeJson root .designAccept ({ id := successor.id } : AgentWorkbench.Cli.IdInput)
    let _ ← invokeJson root .workAdoptDesign ({
      workId := "work-second-design", entryId := "adopt-cross-work-successor"
      agentRun := "agent-public-lifecycle" } : WorkAdoptDesignRequest)
    let state ← Store.loadState (← Store.openReadOnly (root / ".agent-workbench" / "state.db"))
    expect ((state.design? successor.id).any fun value =>
      value.parent == some first.id && value.workId == some "work-second-design")
      "a later Work could not author and adopt a strict successor Design"

private def exerciseCrossWorkRejectIsDenied : IO Unit :=
  IO.FS.withTempDir fun root => do
    let sourcePath ← prepareWorkspace root
    let target := "file:.agent-workbench/design/product/design.md"
    let _ ← invokeJson root .workStart ({
      id := "work-reject-seed", outcome := "establish rejection baseline", scope := "project"
      responsibleAgentRun := "agent-public-lifecycle" } : WorkStartRequest)
    IO.FS.writeFile sourcePath "The public lifecycle remains reachable.\n"
    let seedUnits := (← DesignSource.inspectAll root [target]).flatMap (·.units)
    let seed ← proposedDesign (← invokeJson root .designPropose
      (proposal target seedUnits "establish rejection baseline"))
    let _ ← invokeJson root .designAccept ({ id := seed.id } : AgentWorkbench.Cli.IdInput)
    let _ ← invokeJson root .workSuspend ({
      workId := "work-reject-seed", resumeCondition := "later Works exercise rejection" }
      : AgentWorkbench.Cli.SuspendInput)
    let _ ← invokeJson root .workStart ({
      id := "work-reject-a", outcome := "own candidate A", scope := "project"
      responsibleAgentRun := "agent-public-lifecycle" } : WorkStartRequest)
    IO.FS.writeFile sourcePath "# Candidate A\n\nThe public lifecycle remains reachable.\n"
    let unitsA := (← DesignSource.inspectAll root [target]).flatMap (·.units)
    let candidateA ← proposedDesign (← invokeJson root .designPropose
      (proposal target unitsA "candidate A"))
    let _ ← invokeJson root .correctionRecord ({
      entryId := "basis-reject-a", content := "return to candidate A" }
      : CorrectionRecordRequest)
    let _ ← invokeJson root .workSuspend ({
      workId := "work-reject-a", resumeCondition := "return to candidate A" }
      : AgentWorkbench.Cli.SuspendInput)
    let _ ← invokeJson root .workStart ({
      id := "work-reject-b", outcome := "own candidate B", scope := "project"
      responsibleAgentRun := "agent-public-lifecycle" } : WorkStartRequest)
    IO.FS.writeFile sourcePath "# Candidate B\n\nThe public lifecycle remains reachable.\n"
    let unitsB := (← DesignSource.inspectAll root [target]).flatMap (·.units)
    let candidateB ← proposedDesign (← invokeJson root .designPropose
      (proposal target unitsB "candidate B"))
    let _ ← invokeJson root .workSuspend ({
      workId := "work-reject-b", resumeCondition := "return to candidate B" }
      : AgentWorkbench.Cli.SuspendInput)
    let _ ← invokeJson root .workResume ({
      workId := "work-reject-a", entryId := "resume-reject-a"
      satisfaction := "return to candidate A as recorded"
      basisEntryIds := ["basis-reject-a"], agentRun := "agent-public-lifecycle" } : WorkResumeRequest)
    invokeJsonRejected root .designReject ({
      designId := candidateB.id, entryId := "cross-work-rejection"
      reason := "this request must not cross Work authority" } : DesignRejectRequest)
    let state ← Store.loadState (← Store.openReadOnly (root / ".agent-workbench" / "state.db"))
    expect ((state.design? candidateA.id).any (·.status == .candidate) &&
      (state.design? candidateB.id).any (·.status == .candidate) &&
      (state.entry? "cross-work-rejection").isNone)
      "cross-Work Design rejection changed authoritative state"

private def exerciseStructuredAssumptionFinding : IO Unit :=
  IO.FS.withTempDir fun root => do
    let sourcePath ← prepareWorkspace root
    let target := "file:.agent-workbench/design/product/design.md"
    let _ ← invokeJson root .workStart ({
      id := "work-assumption-finding", outcome := "review a structured assumption"
      scope := "project", responsibleAgentRun := "agent-public-lifecycle" } : WorkStartRequest)
    let statementText := "The public lifecycle remains reachable."
    let assumptionText := "The external service remains available."
    IO.FS.writeFile sourcePath s!"{statementText}\n\n{assumptionText}\n"
    let units := (← DesignSource.inspectAll root [target]).flatMap (·.units)
    let statementUnit ← match units.find? (·.text == statementText) with
      | some value => pure value
      | none => throw (IO.userError "statement source unit was not parsed")
    let assumptionUnit ← match units.find? (·.text == assumptionText) with
      | some value => pure value
      | none => throw (IO.userError "assumption source unit was not parsed")
    let assumption : DesignAssumption := {
      id := "assumption-public", text := assumptionText
      sourceUnitIds := [assumptionUnit.id] }
    let designStatement : Statement := {
      id := "statement-public-assumption", text := statementText
      assumptions := [assumption.id] }
    let candidate ← proposedDesign (← invokeJson root .designPropose ({
      producerAgentRun := "agent-public-lifecycle"
      changeRationale := "exercise the exact structured-assumption Review route"
      sourceDocumentTargets := [target]
      sourceUnitDispositions := [
        { unitId := statementUnit.id, role := .requirement },
        { unitId := assumptionUnit.id, role := .assumption }]
      assumptions := [assumption]
      statements := [designStatement]
      statementCoverage := [{
        statementId := designStatement.id, sourceUnitIds := [statementUnit.id]
        leanClaims := { noSelectionReason := some "no logical Claim is needed for this fixture" }
        acceptanceCriteria := { noSelectionReason := some "the Review route is the observation" }
        implementationRequired := false
        noImplementationReason := some "the route itself is the completed behavior" }]
      acceptanceCriteria := []
      assuranceContracts := some [{
        fixtureAssuranceInput designStatement [] [] false with
        trustedBoundaryAssumptionIds := [assumption.id] }] } : DesignProposalRequest))
    let _ ← invokeJson root .reviewStart ({
      entryId := "review-assumption", reviewId := "review-assumption"
      purpose := .design, targetDesignRevision := some candidate.id
      reviewerAgentRun := "reviewer-assumption" } : ReviewStartRequest)
    let _ ← invokeJson root .reviewFinding ({
      entryId := "finding-assumption", reviewEntryId := "review-assumption"
      subject := { kind := .assumption, id := assumption.id, exactQuote := assumption.text }
      summary := "the structured assumption requires explicit attention" } : FindingRecordRequest)
    let state ← Store.loadState (← Store.openReadOnly (root / ".agent-workbench" / "state.db"))
    expect ((state.entry? "finding-assumption").any fun entry =>
      match entry.payload with
      | .finding finding => finding.subject.id == assumption.id
      | _ => false)
      "public Review route could not record an exact structured-assumption Finding"

def run : IO Unit := do
  exerciseAmendAndReject
  exerciseAdoptIncorporateAndWithdraw
  exerciseSuccessorAcrossWorks
  exerciseCrossWorkRejectIsDenied
  exerciseStructuredAssumptionFinding

end AgentWorkbenchTest.PublicDesignWorkRoute
