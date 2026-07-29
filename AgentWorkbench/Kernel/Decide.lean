import AgentWorkbench.Domain.Evidence
import AgentWorkbench.Domain.Review

namespace AgentWorkbench.Kernel

open AgentWorkbench.Domain

structure State where
  design : Design.Package
  work : List Work.Unit
  tasks : List Work.Task
  phases : List Work.Phase
  evidenceSpecs : List Evidence.Spec
  evidenceResults : List Evidence.Result
  formalSpecs : List Evidence.FormalSpec
  formalResults : List Evidence.FormalResult
  reviewRequests : List Review.Request
  reviewResults : List Review.Result
  reviewDispositions : List Review.Disposition
  focus : Work.Focus
deriving DecidableEq, Repr, BEq

def State.wellFormed (state : State) : Bool :=
  state.design.wellFormed &&
    (state.design.designItems.map (·.ref)).Nodup &&
    state.work.all Work.Unit.wellFormed &&
    (state.work.map (·.ref)).Nodup &&
    state.tasks.all Work.Task.wellFormed &&
    (state.tasks.map (·.ref)).Nodup &&
    (state.tasks.all fun task => state.work.any (·.ref == task.work)) &&
    (state.phases.map (·.key)).Nodup &&
    (state.phases.map (·.name)).Nodup &&
    (state.phases.all fun phase =>
      !phase.key.isEmpty && !phase.name.isEmpty) &&
    state.evidenceSpecs.all Evidence.Spec.wellFormed &&
    (state.evidenceSpecs.map (·.ref)).Nodup &&
    (state.evidenceResults.all fun result =>
      result.wellFormed &&
        state.evidenceSpecs.any (· == result.spec)) &&
    state.formalSpecs.all Evidence.FormalSpec.wellFormed &&
    (state.formalSpecs.map fun spec => (spec.key, spec.design)).Nodup &&
    (state.formalResults.all fun result =>
      state.formalSpecs.any fun spec =>
        result.currentFor spec [spec.design]) &&
    state.reviewRequests.all (·.scope.wellFormed) &&
    (state.reviewRequests.map (·.ref)).Nodup &&
    (state.reviewRequests.all fun request =>
      state.work.any (·.ref == request.scope.work)) &&
    (state.reviewResults.all fun result =>
      state.reviewRequests.any (result.exactFor ·)) &&
    (state.reviewDispositions.all fun disposition =>
      state.reviewResults.any fun result =>
        result.review == disposition.review &&
          result.observations.any
            (disposition.wellFormedFor ·)) &&
    state.focus.wellFormed &&
    state.work.any (·.ref == state.focus.work)

private def retainCurrentDependencies : Nat → List Design.Item →
    List Design.Item
  | 0, items => items
  | remaining + 1, items =>
      let refs := items.map (·.ref)
      let retained := items.filter fun item =>
        item.dependencies.all refs.contains
      retainCurrentDependencies remaining retained

private def latestAuthoritativeDesignItems (state : State) :
    List Design.Item :=
  let authoritative := state.design.designItems.filter fun item =>
    match item.authority with
    | .unaccepted => false
    | .acceptedByCaller _ | .retiredByCaller _ => true
  authoritative.filter fun item =>
    !authoritative.any fun successor =>
      successor.ref.key == item.ref.key &&
        successor.ref.version > item.ref.version

def currentDesignItems (state : State) : List Design.Item :=
  let latest := (latestAuthoritativeDesignItems state).filter fun item =>
    item.acceptedRef?.isSome
  retainCurrentDependencies latest.length latest

structure AffectedDesign where
  key : String
  path : List DesignRef
deriving DecidableEq, Repr, BEq

private def extendAffectedDesigns (items : List Design.Item) :
    Nat → List AffectedDesign → List AffectedDesign
  | 0, affected => affected
  | remaining + 1, affected =>
      let extended := items.filterMap fun item =>
        if affected.any (·.key == item.ref.key) then
          none
        else
          affected.findSome? fun prior =>
            match prior.path.reverse.head? with
            | some dependency =>
                if item.dependencies.contains dependency then
                  some { key := item.ref.key, path := prior.path ++ [item.ref] }
                else
                  none
            | none => none
      if extended.isEmpty then affected
      else
        extendAffectedDesigns items remaining
          (affected ++ extended)

def affectedDesigns (state : State) : List AffectedDesign :=
  let latest := latestAuthoritativeDesignItems state
  let changes := latest.filter fun item => item.predecessor.isSome
  changes.flatMap fun change =>
    let predecessor := change.predecessor.getD change.ref
    let dependants := latest.filter fun item =>
      item.acceptedRef?.isSome && item.ref.key != change.ref.key
    let affected := extendAffectedDesigns dependants dependants.length
      [{ key := change.ref.key, path := [predecessor] }]
    let predecessorSelected :=
      (state.work.filter fun work =>
        !state.work.any fun successor =>
          successor.ref.key == work.ref.key &&
            successor.ref.version > work.ref.version).any fun work =>
        work.completionBoundary.any fun member =>
          let basisSelected :=
            match member.basis with
            | .design items => items.any (·.ref == predecessor)
            | .workBoundary _ => false
          basisSelected ||
            match member.target with
            | .taskSatisfied taskRef =>
                state.tasks.find? (·.ref == taskRef) |>.any fun task =>
                  task.designScope.any (·.ref == predecessor)
            | _ => false
    if predecessorSelected || affected.length > 1 then affected else []

def currentDesignRefs (state : State) : List DesignRef :=
  (currentDesignItems state).map (·.ref)

private def latestUnacceptedFormalDesign? (state : State) (key : String) :
    Option Design.Item :=
  state.design.designItems.reverse.find? fun item =>
    item.authority == .unaccepted &&
      item.assurance.obligations.any fun obligation =>
        obligation.key == key && obligation.method == .formal

