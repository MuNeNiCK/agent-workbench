import AgentWorkbench.Adapter.SQLite
import Lean.Data.Json

namespace AgentWorkbench.Cli

open AgentWorkbench
open AgentWorkbench.Domain

private def statePath : IO System.FilePath := do
  match ← IO.getEnv "AGENT_WORKBENCH_STATE_PATH" with
  | some path => return System.FilePath.mk path
  | none => return (← IO.currentDir) / ".agent-workbench" / "state.sqlite3"

private def privateToken : IO String := do
  match ← IO.getEnv "AGENT_WORKBENCH_PRIVATE_TOKEN" with
  | some token =>
      if token.isEmpty then
        throw <| IO.userError "The Skill did not provide a private operation context."
      return token
  | none =>
      throw <| IO.userError "The Skill did not provide a private operation context."

private def sourceContext (fallback : String) : IO String :=
  return (← IO.getEnv "AGENT_WORKBENCH_SOURCE_CONTEXT").getD fallback

private def expectedRevision : IO (Option Nat) := do
  match ← IO.getEnv "AGENT_WORKBENCH_EXPECTED_REVISION" with
  | none => pure none
  | some value =>
      match value.toNat? with
      | some revision => pure (some revision)
      | none =>
          throw <| IO.userError "The Skill provided an invalid project version."

private def expectedInstance : IO (Option String) := do
  match ← IO.getEnv "AGENT_WORKBENCH_EXPECTED_INSTANCE" with
  | none => pure none
  | some value =>
      if value.isEmpty then
        throw <| IO.userError "The Skill provided an invalid project identity."
      pure (some value)

private def parseFormalResultIdentity (value : String) :
    Except String Evidence.FormalResultIdentity := do
  let json ← Lean.Json.parse value
  let key ← json.getObjValAs? String "assurance"
  let designKey ← json.getObjValAs? String "design"
  let designVersion ← json.getObjValAs? Nat "version"
  let previewIdentity ← json.getObjValAs? String "result"
  let identity : Evidence.FormalResultIdentity :=
    { key
      design := { key := designKey, version := designVersion }
      previewIdentity }
  if identity.key.isEmpty || identity.design.key.isEmpty ||
      identity.previewIdentity.isEmpty then
    throw "The stale formal-result identity is incomplete."
  pure identity

private def staleFormalResultIdentities :
    IO (List Evidence.FormalResultIdentity) := do
  match ← IO.getEnv
      "AGENT_WORKBENCH_STALE_FORMAL_RESULT_IDENTITIES_FILE" with
  | none => return []
  | some selected =>
      if selected.isEmpty then
        throw <| IO.userError
          "The Skill provided an invalid stale formal-result file."
      let value ← IO.FS.readFile (System.FilePath.mk selected)
      let encoded := (value.splitOn "\n").filter fun item => !item.isEmpty
      match encoded.mapM parseFormalResultIdentity with
      | .ok identities => return identities
      | .error reason => throw <| IO.userError reason

private def callerDecision (token reason : String) : CallerDecision :=
  let source : Source :=
    { id := ⟨token⟩
      kind := .caller
      description := "Caller instruction" }
  { source, reason }

private def source (token : String) (kind : SourceKind) : Source :=
  { id := ⟨token⟩
    kind
    description :=
      match kind with
      | .caller => "Caller statement"
      | .agent => "Agent statement"
      | .reviewer => "Reviewer statement"
      | .repository => "Repository observation"
      | .document => "Project document" }

def parseRole : String → Except String Design.Role
  | "goal" => .ok .goal
  | "functional" => .ok .functionalRequirement
  | "non-functional" => .ok .nonFunctionalRequirement
  | "constraint" => .ok .constraint
  | "decision" => .ok .decision
  | "structure" => .ok .projectStructure
  | "fact" => .ok .projectFact
  | "boundary" => .ok .trustedBoundary
  | _ => .error "Unknown design role."

def parseAssurance (key statement : String) :
    String → Except String Design.AssuranceSelection
  | "none" => .ok { kind := .none, obligations := [] }
  | "formal" =>
      .ok
        { kind := .formal
          obligations :=
            [{ key, method := .formal, description := statement }] }
  | "evidence" =>
      .ok
        { kind := .evidence
          obligations :=
            [{ key, method := .evidence, description := statement }] }
  | "mixed" =>
      let formal : Design.AssuranceObligation :=
        { key := s!"{key}-formal", method := .formal, description := statement }
      let evidence : Design.AssuranceObligation :=
        { key := s!"{key}-evidence", method := .evidence, description := statement }
      .ok { kind := .mixed, obligations := [formal, evidence] }
  | _ => .error "Assurance must be formal, evidence, mixed, or none."

def parseReviewPurpose : String → Except String Review.Purpose
  | "design" => .ok .designMeaning
  | "implementation" => .ok .implementation
  | "reuse" => .ok .reuseDecision
  | _ => .error "Review purpose must be design, implementation, or reuse."

def parseReviewDecision : String → Except String Review.Decision
  | "accepted" => .ok .accepted
  | "rejected" => .ok .rejected
  | "rescoped" => .ok .rescoped
  | "deferred" => .ok .deferred
  | "needs-evidence" => .ok .needsEvidence
  | _ => .error "Unknown review decision."

def parseCommandDisposition : String →
    Except String CommandProfile.Disposition
  | "required" => .ok .required
  | "recommended" => .ok .recommended
  | "discouraged" => .ok .discouraged
  | _ =>
      .error "Command Profile disposition must be required, recommended, or discouraged."

def parseKPTCategory : String → Except String KPT.Category
  | "keep" => .ok .keep
  | "problem" => .ok .problem
  | "try" => .ok .try
  | _ => .error "KPT category must be keep, problem, or try."

def parseKPTRelation (kind key member observedValue passedName : String) :
    Except String (Option KPT.RelationSelector) :=
  match kind with
  | "-" =>
      if key == "-" && member == "-" && observedValue == "-" &&
          passedName == "-" then
        .ok none
      else
        .error "An absent KPT relation requires five '-' fields."
  | "command-profile" =>
      if key.isEmpty || key == "-" || observedValue != "-" ||
          passedName != "-" then
        .error "A Command Profile relation requires its key and scope."
      else match member with
        | "project" => .ok (some (.commandProfile key .project))
        | "work" => .ok (some (.commandProfile key .focusedWork))
        | _ =>
            .error
              "A Command Profile relation scope must be project or work."
  | "design" =>
      if key.isEmpty || key == "-" || observedValue != "-" ||
          passedName != "-" then
        .error "A Design relation requires its key and authority."
      else match member with
        | "accepted" => .ok (some (.design key .accepted))
        | "candidate" => .ok (some (.design key .candidate))
        | _ =>
            .error "A Design relation authority must be accepted or candidate."
  | "task" =>
      if key.isEmpty || key == "-" || member != "work" ||
          observedValue != "-" || passedName != "-" then
        .error "A Task relation requires its description and work."
      else
        .ok (some (.task key))
  | "review-observation" =>
      if key.isEmpty || key == "-" || member.isEmpty || member == "-" ||
          observedValue != "-" || passedName != "-" then
        .error "A Review relation requires review and observation keys."
      else
        .ok (some (.reviewObservation key member))
  | "evidence-result" => do
      if key.isEmpty || key == "-" || member.isEmpty || member == "-" ||
          observedValue.isEmpty then
        .error
          "An Evidence relation requires its key, basis, and observed value."
      else
        let passed ← match passedName with
          | "pass" => .ok true
          | "fail" => .ok false
          | _ => .error "An Evidence relation result must be pass or fail."
        if member == "work" then
          .ok (some (.evidenceResult key .focusedWork observedValue passed))
        else if member.startsWith "design:" then
          let designKey := (member.drop 7).toString
          if designKey.isEmpty then
            .error "An Evidence Design basis requires a Design key."
          else
            .ok
              (some
                (.evidenceResult key (.design designKey) observedValue passed))
        else
          .error
            "An Evidence relation basis must be work or design:<Design-key>."
  | _ =>
      .error
        "KPT relation kind must be command-profile, design, task, review-observation, evidence-result, or '-'."

