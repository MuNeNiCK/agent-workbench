import AgentWorkbench.Domain.State
import AgentWorkbench.Domain.Lookup
import AgentWorkbench.Adapter.Snapshot
import AgentWorkbench.Adapter.ContentDigest

namespace AgentWorkbench.ReviewTarget

structure Fixed where
  sourceId : String
  target : String
  snapshot : String
  producerAgentRun : String

def fromReference
    (state : ProjectState) (purpose : ReviewPurpose) (sourceId : String) : Except String Fixed := do
  match purpose with
  | .design =>
      let design ← match state.design? sourceId with
        | some value => pure value
        | none => throw s!"no DesignRevision {sourceId}"
      pure {
        sourceId := design.id
        target := s!"design:{design.id}"
        snapshot := ContentDigest.string (Lean.toJson design).compress
        producerAgentRun := design.producerAgentRun }
  | .implementation =>
      let entry ← match state.entry? sourceId with
        | some value => pure value
        | none => throw s!"no implementation target evidence {sourceId}"
      match entry.payload with
      | .artifactObservation evidence =>
          pure {
            sourceId := entry.id
            target := evidence.target
            snapshot := evidence.snapshot
            producerAgentRun := evidence.producerAgentRun }
      | .commandExecution evidence =>
          let target ← match evidence.target with
            | some value => pure value
            | none => throw s!"command evidence {entry.id} has no target"
          let snapshot ← match evidence.snapshot with
            | some value => pure value
            | none => throw s!"command evidence {entry.id} has no snapshot"
          pure {
            sourceId := entry.id
            target := target
            snapshot := snapshot
            producerAgentRun := evidence.producerAgentRun }
      | _ => throw s!"entry {entry.id} is not implementation target evidence"

def currentSnapshot
    (projectRoot : System.FilePath) (state : ProjectState)
    (purpose : ReviewPurpose) (target : String) : IO String := do
  match purpose with
  | .design =>
      if !target.startsWith "design:" then
        throw (IO.userError "Design Review target is not a DesignRevision")
      let designId := (target.drop 7).toString
      let design ← match state.design? designId with
        | some value => pure value
        | none => throw (IO.userError s!"no DesignRevision {designId}")
      pure (ContentDigest.string (Lean.toJson design).compress)
  | .implementation => Snapshot.target projectRoot target

end AgentWorkbench.ReviewTarget