def selectedFormalSpecs (state : State) (key : String) :
    List Evidence.FormalSpec :=
  let matching := state.formalSpecs.filter (·.key == key)
  let accepted := matching.filter fun spec =>
    (currentDesignRefs state).contains spec.design
  match latestUnacceptedFormalDesign? state key with
  | none => accepted
  | some candidate =>
      let proposed := matching.filter (·.design == candidate.ref)
      if proposed.isEmpty then accepted else proposed

def formalResultsRequiringVerification (state : State) :
    List Evidence.FormalResult :=
  let current := currentDesignRefs state
  let activeSpecs := state.formalSpecs.filter fun spec =>
    current.contains spec.design ||
      (latestUnacceptedFormalDesign? state spec.key).any
        (·.ref == spec.design)
  activeSpecs.filterMap fun spec =>
    state.formalResults.reverse.find? fun result =>
      result.currentFor spec [spec.design]

def workCurrent (state : State) (work : WorkRef) : Bool :=
  state.work.any (·.ref == work) &&
    !state.work.any fun candidate =>
      candidate.ref.key == work.key && candidate.ref.version > work.version

def taskCurrent (state : State) (task : Work.Task) : Bool :=
  state.tasks.any (·.ref == task.ref) &&
    !(state.tasks.any fun candidate =>
      candidate.ref.key == task.ref.key &&
        candidate.ref.version > task.ref.version) &&
    task.designScope.all fun item =>
      (currentDesignRefs state).contains item.ref

def evidenceSpecCurrent (state : State) (spec : Evidence.Spec) : Bool :=
  state.evidenceSpecs.any (·.ref == spec.ref) &&
    !state.evidenceSpecs.any fun candidate =>
      candidate.ref.key == spec.ref.key &&
        candidate.ref.version > spec.ref.version

def reviewScopeCurrent (state : State) (scope : Review.Scope) : Bool :=
  state.work.any fun work =>
    work.ref.key == scope.work.key && workCurrent state work.ref &&
    (match scope.purpose with
    | .designMeaning =>
        scope.design.all fun selected =>
          state.design.designItems.find? (·.ref == selected)
            |>.any fun item =>
              match item.authority with
              | .unaccepted =>
                  state.design.designItems.reverse.find?
                    (·.ref.key == selected.key)
                    |>.any (·.ref == selected)
              | .acceptedByCaller _ =>
                  (currentDesignRefs state).contains selected
              | .retiredByCaller _ => false
    | .implementation | .reuseDecision =>
        scope.design.all (currentDesignRefs state).contains) &&
      match scope.task with
      | none => true
      | some taskRef =>
          state.tasks.any fun task =>
            task.ref == taskRef &&
              task.work.key == scope.work.key &&
              taskCurrent state task

def reviewRequestCurrent (state : State) (request : Review.Request) : Bool :=
  state.reviewRequests.any (·.ref == request.ref) &&
    !(state.reviewRequests.any fun candidate =>
      candidate.ref.key == request.ref.key &&
        candidate.ref.version > request.ref.version) &&
    reviewScopeCurrent state request.scope

def basisCurrent (state : State) : Work.DerivationBasis → Bool
  | .design items =>
      items.all fun item => (currentDesignRefs state).contains item.ref
  | .workBoundary work =>
      state.work.any fun candidate =>
        candidate.ref.key == work.key && workCurrent state candidate.ref

def evidenceResultCurrent (state : State) (result : Evidence.Result) : Bool :=
  evidenceSpecCurrent state result.spec &&
    result.currentFor result.spec (currentDesignRefs state)
      (state.work.filterMap fun work =>
        if workCurrent state work.ref then some work.ref else none)

def assuranceSatisfied (state : State) (key : String) : Bool :=
  (Evidence.selectedAssurance (currentDesignItems state)).any fun assurance =>
    assurance.key == key &&
      assurance.basis.wellFormed &&
      match assurance.method with
      | .formal =>
          state.formalResults.any fun result =>
            result.spec.key == key &&
              state.formalSpecs.any fun spec =>
                spec.key == key &&
                  result.conformsFor spec (currentDesignRefs state)
      | .evidence =>
          state.evidenceResults.any fun result =>
            result.spec.ref.key == key &&
              result.passed &&
              evidenceResultCurrent state result

def reviewResolved (state : State) (review : ReviewRef) : Bool :=
  match state.reviewRequests.find? (·.ref == review) with
  | none => false
  | some request =>
      reviewRequestCurrent state request &&
        state.reviewResults.any fun result =>
          result.resolvedBy request state.reviewDispositions

def completionMemberSatisfied (state : State) (work : WorkRef)
    (member : Work.CompletionMember) : Bool :=
  member.wellFormedFor work &&
    basisCurrent state member.basis &&
    match member.target with
    | .taskSatisfied task =>
        state.tasks.any fun candidate =>
          candidate.ref == task &&
            candidate.work.key == work.key &&
            candidate.state == .satisfied &&
            candidate.wellFormed &&
            taskCurrent state candidate
    | .assurance key => assuranceSatisfied state key
    | .reviewResolved review => reviewResolved state review
    | .externalObservation evidence =>
        state.evidenceResults.any fun result =>
          result.spec.ref == evidence &&
            result.passed &&
            evidenceResultCurrent state result

def missingCompletion (state : State) (work : WorkRef) :
    List Work.CompletionMember :=
  match state.work.find? (·.ref == work) with
  | none => []
  | some selected =>
      if workCurrent state selected.ref && selected.wellFormed then
        selected.completionBoundary.filter fun member =>
          !completionMemberSatisfied state selected.ref member
      else
        []

def currentlyComplete (state : State) (work : WorkRef) : Bool :=
  workCurrent state work &&
    (state.work.find? (·.ref == work)).any fun selected =>
      selected.wellFormed &&
        (missingCompletion state work).isEmpty

private def rebaseBoundary (prior next : WorkRef)
    (member : Work.CompletionMember) : Work.CompletionMember :=
  match member.basis with
  | .workBoundary selected =>
      if selected == prior then
        { member with basis := .workBoundary next }
      else
        member
  | .design _ => member