private def parseMemoryScope (state : Kernel.State) :
    String → Except String MemoryScope
  | "project" => .ok .project
  | "work" => .ok (.work state.focus.work.key)
  | _ => .error "Scope must be project or work."

private def initialState (token outcome firstTask : String) :
    Except String Kernel.State := do
  if outcome.isEmpty || firstTask.isEmpty then
    throw "An outcome and its first task are required."
  let authority := callerDecision token "Start this outcome."
  let workRef : WorkRef := { key := "current-outcome", version := 0 }
  let taskRef : TaskRef := { key := "task-1", version := 0 }
  let task : Work.Task :=
    { ref := taskRef
      work := workRef
      description := firstTask
      basis := .workBoundary workRef
      designScope := []
      phase := none
      state := .pending }
  let work : Work.Unit :=
    { ref := workRef
      outcome
      completionBoundary :=
        [{ target := .taskSatisfied taskRef
           basis := .workBoundary workRef }]
      authority }
  let state : Kernel.State :=
    { design := { effects := [] }
      work := [work]
      tasks := [task]
      phases := []
      evidenceSpecs := []
      evidenceResults := []
      formalSpecs := []
      formalResults := []
      reviewRequests := []
      reviewResults := []
      reviewDispositions := []
      commandProfiles := []
      commandDeviations := []
      kpt := []
      focus :=
        { work := workRef
          task := some taskRef
          returnPoint := none } }
  if state.wellFormed then
    return state
  throw "The initial project state is invalid."

private def currentWork? (state : Kernel.State) : Option Work.Unit :=
  state.work.find? (·.ref == state.focus.work)

private def taskDescription (state : Kernel.State) (task : TaskRef) : String :=
  (state.tasks.find? (·.ref == task)).map (·.description) |>.getD "the selected task"

private def describeTaskRequirement (state : Kernel.State)
    (taskRef : TaskRef) : String :=
  match state.tasks.find? (·.ref == taskRef) with
  | none => "satisfy the selected task"
  | some task =>
      if Kernel.taskCurrent state task then
        task.description
      else
        let current := Kernel.currentDesignItems state
        let stale := task.designScope.find? fun accepted =>
          !(current.map (·.ref)).contains accepted.ref
        match stale with
        | none => s!"replace the outdated task: {task.description}"
        | some accepted =>
            if current.any (·.ref.key == accepted.ref.key) then
              s!"add a replacement task for: {task.description}"
            else
              s!"update and accept the design required by: {task.description}"

private def assuranceDesignKey? (basis : Work.DerivationBasis) : Option String :=
  match basis with
  | .design [accepted] => some accepted.ref.key
  | _ => none

private def describeCommandProfile (state : Kernel.State)
    (selected : CommandProfileRef) : String :=
  match state.commandProfiles.find? (·.ref == selected) with
  | none => s!"Command Profile {selected.key}@{selected.version}"
  | some profile =>
      let cwd := profile.cwd.map (fun selected =>
        s!" from {(Lean.toJson selected).compress}")
        |>.getD ""
      s!"Command Profile {selected.key}@{selected.version} ({(Lean.toJson profile.argv).compress}{cwd})"

def describeMember (state : Kernel.State)
    (member : Work.CompletionMember) : String :=
  match member.target with
  | .taskSatisfied task => describeTaskRequirement state task
  | .assurance key =>
      match Kernel.selectedAssuranceForBasis? state key member.basis,
          assuranceDesignKey? member.basis with
      | some assurance, some designKey =>
          match assurance.method with
          | .formal =>
              s!"run formal-check {key} {designKey} for: {assurance.description}"
          | .evidence =>
              let selectedProfile :=
                (state.evidenceSpecs.reverse.find? fun spec =>
                  spec.ref.key == key &&
                    spec.basis == member.basis &&
                    Kernel.evidenceSpecCurrent state spec)
                  |>.bind (·.commandProfile)
              match selectedProfile with
              | some profile =>
                  s!"record-evidence {key} ... {designKey} using exact {describeCommandProfile state profile} for: {assurance.description}"
              | none =>
                  s!"run add-evidence {key} ... {designKey}, then record-evidence {key} ... {designKey} for: {assurance.description}"
      | _, _ => "satisfy the selected exact assurance"
  | .reviewResolved _ => "resolve the selected review observations"
  | .externalObservation evidence =>
      match state.evidenceSpecs.find? (·.ref == evidence) with
      | none => "record the selected external observation"
      | some spec =>
          match spec.commandProfile with
          | none => spec.observation
          | some profile =>
              s!"{spec.observation} using exact {describeCommandProfile state profile}"

def roleName : Design.Role → String
  | .goal => "Goal"
  | .functionalRequirement => "Functional requirement"
  | .nonFunctionalRequirement => "Non-functional requirement"
  | .constraint => "Constraint"
  | .decision => "Decision"
  | .projectStructure => "Project structure"
  | .projectFact => "Project fact"
  | .trustedBoundary => "Trusted boundary"

def reviewDecisionName : Review.Decision → String
  | .accepted => "accepted"
  | .rejected => "rejected"
  | .rescoped => "rescoped"
  | .deferred => "deferred"
  | .needsEvidence => "needs evidence"

def reviewPurposeName : Review.Purpose → String
  | .designMeaning => "design meaning"
  | .implementation => "implementation"
  | .reuseDecision => "reuse decision"

def commandDispositionName : CommandProfile.Disposition → String
  | .required => "required"
  | .recommended => "recommended"
  | .discouraged => "discouraged"

def kptCategoryName : KPT.Category → String
  | .keep => "Keep"
  | .problem => "Problem"
  | .try => "Try"

def memoryScopeName (state : Kernel.State) : MemoryScope → String
  | .project => "project"
  | .work key =>
      let outcome :=
        state.work.find? (·.ref.key == key) |>.map (·.outcome)
          |>.getD "unavailable Work"
      s!"Work: {outcome}"

private def sourceKindName : SourceKind → String
  | .caller => "caller"
  | .agent => "agent"
  | .reviewer => "reviewer"
  | .repository => "repository"
  | .document => "document"

private def kptRelationName (state : Kernel.State) : KPT.Relation → String
  | .commandProfile ref =>
      let purpose :=
        state.commandProfiles.find? (·.ref == ref) |>.map (·.purpose)
          |>.getD "unavailable purpose"
      s!"Command Profile {ref.key}@{ref.version}: {purpose}"
  | .design ref =>
      let statement :=
        state.design.designItems.find? (·.ref == ref) |>.map (·.statement)
          |>.getD "unavailable Design"
      s!"Design {ref.key}@{ref.version}: {statement}"
  | .task ref =>
      let description :=
        state.tasks.find? (·.ref == ref) |>.map (·.description)
          |>.getD "unavailable Task"
      s!"Task {description} [{ref.key}@{ref.version}]"
  | .reviewObservation ref =>
      let summary :=
        state.reviewResults.findSome? fun result =>
          if result.review == ref.review then
            result.observations.find? (·.key == ref.observation) |>.map (·.summary)
          else
            none
        |>.getD "unavailable Review observation"
      s!"Review {ref.review.key}@{ref.review.version} observation {ref.observation}: {summary}"
  | .evidenceResult ref =>
      let outcome := if ref.passed then "passed" else "failed"
      s!"Evidence {ref.evidence.key}@{ref.evidence.version} {outcome}: {ref.observedValue}"

