namespace AgentWorkbench

inductive Operation where
  | init | describe
  | designPropose | designAmend | designAccept | designReject | designGet | designInspectSources
  | designSource | designDiff | designExport
  | workStart | workGet | workFocus | workSuspend | workResume
  | workHandoff | workAdoptDesign | workAdoptionImpact | workWithdraw | workComplete
  | planPropose | planReplace | planMaterialize | planGet | planInspectSources
  | planSource | planDiff | planExport
  | taskClose
  | profileDefine | profileReplace
  | artifactObserve
  | correctionRecord | correctionSupersede | correctionResolve | correctionIncorporate
  | kptRecord | kptApply
  | reviewStart | reviewResume | reviewHandoff | reviewFinding | reviewDisposition
  | reviewConclude | reviewVerify | reviewContext | reviewInspect
  | entryGet | history | context | ready
  | commandShow | commandRun | proofDigest | proofRun
  deriving Repr, DecidableEq

inductive OperationKind where
  | mutation | query
  deriving Repr, DecidableEq

/-- Closed capability classification for every public operation. -/
def Operation.kind : Operation → OperationKind
  | .init | .designPropose | .designAmend | .designAccept | .designReject
  | .workStart | .workFocus | .workSuspend | .workResume | .workHandoff
  | .workAdoptDesign | .workWithdraw | .workComplete
  | .planPropose | .planReplace | .planMaterialize | .taskClose
  | .profileDefine | .profileReplace | .artifactObserve
  | .correctionRecord | .correctionSupersede | .correctionResolve | .correctionIncorporate
  | .kptRecord | .kptApply | .reviewStart | .reviewResume | .reviewHandoff
  | .reviewFinding | .reviewDisposition | .reviewConclude | .reviewVerify
  | .commandRun | .proofRun => .mutation
  | .describe | .designGet | .designInspectSources | .designSource | .designDiff
  | .designExport | .workGet | .workAdoptionImpact | .planGet | .planInspectSources
  | .planSource | .planDiff | .planExport | .reviewContext | .reviewInspect
  | .entryGet | .history | .context | .ready | .commandShow | .proofDigest => .query

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
  | .workWithdraw => "work withdraw" | .workComplete => "work complete"
  | .planPropose => "plan propose" | .planReplace => "plan replace"
  | .planMaterialize => "plan materialize" | .planGet => "plan get"
  | .planInspectSources => "plan inspect-sources" | .planSource => "plan source"
  | .planDiff => "plan diff" | .planExport => "plan export"
  | .taskClose => "task close"
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
      .workWithdraw, .workComplete,
      .planPropose, .planReplace, .planMaterialize, .planGet, .planInspectSources,
      .planSource, .planDiff, .planExport, .taskClose, .profileDefine,
      .profileReplace, .artifactObserve, .correctionRecord, .correctionSupersede,
      .correctionResolve, .correctionIncorporate, .kptRecord, .kptApply, .reviewStart,
      .reviewResume, .reviewHandoff, .reviewFinding, .reviewDisposition, .reviewConclude,
      .reviewVerify, .reviewContext, .reviewInspect,
      .entryGet, .history, .context, .ready, .commandShow, .commandRun, .proofDigest, .proofRun]

/-- Independent exhaustive witness for the public constructor catalog. Adding an Operation without
placing it in `all` leaves the new constructor case unprovable and stops the production build. -/
theorem Operation.mem_all (operation : Operation) : operation ∈ Operation.all := by
  cases operation <;> decide

def Operation.parse? (name : String) : Option Operation :=
  Operation.all.find? (·.name == name)

def Operation.parseCommand? (command : List String) : Option Operation :=
  Operation.parse? (String.intercalate " " command)

end AgentWorkbench