private def reviseFocusedWork (state : State)
    (change :
      WorkRef → List Work.CompletionMember →
        List Work.CompletionMember) :
    Except String (State × WorkRef) :=
  match state.work.find? (·.ref == state.focus.work) with
  | none => .error "The current outcome does not exist."
  | some current =>
      if !workCurrent state current.ref then
        .error "The current outcome has changed."
      else
        let nextRef : WorkRef :=
          { key := current.ref.key, version := current.ref.version + 1 }
        let retained := current.completionBoundary.filter fun member =>
          match member.target with
          | .reviewResolved review =>
              state.reviewRequests.find? (·.ref == review)
                |>.any (reviewRequestCurrent state)
          | _ => true
        let rebased := retained.map (rebaseBoundary current.ref nextRef)
        let successor :=
          { current with
            ref := nextRef
            completionBoundary := change nextRef rebased }
        if !successor.wellFormed then
          .error "The revised completion boundary is invalid."
        else
          .ok
            ({ state with
                work := state.work ++ [successor]
                focus := { state.focus with work := nextRef } },
              nextRef)

def addTaskForDesign (state : State) (description : String)
    (designKeys : List String) :
    Except String State := do
  if description.isEmpty then
    throw "A task description is required."
  let key := s!"task-{state.tasks.length + 1}"
  let taskRef : TaskRef := { key, version := 0 }
  let designScope :=
    (currentDesignItems state).filterMap fun item =>
      if designKeys.contains item.ref.key then item.acceptedRef? else none
  if designScope.length != designKeys.eraseDups.length then
    throw "One or more selected design statements are not accepted and current."
  let basis : Work.DerivationBasis :=
    if designScope.isEmpty then .workBoundary state.focus.work
    else .design designScope
  let (revised, work) ← reviseFocusedWork state fun selected boundary =>
    let currentBoundary := boundary.filter fun member =>
      match member.target with
      | .taskSatisfied prior =>
          state.tasks.find? (·.ref == prior) |>.any (taskCurrent state)
      | _ => true
    let assuranceMembers := designScope.flatMap fun accepted =>
      match (currentDesignItems state).find? (·.ref == accepted.ref) with
      | none => []
      | some item =>
          item.assurance.obligations.map fun obligation =>
            { target := Work.CompletionTarget.assurance obligation.key
              basis := Work.DerivationBasis.design [accepted] }
    let retained := currentBoundary.filter fun member =>
      match member.target with
      | .assurance key =>
          !assuranceMembers.any fun assurance =>
            assurance.target == .assurance key
      | _ => true
    { target := .taskSatisfied taskRef
      basis :=
        match basis with
        | .workBoundary _ => .workBoundary selected
        | .design items => .design items } :: assuranceMembers ++ retained
  let task : Work.Task :=
    { ref := taskRef
      work
      description
      basis :=
        match basis with
        | .workBoundary _ => .workBoundary work
        | .design items => .design items
      designScope
      phase := none
      state := .pending }
  if !task.wellFormed then
    throw "The task is not valid for the current outcome."
  pure
    { revised with
      tasks := revised.tasks ++ [task]
      focus := { revised.focus with task := some taskRef } }

def addTask (state : State) (description : String) : Except String State :=
  addTaskForDesign state description []

def finishCurrentTask (state : State) : Except String State := do
  let candidates := state.tasks.filter fun task =>
    task.work.key == state.focus.work.key &&
      task.state == .pending &&
      taskCurrent state task
  let selected ← match state.focus.task with
    | some task =>
        match candidates.find? (·.ref == task) with
        | some current => pure current
        | none => throw "The selected task is not pending."
    | none =>
        match candidates with
        | [task] => pure task
        | [] => throw "No task is pending for the current outcome."
        | _ => throw "More than one task is pending; select one task first."
  pure
    { state with
      tasks := state.tasks.map fun task =>
        if task.ref == selected.ref then
          { task with state := .satisfied }
        else
          task }

private def taskByDescription (state : State)
    (description : String) : Except String Work.Task :=
  match state.tasks.filter fun task =>
      task.description == description && taskCurrent state task with
  | [task] => .ok task
  | [] => .error "No current task has that description."
  | _ => .error "The task description is ambiguous."

def assignPhase (state : State) (taskDescription phaseName : String)
    (displayOrder : Nat) : Except String State := do
  if phaseName.isEmpty then
    throw "A Phase name is required."
  let task ← taskByDescription state taskDescription
  let (phases, phase) :=
    match state.phases.find? (·.name == phaseName) with
    | some phase => (state.phases, phase)
    | none =>
        let phase : Work.Phase :=
          { key := s!"phase-{state.phases.length + 1}"
            name := phaseName
            displayOrder }
        (state.phases ++ [phase], phase)
  pure
    { state with
      phases
      tasks := state.tasks.map fun candidate =>
        if candidate.ref == task.ref then
          { candidate with phase := some phase.key }
        else
          candidate }

def renamePhase (state : State) (currentName nextName : String) :
    Except String State := do
  if nextName.isEmpty || state.phases.any (·.name == nextName) then
    throw "The new Phase name is empty or already used."
  if !state.phases.any (·.name == currentName) then
    throw "No Phase has that name."
  pure
    { state with
      phases := state.phases.map fun phase =>
        if phase.name == currentName then { phase with name := nextName }
        else phase }

def orderPhase (state : State) (name : String) (displayOrder : Nat) :
    Except String State := do
  if !state.phases.any (·.name == name) then
    throw "No Phase has that name."
  pure
    { state with
      phases := state.phases.map fun phase =>
        if phase.name == name then { phase with displayOrder }
        else phase }