private def printKPTEntry (state : Kernel.State) (entry : KPT.Entry) :
    IO Unit := do
  IO.println s!"  - [{kptCategoryName entry.category}:{entry.ref.key}@{entry.ref.version}] {entry.statement}"
  IO.println s!"    Scope: {memoryScopeName state entry.scope}"
  IO.println s!"    Author: {entry.author}"
  IO.println s!"    Source: {sourceKindName entry.source.kind} ({entry.source.description})"
  IO.println s!"    Relation: {entry.relation.map (kptRelationName state) |>.getD "-"}"
  IO.println s!"    Predecessor: {entry.predecessor.map (fun prior => s!"{prior.key}@{prior.version}") |>.getD "-"}"
  match entry.authority with
  | .nonAuthoritative => IO.println "    Authority: non-authoritative"
  | .callerOwned decision =>
      IO.println s!"    Authority: caller-owned ({decision.reason})"

private def printKPTHistory (state : Kernel.State) (key : String)
    (scope : MemoryScope) : IO Unit := do
  let entries := state.kpt.filter fun entry =>
    entry.ref.key == key && entry.scope == scope
  if entries.isEmpty then
    throw <| IO.userError "No KPT history matches that key and scope."
  IO.println s!"KPT history [kpt:{key}] for {memoryScopeName state scope}:"
  for entry in entries do
    printKPTEntry state entry

private def printReturnAssumption (state : Kernel.State)
    (assumption : Work.ReturnAssumption) : IO Unit :=
  match assumption with
  | .workBoundary work =>
      let outcome :=
        state.work.find? (·.ref == work) |>.map (·.outcome)
          |>.getD "saved outcome"
      IO.println s!"  - Outcome boundary: {outcome}"
  | .design item =>
      let statement :=
        state.design.designItems.find? (·.ref == item) |>.map (·.statement)
          |>.getD "saved design statement"
      IO.println s!"  - Design: {statement}"

private def printFormalMeaning (result : Evidence.FormalResult)
    (stale : List Evidence.FormalResultIdentity) : IO Unit := do
  if stale.contains result.identity then
    IO.println s!"    Stale formal meaning (run formal-check {result.spec.key} {result.spec.design.key}):"
  else
    IO.println "    Verified formal meaning:"
  IO.println result.semanticPreview
  IO.println s!"    Preview identity: {result.previewIdentity}"
  IO.println s!"    Checked closure: {String.intercalate ", " result.checkedClosure}"
  IO.println s!"    Checked artifacts: {String.intercalate ", " result.checkedArtifacts}"

