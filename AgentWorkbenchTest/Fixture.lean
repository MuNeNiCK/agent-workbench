import AgentWorkbench

namespace AgentWorkbenchTest

open AgentWorkbench

def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw (IO.userError message)

def fromExcept : Except String α → IO α
  | .ok value => pure value
  | .error message => throw (IO.userError message)

def statement : Statement :=
  { id := "statement-1", text := "current evidence is required"
    assumptions := ["artifact observation is externally truthful"] }

def criterion : AcceptanceCriterion :=
  { id := "criterion-1", statement := "artifact check succeeds"
    target := "file:artifact.txt", evidenceKind := "artifact" }

def claim : LeanClaim :=
  { id := "claim-1"
    input :=
      { statementId := statement.id, statementText := statement.text
        mapping := "statement maps to readiness"
        proposition := "True"
        witness := "designClaim"
        proofRoot := "proof"
        declaredSources := [{ path := "Proof.lean" }]
        check := { executable := "lake", arguments := #["build"] }
        toolchain := Runtime.toolchain } }

def design : DesignRevision :=
  { id := "design-1", producerAgentRun := "designer-1", statements := [statement]
    acceptanceCriteria := [criterion], leanClaims := [claim] }

def work : Work :=
  { id := "work-1", outcome := "produce verified artifact", scope := "project"
    designRevision := design.id, status := .focused
    responsibleAgentRun := "agent-1"
    delegatedReviewDecisions := [.accepted, .rejected, .replaced] }

def readyState : Except String ProjectState := do
  let proposed ← proposeDesign .empty design
  let accepted ← acceptDesign proposed design.id
  let started ← startWork accepted work
  let withTask ← appendEntry started {
    id := "entry-task", order := 1, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .task
      { criterionId := some criterion.id, description := "build artifact"
        required := true, closed := true } }
  let withEvidence ← appendEntry withTask {
    id := "entry-evidence", order := 2, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .artifactObservation
      { criterionId := criterion.id, target := criterion.target, snapshot := "snapshot-a"
        operation := "check artifact", result := "success", successful := true
        producerAgentRun := work.responsibleAgentRun } }
  appendEntry withEvidence {
    id := "entry-proof", order := 3, scope := work.scope
    workId := some work.id, designRevision := some design.id
    payload := .leanProofReceipt
      { claimId := claim.id, claimInput := claim.input, inputDigest := "proof-input-a"
        sourceDigests := [{ path := "Proof.lean", digest := "source-a" }]
        toolchain := claim.input.toolchain, exitCode := 0
        outputDigest := "output-a", kernelAccepted := true } }

def observations : List TargetObservation :=
  [{ target := criterion.target, snapshot := "snapshot-a" }]

def digests : List CurrentClaimDigest :=
  [{
    claimId := claim.id
    claimInput := claim.input
    sourceDigests := [{ path := "Proof.lean", digest := "source-a" }]
    inputDigest := "proof-input-a" }]

end AgentWorkbenchTest