def addEvidence (state : State) (key observation method environment : String)
    (inputs : List String)
    (acceptanceCondition trustedBoundary artifactIdentity : String) :
    Except String State := do
  if [key, observation, method, environment, acceptanceCondition,
      trustedBoundary, artifactIdentity].any String.isEmpty then
    throw "The evidence description is incomplete."
  let version :=
    (state.evidenceSpecs.filter (·.ref.key == key)).foldl
      (fun next spec => max next (spec.ref.version + 1)) 0
  let evidenceRef : EvidenceRef := { key, version }
  let selectedWork ← match state.work.find? (·.ref == state.focus.work) with
    | some work => pure work
    | none => throw "The current outcome does not exist."
  let selectedAssurance := selectedWork.completionBoundary.findSome? fun member =>
    match member.target, member.basis with
    | .assurance selectedKey, .design accepted =>
        if selectedKey == key then some accepted else none
    | _, _ => none
  let (revised, basis) ← match selectedAssurance with
    | some accepted =>
        pure (state, Work.DerivationBasis.design accepted)
    | none => do
        let (revised, work) ← reviseFocusedWork state fun selected boundary =>
          let retained := boundary.filter fun member =>
            match member.target with
            | .externalObservation prior => prior.key != key
            | _ => true
          { target := .externalObservation evidenceRef
            basis := .workBoundary selected } :: retained
        pure (revised, Work.DerivationBasis.workBoundary work)
  let spec : Evidence.Spec :=
    { ref := evidenceRef
      observation
      method
      environment
      inputs
      acceptanceCondition
      trustedBoundary
      artifactIdentity
      basis }
  if !spec.wellFormed then
    throw "The evidence description is invalid."
  return { revised with evidenceSpecs := revised.evidenceSpecs ++ [spec] }

def recordEvidence (state : State) (key observedValue : String)
    (passed : Bool) : Except String State := do
  if observedValue.isEmpty then
    throw "An observed value is required."
  let candidates := state.evidenceSpecs.filter fun spec =>
    spec.ref.key == key && evidenceSpecCurrent state spec
  let spec ← match candidates with
    | [spec] => pure spec
    | [] => throw "No current evidence description has that name."
    | _ => throw "The evidence name is ambiguous."
  let result : Evidence.Result := { spec, observedValue, passed }
  if !result.wellFormed then
    throw "The evidence result is invalid."
  return { state with evidenceResults := state.evidenceResults ++ [result] }

def selectFormal (state : State) (key designKey : String)
    (oracle : Option String)
    (modules implementationSurfaces cases : List String)
    (adapter : Option String) : Except String State := do
  let candidate := state.design.designItems.reverse.find? fun item =>
    item.ref.key == designKey
  let design ← match candidate with
    | some item =>
        if item.assurance.obligations.any fun obligation =>
            obligation.key == key && obligation.method == .formal then
          pure item
        else
          throw "The selected design does not require that formal assurance."
    | none => throw "No design statement has that name."
  let currentAccepted := currentDesignRefs state
  let reviewable :=
    match design.authority with
    | .unaccepted => true
    | .acceptedByCaller _ => currentAccepted.contains design.ref
    | .retiredByCaller _ => false
  if !reviewable then
    throw "The selected formal design is superseded."
  let spec : Evidence.FormalSpec :=
    { key
      design := design.ref
      modules
      oracle
      implementationSurfaces
      cases
      adapter }
  if !spec.wellFormed then
    throw "The formal assurance description is incomplete."
  if state.formalSpecs.any (· == spec) then
    return state
  return { state with formalSpecs := state.formalSpecs ++ [spec] }

def recordFormalResult (state : State) (key toolIdentity : String)
    (oracleArtifact : Option String)
    (checkedClosure checkedArtifacts : List String)
    (conformancePassed : Option Bool)
    (semanticPreview previewIdentity : String) : Except String State := do
  let candidates := selectedFormalSpecs state key
  let spec ← match candidates with
    | [spec] => pure spec
    | [] => throw "No current formal assurance has that name."
    | _ => throw "The formal assurance name is ambiguous."
  let result : Evidence.FormalResult :=
    { spec
      toolIdentity
      checkedClosure
      checkedArtifacts
      oracleArtifact
      conformancePassed
      semanticPreview
      previewIdentity }
  if !spec.wellFormed then
    throw "The selected formal scope is invalid."
  if toolIdentity.isEmpty then
    throw "The formal tool identity is missing."
  if semanticPreview.isEmpty || previewIdentity.isEmpty then
    throw "The formal meaning preview identity is missing."
  if !spec.modules.all checkedClosure.contains then
    throw "The checked module closure does not contain every selected module."
  if checkedArtifacts.isEmpty ||
      !checkedArtifacts.all (fun artifact => !artifact.isEmpty) then
    throw "The checked formal artifact identity is incomplete."
  match spec.oracle, oracleArtifact, spec.adapter, conformancePassed with
  | none, none, none, none => pure ()
  | some _, some artifact, none, none =>
      if artifact.isEmpty then throw "The checked oracle identity is missing."
  | some _, some artifact, some _, some _ =>
      if artifact.isEmpty then
        throw "The checked oracle identity is missing."
  | _, _, _, _ =>
      throw "The formal result does not match the selected assurance method."
  if state.formalResults.contains result then
    return state
  return { state with formalResults := state.formalResults ++ [result] }

private def designReviewArtifacts? (state : State)
    (candidate : Design.Item) : Option (List String) :=
  let formalKeys := candidate.assurance.obligations.filterMap fun obligation =>
    if obligation.method == .formal then some obligation.key else none
  if formalKeys.isEmpty then
    some [candidate.statement]
  else
    let results := formalKeys.filterMap fun key =>
      state.formalResults.reverse.find? fun result =>
        result.spec.key == key &&
          result.spec.design == candidate.ref &&
          result.currentFor result.spec [candidate.ref]
    if results.length != formalKeys.length then
      none
    else
      some <| results.flatMap fun result =>
        [result.previewIdentity, result.toolIdentity] ++
          result.oracleArtifact.toList ++ result.checkedArtifacts