private def printStatusWithStale (state : Kernel.State)
    (stale : List Evidence.FormalResultIdentity) : IO Unit := do
  match currentWork? state with
  | none =>
      IO.println "The current outcome is unavailable."
  | some work =>
      IO.println s!"Outcome: {work.outcome}"
      match state.focus.task with
      | none => pure ()
      | some task => IO.println s!"Selected task: {taskDescription state task}"
      let currentDesign := Kernel.currentDesignItems state
      unless currentDesign.isEmpty do
        IO.println "Accepted design:"
        for item in currentDesign do
          IO.println s!"  - [design:{item.ref.key}] {roleName item.role}: {item.statement}"
          match item.authority with
          | .unaccepted => pure ()
          | .acceptedByCaller decision =>
              IO.println s!"    Accepted because: {decision.reason}"
          | .retiredByCaller _ => pure ()
          match item.complexityRationale with
          | none => pure ()
          | some rationale =>
              IO.println s!"    Necessary because: {rationale.necessity}"
              IO.println s!"    Simpler option insufficient: {rationale.simplerAlternativeInsufficient}"
              IO.println s!"    Bounded scope: {rationale.boundedScope}"
              IO.println s!"    Maintenance cost: {rationale.maintenanceCost}"
          for spec in state.formalSpecs do
            if spec.design == item.ref then
              match Kernel.latestFormalResultForSpec? state spec with
              | some result => printFormalMeaning result stale
              | none => pure ()
      let proposedDesign := state.design.designItems.filter fun item =>
        item.authority == .unaccepted &&
          !(state.design.designItems.any fun successor =>
            successor.ref.key == item.ref.key &&
              successor.ref.version > item.ref.version)
      unless proposedDesign.isEmpty do
        IO.println "Proposed design awaiting review and caller decision:"
        for item in proposedDesign do
          IO.println s!"  - [design:{item.ref.key}] {roleName item.role}: {item.statement}"
          for obligation in item.assurance.obligations do
            let latest :=
              (state.formalSpecs.find? fun spec =>
                spec.key == obligation.key && spec.design == item.ref)
                |>.bind (Kernel.latestFormalResultForSpec? state)
            let result := latest.map (fun formal =>
              if stale.contains formal.identity then
                "preview pending"
              else
                "preview verified") |>.getD "preview pending"
            IO.println s!"    [assurance:{obligation.key}] {obligation.description} ({result})"
          for spec in state.formalSpecs do
            if spec.design == item.ref then
              match Kernel.latestFormalResultForSpec? state spec with
              | some result => printFormalMeaning result stale
              | none => pure ()
      let retiredDesign := state.design.designItems.filter fun item =>
        (match item.authority with
        | .retiredByCaller _ => true
        | _ => false) &&
          !(state.design.designItems.any fun successor =>
            successor.ref.key == item.ref.key &&
              successor.ref.version > item.ref.version)
      unless retiredDesign.isEmpty do
        IO.println "Caller-retired design:"
        for item in retiredDesign do
          match item.authority with
          | .retiredByCaller decision =>
              IO.println s!"  - [design:{item.ref.key}] {item.statement}"
              IO.println s!"    Retired because: {decision.reason}"
          | _ => pure ()
      let affected := Kernel.affectedDesigns state
      unless affected.isEmpty do
        IO.println "Design correction impact:"
        for item in affected do
          let path := item.path.map (·.key) |>.eraseDups
          IO.println s!"  - [design:{item.key}] via {String.intercalate " -> " path}"
      let instructions := state.design.instructions
      unless instructions.isEmpty do
        IO.println "Binding project instructions:"
        for instruction in instructions do
          IO.println s!"  - {instruction.statement}"
      let profiles := state.commandProfiles.filter
        (Kernel.commandProfileApplicable state)
      unless profiles.isEmpty do
        IO.println "Accepted Command Profiles:"
        for profile in profiles do
          IO.println s!"  - [command-profile:{profile.ref.key}] {profile.purpose}"
          IO.println s!"    Scope: {memoryScopeName state profile.scope}"
          IO.println s!"    Disposition: {commandDispositionName profile.disposition}"
          IO.println s!"    argv: {(Lean.toJson profile.argv).compress}"
          IO.println s!"    cwd: {profile.cwd.map (fun selected => (Lean.toJson selected).compress) |>.getD "-"}"
      let proposedProfiles := Kernel.pendingCommandProfileProposals state
      unless proposedProfiles.isEmpty do
        IO.println "Proposed Command Profiles awaiting caller decision:"
        for profile in proposedProfiles do
          IO.println s!"  - [command-profile:{profile.ref.key}] {profile.purpose} ({memoryScopeName state profile.scope})"
      let deviations := state.commandDeviations.filter fun deviation =>
        state.commandProfiles.find? (·.ref == deviation.profile)
          |>.any fun profile => Kernel.memoryScopeApplicable state profile.scope
      unless deviations.isEmpty do
        IO.println "Recorded recommended Command Profile deviations:"
        for deviation in deviations do
          IO.println s!"  - [command-profile:{deviation.profile.key}@{deviation.profile.version}] actual argv: {(Lean.toJson deviation.actualArgv).compress}"
          IO.println s!"    actual cwd: {deviation.actualCwd.map (fun selected => (Lean.toJson selected).compress) |>.getD "-"}"
          IO.println s!"    Reason: {deviation.reason}"
          IO.println s!"    Source: {sourceKindName deviation.source.kind} ({deviation.source.description})"
          IO.println s!"    Evidence: {deviation.evidence.map (fun selected => s!"{selected.key}@{selected.version}") |>.getD "-"}"
      let kpt := Kernel.relevantKPT state
      unless kpt.isEmpty do
        IO.println "KPT project memory:"
        for entry in kpt do
          printKPTEntry state entry
      let proposedKPT := Kernel.pendingKPTCandidates state
      unless proposedKPT.isEmpty do
        IO.println "Agent-authored KPT candidates:"
        for entry in proposedKPT do
          printKPTEntry state entry
      let context := state.design.effects.filterMap fun effect =>
        match effect.content with
        | .nonAuthoritative record => some record
        | _ => none
      unless context.isEmpty do
        IO.println "Recorded proposals, questions, and decisions:"
        for record in context do
          let label :=
            match record.kind with
            | .proposal => "Proposal"
            | .question => "Question"
            | .context => "Context"
            | .rejection => "Rejected proposal"
          IO.println s!"  - {label}: {record.statement}"
      let currentWork := state.work.filter fun candidate =>
        Kernel.workCurrent state candidate.ref
      unless currentWork.isEmpty do
        IO.println "Current outcomes and tasks:"
        for candidate in currentWork do
          IO.println s!"  - {candidate.outcome} ({candidate.authority.reason})"
          for task in state.tasks do
            if task.work.key == candidate.ref.key && Kernel.taskCurrent state task then
              let marker := if state.focus.task == some task.ref then "*" else "-"
              let taskState :=
                if task.state == .satisfied then "satisfied" else "pending"
              IO.println s!"    {marker} {task.description} ({taskState})"
              match task.phase with
              | none => pure ()
              | some phaseKey =>
                  let phaseName :=
                    state.phases.find? (·.key == phaseKey)
                      |>.map (·.name)
                      |>.getD "selected Phase"
                  let displayOrder :=
                    state.phases.find? (·.key == phaseKey)
                      |>.map (·.displayOrder)
                      |>.getD 0
                  IO.println s!"      Phase: {phaseName} (display order {displayOrder})"
              for accepted in task.designScope do
                let statement :=
                  state.design.designItems.find? (·.ref == accepted.ref)
                    |>.map (·.statement)
                    |>.getD "selected design statement"
                IO.println s!"      Design [design:{accepted.ref.key}]: {statement}"
      let assurances := work.completionBoundary.filterMap fun member =>
        match member.target with
        | .assurance key =>
            Kernel.selectedAssuranceForBasis? state key member.basis
        | _ => none
      unless assurances.isEmpty do
        IO.println "Required assurance:"
        for assurance in assurances do
          let result :=
            if Kernel.assuranceSatisfiedForBasis state assurance.key
                assurance.basis stale then
              "satisfied"
            else
              "pending"
          let design :=
            (assuranceDesignKey? assurance.basis)
              |>.map (fun key => s!" [design:{key}]")
              |>.getD ""
          IO.println s!"  - [assurance:{assurance.key}]{design} {assurance.description} ({result})"
      let evidenceSpecs := state.evidenceSpecs.filter
        (Kernel.evidenceSpecCurrent state)
      unless evidenceSpecs.isEmpty do
        IO.println "Current external evidence:"
        for spec in evidenceSpecs do
          let observed :=
            state.evidenceResults.reverse.find? fun result =>
              result.spec == spec && Kernel.evidenceResultCurrent state result
          let result :=
            match observed with
            | some observation =>
                if observation.passed then
                  s!"passed: {observation.observedValue}"
                else
                  s!"failed: {observation.observedValue}"
            | none => "pending"
          IO.println s!"  - [evidence:{spec.ref.key}] {spec.observation} ({result})"
          IO.println s!"    Method: {spec.method} in {spec.environment}"
          unless spec.inputs.isEmpty do
            IO.println s!"    Inputs: {String.intercalate ", " spec.inputs}"
          IO.println s!"    Acceptance: {spec.acceptanceCondition}"
          IO.println s!"    Trusted boundary: {spec.trustedBoundary}"
          IO.println s!"    Artifact: {spec.artifactIdentity}"
          match spec.commandProfile with
          | none => pure ()
          | some selected =>
              IO.println s!"    Command Profile: {selected.key}@{selected.version}"
              match spec.commandProfileDecision with
              | none => pure ()
              | some decision =>
                  IO.println s!"    Profile selection: caller-owned ({decision.reason})"
      let reviews := state.reviewRequests.filter (Kernel.reviewRequestCurrent state)
      unless reviews.isEmpty do
        IO.println "Current reviews:"
        for review in reviews do
          let result :=
            if Kernel.reviewResolved state review.ref then "resolved" else "pending"
          IO.println s!"  - [review:{review.ref.key}] {String.intercalate ", " review.scope.artifacts} ({result})"
          IO.println s!"    Purpose: {reviewPurposeName review.scope.purpose}"
          for designRef in review.scope.design do
            let statement :=
              state.design.designItems.find? (·.ref == designRef)
                |>.map (·.statement)
                |>.getD "selected design statement"
            IO.println s!"    Design [design:{designRef.key}]: {statement}"
          match review.scope.task with
          | none => pure ()
          | some task =>
              IO.println s!"    Task: {taskDescription state task}"
          match state.reviewResults.find? (·.review == review.ref) with
          | none => pure ()
          | some reviewResult =>
              for observation in reviewResult.observations do
                let decision :=
                  Review.latestDisposition? review.ref observation.key
                    state.reviewDispositions
                    |>.map (fun disposition =>
                      reviewDecisionName disposition.decision)
                    |>.getD "awaiting caller decision"
                IO.println s!"    - [observation:{observation.key}] {observation.summary} ({decision})"
      match state.focus.returnPoint with
      | none => pure ()
      | some point =>
          let savedOutcome :=
            state.work.find? (·.ref == point.work) |>.map (·.outcome)
              |>.getD "saved outcome"
          IO.println s!"Saved return outcome: {savedOutcome}"
          match point.task with
          | none => pure ()
          | some task => IO.println s!"Saved return task: {taskDescription state task}"
          IO.println "Saved return assumptions:"
          for assumption in point.assumptions do
            printReturnAssumption state assumption
      match (Kernel.missingCompletion state work.ref stale).head? with
      | none => IO.println "Completion: satisfied"
      | some member =>
          IO.println s!"Next required result: {describeMember state member}"

