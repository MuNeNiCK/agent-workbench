import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Adapter

open AgentWorkbench AgentWorkbenchTest

def run : IO Unit := do
  let leanExecutable := if System.Platform.isWindows then "lean.exe" else "lean"
  expect (ContentDigest.string "" ==
    "sha3-256:a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a")
    "SHA3-256 empty-string vector failed"
  expect (ContentDigest.string "abc" ==
    "sha3-256:3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532")
    "SHA3-256 abc vector failed"
  let identityRoot := System.mkFilePath ["packet", "self-application"]
  let identitySource := System.mkFilePath ["packet", "product", "Proof.lean"]
  expect (ProofInput.pathIdentity identityRoot identitySource == "../product/Proof.lean")
    "proof source identity retained its relocatable packet prefix"
  let elanHome := System.mkFilePath ["packet", ".agent-workbench", "toolchains"]
  let toolchainSource := elanHome / "leanprover" / "Init.lean"
  let prefixSibling := System.mkFilePath
    ["packet", ".agent-workbench", "toolchains-evil", "Evil.lean"]
  expect (ProofInput.pathWithin elanHome toolchainSource &&
    !ProofInput.pathWithin elanHome prefixSibling)
    "toolchain exclusion used a string prefix instead of path-component containment"
  let packageRoot : System.FilePath := ".lake/build/proof-package-configuration"
  if ← packageRoot.pathExists then IO.FS.removeDirAll packageRoot
  IO.FS.createDirAll (packageRoot / "Source")
  IO.FS.writeFile (packageRoot / "lakefile.toml") "name = \"dependency\""
  IO.FS.writeFile (packageRoot / "Source" / "Proof.lean") "theorem proof : True := by trivial"
  let packageConfigurations ←
    ProofInput.packageConfigurationSources (packageRoot / "Source" / "Proof.lean")
  expect (packageConfigurations.any (fun path => path.fileName == some "lakefile.toml"))
    "imported source package configuration was omitted from the proof closure"
  IO.FS.removeDirAll packageRoot
  let streamedPath : System.FilePath := ".lake/build/content-digest-boundary.bin"
  let streamedInput := ByteArray.mk (Array.replicate (65536 + 17) 0x61)
  IO.FS.writeBinFile streamedPath streamedInput
  expect ((← ContentDigest.file streamedPath) == ContentDigest.bytes streamedInput)
    "content digest changed across its streamed-file chunk boundary"
  IO.FS.removeFile streamedPath
  let treeRoot : System.FilePath := ".lake/build/tree-snapshot-boundary"
  if ← treeRoot.pathExists then IO.FS.removeDirAll treeRoot
  IO.FS.createDirAll treeRoot
  IO.FS.writeFile (treeRoot / "source.txt") "current source"
  let treeSnapshot ← Snapshot.target "." s!"tree:{treeRoot}"
  IO.FS.createDirAll (treeRoot / ".lake")
  IO.FS.writeFile (treeRoot / ".lake" / "build-output") "ignored output"
  expect ((← Snapshot.target "." s!"tree:{treeRoot}") == treeSnapshot)
    "tree snapshot included its ignored build directory"
  IO.FS.createDirAll (treeRoot / "design")
  expect ((← Snapshot.target "." s!"tree:{treeRoot}") != treeSnapshot)
    "tree snapshot ignored a newly added empty source directory"
  IO.FS.removeDirAll treeRoot
  let designSourcePath : System.FilePath := ".lake/build/design-source-binding.md"
  IO.FS.writeFile designSourcePath "accepted design source"
  let (proposedSourceDesign, generatedDesign) ← proposeDesignRequest "." ProjectState.empty {
    producerAgentRun := "designer-generated"
    sourceDocumentTargets := [s!"file:{designSourcePath}"]
    statements := [statement], acceptanceCriteria := [criterion], leanClaims := [claim] }
  expect (generatedDesign.id == "design-1" && generatedDesign.sourceDocuments.length == 1)
    "Design identity or source snapshot was not derived by Workbench"
  IO.FS.writeFile designSourcePath "changed without a successor Design"
  let staleDesignRejected ← try
      let _ ← acceptDesignRequest "." proposedSourceDesign generatedDesign.id
      pure false
    catch _ => pure true
  expect staleDesignRejected "acceptance allowed a changed Design source snapshot"
  IO.FS.removeFile designSourcePath
  let missingDesignSourceRejected ← try
      let _ ← proposeDesignRequest "." ProjectState.empty {
        producerAgentRun := "designer-missing-source"
        sourceDocumentTargets := ["file:.lake/build/does-not-exist-design.md"]
        statements := [statement], acceptanceCriteria := [criterion], leanClaims := [claim] }
      pure false
    catch _ => pure true
  expect missingDesignSourceRejected
    "Design proposal converted a missing normative source into an authoritative snapshot"
  let forgedMissingSource :=
    { design with sourceDocuments := [{ target := "file:missing.md", snapshot := "missing" }] }
  match proposeDesign .empty forgedMissingSource with
  | .error _ => pure ()
  | .ok _ => throw (IO.userError "state validation accepted a forged missing Design source")
  let state ← fromExcept readyState
  let (successorState, generatedSuccessor) ← proposeDesignRequest "." state {
    producerAgentRun := "designer-successor"
    statements := [statement], acceptanceCriteria := [criterion], leanClaims := [claim] }
  expect (generatedSuccessor.createdAfterEntryOrder == 3 &&
    successorState.design? generatedSuccessor.id == some generatedSuccessor)
    "Design proposal response differed from the authoritative stored Design"

  let commandRoot : System.FilePath := ".lake/build/command-integration"
  IO.FS.createDirAll commandRoot
  let commandRoot ← IO.FS.realPath commandRoot
  IO.FS.writeFile (commandRoot / "artifact.txt") "first snapshot"
  let profileEntry : LedgerEntry := {
    id := "entry-profile", order := 4, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .commandProfile {
      purpose := "verify Lean command execution", target := some criterion.target
      command := { executable := leanExecutable, arguments := #["--version"] } } }
  let commandState ← fromExcept (appendEntry state profileEntry)
  let shown ← match resolveCommandProfile? commandRoot commandState profileEntry.id with
    | some value => pure value
    | none => throw (IO.userError "applicable Command Profile was not resolved")
  let (afterCommand, commandResult) ← runCommandProfile commandRoot commandState
    { profileEntryId := profileEntry.id, entryId := "entry-command"
      criterionId := none }
  expect (commandResult.entry.order == 5 && commandResult.stdout.startsWith "Lean")
    "resolved Command Profile did not execute"
  match commandResult.entry.payload with
  | .commandExecution execution =>
      expect (execution.command == shown.command && execution.profileEntryId == shown.profileEntryId)
        "shown and executed commands came from different resolutions"
      expect (execution.command.workingDirectory == some commandRoot.toString)
        "Command Profile did not record its resolved working directory"
      expect (execution.snapshot.map (fun value => value.startsWith "sha3-256:") == some true)
        "command execution did not bind its target snapshot"
  | _ => throw (IO.userError "command runner recorded the wrong payload")
  expect (afterCommand.ledgerEntries.length == 5) "command execution was not appended"
  let firstSnapshot ← Snapshot.target commandRoot criterion.target
  IO.FS.writeFile (commandRoot / "artifact.txt") "second snapshot"
  let secondSnapshot ← Snapshot.target commandRoot criterion.target
  expect (firstSnapshot != secondSnapshot) "artifact mutation did not change its content snapshot"
  IO.FS.writeFile (commandRoot / "review-only.txt") "before remediation"
  let reviewProfileState ← fromExcept (defineProfile afterCommand {
    entryId := "entry-review-profile", purpose := "verify a non-criterion Review target"
    target := some "file:review-only.txt"
    command := { executable := leanExecutable, arguments := #["--version"] } })
  let (reviewEvidenceState, _) ← runCommandProfile commandRoot reviewProfileState {
    profileEntryId := "entry-review-profile", entryId := "entry-review-evidence-before" }
  let freshReviewState ← startReview commandRoot reviewEvidenceState {
    entryId := "entry-review-outside-criterion", reviewId := "review-outside-criterion"
    purpose := .implementation, targetSourceId := "entry-review-evidence-before"
    reviewerAgentRun := "reviewer-outside-criterion" }
  let findingState ← fromExcept (recordFinding freshReviewState {
    entryId := "entry-review-outside-finding"
    reviewEntryId := "entry-review-outside-criterion"
    subject := { kind := .criterion, id := criterion.id, exactQuote := criterion.statement }
    mismatchEvidenceId := "entry-review-evidence-before"
    summary := "non-criterion target requires remediation" })
  let acceptedFindingState ← fromExcept (recordDisposition findingState {
    entryId := "entry-review-outside-disposition"
    findingEntryId := "entry-review-outside-finding", decision := .accepted
    reason := "the fixed target demonstrates the mismatch" })
  IO.FS.writeFile (commandRoot / "review-only.txt") "after remediation"
  let (remediatedState, _) ← runCommandProfile commandRoot acceptedFindingState {
    profileEntryId := "entry-review-profile", entryId := "entry-review-evidence-after" }
  let resumedState ← resumeReview commandRoot remediatedState {
    entryId := "entry-review-outside-resume"
    continuesEntryId := "entry-review-outside-criterion" }
  let verifiedState ← fromExcept (recordVerification resumedState {
    entryId := "entry-review-outside-verification"
    findingEntryId := "entry-review-outside-finding"
    reviewEntryId := "entry-review-outside-resume"
    evidenceEntryId := "entry-review-evidence-after" })
  let currentInputs ← evaluateCurrentInputs commandRoot verifiedState
  expect (currentInputs.observations.any (fun observation =>
    observation.target == "file:review-only.txt"))
    "Current input collection omitted a Review target outside acceptance criteria"
  let verifiedProjection ← match currentProjection? verifiedState with
    | some value => pure value
    | none => throw (IO.userError "projection disappeared after Review verification")
  let verifiedFinding ← match verifiedState.entry? "entry-review-outside-finding" with
    | some entry => match entry.payload with
      | .finding value => pure (entry, value)
      | _ => throw (IO.userError "review finding changed payload kind")
    | none => throw (IO.userError "review finding disappeared")
  expect (acceptedFindingResolved verifiedProjection currentInputs.observations
      verifiedFinding.1 verifiedFinding.2)
    "valid resumed verification left a non-criterion Review Finding unresolved"
  IO.FS.removeDirAll commandRoot

  let database := ".lake/build/sqlite-integration.db"
  if ← System.FilePath.pathExists database then IO.FS.removeFile database
  let connection ← SQLite.open database
  SQLite.runScript connection "CREATE TABLE sample(id TEXT PRIMARY KEY, value TEXT NOT NULL);"
  SQLite.transaction connection do
    SQLite.execute connection "INSERT INTO sample(id, value) VALUES (?1, ?2)"
      #["id-1", "bound value with ' quote"]
  let stored ← SQLite.queryScalar connection
    "SELECT value FROM sample WHERE id = ?1" #["id-1"]
  expect (stored == "bound value with ' quote") "SQLite prepared binding changed the value"
  let transactionRejected ← try
      SQLite.transaction connection do
        SQLite.execute connection "INSERT INTO sample(id, value) VALUES (?1, ?2)"
          #["rolled-back", "first"]
        SQLite.execute connection "INSERT INTO sample(id, value) VALUES (?1, ?2)"
          #["rolled-back", "duplicate"]
      pure false
    catch _ => pure true
  expect transactionRejected "SQLite transaction accepted a duplicate primary key"
  let rolledBack ← SQLite.queryScalar connection
    "SELECT COUNT(*) FROM sample WHERE id = ?1" #["rolled-back"]
  expect (rolledBack == "0") "SQLite transaction did not roll back atomically"

  let store ← Store.open database
  let empty ← Store.loadState store
  expect (empty == ProjectState.empty) "new store did not reconstruct the empty state"
  let candidate ← Store.proposeDesignRequest "." store {
    producerAgentRun := design.producerAgentRun, statements := design.statements
    acceptanceCriteria := design.acceptanceCriteria, leanClaims := design.leanClaims }
  let _ ← Store.acceptDesignRequest "." store candidate.id
  let _ ← Store.startWorkRequest store {
    id := work.id, outcome := work.outcome, scope := work.scope
    responsibleAgentRun := work.responsibleAgentRun
    delegatedReviewDecisions := work.delegatedReviewDecisions }
  let persistedTask ← Store.addTask store {
    entryId := "entry-persisted-task", criterionId := some criterion.id
    description := "exercise typed persistence", required := true }
  let reconstructed ← Store.loadState store
  expect (reconstructed == persistedTask && reconstructed.designRevisions.length == 1 &&
    reconstructed.works.length == 1 && reconstructed.ledgerEntries.length == 1)
    "store did not reconstruct all three persisted roots"
  let inapplicableRevision := reconstructed.revision
  let inapplicableProposalRejected ← try
      let _ ← Store.proposeDesignRequest "." store {
        producerAgentRun := "designer-successor", statements := design.statements
        acceptanceCriteria := design.acceptanceCriteria, leanClaims := design.leanClaims }
      pure false
    catch _ => pure true
  expect inapplicableProposalRejected
    "mutation store committed an operation declared inapplicable for the current state"
  expect ((← Store.loadState store).revision == inapplicableRevision)
    "rejected inapplicable operation advanced the authoritative revision"
  let persistedHandoff ← Store.handoffWork store work.id "entry-handoff" "agent-2"
    "session boundary"
  expect ((persistedHandoff.work? work.id).map (·.responsibleAgentRun) == some "agent-2")
    "store did not atomically persist Work handoff"
  IO.FS.removeFile database

end AgentWorkbenchTest.Adapter
