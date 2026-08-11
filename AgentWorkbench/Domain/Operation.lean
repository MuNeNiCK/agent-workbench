import Lean.Data.Json

namespace AgentWorkbench

inductive Operation where
  | init | describe
  | designPropose | designAmend | designAccept | designReject | designGet | designInspectSources
  | designSource | designDiff | designExport
  | workStart | workGet | workFocus | workSuspend | workResume
  | workHandoff | workAdoptDesign | workAdoptionImpact | workBindRemediation
  | workWithdraw | workComplete
  | planPropose | planReplace | planMaterialize | planGet | planInspectSources
  | planSource | planDiff | planExport
  | taskClose | taskReopenStale
  | profileDefine | profileReplace
  | artifactObserve
  | correctionRecord | correctionSupersede | correctionResolve | correctionIncorporate
  | kptRecord | kptApply
  | reviewStart | reviewResume | reviewHandoff | reviewFinding | reviewDisposition
  | reviewConclude | reviewVerify | reviewContext | reviewInspect
  | entryGet | history | context | ready
  | commandShow | commandRun | proofDigest | proofRun
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

/-- Closed production-relevant effects. Status changes are deliberately directional: adding a
new status branch to an existing operation cannot hide behind generic access to the `works`
collection. Collection content outside these classified changes is an invalid effect. -/
inductive ProductionEffect where
  | stateRevisionAdvanced
  | acceptedDesignChanged
  | focusedWorkChanged
  | designInserted
  | designCandidateAccepted
  | designAcceptedSuperseded
  | designCandidateSuperseded
  | designCandidateRejected
  | workInserted
  | workActiveSuspended
  | workSuspendedActive
  | workActiveWithdrawn
  | workSuspendedWithdrawn
  | workActiveCompleted
  | workDesignChanged
  | workResponsibleChanged
  | workResumeConditionChanged
  | planInserted
  | planCandidateSuperseded
  | planCandidateCurrent
  | planCurrentSuperseded
  | ledgerAppended
  | invalidStateChange
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def ProductionEffect.authorizable : List ProductionEffect :=
  [.stateRevisionAdvanced, .acceptedDesignChanged, .focusedWorkChanged,
   .designInserted, .designCandidateAccepted, .designAcceptedSuperseded,
   .designCandidateSuperseded, .designCandidateRejected, .workInserted,
   .workActiveSuspended, .workSuspendedActive, .workActiveWithdrawn,
   .workSuspendedWithdrawn, .workActiveCompleted, .workDesignChanged,
   .workResponsibleChanged, .workResumeConditionChanged, .planInserted,
   .planCandidateSuperseded, .planCandidateCurrent, .planCurrentSuperseded,
   .ledgerAppended]

/-- One independently classifiable branch in the public production mutation surface. -/
structure ProductionEffectKey where
  operation : Operation
  effect : ProductionEffect
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

theorem ProductionEffect.mem_authorizable_or_invalid (effect : ProductionEffect) :
    effect ∈ ProductionEffect.authorizable ∨ effect = .invalidStateChange := by
  cases effect <;> simp [ProductionEffect.authorizable]


inductive OperationKind where
  | mutation | query
  deriving Repr, DecidableEq

/-- Closed capability classification for every public operation. -/
def Operation.kind : Operation → OperationKind
  | .init | .designPropose | .designAmend | .designAccept | .designReject
  | .workStart | .workFocus | .workSuspend | .workResume | .workHandoff
  | .workAdoptDesign | .workBindRemediation | .workWithdraw | .workComplete
  | .planPropose | .planReplace | .planMaterialize | .taskClose | .taskReopenStale
  | .profileDefine | .profileReplace | .artifactObserve
  | .correctionRecord | .correctionSupersede | .correctionResolve | .correctionIncorporate
  | .kptRecord | .kptApply | .reviewStart | .reviewResume | .reviewHandoff
  | .reviewFinding | .reviewDisposition | .reviewConclude | .reviewVerify
  | .commandRun | .proofRun => .mutation
  | .describe | .designGet | .designInspectSources | .designSource | .designDiff
  | .designExport | .workGet | .workAdoptionImpact | .planGet | .planInspectSources
  | .planSource | .planDiff | .planExport | .reviewContext | .reviewInspect
  | .entryGet | .history | .context | .ready | .commandShow | .proofDigest => .query

