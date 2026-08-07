import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.Guidance

open AgentWorkbench AgentWorkbenchTest

def run : IO Unit :=
  IO.FS.withTempDir fun root => do
    IO.FS.writeFile (root / "artifact.txt") "observed"
    let learned ← fromExcept <| recordKpt baseState {
      entryId := "kpt-try", problem := some "the verification path was unclear"
      tryNext := some "use the current Task-bound artifact observation" }
    expect (learned.implementationPlans == baseState.implementationPlans &&
      learned.works == baseState.works &&
      learned.ledgerEntries.filterMap (fun entry =>
        match entry.payload with | .task _ => some entry.id | _ => none) == ["task-open"])
      "KPT learning created or changed Design, Plan, Work, or Task authority"
    expectError (applyKpt learned {
      entryId := "kpt-too-early", kptEntryId := "kpt-try"
      actionEntryId := "task-open", outcome := "not actually applied" })
      "KPT Try was marked applied without a later same-bound action"
    let acted ← observeArtifact root learned {
      entryId := "evidence-after-kpt", taskEntryId := "task-open"
      criterionId := criterion.id, operation := "inspect artifact"
      result := "artifact exists", successful := true }
    let applied ← fromExcept <| applyKpt acted {
      entryId := "kpt-applied", kptEntryId := "kpt-try"
      actionEntryId := "evidence-after-kpt", outcome := "the Try was used" }
    fromExcept (validateState applied)

end AgentWorkbenchTest.Guidance
