import AgentWorkbench.Decision.Projection

namespace AgentWorkbench

private def currentHasEntry
    (state : ProjectState) (predicate : EntryPayload → Bool) : Bool :=
  match currentProjection? state with
  | none => false
  | some projection => projection.entries.any (fun entry => predicate entry.payload)

def operationApplicable (state : ProjectState) (operation : String) : Bool :=
  let current := (currentProjection? state).isSome
  let unfocused := state.focusedWorkId.isNone
  match operation with
  | "init" | "describe" | "design get" | "work get" | "entry get" | "history" |
      "context" | "ready" | "review context" => true
  | "design propose" => unfocused
  | "design accept" => unfocused && state.designRevisions.any (fun design =>
      design.status == .candidate && design.parent == state.acceptedDesignId)
  | "work start" => unfocused && state.acceptedDesignId.isSome
  | "work focus" | "work resume" => unfocused && state.works.any (fun work =>
      (work.status == .suspended || work.status == .blocked) &&
        some work.designRevision == state.acceptedDesignId)
  | "work adopt-design" => unfocused && state.acceptedDesignId.isSome &&
      state.works.any (fun work => (work.status == .suspended || work.status == .blocked) &&
        some work.designRevision != state.acceptedDesignId)
  | "work suspend" | "work handoff" | "work complete" | "task add" |
      "profile define" | "correction record" | "kpt record" | "review start" => current
  | "task close" => current && currentHasEntry state (fun
      | .task task => !task.closed
      | _ => false)
  | "profile replace" | "command show" | "command run" =>
      current && currentHasEntry state (fun | .commandProfile _ => true | _ => false)
  | "artifact observe" => current && state.currentDesign?.any (fun design =>
      design.acceptanceCriteria.any (·.evidenceKind == "artifact"))
  | "correction supersede" | "correction resolve" | "correction incorporate" =>
      current && currentHasEntry state (fun
      | .userCorrection correction => correction.resolvedByEntryId.isNone &&
          correction.incorporatedIn.isNone
      | _ => false)
  | "kpt apply" => current && currentHasEntry state (fun
      | .kpt kpt => kpt.tryNext.isSome
      | _ => false)
  | "review resume" | "review finding" =>
      current && currentHasEntry state (fun | .review _ => true | _ => false)
  | "review disposition" => current && currentHasEntry state (fun
      | .finding _ => true
      | _ => false)
  | "review verify" => current && currentHasEntry state (fun
      | .review review => review.context == .resume
      | _ => false)
  | "proof digest" | "proof run" =>
      current && state.currentDesign?.any (fun design => !design.leanClaims.isEmpty)
  | _ => false

end AgentWorkbench