def recordDesign (state : State) (source : Source) (key statement : String)
    (role : Design.Role) (assurance : Design.AssuranceSelection)
    (dependencyKeys : List String := []) (addsComplexity : Bool := false) :
    Except String State := do
  if key.isEmpty || statement.isEmpty || source.id.value.isEmpty then
    throw "The design statement is incomplete."
  let version :=
    (state.design.designItems.filter (·.ref.key == key)).foldl
      (fun next item => max next (item.ref.version + 1)) 0
  let predecessor :=
    state.design.designItems
      |>.filter (·.ref.key == key)
      |>.foldl
        (fun current item =>
          match current with
          | none => some item.ref
          | some selected =>
              if item.ref.version > selected.version then some item.ref
              else current)
        none
  let dependencies :=
    (currentDesignItems state).filterMap fun item =>
      if dependencyKeys.contains item.ref.key && item.ref.key != key then
        some item.ref
      else
        none
  if dependencies.length !=
      (dependencyKeys.filter (· != key)).eraseDups.length then
    throw "One or more design dependencies are not accepted and current."
  let item : Design.Item :=
    { ref := { key, version }
      predecessor
      statement
      role
      source
      dependencies
      assurance
      addsComplexity
      authority := .unaccepted }
  let effect : Design.Effect := { source, content := .design item }
  if !effect.wellFormed then
    throw "The design statement is invalid."
  pure
    { state with
      design := { effects := state.design.effects ++ [effect] } }

def acceptDesign (state : State) (key : String)
    (decision : CallerDecision)
    (complexity : Option Design.ComplexityRationale := none) :
    Except String State := do
  let candidate ← match
      (state.design.designItems.filter (·.ref.key == key)).reverse.head? with
    | some item =>
        match item.authority with
        | .unaccepted => pure item
        | .acceptedByCaller _ =>
            throw "That design statement is already accepted."
        | .retiredByCaller _ =>
            throw "That design statement is caller-retired."
    | none => throw "No design statement has that name."
  let meaningReviewed :=
    (designReviewArtifacts? state candidate).any fun artifacts =>
      state.reviewRequests.any fun request =>
        request.scope.purpose == .designMeaning &&
          request.scope.design == [candidate.ref] &&
          request.scope.artifacts == artifacts &&
          reviewResolved state request.ref
  if !meaningReviewed then
    throw "The proposed design meaning requires a current resolved review before caller acceptance."
  let formalReady :=
    candidate.assurance.obligations.all fun obligation =>
      if obligation.method == .formal then
        state.formalSpecs.any fun spec =>
          spec.key == obligation.key &&
            spec.design == candidate.ref &&
            state.formalResults.any fun result =>
              result.spec == spec &&
                result.currentFor spec [candidate.ref]
      else
        true
  if !formalReady then
    throw "The proposed formal design requires a current verified formal result before caller acceptance."
  let accepted : Design.Item :=
    { candidate with
      complexityRationale := complexity
      authority := .acceptedByCaller decision }
  if !accepted.wellFormed then
    throw "The caller acceptance is invalid."
  pure
    { state with
      design :=
        { effects := state.design.effects.map fun effect =>
            match effect.content with
            | .design item =>
                if item.ref == candidate.ref then
                  { effect with content := .design accepted }
                else
                  effect
            | _ => effect } }

def retireDesign (state : State) (key : String)
    (decision : CallerDecision) : Except String State := do
  let current ← match (currentDesignItems state).filter (·.ref.key == key) with
    | [item] => pure item
    | [] => throw "No current accepted design statement has that name."
    | _ => throw "The current design statement name is ambiguous."
  if !decision.wellFormed then
    throw "The caller retirement decision is incomplete."
  let retired : Design.Item :=
    { ref := { key, version := current.ref.version + 1 }
      predecessor := some current.ref
      statement := current.statement
      role := current.role
      source := decision.source
      dependencies := []
      assurance := { kind := .none, obligations := [] }
      authority := .retiredByCaller decision }
  let effect : Design.Effect :=
    { source := decision.source, content := .design retired }
  if !effect.wellFormed then
    throw "The design retirement is invalid."
  pure
    { state with
      design := { effects := state.design.effects ++ [effect] } }

def recordInstruction (state : State) (decision : CallerDecision)
    (statement : String) : Except String State := do
  let instruction : Design.OperatingInstruction :=
    { source := decision.source, statement, authority := decision }
  let effect : Design.Effect :=
    { source := decision.source, content := .instruction instruction }
  if !effect.wellFormed then
    throw "The caller instruction is invalid."
  pure
    { state with
      design := { effects := state.design.effects ++ [effect] } }

def recordNonAuthoritative (state : State) (source : Source)
    (kind : Design.NonAuthoritativeKind) (statement : String)
    (target : Option String := none) : Except String State := do
  let effect : Design.Effect :=
    { source
      content := .nonAuthoritative { kind, statement, target } }
  if !effect.wellFormed then
    throw "The contextual design record is invalid."
  pure
    { state with
      design := { effects := state.design.effects ++ [effect] } }

def requestReview (state : State) (key artifact : String)
    (purpose : Review.Purpose) : Except String State := do
  if key.isEmpty || artifact.isEmpty then
    throw "The review request is incomplete."
  let version :=
    (state.reviewRequests.filter (·.ref.key == key)).foldl
      (fun next request => max next (request.ref.version + 1)) 0
  let reviewRef : ReviewRef := { key, version }
  let selectedTask := state.focus.task.bind fun selected =>
    state.tasks.find? fun task =>
      task.ref == selected && taskCurrent state task
  let designScope :=
    match selectedTask with
    | some task => task.designScope.map (·.ref)
    | none => []
  let (revised, work) ← reviseFocusedWork state fun selected boundary =>
    let retained := boundary.filter fun member =>
      match member.target with
      | .reviewResolved prior => prior.key != key
      | _ => true
    { target := .reviewResolved reviewRef
      basis := .workBoundary selected } :: retained
  let request : Review.Request :=
    { ref := reviewRef
      scope :=
        { work
          design := designScope
          task := state.focus.task
          purpose
          artifacts := [artifact] } }
  if !request.scope.wellFormed then
    throw "The review scope is invalid."
  return { revised with reviewRequests := revised.reviewRequests ++ [request] }