private def printNextWithStale (recorded : Kernel.State)
    (stale : List Evidence.FormalResultIdentity) : IO Unit := do
  match Kernel.nextAction recorded stale with
  | none => IO.println "The current outcome is complete."
  | some (.satisfy member) =>
      IO.println s!"Next: {describeMember recorded member}"
  | some .returnToSavedWork =>
      IO.println "Next: return to the saved outcome."
  | some (.replanReturn changed) => do
      IO.println "Next: choose the current outcome to resume with replan-return."
      IO.println "Changed saved assumptions:"
      for assumption in changed do
        printReturnAssumption recorded assumption
  | some (.cannotAdvance reason) =>
      IO.println s!"Cannot advance: {reason}"

private def load (path : System.FilePath) : IO Kernel.State := do
  match ← Adapter.SQLite.inspect path with
  | .ok snapshot => return snapshot.state
  | .error .uninitialized =>
      throw <| IO.userError "Agent Workbench is not initialized in this project."
  | .error (.corrupt reason) =>
      throw <| IO.userError reason

def mutationIntent (arguments : List String) : String :=
  (Lean.toJson arguments).compress

private def applyMutation (path : System.FilePath) (arguments : List String)
    (transition : Kernel.State → Except String Kernel.State) : IO Kernel.State := do
  let token ← privateToken
  let intent := mutationIntent arguments
  match ← Adapter.SQLite.mutate path token intent
      (← expectedInstance) (← expectedRevision) transition with
  | .ok snapshot => return snapshot.state
  | .error (.openError .uninitialized) =>
      throw <| IO.userError "Agent Workbench is not initialized in this project."
  | .error (.openError (.corrupt reason)) =>
      throw <| IO.userError reason
  | .error .intentConflict =>
      throw <| IO.userError "The pending operation belongs to a different intention."
  | .error .stale =>
      IO.eprintln "The project changed before this action was applied."
      match ← Adapter.SQLite.inspect path with
      | .ok snapshot =>
          let stale ← staleFormalResultIdentities
          printStatusWithStale snapshot.state stale
          printNextWithStale snapshot.state stale
      | .error .uninitialized =>
          IO.eprintln "Agent Workbench is not initialized in this project."
      | .error (.corrupt reason) => IO.eprintln reason
      IO.Process.exit 74
  | .error .wait =>
      IO.eprintln "Project memory is busy; wait and retry this action."
      IO.Process.exit 76
  | .error (.rejected reason) =>
      throw <| IO.userError reason
  | .error .uncertain =>
      IO.eprintln
        "The result is uncertain; run the same project action again through the Skill."
      IO.Process.exit 75

def parsePassed : String → Except String Bool
  | "pass" => .ok true
  | "fail" => .ok false
  | _ => .error "Evidence result must be 'pass' or 'fail'."

def commaSeparated (value : String) : List String :=
  if value == "-" then []
  else (value.splitOn ",").filter fun item => !item.isEmpty

private def printHelp : IO Unit :=
  IO.println
    "Agent Workbench project actions:
  init, status, next, start-work, switch-work, add-task, add-task-for-design, finish-task
  record-design, propose-design, request-design-review, accept-design
  accept-design-with-kpt, retire-design
  accept-complex-design, record-instruction
  record-question, reject-proposal, record-source-effects, add-evidence, record-evidence
  record-command-profile, propose-command-profile, accept-command-profile
  record-command-deviation, record-kpt, propose-kpt, accept-kpt
  record-kpt-command-profile, record-kpt-instruction, record-kpt-design
  kpt-history
  preview-formal, formal-check, request-review, record-review
  record-clean-review, resolve-review
  adopt-review-proposal, adopt-complex-review-proposal, correct-review
  interrupt, return, replan-return, complete
  assign-phase, rename-phase, order-phase"

private def readJson (path : System.FilePath) : IO Lean.Json := do
  let content ← IO.FS.readFile path
  match Lean.Json.parse content.trimAscii.toString with
  | .ok json => pure json
  | .error reason => throw <| IO.userError reason

private def readBoundedFormalFile (path : System.FilePath)
    (description : String) : IO String := do
  let content ← IO.FS.readFile path
  if content.toUTF8.size > 1048576 then
    throw <| IO.userError s!"The {description} exceeds 1048576 bytes."
  pure content

private def readBoundedFormalLines (path : System.FilePath)
    (description : String) : IO (List String) := do
  let content ← readBoundedFormalFile path description
  pure <| (content.splitOn "\n").filter fun item => !item.isEmpty

def formalResultMutationIntentArguments
    (key designKey designVersion tool oracle conformance previewIdentity : String)
    (checkedClosure checkedArtifacts : List String)
    (semanticPreview : String) : List String :=
  ["record-formal-result-files", key, designKey, designVersion, tool, oracle,
   String.intercalate "\n" checkedClosure,
   String.intercalate "\n" checkedArtifacts,
   conformance, semanticPreview, previewIdentity]

private def currentFormalSpec (state : Kernel.State) (key : String)
    (designKey : Option String := none) (preview : Bool := false) :
    Except String Evidence.FormalSpec :=
  let candidates :=
    match designKey, preview with
    | none, false =>
        let completion := Kernel.selectedFormalSpecsForCompletion state key
        if !completion.isEmpty then completion
        else Kernel.selectedFormalSpecs state key
    | none, true => Kernel.selectedFormalSpecs state key
    | some selected, true =>
        Kernel.selectedFormalSpecsForPreview state key selected
    | some selected, false =>
        Kernel.selectedFormalSpecsForDesign state key selected
  match candidates with
  | [spec] => .ok spec
  | [] => .error "No current formal selection has that assurance key."
  | _ =>
      .error
        "The current formal selection is ambiguous; select its Design key."

private def printFormalPlanField (state : Kernel.State) (key field : String)
    (designKey : Option String := none) (preview : Bool := false) :
    IO Unit := do
  let spec ← match currentFormalSpec state key designKey preview with
    | .ok spec => pure spec
    | .error reason => throw <| IO.userError reason
  match field with
  | "oracle" => IO.println (spec.oracle.getD "-")
  | "modules" => IO.println (String.intercalate "," spec.modules)
  | "surfaces" =>
      if spec.implementationSurfaces.isEmpty then IO.println "-"
      else IO.println (String.intercalate "," spec.implementationSurfaces)
  | "adapter" => IO.println (spec.adapter.getD "-")
  | "cases" =>
      if spec.cases.isEmpty then IO.println "-"
      else IO.println (String.intercalate "," spec.cases)
  | "statement" =>
      match state.design.designItems.find? (·.ref == spec.design) with
      | some item => IO.println item.statement
      | none => throw <| IO.userError "The selected design statement is unavailable."
  | "design-key" => IO.println spec.design.key
  | "design-version" => IO.println spec.design.version
  | _ => throw <| IO.userError "Unknown formal plan field."

private def formalResultIdentityJson
    (identity : Evidence.FormalResultIdentity) : String :=
  (Lean.Json.mkObj
    [("assurance", .str identity.key),
     ("design", .str identity.design.key),
     ("version", Lean.toJson identity.design.version),
     ("result", .str identity.previewIdentity)]).compress

private def printCurrentFormalArtifacts (state : Kernel.State) : IO Unit := do
  for result in Kernel.formalResultsRequiringVerification state do
    let identity := formalResultIdentityJson result.identity
    for artifact in result.checkedArtifacts do
      IO.println s!"{identity}\t{artifact}"

