import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.PlanSource
import AgentWorkbench.Adapter.PlanArchive
import AgentWorkbench.Adapter.Store
import AgentWorkbenchTest.RouteReceipt

namespace AgentWorkbenchTest.PublicRoute

open AgentWorkbench AgentWorkbenchTest

private def executablePath : System.FilePath :=
  if System.Platform.isWindows then ".lake/build/bin/agent-workbench.exe"
  else ".lake/build/bin/agent-workbench"

private def testExecutablePath : System.FilePath :=
  if System.Platform.isWindows then ".lake/build/bin/agent-workbench-tests.exe"
  else ".lake/build/bin/agent-workbench-tests"

private def invoke
    (root : System.FilePath) (command : List String) (input : Option String := none) :
    IO IO.Process.Output :=
  IO.Process.output {
    cmd := executablePath.toString
    args := #["--project", root.toString] ++ command.toArray } input

private def invokeOk
    (root : System.FilePath) (command : List String) (input : Option String := none) : IO String := do
  let output ← invoke root command input
  unless output.exitCode == 0 do
    throw (IO.userError s!"public binary route failed for {command}: {output.stderr}")
  match Operation.parseCommand? command with
  | some operation => RouteReceipt.recordSuccessful .publicRoute operation
  | none => pure ()
  pure output.stdout

private def invokeJson [Lean.ToJson α]
    (root : System.FilePath) (command : List String) (input : α) : IO String :=
  invokeOk root command (some (Lean.toJson input).compress)

private def decodeOutput [Lean.FromJson α] (source : String) : IO α := do
  let json ← match Lean.Json.parse source with
    | .ok value => pure value
    | .error message => throw (IO.userError s!"public binary returned invalid JSON: {message}")
  match Lean.fromJson? json with
  | .ok value => pure value
  | .error message => throw (IO.userError s!"public binary returned the wrong JSON shape: {message}")

private def createDanglingSymlink
    (target link : System.FilePath) : IO Unit := do
  let result ← IO.Process.output { cmd := "ln", args := #["-s", target.toString, link.toString] }
  unless result.exitCode == 0 do
    throw (IO.userError s!"could not create dangling symlink: {result.stderr}")

private def firstExisting : List System.FilePath → IO (Option System.FilePath)
  | [] => pure none
  | path :: rest => do
      if ← path.pathExists then pure (some path) else firstExisting rest

private def elanExecutable : IO System.FilePath := do
  let elanHome := (← IO.getEnv "ELAN_HOME").map System.FilePath.mk
  let home := (← IO.getEnv "HOME").map fun value => System.FilePath.mk value / ".elan"
  let profile := (← IO.getEnv "USERPROFILE").map fun value => System.FilePath.mk value / ".elan"
  let names := if System.Platform.isWindows then ["elan.exe", "elan"] else ["elan"]
  let candidates := [elanHome, home, profile].filterMap id |>.flatMap fun root =>
    names.map fun name => root / "bin" / name
  match ← firstExisting candidates with
  | some path => pure path
  | none => throw (IO.userError "public init route cannot locate Elan")

private def exerciseInitAndProofRoute : IO Unit :=
  IO.FS.withTempDir fun root => do
    let productSource := root / "src" / "Product.txt"
    let productBuildInput := root / "product.build"
    IO.FS.createDirAll (root / "src")
    IO.FS.writeFile productSource "product source independent of Workbench\n"
    IO.FS.writeFile productBuildInput "product build input independent of Workbench\n"
    let sourceBefore ← IO.FS.readBinFile productSource
    let buildInputBefore ← IO.FS.readBinFile productBuildInput
    let workbenchRoot := root / ".agent-workbench"
    let bundledDirectory := workbenchRoot / "bin"
    IO.FS.createDirAll bundledDirectory
    let bundledElan := bundledDirectory /
      (if System.Platform.isWindows then "elan.exe" else "elan")
    IO.FS.writeBinFile bundledElan (← IO.FS.readBinFile (← elanExecutable))
    if !System.Platform.isWindows then
      let chmod ← IO.Process.output { cmd := "chmod", args := #["+x", bundledElan.toString] }
      unless chmod.exitCode == 0 do
        throw (IO.userError s!"public init route could not mark Elan executable: {chmod.stderr}")
    let _ ← invokeOk root ["init"]
    expect ((← IO.FS.readBinFile productSource) == sourceBefore)
      "init changed pre-existing product source"
    expect ((← IO.FS.readBinFile productBuildInput) == buildInputBefore)
      "init changed a pre-existing product build input"
    expect (!(← (root / ".github").pathExists))
      "init exposed Workbench-only verification in ordinary project infrastructure"
    let productDirectory := workbenchRoot / "design" / "product"
    let proofDirectory := workbenchRoot / "design" / "proofs" / "route"
    let proofModuleDirectory := proofDirectory / "RouteDesign"
    IO.FS.createDirAll productDirectory
    IO.FS.createDirAll proofModuleDirectory
    let designPath := productDirectory / "design.md"
    IO.FS.writeFile designPath "The artifact exists.\n"
    IO.FS.writeFile (proofDirectory / "lean-toolchain") "leanprover/lean4:v4.32.2\n"
    IO.FS.writeFile (proofDirectory / "lakefile.lean")
      "import Lake\nopen Lake DSL\npackage «public-route-proof»\n@[default_target] lean_lib RouteDesign\n"
    IO.FS.writeFile (proofDirectory / "RouteDesign.lean")
      "import RouteDesign.Base\nnamespace RouteDesign\ndef Property : Prop := Base\ntheorem property : Property := by trivial\nend RouteDesign\n"
    IO.FS.writeFile (proofModuleDirectory / "Base.lean")
      "namespace RouteDesign\ndef Base : Prop := True\nend RouteDesign\n"
    let routeClaim : LeanClaim := {
      id := "claim-public-route"
      input := {
        statementId := statement.id, statementText := statement.text
        mapping := "RouteDesign.Property represents the selected public-route Statement"
        proposition := "RouteDesign.Property", witness := "RouteDesign.property"
        proofRoot := ".agent-workbench/design/proofs/route"
        declaredSources := [
          { path := "RouteDesign.lean" }, { path := "RouteDesign/Base.lean" }]
        check := { executable := "lake", arguments := #["build"] }
        toolchain := ProofToolchain.identifier } }
    let _ ← invokeJson root ["work", "start"] ({
      id := "work-proof-route", outcome := "verify init and proof public routes"
      scope := "project", responsibleAgentRun := "agent-proof-route" } : WorkStartRequest)
    let designTarget := "file:.agent-workbench/design/product/design.md"
    let units := (← DesignSource.captureAll root [designTarget]).flatMap (·.units)
    let proposed ← invokeJson root ["design", "propose"] ({
      producerAgentRun := "agent-proof-route", changeRationale := "public proof route"
      sourceDocumentTargets := [designTarget]
      sourceUnitDispositions := units.map fun unit =>
        { unitId := unit.id, role := DesignSourceRole.requirement }
      statements := [statement]
      statementCoverage := [{
        statementId := statement.id, sourceUnitIds := units.map (·.id)
        leanClaims := { selectedIds := [routeClaim.id] }
        acceptanceCriteria := { noSelectionReason := some "the route has no external criterion" }
        implementationRequired := false
        noImplementationReason := some "the public proof route verifies the Claim itself" }]
      acceptanceCriteria := [], leanClaims := [routeClaim] } : DesignProposalRequest)
    let candidate : DesignRevision ← decodeOutput proposed
    let _ ← invokeJson root ["design", "accept"]
      ({ id := candidate.id } : AgentWorkbench.Cli.IdInput)
    let _ ← invokeJson root ["proof", "run"] ({
      entryId := "proof-public-route", claimId := routeClaim.id } : ProofRunRequest)

private def exerciseTaskLocalContractRoute : IO Unit :=
  IO.FS.withTempDir fun root => do
    let designDirectory := root / ".agent-workbench" / "design" / "product"
    let implementationDirectory := root / ".agent-workbench" / "design" / "implementation"
    let planDirectory := root / ".agent-workbench" / "design" / "plans" / "work-contract-route"
    IO.FS.createDirAll designDirectory
    IO.FS.createDirAll implementationDirectory
    IO.FS.createDirAll planDirectory
    let designPath := designDirectory / "design.md"
    IO.FS.writeFile designPath "The implementation provides the local behavior.\n"
    let contractStatement : Statement := {
      id := "statement-contract-route"
      text := "the implementation provides the local behavior" }
    let designTarget := "file:.agent-workbench/design/product/design.md"
    let designUnits := (← DesignSource.captureAll root [designTarget]).flatMap (·.units)
    let _ ← invokeJson root ["work", "start"] ({
      id := "work-contract-route", outcome := "provide the local behavior"
      scope := "project", responsibleAgentRun := "agent-contract-route" } : WorkStartRequest)
    let proposed ← invokeJson root ["design", "propose"] ({
      producerAgentRun := "agent-contract-route", changeRationale := "contract-only route"
      sourceDocumentTargets := [designTarget]
      sourceUnitDispositions := designUnits.map fun unit =>
        { unitId := unit.id, role := DesignSourceRole.requirement }
      statements := [contractStatement]
      statementCoverage := [{
        statementId := contractStatement.id, sourceUnitIds := designUnits.map (·.id)
        leanClaims := { noSelectionReason := some "no Design-time logical Claim is selected" }
        acceptanceCriteria := {
          noSelectionReason := some "verification is local to the implementation Task" }
        implementationRequired := true }]
      acceptanceCriteria := [] } : DesignProposalRequest)
    let contractDesign : DesignRevision ← decodeOutput proposed
    let _ ← invokeJson root ["design", "accept"]
      ({ id := contractDesign.id } : AgentWorkbench.Cli.IdInput)

    let artifactTarget := "file:contract-artifact.txt"
    let commandTarget := "file:contract-command.txt"
    let contractStep : PlanStep := {
      id := "contract-step", description := "implement and verify the local behavior"
      outputScopes := [artifactTarget, commandTarget]
      taskVerificationContracts := [
        { id := "contract-artifact", kind := .artifact, target := artifactTarget },
        { id := "contract-command", kind := .command, target := commandTarget }] }
    let planPath := planDirectory / "plan.md"
    IO.FS.writeFile planPath "Implement and verify the local behavior.\n"
    let planTarget := "file:.agent-workbench/design/plans/work-contract-route/plan.md"
    let planUnits := (← PlanSource.captureAll root "work-contract-route" [planTarget]).flatMap (·.units)
    let proposedPlan ← invokeJson root ["plan", "propose"] ({
      producerAgentRun := "agent-contract-route", reason := "implement the accepted behavior"
      sourceDocumentTargets := [planTarget]
      sourceUnitDispositions := planUnits.map fun unit =>
        { unitId := unit.id, stepId := some contractStep.id }
      statementDispositions := [{
        statementId := contractStatement.id, statementText := contractStatement.text
        deltaKind := .added, stepIds := [contractStep.id] }]
      steps := [contractStep] } : PlanProposalRequest)
    let contractPlan : ImplementationPlan ← decodeOutput proposedPlan
    let _ ← invokeJson root ["plan", "materialize"]
      ({ id := contractPlan.id } : AgentWorkbench.Cli.IdInput)
    let taskId := s!"task-{contractPlan.id}-{contractStep.id}"
    let artifactPath := root / "contract-artifact.txt"
    let commandPath := root / "contract-command.txt"
    IO.FS.writeFile artifactPath "artifact-v1\n"
    let testHelper ← IO.FS.realPath testExecutablePath
    let command : CommandSpec := {
      executable := testHelper.toString
      arguments := #["write-artifact", commandPath.toString, "command-v1\n"] }
    let _ ← invokeJson root ["profile", "define"] ({
      entryId := "profile-contract-route", purpose := "verify the Task-local command behavior"
      taskEntryId := taskId, outputScope := commandTarget
      taskVerificationIds := ["contract-command"], command } : ProfileDefineRequest)
    let _ ← invokeJson root ["command", "run"] ({
      profileEntryId := "profile-contract-route", entryId := "command-contract-route"
      taskVerificationId := some "contract-command" } : CommandRunRequest)
    let _ ← invokeJson root ["artifact", "observe"] ({
      entryId := "artifact-contract-route", taskEntryId := taskId
      taskVerificationId := some "contract-artifact"
      operation := "inspect Task-local artifact", result := "artifact exists"
      successful := true } : ArtifactObserveRequest)
    let _ ← invokeJson root ["task", "close"] ({
      entryId := "task-contract-closed", taskEntryId := taskId } : TaskCloseRequest)
    let ready ← invokeOk root ["ready"]
    expect (ready.contains "\"ready\":true")
      s!"contract-only public route was not ready after exact evidence closure: {ready}"

    IO.FS.writeFile artifactPath "artifact-v2\n"
    let stale ← invokeOk root ["context"]
    expect (stale.contains "task reopen-stale")
      s!"contract-only output drift did not expose Task recovery: {stale}"
    let _ ← invokeOk root ["task", "reopen-stale"]
    let reopenedState ← Store.loadState
      (← Store.openReadOnly (root / ".agent-workbench" / "state.db"))
    let reopenedTask ← match reopenedState.ledgerEntries.find? fun entry =>
        !entryIsSuperseded reopenedState entry && match entry.payload with
        | .task task => task.lineageId == some "work-contract-route:contract-step" && !task.closed
        | _ => false with
      | some value => pure value
      | none => throw (IO.userError "contract-only recovery omitted its current Task")
    let _ ← invokeJson root ["profile", "replace"] ({
      entryId := "profile-contract-route-reopened", profileEntryId := "profile-contract-route"
      purpose := "reverify the Task-local command behavior", taskEntryId := reopenedTask.id
      outputScope := commandTarget, taskVerificationIds := ["contract-command"]
      command } : ProfileReplaceRequest)
    let _ ← invokeJson root ["command", "run"] ({
      profileEntryId := "profile-contract-route-reopened"
      entryId := "command-contract-route-reopened"
      taskVerificationId := some "contract-command" } : CommandRunRequest)
    let _ ← invokeJson root ["artifact", "observe"] ({
      entryId := "artifact-contract-route-reopened", taskEntryId := reopenedTask.id
      taskVerificationId := some "contract-artifact"
      operation := "reinspect Task-local artifact", result := "artifact exists"
      successful := true } : ArtifactObserveRequest)
    let _ ← invokeJson root ["task", "close"] ({
      entryId := "task-contract-reclosed", taskEntryId := reopenedTask.id } : TaskCloseRequest)
    let reready ← invokeOk root ["ready"]
    expect (reready.contains "\"ready\":true")
      s!"contract-only public route was not ready after re-verification: {reready}"

def run : IO Unit := do
  exerciseInitAndProofRoute
  exerciseTaskLocalContractRoute
  IO.FS.withTempDir fun sandbox => do
    let root := sandbox / "project"
    let outsideDirectory := sandbox / "outside"
    IO.FS.createDirAll root
    let commandCriterion : AcceptanceCriterion := {
      id := "criterion-command-route", statementId := some statement.id
      statement := "the artifact command succeeds", target := criterion.target
      evidenceKind := "command" }
    let workbenchRoot := root / ".agent-workbench"
    let database := workbenchRoot / "state.db"
    let designDirectory := workbenchRoot / "design" / "product"
    let implementationDirectory := workbenchRoot / "design" / "implementation"
    let planDirectory := workbenchRoot / "design" / "plans" / "work-route"
    IO.FS.createDirAll designDirectory
    IO.FS.createDirAll implementationDirectory
    IO.FS.createDirAll planDirectory
    let _ ← invokeJson root ["work", "start"] ({
      id := "work-route", outcome := "produce a public-route artifact"
      scope := "project", responsibleAgentRun := "agent-route" } : WorkStartRequest)
    let designPath := designDirectory / "design.md"
    IO.FS.writeFile designPath "The artifact exists.\n"
    let routeStep : PlanStep := { step with
      verificationCriterionIds := [criterion.id, commandCriterion.id] }
    let danglingLeafStep : PlanStep := {
      id := "dangling-leaf-step", description := "verify dangling leaf rejection"
      outputScopes := ["file:dangling-command-leaf"]
      taskVerificationContracts := [
        { id := "dangling-leaf-command", kind := .command,
          target := "file:dangling-command-leaf" }] }
    let danglingIntermediateStep : PlanStep := {
      id := "dangling-intermediate-step", description := "verify dangling intermediate rejection"
      outputScopes := ["file:dangling-command-intermediate/output.txt"]
      taskVerificationContracts := [
        { id := "dangling-intermediate-command", kind := .command,
          target := "file:dangling-command-intermediate/output.txt" }] }
    let routeSteps := if System.Platform.isWindows then [routeStep]
      else [routeStep, danglingLeafStep, danglingIntermediateStep]
    let designTarget := "file:.agent-workbench/design/product/design.md"
    let capturedDesign ← DesignSource.captureAll root [designTarget]
    let designUnits := capturedDesign.flatMap (·.units)
    expect (!designUnits.isEmpty) "public route produced no Design source unit"
    let productSentinel := root / "protected-output-product.txt"
    IO.FS.writeFile productSentinel "product content before rejected output scopes\n"
    let productSentinelBefore ← IO.FS.readBinFile productSentinel
    let designResult ← invokeJson root ["design", "propose"] ({
      producerAgentRun := "agent-route"
      changeRationale := "initial public-route Design"
      sourceDocumentTargets := [designTarget]
      sourceUnitDispositions := designUnits.map fun unit =>
        { unitId := unit.id, role := DesignSourceRole.requirement }
      statements := [statement]
      statementCoverage := [{
        statementId := statement.id, sourceUnitIds := designUnits.map (·.id)
        leanClaims := { noSelectionReason := some "the route has no Design-time logical Claim" }
        acceptanceCriteria := { selectedIds := [criterion.id, commandCriterion.id] }
        implementationRequired := true }]
      acceptanceCriteria := [criterion, commandCriterion] } : DesignProposalRequest)
    let candidate : DesignRevision ← decodeOutput designResult
    let candidateId := candidate.id
    let _ ← invokeJson root ["design", "accept"]
      ({ id := candidateId } : AgentWorkbench.Cli.IdInput)

    let _ ← invokeJson root ["correction", "record"] ({
      entryId := "correction-route-initial"
      content := "clarify how the installed route records its artifact" } : CorrectionRecordRequest)
    let _ ← invokeJson root ["correction", "supersede"] ({
      entryId := "correction-route-current"
      correctionEntryId := "correction-route-initial"
      content := "use the current command route as the clarified action" } : CorrectionSupersedeRequest)
    let _ ← invokeJson root ["work", "suspend"] ({
      workId := "work-route", resumeCondition := "resume after the clarification is recorded" } : AgentWorkbench.Cli.SuspendInput)
    let _ ← invokeJson root ["work", "resume"] ({
      workId := "work-route", entryId := "resume-route"
      satisfaction := "the current clarification records the required basis"
      basisEntryIds := ["correction-route-current"], agentRun := "agent-route" } : WorkResumeRequest)
    let _ ← invokeJson root ["work", "handoff"] ({
      workId := "work-route", entryId := "handoff-route"
      successorRun := "agent-route-2", reason := "continue through a distinct responsible run" } : AgentWorkbench.Cli.HandoffInput)
    let _ ← invokeJson root ["kpt", "record"] ({
      entryId := "kpt-route", tryNext := some "run the current Task-bound Command Profile" } : KptRecordRequest)

    let planPath := planDirectory / "plan.md"
    let planBytes :=
      "Create and verify the artifact.\r\n\r\nVerify dangling leaf rejection.\r\n\r\nVerify dangling intermediate rejection.\r\n".toUTF8
    IO.FS.writeBinFile planPath planBytes
    let planTarget := "file:.agent-workbench/design/plans/work-route/plan.md"
    let capturedPlan ← PlanSource.captureAll root "work-route" [planTarget]
    let planUnits := capturedPlan.flatMap (·.units)
    expect (planUnits.length == 3) "public route did not produce the three expected Plan units"
    let (planUnit0, planUnit1, planUnit2) ← match planUnits with
      | [first, second, third] => pure (first, second, third)
      | _ => throw (IO.userError "public route could not bind the expected Plan source units")
    let planSourceDispositions : List PlanSourceUnitDisposition :=
      if System.Platform.isWindows then [
        { unitId := planUnit0.id, stepId := some routeStep.id },
        { unitId := planUnit1.id,
          noStepReason := some "dangling symlink regression is unavailable on Windows" },
        { unitId := planUnit2.id,
          noStepReason := some "dangling symlink regression is unavailable on Windows" }]
      else [
        { unitId := planUnit0.id, stepId := some routeStep.id },
        { unitId := planUnit1.id, stepId := some danglingLeafStep.id },
        { unitId := planUnit2.id, stepId := some danglingIntermediateStep.id }]
    let beforeProtectedScopes ← Store.loadState (← Store.openReadOnly database)
    for outputScope in
        ["tree:.", "tree:.agent-workbench", "file:.agent-workbench/state.db"] do
      let protectedStep : PlanStep := {
        routeStep with outputScopes := [criterion.target, outputScope] }
      let rejected ← invoke root ["plan", "propose"] (some (Lean.toJson ({
        producerAgentRun := "agent-route"
        reason := "protected managed-output regression"
        sourceDocumentTargets := [planTarget]
        sourceUnitDispositions := planUnits.map fun unit =>
          { unitId := unit.id, stepId := some protectedStep.id }
        statementDispositions := [{
          statementId := statement.id, statementText := statement.text
          deltaKind := .added, stepIds := [protectedStep.id] }]
        steps := [protectedStep] } : PlanProposalRequest)).compress)
      expect (rejected.exitCode != 0 && rejected.stderr.contains "managed output")
        s!"public Plan route accepted protected output scope {outputScope}"
      expect ((← Store.loadState (← Store.openReadOnly database)) == beforeProtectedScopes)
        s!"protected output scope changed authoritative state: {outputScope}"
      expect ((← IO.FS.readBinFile productSentinel) == productSentinelBefore)
        s!"protected output scope changed product content: {outputScope}"
    let planResult ← invokeJson root ["plan", "propose"] ({
      producerAgentRun := "agent-route"
      reason := "implement the complete initial Design delta"
      sourceDocumentTargets := [planTarget]
      sourceUnitDispositions := planSourceDispositions
      statementDispositions := [{
        statementId := statement.id, statementText := statement.text
        deltaKind := .added, stepIds := routeSteps.map (·.id) }]
      steps := routeSteps } : PlanProposalRequest)
    let planCandidate : ImplementationPlan ← decodeOutput planResult
    let planId := planCandidate.id
    let archivedPlan ← AgentWorkbench.PlanArchive.source root planId planTarget
    expect (archivedPlan.contentBytes == planBytes.data.toList.map (·.toNat))
      "public Plan proposal did not preserve exact source bytes in SQLite"
    let _ ← invokeJson root ["plan", "materialize"]
      ({ id := planId } : AgentWorkbench.Cli.IdInput)

    let taskId := s!"task-{planId}-{routeStep.id}"
    let artifactPath := root / "artifact.txt"
    IO.FS.writeFile artifactPath "baseline artifact\n"
    IO.FS.writeFile (root / "command-input.txt") "current input\n"
    let testHelper ← IO.FS.realPath testExecutablePath
    let successfulCommand : CommandSpec := {
      executable := testHelper.toString
      arguments := #["write-artifact", artifactPath.toString, "command-output\n"] }
    let helperCheck ← AgentWorkbench.Process.execute root successfulCommand
    unless helperCheck.exitCode == 0 do
      throw (IO.userError s!"native command helper failed: {helperCheck.stderr}")
    IO.FS.writeFile artifactPath "baseline artifact\n"
    let beforeProtectedProfiles ← Store.loadState (← Store.openReadOnly database)
    for outputScope in
        ["tree:.", "tree:.agent-workbench", "file:.agent-workbench/state.db"] do
      let rejected ← invoke root ["profile", "define"] (some (Lean.toJson ({
        entryId := "profile-protected-output"
        purpose := "protected managed-output regression"
        taskEntryId := taskId, outputScope
        criterionIds := [commandCriterion.id], command := successfulCommand
        } : ProfileDefineRequest)).compress)
      expect (rejected.exitCode != 0 && rejected.stderr.contains "managed output")
        s!"public Profile route accepted protected output scope {outputScope}"
      expect ((← Store.loadState (← Store.openReadOnly database)) == beforeProtectedProfiles)
        s!"protected Profile output changed authoritative state: {outputScope}"
      expect ((← IO.FS.readBinFile productSentinel) == productSentinelBefore)
        s!"protected Profile output changed product content: {outputScope}"
    let _ ← invokeJson root ["profile", "define"] ({
      entryId := "profile-route", purpose := "produce the Task output"
      taskEntryId := taskId, inputTargets := ["file:command-input.txt"]
      outputScope := criterion.target
      criterionIds := [commandCriterion.id], command := successfulCommand } : ProfileDefineRequest)
    let secretProfile := Lean.Json.mkObj [
      ("entryId", "profile-secret-projection"),
      ("purpose", "verify environment disclosure projection"),
      ("taskEntryId", taskId),
      ("inputTargets", Lean.Json.arr #[]),
      ("outputScope", criterion.target),
      ("criterionIds", Lean.Json.arr #[commandCriterion.id]),
      ("command", Lean.Json.mkObj [
        ("executable", testHelper.toString),
        ("arguments", Lean.Json.arr #[]),
        ("workingDirectory", Lean.Json.null),
        ("environment", Lean.Json.arr #[Lean.Json.arr #["API_TOKEN", "super-secret-value"]])])]
    let beforeSecretInput ← Store.loadState (← Store.openReadOnly database)
    let projectedProfile ← invoke root ["profile", "define"] (some secretProfile.compress)
    expect (projectedProfile.exitCode != 0)
      "profile definition accepted a raw environment value"
    expect ((← Store.loadState (← Store.openReadOnly database)) == beforeSecretInput)
      "rejected raw environment input changed authoritative state"
    let environmentIdentityCommand : CommandSpec := {
      successfulCommand with environment := #["API_TOKEN"] }
    let _ ← invokeJson root ["profile", "define"] ({
      entryId := "profile-environment-identity"
      purpose := "record only an environment name"
      taskEntryId := taskId, outputScope := criterion.target
      criterionIds := [commandCriterion.id]
      command := environmentIdentityCommand } : ProfileDefineRequest)
    let projectedEntry ← invoke root ["entry", "get"]
      (some (Lean.toJson ({ id := "profile-environment-identity" } : AgentWorkbench.Cli.IdInput)).compress)
    expect (projectedEntry.exitCode == 0 && projectedEntry.stdout.contains "API_TOKEN" &&
      !projectedEntry.stdout.contains "super-secret-value")
      "persisted Command Profile retained a raw environment value"
    let _ ← invokeJson root ["profile", "replace"] ({
      entryId := "profile-route-current", profileEntryId := "profile-route"
      purpose := "produce the current Task output", taskEntryId := taskId
      inputTargets := ["file:command-input.txt"]
      outputScope := criterion.target, criterionIds := [commandCriterion.id]
      command := successfulCommand } : ProfileReplaceRequest)
    if !System.Platform.isWindows then
      let leafTaskId := s!"task-{planId}-{danglingLeafStep.id}"
      let intermediateTaskId := s!"task-{planId}-{danglingIntermediateStep.id}"
      IO.FS.createDirAll outsideDirectory
      let outsideSentinel := outsideDirectory / "sentinel.txt"
      IO.FS.writeFile outsideSentinel "outside content before rejected command\n"
      let outsideBefore ← IO.FS.readBinFile outsideSentinel
      let leafLink := root / "dangling-command-leaf"
      let leafReferent := outsideDirectory / "missing-command-leaf"
      createDanglingSymlink leafReferent leafLink
      let leafCommand : CommandSpec := {
        executable := testHelper.toString
        arguments := #["write-artifact", leafLink.toString, "command executed\n"] }
      let _ ← invokeJson root ["profile", "define"] ({
        entryId := "profile-dangling-leaf", purpose := "reject a dangling leaf output"
        taskEntryId := leafTaskId, outputScope := "file:dangling-command-leaf"
        taskVerificationIds := ["dangling-leaf-command"]
        command := leafCommand } : ProfileDefineRequest)
      let beforeDanglingLeaf ← Store.loadState (← Store.openReadOnly database)
      let danglingLeaf ← invoke root ["command", "run"] (some (Lean.toJson ({
        profileEntryId := "profile-dangling-leaf", entryId := "command-dangling-leaf"
        taskVerificationId := some "dangling-leaf-command" } : CommandRunRequest)).compress)
      expect (danglingLeaf.exitCode != 0 && danglingLeaf.stderr.contains "symlink")
        "public command route accepted a dangling leaf managed output"
      expect ((← Store.loadState (← Store.openReadOnly database)) == beforeDanglingLeaf)
        "dangling leaf command rejection changed authoritative state"
      expect ((← IO.FS.readBinFile productSentinel) == productSentinelBefore &&
          (← IO.FS.readBinFile outsideSentinel) == outsideBefore &&
          !(← leafReferent.pathExists))
        "dangling leaf command executed or changed content outside the project"

      let intermediateLink := root / "dangling-command-intermediate"
      let intermediateReferent := outsideDirectory / "missing-command-directory"
      createDanglingSymlink intermediateReferent intermediateLink
      let intermediateUnsafeCommand : CommandSpec := {
        executable := testHelper.toString
        arguments := #["write-artifact", productSentinel.toString, "command executed\n"] }
      let _ ← invokeJson root ["profile", "define"] ({
        entryId := "profile-dangling-intermediate"
        purpose := "reject a dangling intermediate output"
        taskEntryId := intermediateTaskId
        outputScope := "file:dangling-command-intermediate/output.txt"
        taskVerificationIds := ["dangling-intermediate-command"]
        command := intermediateUnsafeCommand } : ProfileDefineRequest)
      let beforeDanglingIntermediate ← Store.loadState (← Store.openReadOnly database)
      let danglingIntermediate ← invoke root ["command", "run"] (some (Lean.toJson ({
        profileEntryId := "profile-dangling-intermediate"
        entryId := "command-dangling-intermediate"
        taskVerificationId := some "dangling-intermediate-command" } : CommandRunRequest)).compress)
      expect (danglingIntermediate.exitCode != 0 && danglingIntermediate.stderr.contains "symlink")
        "public command route accepted a dangling intermediate managed output"
      expect ((← Store.loadState (← Store.openReadOnly database)) == beforeDanglingIntermediate)
        "dangling intermediate command rejection changed authoritative state"
      expect ((← IO.FS.readBinFile productSentinel) == productSentinelBefore &&
          (← IO.FS.readBinFile outsideSentinel) == outsideBefore &&
          !(← intermediateReferent.pathExists))
        "dangling intermediate command executed or changed content outside the project"
      let danglingJournalRows ← AgentWorkbench.SQLite.queryScalar
        (AgentWorkbench.Store.readConnection (← Store.openReadOnly database))
        "SELECT CAST(COUNT(*) AS TEXT) FROM managed_operations" #[]
      expect (danglingJournalRows == "0")
        "pre-execution dangling output rejection created a recovery journal"
      IO.FS.removeFile leafLink
      let _ ← invokeJson root ["profile", "replace"] ({
        entryId := "profile-dangling-leaf-current"
        profileEntryId := "profile-dangling-leaf"
        purpose := "verify the safe leaf output"
        taskEntryId := leafTaskId, outputScope := "file:dangling-command-leaf"
        taskVerificationIds := ["dangling-leaf-command"]
        command := leafCommand } : ProfileReplaceRequest)
      let _ ← invokeJson root ["command", "run"] ({
        profileEntryId := "profile-dangling-leaf-current"
        entryId := "command-dangling-leaf-current"
        taskVerificationId := some "dangling-leaf-command" } : CommandRunRequest)
      IO.FS.removeFile intermediateLink
      IO.FS.createDirAll (root / "dangling-command-intermediate")
      let intermediateSafeCommand : CommandSpec := {
        executable := testHelper.toString
        arguments := #["write-artifact",
          (root / "dangling-command-intermediate" / "output.txt").toString,
          "safe command output\n"] }
      let _ ← invokeJson root ["profile", "replace"] ({
        entryId := "profile-dangling-intermediate-current"
        profileEntryId := "profile-dangling-intermediate"
        purpose := "verify the safe intermediate output"
        taskEntryId := intermediateTaskId
        outputScope := "file:dangling-command-intermediate/output.txt"
        taskVerificationIds := ["dangling-intermediate-command"]
        command := intermediateSafeCommand } : ProfileReplaceRequest)
      let _ ← invokeJson root ["command", "run"] ({
        profileEntryId := "profile-dangling-intermediate-current"
        entryId := "command-dangling-intermediate-current"
        taskVerificationId := some "dangling-intermediate-command" } : CommandRunRequest)
      let _ ← invokeJson root ["task", "close"] ({
        entryId := "task-dangling-leaf-closed", taskEntryId := leafTaskId } : TaskCloseRequest)
      let _ ← invokeJson root ["task", "close"] ({
        entryId := "task-dangling-intermediate-closed"
        taskEntryId := intermediateTaskId } : TaskCloseRequest)
    let beforePostCommitFault ← Store.loadState (← Store.openReadOnly database)
    let postCommitRejected ← try
        let _ ← Store.executeMutationWithPostCommitVerification root database
          (.commandRun {
            profileEntryId := "profile-route-current", entryId := "command-route"
            criterionId := some commandCriterion.id })
          (throw (IO.userError "injected post-commit verification fault"))
        pure false
      catch _ => pure true
    expect postCommitRejected
      "post-commit verification fault did not reject the command response"
    let afterPostCommitFault ← Store.loadState (← Store.openReadOnly database)
    expect (afterPostCommitFault.revision == beforePostCommitFault.revision + 1 &&
      afterPostCommitFault.ledgerEntries.any (fun entry => entry.id == "command-route"))
      "post-commit verification fault lost committed command authority"
    expect ((← IO.FS.readFile artifactPath).startsWith "command-output")
      "post-commit verification fault restored the old managed output"
    let recoveryRows ← AgentWorkbench.SQLite.queryTextRows
      (AgentWorkbench.Store.readConnection (← Store.openReadOnly database))
      "SELECT COALESCE(CAST(committed_state_revision AS TEXT), '')
       FROM managed_operations WHERE committed_state_revision IS NOT NULL"
      #[] 1
    expect (recoveryRows.size == 1 && recoveryRows[0]![0]! == toString afterPostCommitFault.revision)
      "post-commit verification fault cleared or misclassified the durable recovery marker"
    let _ ← invokeJson root ["correction", "resolve"] ({
      entryId := "correction-route-resolved"
      correctionEntryId := "correction-route-current", actionEntryId := "command-route"
      reason := "the current Task-bound command applied the clarification" } : CorrectionResolveRequest)
    expect ((← IO.FS.readFile (root / "artifact.txt")).startsWith "command-output")
      "managed-output recovery rolled back the committed command output"
    let remainingRecoveryRows ← AgentWorkbench.SQLite.queryScalar
      (AgentWorkbench.Store.readConnection (← Store.openReadOnly database))
      "SELECT CAST(COUNT(*) AS TEXT) FROM managed_operations" #[]
    expect (remainingRecoveryRows == "0")
      "next operation did not recover and clear the committed managed-output marker"
    let _ ← invokeJson root ["kpt", "apply"] ({
      entryId := "kpt-route-applied", kptEntryId := "kpt-route"
      actionEntryId := "command-route", outcome := "the Try produced current command evidence" } : KptApplyRequest)

    let failingCommand : CommandSpec := {
      executable := testHelper.toString
      arguments := #["write-artifact-fail", artifactPath.toString, "partial-output\n"] }
    let _ ← invokeJson root ["profile", "define"] ({
      entryId := "profile-failing", purpose := "exercise failed managed output restoration"
      taskEntryId := taskId, outputScope := criterion.target
      criterionIds := [commandCriterion.id], command := failingCommand } : ProfileDefineRequest)
    let beforeFailure ← Store.loadState (← Store.openReadOnly database)
    let beforeFailureOutput ← IO.FS.readFile (root / "artifact.txt")
    let failingRequest : CommandRunRequest := {
      profileEntryId := "profile-failing", entryId := "command-failing"
      criterionId := some commandCriterion.id }
    let failedOutput ← invoke root ["command", "run"]
      (some (Lean.toJson failingRequest).compress)
    let rejected := failedOutput.exitCode != 0
    expect rejected "failed public command was accepted"
    expect ((← IO.FS.readFile (root / "artifact.txt")) == beforeFailureOutput)
      "failed public command did not restore the prior managed output"
    expect ((← Store.loadState (← Store.openReadOnly database)) == beforeFailure)
      "failed public command changed authoritative state"

    let _ ← invokeJson root ["artifact", "observe"] ({
      entryId := "evidence-route", taskEntryId := taskId
      criterionId := criterion.id, operation := "inspect artifact"
      result := "artifact exists", successful := true } : ArtifactObserveRequest)
    let beforeInputChange ← Store.loadState (← Store.openReadOnly database)
    IO.FS.writeFile (root / "command-input.txt") "changed after command evidence\n"
    let staleCloseRequest : TaskCloseRequest := {
      entryId := "task-close-stale-input", taskEntryId := taskId }
    let staleClose ← invoke root ["task", "close"]
      (some (Lean.toJson staleCloseRequest).compress)
    expect (staleClose.exitCode != 0)
      "Task close reused command evidence after a declared input changed"
    expect ((← Store.loadState (← Store.openReadOnly database)) == beforeInputChange)
      "stale command-input rejection changed authoritative state"
    IO.FS.writeFile (root / "command-input.txt") "current input\n"
    let _ ← invokeJson root ["task", "close"] ({
      entryId := "task-closed-route", taskEntryId := taskId } : TaskCloseRequest)

    -- A declared input can change after a Plan replacement legitimately inherited a closed Task.
    -- The public Task-state transition must make re-verification reachable without inventing
    -- another Plan revision.
    IO.FS.writeFile (root / "command-input.txt") "changed after Task closure\n"
    let staleContext ← invokeOk root ["context"]
    expect ((staleContext.splitOn "task reopen-stale").length > 1)
      s!"context did not expose the stale closed-Task recovery transition: {staleContext}"
    let _ ← invokeOk root ["task", "reopen-stale"]
    let reopenedState ← Store.loadState (← Store.openReadOnly database)
    let reopenedTask ← match reopenedState.ledgerEntries.find? fun entry =>
        !entryIsSuperseded reopenedState entry && match entry.payload with
        | .task task => task.lineageId == some s!"work-route:{routeStep.id}" && !task.closed
        | _ => false with
      | some value => pure value
      | none => throw (IO.userError "stale Task recovery did not create a current open Task")
    let _ ← invokeJson root ["profile", "replace"] ({
      entryId := "profile-route-reopened", profileEntryId := "profile-route-current"
      purpose := "produce the Task output after an input change"
      taskEntryId := reopenedTask.id, inputTargets := ["file:command-input.txt"]
      outputScope := criterion.target, criterionIds := [commandCriterion.id]
      command := successfulCommand } : ProfileReplaceRequest)
    let _ ← invokeJson root ["command", "run"] ({
      profileEntryId := "profile-route-reopened", entryId := "command-route-reopened"
      criterionId := some commandCriterion.id } : CommandRunRequest)
    let _ ← invokeJson root ["artifact", "observe"] ({
      entryId := "evidence-route-reopened", taskEntryId := reopenedTask.id
      criterionId := criterion.id, operation := "inspect regenerated artifact"
      result := "artifact exists", successful := true } : ArtifactObserveRequest)
    let _ ← invokeJson root ["task", "close"] ({
      entryId := "task-reclosed-route", taskEntryId := reopenedTask.id } : TaskCloseRequest)
    let beforeCurrentReopen ← Store.loadState (← Store.openReadOnly database)
    let rejectedCurrentReopen ← invoke root ["task", "reopen-stale"]
    expect (rejectedCurrentReopen.exitCode != 0)
      "Task recovery acted as a generic reopen while all closing evidence was current"
    expect ((← Store.loadState (← Store.openReadOnly database)) == beforeCurrentReopen)
      "rejected current-Task reopen changed authoritative state or revision"

    let beforeEmptyReviewer ← Store.loadState (← Store.openReadOnly database)
    let emptyReviewer ← invoke root ["review", "start"]
      (some (Lean.toJson ({
        entryId := "review-empty-route", reviewId := "review-empty-route"
        purpose := ReviewPurpose.implementation, reviewerAgentRun := "" } : ReviewStartRequest)).compress)
    expect (emptyReviewer.exitCode != 0)
      "public Review route accepted an empty reviewer identity"
    expect ((← Store.loadState (← Store.openReadOnly database)) == beforeEmptyReviewer)
      "empty reviewer rejection changed authoritative state"

    let _ ← invokeJson root ["review", "start"] ({
      entryId := "review-route", reviewId := "review-lineage-route"
      purpose := ReviewPurpose.implementation, reviewerAgentRun := "reviewer-route-1" } : ReviewStartRequest)
    let currentInspection : ReviewInspection ← decodeOutput (← invokeJson root
      ["review", "inspect"] ({ id := "review-route" } : AgentWorkbench.Cli.IdInput))
    expect (currentInspection.targetCurrent &&
      currentInspection.currentTargetSnapshot == (match currentInspection.review.payload with
        | .review review => some review.targetSnapshot
        | _ => none))
      "Review inspection did not expose the current immutable target"
    let reviewedArtifact ← IO.FS.readFile (root / "artifact.txt")
    IO.FS.writeFile (root / "artifact.txt") "changed after the fixed Review\n"
    let staleInspection : ReviewInspection ← decodeOutput (← invokeJson root
      ["review", "inspect"] ({ id := "review-route" } : AgentWorkbench.Cli.IdInput))
    expect (!staleInspection.targetCurrent && staleInspection.currentTargetSnapshot.isSome)
      "Review inspection treated a changed implementation target as current"
    IO.FS.writeFile (root / "artifact.txt") reviewedArtifact
    let _ ← invokeJson root ["review", "handoff"] ({
      entryId := "review-handoff-route", reviewEntryId := "review-route"
      successorReviewerRun := "reviewer-route-2", reason := "continue the same fixed Review" } : ReviewHandoffRequest)
    let findingSubject : FindingSubject := {
      kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
    let _ ← invokeJson root ["review", "finding"] ({
      entryId := "finding-route", reviewEntryId := "review-route"
      subject := findingSubject
      summary := "the implementation requires one explicit remediation" } : FindingRecordRequest)
    let _ ← invokeJson root ["review", "conclude"] ({
      entryId := "review-conclusion-route", reviewEntryId := "review-route"
      clean := false, summary := "one fixed-target Finding was recorded" } : ReviewConclusionRequest)
    let _ ← invokeJson root ["review", "disposition"] ({
      entryId := "disposition-route", findingEntryId := "finding-route"
      decision := DispositionDecision.accepted
      reason := "materialize the remediation through the Work Plan" } : DispositionRecordRequest)

    IO.FS.writeFile planPath "Remediate and verify the artifact.\n"
    let replacementCapture ← PlanSource.captureAll root "work-route" [planTarget]
    let replacementUnits := replacementCapture.flatMap (·.units)
    let replacementStep : PlanStep := { routeStep with
      description := "remediate and verify the artifact"
      acceptedFindingEntryIds := ["finding-route"] }
    let replacementResult ← invokeJson root ["plan", "replace"] ({
      predecessorPlanId := some planId, producerAgentRun := "agent-route-2"
      reason := "apply the accepted fixed-target Finding"
      changeBasisEntryIds := ["finding-route"]
      sourceDocumentTargets := [planTarget]
      sourceUnitDispositions := replacementUnits.map fun unit =>
        { unitId := unit.id, stepId := some replacementStep.id }
      statementDispositions := [{
        statementId := statement.id, statementText := statement.text
        deltaKind := .added, stepIds := [replacementStep.id] }]
      steps := [replacementStep] } : PlanProposalRequest)
    let replacement : ImplementationPlan ← decodeOutput replacementResult
    let _ ← invokeJson root ["plan", "materialize"]
      ({ id := replacement.id } : AgentWorkbench.Cli.IdInput)
    let replacementTaskId := s!"task-{replacement.id}-{replacementStep.id}"
    let _ ← invokeJson root ["profile", "define"] ({
      entryId := "profile-remediation", purpose := "produce the remediated Task output"
      taskEntryId := replacementTaskId, inputTargets := ["file:command-input.txt"]
      outputScope := criterion.target
      criterionIds := [commandCriterion.id], command := successfulCommand } : ProfileDefineRequest)
    let _ ← invokeJson root ["command", "run"] ({
      profileEntryId := "profile-remediation", entryId := "command-remediation"
      criterionId := some commandCriterion.id } : CommandRunRequest)
    let _ ← invokeJson root ["artifact", "observe"] ({
      entryId := "evidence-remediation", taskEntryId := replacementTaskId
      criterionId := criterion.id, operation := "inspect remediated artifact"
      result := "remediated artifact exists", successful := true } : ArtifactObserveRequest)
    let _ ← invokeJson root ["task", "close"] ({
      entryId := "task-closed-remediation", taskEntryId := replacementTaskId } : TaskCloseRequest)
    let _ ← invokeJson root ["review", "resume"] ({
      entryId := "review-resume-route", continuesEntryId := "review-route" } : ReviewResumeRequest)
    let _ ← invokeJson root ["review", "verify"] ({
      entryId := "review-verification-route", findingEntryId := "finding-route"
      reviewEntryId := "review-resume-route", evidenceEntryId := "evidence-remediation" } : VerificationRecordRequest)

    let _ ← invokeOk root ["work", "complete"]
    let completed ← Store.loadState (← Store.openReadOnly database)
    expect (completed.focusedWorkId.isNone &&
      completed.works.head?.any (·.status == .completed))
      "public Store route did not complete the same Work"
    let completionEntries := completed.ledgerEntries.filter fun entry => match entry.payload with
      | .workCompletion value => value.workId == "work-route" && !value.inputDigest.isEmpty
      | _ => false
    expect (completionEntries.length == 1)
      "public Store route did not create exactly one completion authority"
    let reloaded ← Store.loadState (← Store.openReadOnly database)
    expect (reloaded == completed)
      "SQLite round trip changed the completed public-route state"

end AgentWorkbenchTest.PublicRoute