def requestDesignReview (state : State) (key designKey : String) :
    Except String State := do
  if key.isEmpty || designKey.isEmpty then
    throw "The design review request is incomplete."
  let candidate ← match
      state.design.designItems.reverse.find? (·.ref.key == designKey) with
    | none => throw "No proposed design statement has that name."
    | some item =>
        match item.authority with
        | .unaccepted => pure item
        | .acceptedByCaller _ =>
            throw "That design statement is already caller-accepted."
        | .retiredByCaller _ =>
            throw "That design statement is caller-retired."
  let artifacts ← match designReviewArtifacts? state candidate with
    | some artifacts => pure artifacts
    | none =>
        throw "Run the proposed design's selected formal assurance before requesting its review."
  let version :=
    (state.reviewRequests.filter (·.ref.key == key)).foldl
      (fun next request => max next (request.ref.version + 1)) 0
  let reviewRef : ReviewRef := { key, version }
  let (revised, work) ← reviseFocusedWork state fun selected boundary =>
    let retained := boundary.filter fun member =>
      match member.target with
      | .reviewResolved prior => prior.key != key
      | _ => true
    { target := .reviewResolved reviewRef
      basis := .workBoundary selected } :: retained
  let request : Review.Request :=
    { ref := reviewRef
      scope :=
        { work
          design := [candidate.ref]
          task := none
          purpose := .designMeaning
          artifacts } }
  if !request.scope.wellFormed then
    throw "The design review scope is invalid."
  return { revised with reviewRequests := revised.reviewRequests ++ [request] }

def recordReviewResult (state : State) (reviewKey reviewer : String)
    (observation : Review.Observation) : Except String State := do
  let requests := state.reviewRequests.filter fun request =>
    request.ref.key == reviewKey && reviewRequestCurrent state request
  let request ← match requests with
    | [request] => pure request
    | [] => throw "No current review has that name."
    | _ => throw "The review name is ambiguous."
  let existing := state.reviewResults.find? (·.review == request.ref)
  let result : Review.Result ← match existing with
    | none =>
        pure
          { review := request.ref
            scope := request.scope
            reviewer
            observations := [observation] }
    | some result =>
        if result.reviewer != reviewer then
          throw "Additional observations must come from the same reviewer."
        if result.observations.any (·.key == observation.key) then
          throw "That review observation already exists."
        pure { result with observations := result.observations ++ [observation] }
  if !result.exactFor request then
    throw "The review result does not match its requested scope."
  let results :=
    match existing with
    | none => state.reviewResults ++ [result]
    | some prior =>
        state.reviewResults.map fun candidate =>
          if candidate == prior then result else candidate
  pure { state with reviewResults := results }

def recordCleanReview (state : State) (reviewKey reviewer : String) :
    Except String State := do
  let requests := state.reviewRequests.filter fun request =>
    request.ref.key == reviewKey && reviewRequestCurrent state request
  let request ← match requests with
    | [request] => pure request
    | [] => throw "No current review has that name."
    | _ => throw "The review name is ambiguous."
  if state.reviewResults.any (·.review == request.ref) then
    throw "A result already exists for that review scope."
  let result : Review.Result :=
    { review := request.ref
      scope := request.scope
      reviewer
      observations := [] }
  if !result.exactFor request then
    throw "The clean review result does not match its requested scope."
  pure { state with reviewResults := state.reviewResults ++ [result] }

def recordReviewDisposition (state : State) (reviewKey observationKey : String)
    (decision : Review.Decision) (caller : CallerDecision)
    (successor : Option Design.AcceptedRef := none)
    (complexity : Option Review.ComplexityRationale := none) :
    Except String State := do
  let results := state.reviewResults.filter fun result =>
    result.review.key == reviewKey &&
      state.reviewRequests.any fun request =>
        request.ref == result.review && reviewRequestCurrent state request
  let result ← match results with
    | [result] => pure result
    | [] => throw "No current review result has that name."
    | _ => throw "The review name is ambiguous."
  let observation ← match result.observations.find? (·.key == observationKey) with
    | some observation => pure observation
    | none => throw "That review observation does not exist."
  let disposition : Review.Disposition :=
    { review := result.review
      observation := observation.key
      decision
      caller
      successorDesign := successor
      complexity }
  if !disposition.wellFormedFor observation then
    throw "The caller disposition is incomplete for that observation."
  pure
    { state with
      reviewDispositions := state.reviewDispositions ++ [disposition] }

def adoptReviewProposal (state : State) (reviewKey observationKey
    successorKey : String) (caller : CallerDecision)
    (complexity : Option Review.ComplexityRationale := none) :
    Except String State := do
  let state ← acceptDesign state successorKey caller complexity
  let successors :=
    (currentDesignItems state).filterMap fun item =>
      if item.ref.key == successorKey then item.acceptedRef? else none
  let successor ← match successors with
    | [successor] => pure successor
    | [] => throw "The adopted successor design is not accepted and current."
    | _ => throw "The successor design name is ambiguous."
  recordReviewDisposition state reviewKey observationKey .accepted caller
    (some successor) complexity

inductive InterruptResult
  | accepted (state : State)
  | callerDecision (reason : String)
  | invalid (reason : String)
deriving DecidableEq, Repr, BEq