def run (arguments : List String) : IO Unit := do
  let path ← statePath
  -- Validate every projection input before any mutation can commit. All
  -- post-mutation rendering below reuses this immutable observation.
  let stale ← staleFormalResultIdentities
  let printStatus := fun state => printStatusWithStale state stale
  let printNext := fun state => printNextWithStale state stale
  match arguments with
  | ["--version"] =>
      IO.println "agent-workbench 0.2.3"
  | ["--help"] | ["-h"] =>
      printHelp
  | ["compare-json-files", expected, actual] =>
      let expectedJson ← readJson (System.FilePath.mk expected)
      let actualJson ← readJson (System.FilePath.mk actual)
      if expectedJson.compress == actualJson.compress then
        IO.println "match"
      else
        throw <| IO.userError "The product observation differs from the Lean oracle."
  | ["validate-json-file", selected] =>
      let _ ← readJson (System.FilePath.mk selected)
      IO.println "valid"
  | ["formal-plan", key, "completion", field] =>
      printFormalPlanField (← load path) key field
  | ["formal-plan", key, designKey, "completion", field] =>
      printFormalPlanField (← load path) key field (some designKey)
  | ["formal-plan", key, designKey, "preview", field] =>
      printFormalPlanField (← load path) key field (some designKey) true
  | ["formal-artifacts"] =>
      printCurrentFormalArtifacts (← load path)
  | ["remaining-stale-formal-identities", key, designKey, version] =>
      let designVersion ← match version.toNat? with
        | some selected => pure selected
        | none =>
            throw <| IO.userError
              "The selected formal Design version is invalid."
      for identity in ← staleFormalResultIdentities do
        if identity.key != key ||
            identity.design != ({ key := designKey, version := designVersion } :
              DesignRef) then
          IO.println (formalResultIdentityJson identity)
  | ["state-revision"] =>
      IO.println (← Adapter.SQLite.inspect path >>= fun
        | .ok snapshot => pure snapshot.revision
        | .error .uninitialized =>
            throw <| IO.userError
              "Agent Workbench is not initialized in this project."
        | .error (.corrupt reason) => throw <| IO.userError reason)
  | ["state-context"] =>
      match ← Adapter.SQLite.inspect path with
      | .ok snapshot => IO.println s!"{snapshot.revision}\t{snapshot.storeId}"
      | .error .uninitialized =>
          throw <| IO.userError
            "Agent Workbench is not initialized in this project."
      | .error (.corrupt reason) => throw <| IO.userError reason
  | ["init", outcome, firstTask] =>
      let token ← privateToken
      let state ← match initialState token outcome firstTask with
        | .ok state => pure state
        | .error reason => throw <| IO.userError reason
      let intent := mutationIntent arguments
      match ← Adapter.SQLite.initializeStore path token intent state with
      | .ok _ => printStatus state
      | .error .uninitialized =>
          throw <| IO.userError "Agent Workbench could not initialize this project."
      | .error (.corrupt reason) =>
          throw <| IO.userError reason
  | ["status"] =>
      printStatus (← load path)
  | ["next"] =>
      printNext (← load path)
  | ["kpt-history", key, scopeName] =>
      let state ← load path
      let scope ← match parseMemoryScope state scopeName with
        | .ok selected => pure selected
        | .error reason => throw <| IO.userError reason
      printKPTHistory state key scope
  | ["finish-task"] =>
      let state ← applyMutation path arguments Kernel.finishCurrentTask
      printNext state
  | ["start-work", outcome, firstTask] =>
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.startWork state outcome firstTask
          (callerDecision context "Start this independent outcome.")
      printNext state
  | ["switch-work", outcome] =>
      let state ← applyMutation path arguments fun state =>
        Kernel.switchWork state outcome
      printNext state
  | ["add-task", description] =>
      let state ← applyMutation path arguments fun state =>
        Kernel.addTask state description
      printNext state
  | "add-task-for-design" :: description :: designKeys =>
      let state ← applyMutation path arguments fun state =>
        Kernel.addTaskForDesign state description designKeys
      printNext state
  | ["assign-phase", taskDescription, phaseName, order] =>
      let displayOrder ← match order.toNat? with
        | some value => pure value
        | none => throw <| IO.userError "Phase order must be a natural number."
      let state ← applyMutation path arguments fun state =>
        Kernel.assignPhase state taskDescription phaseName displayOrder
      printNext state
  | ["rename-phase", currentName, nextName] =>
      let state ← applyMutation path arguments fun state =>
        Kernel.renamePhase state currentName nextName
      printNext state
  | ["order-phase", name, order] =>
      let displayOrder ← match order.toNat? with
        | some value => pure value
        | none => throw <| IO.userError "Phase order must be a natural number."
      let state ← applyMutation path arguments fun state =>
        Kernel.orderPhase state name displayOrder
      printNext state
  | "record-command-profile" :: key :: purpose :: scopeName ::
      dispositionName :: cwd :: reason :: argv =>
      let disposition ← match parseCommandDisposition dispositionName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let selectedCwd := if cwd == "-" then none else some cwd
      let token ← privateToken
      let context ← sourceContext token
      let accepted := callerDecision context reason
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.recordCommandProfile state accepted.source (some accepted)
          key purpose scope argv selectedCwd disposition
      printNext state
  | "propose-command-profile" :: key :: purpose :: scopeName ::
      dispositionName :: cwd :: argv =>
      let disposition ← match parseCommandDisposition dispositionName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let selectedCwd := if cwd == "-" then none else some cwd
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.recordCommandProfile state (source context .agent) none
          key purpose scope argv selectedCwd disposition
      printNext state
  | ["accept-command-profile", key, scopeName, reason] =>
      let token ← privateToken
      let context ← sourceContext token
      let accepted := callerDecision context reason
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.acceptCommandProfile state key scope accepted
      printNext state
  | "record-command-deviation" :: key :: scopeName :: evidenceKey :: cwd ::
      reason :: argv =>
      let selectedCwd := if cwd == "-" then none else some cwd
      let selectedEvidence :=
        if evidenceKey == "-" then none else some evidenceKey
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.recordCommandDeviation state key argv selectedCwd reason
          (source context .agent) selectedEvidence (some scope)
      printNext state
  | ["record-kpt", author, key, categoryName, scopeName, statement,
      relationKind, relationKey, relationMember, relationObservedValue,
      relationPassed, predecessorAuthor] =>
      let category ← match parseKPTCategory categoryName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let selectedRelation ← match
          parseKPTRelation relationKind relationKey relationMember
            relationObservedValue relationPassed with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let selectedPredecessorAuthor :=
        if predecessorAuthor == "-" then none else some predecessorAuthor
      let token ← privateToken
      let context ← sourceContext token
      let accepted := callerDecision context "Record this caller-owned KPT."
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.recordKPT state accepted.source author (some accepted) key
          category scope statement selectedRelation selectedPredecessorAuthor
      printNext state
  | ["propose-kpt", author, key, categoryName, scopeName, statement,
      relationKind, relationKey, relationMember, relationObservedValue,
      relationPassed] =>
      let category ← match parseKPTCategory categoryName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let selectedRelation ← match
          parseKPTRelation relationKind relationKey relationMember
            relationObservedValue relationPassed with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.recordKPT state (source context .agent) author none key
          category scope statement selectedRelation
      printNext state
  | ["accept-kpt", author, key, scopeName, reason] =>
      let token ← privateToken
      let context ← sourceContext token
      let accepted := callerDecision context reason
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.acceptKPT state key scope author accepted
      printNext state
  | "record-kpt-command-profile" :: author :: kptKey :: categoryName ::
      scopeName :: statement :: relationKind :: relationKey ::
      relationMember :: relationObservedValue :: relationPassed ::
      profileKey :: purpose :: dispositionName :: cwd :: argv =>
      let category ← match parseKPTCategory categoryName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let disposition ← match parseCommandDisposition dispositionName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let selectedRelation ← match
          parseKPTRelation relationKind relationKey relationMember
            relationObservedValue relationPassed with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let selectedCwd := if cwd == "-" then none else some cwd
      let token ← privateToken
      let context ← sourceContext token
      let accepted :=
        callerDecision context "Record the KPT and its Command Profile conclusion."
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.recordKPTWithCommandProfile state accepted.source
          author (some accepted) kptKey category scope statement selectedRelation
          profileKey purpose argv selectedCwd disposition
      printNext state
  | ["record-kpt-instruction", author, key, categoryName, scopeName,
      statement, relationKind, relationKey, relationMember,
      relationObservedValue, relationPassed, instruction] =>
      let category ← match parseKPTCategory categoryName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let selectedRelation ← match
          parseKPTRelation relationKind relationKey relationMember
            relationObservedValue relationPassed with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let token ← privateToken
      let context ← sourceContext token
      let accepted :=
        callerDecision context "Record the KPT and its operating instruction."
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.recordKPTWithInstruction state accepted author key category scope
          statement selectedRelation instruction
      printNext state
  | "record-kpt-design" :: author :: kptKey :: categoryName :: scopeName ::
      statement :: relationKind :: relationKey :: relationMember ::
      relationObservedValue :: relationPassed :: designKey :: roleName ::
      assuranceName :: designStatement :: dependencyKeys =>
      let category ← match parseKPTCategory categoryName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let role ← match parseRole roleName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let assurance ← match
          parseAssurance designKey designStatement assuranceName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let selectedRelation ← match
          parseKPTRelation relationKind relationKey relationMember
            relationObservedValue relationPassed with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let token ← privateToken
      let context ← sourceContext token
      let accepted :=
        callerDecision context "Record the KPT and its unaccepted Design candidate."
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.recordKPTWithDesignCandidate state accepted.source
          author (some accepted) kptKey category scope statement selectedRelation
          designKey designStatement role assurance dependencyKeys
      printNext state
  | ["add-evidence", key, observation, method, environment, inputs,
      acceptanceCondition, trustedBoundary, artifactIdentity] =>
      let state ← applyMutation path arguments fun state =>
        Kernel.addEvidence state key observation method environment
          (commaSeparated inputs) acceptanceCondition trustedBoundary artifactIdentity
      printNext state
  | ["add-evidence", key, observation, method, environment, inputs,
      acceptanceCondition, trustedBoundary, artifactIdentity, designKey] =>
      let state ← applyMutation path arguments fun state =>
        Kernel.addEvidence state key observation method environment
          (commaSeparated inputs) acceptanceCondition trustedBoundary artifactIdentity
          (some designKey)
      printNext state
  | ["add-evidence", key, observation, method, environment, inputs,
      acceptanceCondition, trustedBoundary, artifactIdentity, designKey,
      commandProfileKey, profileScopeName, selectionReason] =>
      let selectedDesign := if designKey == "-" then none else some designKey
      let token ← privateToken
      let context ← sourceContext token
      let selected :=
        callerDecision context selectionReason
      let state ← applyMutation path arguments fun state => do
        let profileScope ← parseMemoryScope state profileScopeName
        Kernel.addEvidence state key observation method environment
          (commaSeparated inputs) acceptanceCondition trustedBoundary artifactIdentity
          selectedDesign (some commandProfileKey) (some profileScope)
          (some selected)
      printNext state
  | ["record-evidence", key, observedValue, result] =>
      let passed ← match parsePassed result with
        | .ok passed => pure passed
        | .error reason => throw <| IO.userError reason
      let state ← applyMutation path arguments fun state =>
        Kernel.recordEvidence state key observedValue passed
      printNext state
  | ["record-evidence", key, observedValue, result, designKey] =>
      let passed ← match parsePassed result with
        | .ok passed => pure passed
        | .error reason => throw <| IO.userError reason
      let state ← applyMutation path arguments fun state =>
        Kernel.recordEvidence state key observedValue passed (some designKey)
      printNext state
  | ["select-formal", key, designKey, oracle, modules, surfaces,
      adapter, cases] =>
      let selectedOracle := if oracle == "-" then none else some oracle
      let selectedAdapter := if adapter == "-" then none else some adapter
      let state ← applyMutation path arguments fun state =>
        Kernel.selectFormal state key designKey selectedOracle
          (commaSeparated modules) (commaSeparated surfaces)
          (commaSeparated cases) selectedAdapter
      printNext state
  | ["record-formal-result", key, designKey, designVersion, tool, oracle,
      closure, artifacts,
      conformance, semanticPreview, previewIdentity] =>
      let version ← match designVersion.toNat? with
        | some version => pure version
        | none =>
            throw <| IO.userError "The selected formal Design version is invalid."
      let oracleArtifact := if oracle == "-" then none else some oracle
      let conformancePassed ← match conformance with
        | "none" => pure none
        | "pass" => pure (some true)
        | "fail" => pure (some false)
        | "execution-failure" => pure none
        | _ =>
            throw <| IO.userError
              "Formal conformance must be none, pass, fail, or execution-failure."
      let state ← applyMutation path arguments fun state =>
        Kernel.recordFormalResult state key tool oracleArtifact
          (commaSeparated closure) (commaSeparated artifacts)
          conformancePassed semanticPreview previewIdentity
          (some designKey) (some version)
      printNext state
  | ["record-formal-result-files", key, designKey, designVersion, tool, oracle, closureFile,
      artifactsFile, conformance, semanticPreviewFile, previewIdentity] =>
      let version ← match designVersion.toNat? with
        | some version => pure version
        | none =>
            throw <| IO.userError "The selected formal Design version is invalid."
      let oracleArtifact := if oracle == "-" then none else some oracle
      let conformancePassed ← match conformance with
        | "none" => pure none
        | "pass" => pure (some true)
        | "fail" => pure (some false)
        | "execution-failure" => pure none
        | _ =>
            throw <| IO.userError
              "Formal conformance must be none, pass, fail, or execution-failure."
      let checkedClosure ← readBoundedFormalLines
        (System.FilePath.mk closureFile) "checked formal closure"
      let checkedArtifacts ← readBoundedFormalLines
        (System.FilePath.mk artifactsFile) "checked formal artifacts"
      let semanticPreview ← readBoundedFormalFile
        (System.FilePath.mk semanticPreviewFile) "formal semantic preview"
      let semanticIntent :=
        formalResultMutationIntentArguments key designKey designVersion tool oracle
          conformance previewIdentity checkedClosure checkedArtifacts
          semanticPreview
      let state ← applyMutation path semanticIntent fun state =>
        Kernel.recordFormalResult state key tool oracleArtifact
          checkedClosure checkedArtifacts conformancePassed semanticPreview
          previewIdentity (some designKey) (some version)
      printNext state
  | "record-design" :: key :: roleName :: assuranceName :: statement ::
      dependencyKeys =>
      let role ← match parseRole roleName with
        | .ok role => pure role
        | .error reason => throw <| IO.userError reason
      let assurance ← match parseAssurance key statement assuranceName with
        | .ok assurance => pure assurance
        | .error reason => throw <| IO.userError reason
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.recordDesign state (source context .caller) key statement role
          assurance dependencyKeys
      printNext state
  | "propose-design" :: impact :: key :: roleName :: assuranceName :: statement ::
      dependencyKeys =>
      let addsComplexity ← match impact with
        | "ordinary" => pure false
        | "complexity" => pure true
        | _ =>
            throw <| IO.userError
              "Agent design impact must be ordinary or complexity."
      let role ← match parseRole roleName with
        | .ok role => pure role
        | .error reason => throw <| IO.userError reason
      let assurance ← match parseAssurance key statement assuranceName with
        | .ok assurance => pure assurance
        | .error reason => throw <| IO.userError reason
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.recordDesign state (source context .agent) key statement role
          assurance dependencyKeys addsComplexity
      printNext state
  | ["accept-design", key, reason] =>
      let token ← privateToken
      let context ← sourceContext token
      let stale ← staleFormalResultIdentities
      let state ← applyMutation path arguments fun state =>
        Kernel.acceptDesign state key (callerDecision context reason)
          none stale
      printNext state
  | ["accept-design-with-kpt", designKey, reason, author, kptKey,
      categoryName, scopeName, statement, relationKind, relationKey,
      relationMember, relationObservedValue, relationPassed] =>
      let category ← match parseKPTCategory categoryName with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let selectedRelation ← match
          parseKPTRelation relationKind relationKey relationMember
            relationObservedValue relationPassed with
        | .ok selected => pure selected
        | .error message => throw <| IO.userError message
      let token ← privateToken
      let context ← sourceContext token
      let accepted := callerDecision context reason
      let stale ← staleFormalResultIdentities
      let state ← applyMutation path arguments fun state => do
        let scope ← parseMemoryScope state scopeName
        Kernel.acceptDesignWithKPT state designKey accepted author kptKey
          category scope statement selectedRelation stale
      printNext state
  | ["accept-complex-design", key, reason, necessity, simpler, scope, cost] =>
      let token ← privateToken
      let context ← sourceContext token
      let rationale : Design.ComplexityRationale :=
        { necessity
          simplerAlternativeInsufficient := simpler
          boundedScope := scope
          maintenanceCost := cost }
      let stale ← staleFormalResultIdentities
      let state ← applyMutation path arguments fun state =>
        Kernel.acceptDesign state key (callerDecision context reason)
          (some rationale) stale
      printNext state
  | ["retire-design", key, reason] =>
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.retireDesign state key (callerDecision context reason)
      printNext state
  | ["record-instruction", statement] =>
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.recordInstruction state (callerDecision context statement) statement
      printNext state
  | ["record-question", statement] =>
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.recordNonAuthoritative state (source context .agent)
          .question statement
      printNext state
  | ["reject-proposal", target, reason] =>
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.recordNonAuthoritative state (source context .caller)
          .rejection reason (some target)
      printNext state
  | "record-source-effects" :: designKey :: roleName :: assuranceName ::
      designStatement :: instruction :: question :: outcome :: firstTask ::
      dependencyKeys =>
      let optional (value : String) :=
        if value == "-" then none else some value
      let role ← match parseRole roleName with
        | .ok role => pure role
        | .error reason => throw <| IO.userError reason
      let assurance ← match parseAssurance designKey designStatement assuranceName with
        | .ok assurance => pure assurance
        | .error reason => throw <| IO.userError reason
      let work ← match optional outcome, optional firstTask with
        | some selectedOutcome, some task => pure (some (selectedOutcome, task))
        | none, none => pure none
        | _, _ =>
            throw <| IO.userError
              "An outcome and first task must be provided together."
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.recordSourceEffects state (source context .caller)
          (optional designKey) (optional designStatement) role assurance
          dependencyKeys (optional instruction) (optional question) work
      printNext state
  | ["interrupt", outcome, firstTask] =>
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.startInterruption state outcome firstTask
          (callerDecision context "Interrupt the current outcome.")
      printNext state
  | ["return"] =>
      let stale ← staleFormalResultIdentities
      let state ← applyMutation path arguments fun state =>
        match Kernel.returnFromInterruption state stale with
        | .accepted returned => .ok returned
        | .replanRequired _ =>
            .error "The saved outcome changed; a caller replan decision is required."
        | .invalid reason => .error reason
      printNext state
  | ["replan-return", outcome, reason] =>
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.replanReturnByOutcome state outcome
          (callerDecision context reason)
      printNext state
  | ["request-review", key, purposeName, artifact] =>
      let purpose ← match parseReviewPurpose purposeName with
        | .ok purpose => pure purpose
        | .error reason => throw <| IO.userError reason
      let state ← applyMutation path arguments fun state =>
        Kernel.requestReview state key artifact purpose
      printNext state
  | ["request-design-review", key, designKey] =>
      let stale ← staleFormalResultIdentities
      let state ← applyMutation path arguments fun state =>
        Kernel.requestDesignReview state key designKey stale
      printNext state
  | ["record-review", reviewKey, reviewer, observationKey, kindName,
      complexityName, summary, evidence] =>
      let kind ← match kindName with
        | "risk" => pure Review.ObservationKind.risk
        | "proposal" => pure .proposal
        | _ => throw <| IO.userError "Observation kind must be risk or proposal."
      let addsComplexity ← match complexityName with
        | "ordinary" => pure false
        | "complexity" => pure true
        | _ => throw <| IO.userError "Proposal impact must be ordinary or complexity."
      let observation : Review.Observation :=
        { key := observationKey, kind, summary, evidence, addsComplexity }
      let state ← applyMutation path arguments fun state =>
        Kernel.recordReviewResult state reviewKey reviewer observation
      printNext state
  | ["record-clean-review", reviewKey, reviewer] =>
      let state ← applyMutation path arguments fun state =>
        Kernel.recordCleanReview state reviewKey reviewer
      printNext state
  | ["resolve-review", reviewKey, observationKey, decisionName, reason] =>
      let decision ← match parseReviewDecision decisionName with
        | .ok decision => pure decision
        | .error message => throw <| IO.userError message
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.recordReviewDisposition state reviewKey observationKey decision
          (callerDecision context reason)
      printNext state
  | ["adopt-review-proposal", reviewKey, observationKey, successorKey,
      reason] =>
      let token ← privateToken
      let context ← sourceContext token
      let stale ← staleFormalResultIdentities
      let state ← applyMutation path arguments fun state =>
        Kernel.adoptReviewProposal state reviewKey observationKey successorKey
          (callerDecision context reason) none stale
      printNext state
  | ["adopt-complex-review-proposal", reviewKey, observationKey, successorKey,
      reason, necessity, simpler, scope, cost] =>
      let token ← privateToken
      let context ← sourceContext token
      let rationale : Review.ComplexityRationale :=
        { necessity
          simplerAlternativeInsufficient := simpler
          boundedScope := scope
          maintenanceCost := cost }
      let stale ← staleFormalResultIdentities
      let state ← applyMutation path arguments fun state =>
        Kernel.adoptReviewProposal state reviewKey observationKey successorKey
          (callerDecision context reason) (some rationale) stale
      printNext state
  | ["correct-review", mistakenKey, intendedOutcome, intendedTask,
      intendedArtifact, reason] =>
      let token ← privateToken
      let context ← sourceContext token
      let state ← applyMutation path arguments fun state =>
        Kernel.correctReviewByOutcome state mistakenKey intendedOutcome
          intendedTask intendedArtifact
          (callerDecision context reason)
      printNext state
  | ["complete"] =>
      let state ← load path
      let stale ← staleFormalResultIdentities
      if Kernel.currentlyComplete state state.focus.work stale then
        IO.println "The current outcome is complete."
      else do
        printNext state
        IO.Process.exit 1
  | _ =>
      throw <| IO.userError
        "Unknown project action."

end AgentWorkbench.Cli