/-- Possible effects for each public operation. This table is independent from the transition
implementation. Runtime comparison checks actual before/after effects against it, so a new branch
inside an existing operation must update this closed declaration and its accepted-Design owner. -/
def Operation.permittedProductionEffects : Operation → List ProductionEffect
  | .init => [.stateRevisionAdvanced]
  | .designPropose => [.stateRevisionAdvanced, .designInserted]
  | .designAmend =>
      [.stateRevisionAdvanced, .designInserted, .designCandidateSuperseded]
  | .designAccept =>
      [.stateRevisionAdvanced, .acceptedDesignChanged, .designCandidateAccepted,
       .designAcceptedSuperseded, .workDesignChanged]
  | .designReject =>
      [.stateRevisionAdvanced, .designCandidateRejected, .ledgerAppended]
  | .workStart =>
      [.stateRevisionAdvanced, .focusedWorkChanged, .workInserted, .ledgerAppended]
  | .workFocus => [.stateRevisionAdvanced, .focusedWorkChanged]
  | .workSuspend =>
      [.stateRevisionAdvanced, .focusedWorkChanged, .workActiveSuspended,
       .workResumeConditionChanged]
  | .workResume =>
      [.stateRevisionAdvanced, .focusedWorkChanged, .workSuspendedActive,
       .workResumeConditionChanged, .ledgerAppended]
  | .workHandoff =>
      [.stateRevisionAdvanced, .workResponsibleChanged, .ledgerAppended]
  | .workAdoptDesign =>
      [.stateRevisionAdvanced, .workDesignChanged, .ledgerAppended]
  | .workBindRemediation => [.stateRevisionAdvanced, .ledgerAppended]
  | .workWithdraw =>
      [.stateRevisionAdvanced, .focusedWorkChanged, .workActiveWithdrawn,
       .workSuspendedWithdrawn, .ledgerAppended]
  | .workComplete =>
      [.stateRevisionAdvanced, .focusedWorkChanged, .workActiveCompleted,
       .workResumeConditionChanged, .ledgerAppended]
  | .planPropose =>
      [.stateRevisionAdvanced, .planInserted]
  | .planReplace =>
      [.stateRevisionAdvanced, .planInserted, .planCandidateSuperseded]
  | .planMaterialize =>
      [.stateRevisionAdvanced, .planCandidateCurrent, .planCurrentSuperseded,
       .ledgerAppended]
  | .taskClose | .taskReopenStale | .profileDefine | .profileReplace |
      .artifactObserve | .correctionRecord | .correctionSupersede |
      .correctionResolve | .correctionIncorporate | .kptRecord | .kptApply |
      .reviewStart | .reviewResume | .reviewHandoff | .reviewFinding |
      .reviewDisposition | .reviewConclude | .reviewVerify | .commandRun | .proofRun =>
      [.stateRevisionAdvanced, .ledgerAppended]
  | .describe | .designGet | .designInspectSources | .designSource | .designDiff |
      .designExport | .workGet | .workAdoptionImpact | .planGet |
      .planInspectSources | .planSource | .planDiff | .planExport | .reviewContext |
      .reviewInspect | .entryGet | .history | .context | .ready | .commandShow |
      .proofDigest => []

def Operation.name : Operation → String
  | .init => "init" | .describe => "describe"
  | .designPropose => "design propose" | .designAmend => "design amend"
  | .designAccept => "design accept" | .designReject => "design reject"
  | .designGet => "design get" | .designInspectSources => "design inspect-sources"
  | .designSource => "design source" | .designDiff => "design diff"
  | .designExport => "design export"
  | .workStart => "work start" | .workGet => "work get" | .workFocus => "work focus"
  | .workSuspend => "work suspend" | .workResume => "work resume"
  | .workHandoff => "work handoff" | .workAdoptDesign => "work adopt-design"
  | .workAdoptionImpact => "work adoption-impact"
  | .workBindRemediation => "work bind-remediation"
  | .workWithdraw => "work withdraw" | .workComplete => "work complete"
  | .planPropose => "plan propose" | .planReplace => "plan replace"
  | .planMaterialize => "plan materialize" | .planGet => "plan get"
  | .planInspectSources => "plan inspect-sources" | .planSource => "plan source"
  | .planDiff => "plan diff" | .planExport => "plan export"
  | .taskClose => "task close" | .taskReopenStale => "task reopen-stale"
  | .profileDefine => "profile define" | .profileReplace => "profile replace"
  | .artifactObserve => "artifact observe"
  | .correctionRecord => "correction record"
  | .correctionSupersede => "correction supersede"
  | .correctionResolve => "correction resolve"
  | .correctionIncorporate => "correction incorporate"
  | .kptRecord => "kpt record" | .kptApply => "kpt apply"
  | .reviewStart => "review start" | .reviewResume => "review resume"
  | .reviewHandoff => "review handoff"
  | .reviewFinding => "review finding" | .reviewDisposition => "review disposition"
  | .reviewConclude => "review conclude" | .reviewVerify => "review verify"
  | .reviewContext => "review context" | .reviewInspect => "review inspect"
  | .entryGet => "entry get" | .history => "history" | .context => "context"
  | .ready => "ready" | .commandShow => "command show" | .commandRun => "command run"
  | .proofDigest => "proof digest" | .proofRun => "proof run"

def Operation.all : List Operation :=
  [.init, .describe, .designPropose, .designAmend, .designAccept, .designReject,
      .designGet, .designInspectSources,
      .designSource, .designDiff, .designExport, .workStart, .workGet, .workFocus,
      .workSuspend, .workResume, .workHandoff, .workAdoptDesign, .workAdoptionImpact,
      .workBindRemediation, .workWithdraw, .workComplete,
      .planPropose, .planReplace, .planMaterialize, .planGet, .planInspectSources,
      .planSource, .planDiff, .planExport, .taskClose, .taskReopenStale, .profileDefine,
      .profileReplace, .artifactObserve, .correctionRecord, .correctionSupersede,
      .correctionResolve, .correctionIncorporate, .kptRecord, .kptApply, .reviewStart,
      .reviewResume, .reviewHandoff, .reviewFinding, .reviewDisposition, .reviewConclude,
      .reviewVerify, .reviewContext, .reviewInspect,
      .entryGet, .history, .context, .ready, .commandShow, .commandRun, .proofDigest, .proofRun]

/-- Derived directly from the closed public-operation and per-operation effect definitions. -/
def productionEffectUniverse : List ProductionEffectKey :=
  Operation.all.flatMap fun operation =>
    operation.permittedProductionEffects.map fun effect => { operation, effect }

/-- Independent exhaustive witness for the public constructor catalog. Adding an Operation without
placing it in `all` leaves the new constructor case unprovable and stops the production build. -/
theorem Operation.mem_all (operation : Operation) : operation ∈ Operation.all := by
  cases operation <;> decide

def Operation.parse? (name : String) : Option Operation :=
  Operation.all.find? (·.name == name)

def Operation.parseCommand? (command : List String) : Option Operation :=
  Operation.parse? (String.intercalate " " command)

end AgentWorkbench