private def prepareWork (state : State) (outcome taskDescription : String)
    (authority : CallerDecision) :
    Except String (State × WorkRef × TaskRef) := do
  if outcome.isEmpty || taskDescription.isEmpty then
    throw "An outcome and its first task are required."
  let workRef : WorkRef :=
    { key := s!"outcome-{state.work.length + 1}", version := 0 }
  let taskRef : TaskRef :=
    { key := s!"task-{state.tasks.length + 1}", version := 0 }
  let work : Work.Unit :=
    { ref := workRef
      outcome
      completionBoundary :=
        [{ target := .taskSatisfied taskRef
           basis := .workBoundary workRef }]
      authority }
  let task : Work.Task :=
    { ref := taskRef
      work := workRef
      description := taskDescription
      basis := .workBoundary workRef
      designScope := []
      phase := none
      state := .pending }
  if !work.wellFormed || !task.wellFormed then
    throw "The new outcome is invalid."
  pure
    ({ state with
        work := state.work ++ [work]
        tasks := state.tasks ++ [task] },
      workRef, taskRef)

def startWork (state : State) (outcome taskDescription : String)
    (authority : CallerDecision) : Except String State := do
  if state.focus.returnPoint.isSome then
    throw "Return to the saved outcome before starting an independent outcome."
  let (prepared, work, task) ←
    prepareWork state outcome taskDescription authority
  pure
    { prepared with
      focus := { work, task := some task, returnPoint := none } }

def switchWork (state : State) (outcome : String) : Except String State := do
  if state.focus.returnPoint.isSome then
    throw "Return to the saved outcome before switching outcomes."
  let selected ← match state.work.filter fun work =>
      work.outcome == outcome && workCurrent state work.ref with
    | [work] => pure work
    | [] => throw "No current outcome has that description."
    | _ => throw "The outcome description is ambiguous."
  let selectedTask :=
    state.tasks.reverse.find? fun task =>
      task.work.key == selected.ref.key && taskCurrent state task
  pure
    { state with
      focus :=
        { work := selected.ref
          task := selectedTask.map (·.ref)
          returnPoint := none } }

def interrupt (state : State) (nextWork : WorkRef)
    (nextTask : Option TaskRef) : InterruptResult :=
  if !workCurrent state nextWork then
    .invalid "The interrupting outcome is not current."
  else
    match state.focus.returnPoint with
    | some _ =>
        .callerDecision
          "Return to the saved outcome first, or explicitly replace the return plan."
    | none =>
        let selectedTask : Option Work.Task :=
          match state.focus.task with
          | none => none
          | some selected =>
              state.tasks.find? fun task =>
                task.ref == selected && taskCurrent state task
        let selectedDesign :=
          selectedTask.map (fun (task : Work.Task) =>
            task.designScope.map (·.ref))
            |>.getD []
        let point : Work.ReturnPoint :=
          { work := state.focus.work
            task := state.focus.task
            assumptions :=
              .workBoundary state.focus.work ::
                selectedDesign.map .design }
        if point.wellFormed then
          .accepted
            { state with
              focus :=
                { work := nextWork
                  task := nextTask
                  returnPoint := some point } }
        else
          .invalid "The current return point is incomplete."

def startInterruption (state : State) (outcome taskDescription : String)
    (authority : CallerDecision) : Except String State := do
  let (prepared, workRef, taskRef) ←
    prepareWork state outcome taskDescription authority
  match interrupt prepared workRef (some taskRef) with
  | .accepted interrupted => pure interrupted
  | .callerDecision reason | .invalid reason => throw reason

def recordSourceEffects (state : State) (source : Source)
    (designKey : Option String) (statement : Option String)
    (role : Design.Role) (assurance : Design.AssuranceSelection)
    (dependencyKeys : List String) (instruction question : Option String)
    (work : Option (String × String)) : Except String State := do
  if designKey.isNone && instruction.isNone && question.isNone && work.isNone then
    throw "At least one source effect is required."
  let state ← match designKey, statement with
    | some key, some text =>
        recordDesign state source key text role assurance dependencyKeys
    | none, none => pure state
    | _, _ => throw "A design key and statement must be provided together."
  let decision : CallerDecision :=
    { source, reason := "This instruction was stated by the caller." }
  let state ← match instruction with
    | some text => recordInstruction state decision text
    | none => pure state
  let state ← match question with
    | some text => recordNonAuthoritative state source .question text
    | none => pure state
  match work with
  | some (outcome, task) =>
      startWork state outcome task
        { source, reason := "Pursue the caller-stated outcome." }
  | none => pure state

inductive ReturnResult
  | accepted (state : State)
  | replanRequired (changed : List Work.ReturnAssumption)
  | invalid (reason : String)
deriving DecidableEq, Repr, BEq

def returnFromInterruption (state : State) : ReturnResult :=
  match state.focus.returnPoint with
  | none => .invalid "No interruption return point is pending."
  | some point =>
      let changed := point.assumptions.filter fun assumption =>
        match assumption with
        | .design item => !(currentDesignRefs state).contains item
        | .workBoundary work => !workCurrent state work
      if changed.isEmpty then
        .accepted
          { state with
            focus :=
              { work := point.work
                task := point.task
                returnPoint := none } }
      else
        .replanRequired changed

def replanReturnByOutcome (state : State) (outcome : String)
    (caller : CallerDecision) : Except String State := do
  if !caller.wellFormed then
    throw "The return replan requires a caller decision."
  if state.focus.returnPoint.isNone then
    throw "No interruption return point is pending."
  let selected ← match state.work.filter fun work =>
      work.outcome == outcome && workCurrent state work.ref with
    | [work] => pure work
    | [] => throw "No current outcome has that description."
    | _ => throw "The outcome description is ambiguous."
  let selectedTask :=
    state.tasks.reverse.find? fun task =>
      task.work.key == selected.ref.key && taskCurrent state task
  pure
    { state with
      focus :=
        { work := selected.ref
          task := selectedTask.map (·.ref)
          returnPoint := none } }

structure ReviewTargetCorrection where
  mistaken : ReviewRef
  intended : Review.Request
  caller : CallerDecision
deriving DecidableEq, Repr, BEq

inductive ReviewCorrectionResult
  | accepted (state : State)
  | invalid (reason : String)
deriving DecidableEq, Repr, BEq

