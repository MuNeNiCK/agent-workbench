import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.Store

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
  pure output.stdout

private def invokeJson [Lean.ToJson α]
    (root : System.FilePath) (operation : Operation) (input : α) : IO String :=
  invoke root operation (some (Lean.toJson input).compress)

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
    acceptanceCriteria := [] }

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
    let _ ← invokeJson root .workResume ({ id := "work-successor" } : AgentWorkbench.Cli.IdInput)
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

def run : IO Unit := do
  exerciseAmendAndReject
  exerciseAdoptIncorporateAndWithdraw

end AgentWorkbenchTest.PublicDesignWorkRoute