private def selectsReview (review : ReviewRef) (work : Work.Unit) : Bool :=
  work.completionBoundary.any fun member =>
    match member.target with
    | .reviewResolved selected => selected == review
    | _ => false

private def withoutReview (review : ReviewRef)
    (boundary : List Work.CompletionMember) : List Work.CompletionMember :=
  boundary.filter fun member =>
    match member.target with
    | .reviewResolved selected => selected != review
    | _ => true

def correctReviewTarget (state : State)
    (correction : ReviewTargetCorrection) : ReviewCorrectionResult :=
  match state.reviewRequests.find? (·.ref == correction.mistaken) with
  | none => .invalid "The mistaken review does not exist."
  | some mistaken =>
      let intendedWork := correction.intended.scope.work
      let erroneousWork :=
        state.work.filter fun work =>
          workCurrent state work.ref && selectsReview mistaken.ref work
      if !correction.caller.wellFormed then
        .invalid "The correction requires a caller decision."
      else if !reviewRequestCurrent state mistaken then
        .invalid "The mistaken review is not current."
      else if !correction.intended.scope.wellFormed ||
          !workCurrent state intendedWork then
        .invalid "The intended review scope is not current."
      else if correction.intended.ref == correction.mistaken ||
          state.reviewRequests.any (·.ref == correction.intended.ref) then
        .invalid "The intended review identity is not new."
      else if erroneousWork.isEmpty then
        .invalid "The mistaken review is not selected by a current outcome."
      else
        let touched :=
          state.work.filter fun work =>
            workCurrent state work.ref &&
              (selectsReview mistaken.ref work || work.ref == intendedWork)
        let successors := touched.map fun work =>
          let nextRef : WorkRef :=
            { key := work.ref.key, version := work.ref.version + 1 }
          let withoutMistake := withoutReview mistaken.ref work.completionBoundary
          let boundary :=
            if work.ref == intendedWork then
              { target := .reviewResolved correction.intended.ref
                basis := .workBoundary nextRef } :: withoutMistake
            else
              withoutMistake
          { work with ref := nextRef, completionBoundary := boundary }
        match successors.find? (·.ref.key == intendedWork.key) with
        | none => .invalid "The intended outcome could not be revised."
        | some intendedSuccessor =>
            let intendedRequest :=
              { correction.intended with
                scope :=
                  { correction.intended.scope with
                    work := intendedSuccessor.ref } }
            let focus :=
              let currentWork :=
                match successors.find? (·.ref.key == state.focus.work.key) with
                | none => state.focus.work
                | some successor => successor.ref
              let returnPoint := state.focus.returnPoint.map fun point =>
                let returnedWork :=
                  match successors.find? (·.ref.key == point.work.key) with
                  | none => point.work
                  | some successor => successor.ref
                let assumptions := point.assumptions.map fun assumption =>
                  match assumption with
                  | .workBoundary work =>
                      match successors.find? (·.ref.key == work.key) with
                      | none => assumption
                      | some successor => .workBoundary successor.ref
                  | .design _ => assumption
                { point with work := returnedWork, assumptions }
              { state.focus with work := currentWork, returnPoint }
            .accepted
              { state with
                work := state.work ++ successors
                reviewRequests := state.reviewRequests ++ [intendedRequest]
                focus }

def correctReviewByOutcome (state : State) (mistakenKey
    intendedOutcome intendedTaskDescription intendedArtifact : String)
    (caller : CallerDecision) :
    Except String State := do
  if intendedTaskDescription.isEmpty || intendedArtifact.isEmpty then
    throw "The intended review task and artifact are required."
  let mistaken ← match state.reviewRequests.filter fun request =>
      request.ref.key == mistakenKey && reviewRequestCurrent state request with
    | [request] => pure request
    | [] => throw "No current mistaken review has that name."
    | _ => throw "The mistaken review name is ambiguous."
  let intendedWork ← match state.work.filter fun work =>
      work.outcome == intendedOutcome && workCurrent state work.ref with
    | [work] => pure work
    | [] => throw "No current outcome has that description."
    | _ => throw "The intended outcome description is ambiguous."
  let intendedTask ← match state.tasks.filter fun task =>
      task.work.key == intendedWork.ref.key &&
        task.description == intendedTaskDescription &&
        taskCurrent state task with
    | [task] => pure task
    | [] => throw "The intended outcome has no current task with that description."
    | _ => throw "The intended task description is ambiguous."
  let intendedKey := mistaken.ref.key
  let version := mistaken.ref.version + 1
  let intended : Review.Request :=
    { ref := { key := intendedKey, version }
      scope :=
        { mistaken.scope with
          work := intendedWork.ref
          task := some intendedTask.ref
          design := intendedTask.designScope.map (·.ref)
          artifacts := [intendedArtifact] } }
  match correctReviewTarget state { mistaken := mistaken.ref, intended, caller } with
  | .accepted corrected => pure corrected
  | .invalid reason => throw reason

inductive NextAction
  | satisfy (member : Work.CompletionMember)
  | returnToSavedWork
  | replanReturn (changed : List Work.ReturnAssumption)
  | cannotAdvance (reason : String)
deriving DecidableEq, Repr, BEq

def nextAction (state : State) : Option NextAction :=
  if !state.wellFormed then
    some (.cannotAdvance "The recorded project state is invalid.")
  else
    match (missingCompletion state state.focus.work).head? with
    | some member => some (.satisfy member)
    | none =>
        if !currentlyComplete state state.focus.work then
          some (.cannotAdvance "The current outcome has no valid completion boundary.")
        else
          match state.focus.returnPoint with
          | none => none
          | some point =>
              let changed := point.assumptions.filter fun assumption =>
                match assumption with
                | .design item => !(currentDesignRefs state).contains item
                | .workBoundary work => !workCurrent state work
              if changed.isEmpty then
                some .returnToSavedWork
              else
                some (.replanReturn changed)

end AgentWorkbench.Kernel
