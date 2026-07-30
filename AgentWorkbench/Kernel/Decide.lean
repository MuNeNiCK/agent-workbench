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
  commandProfiles : List CommandProfile.Profile
  commandDeviations : List CommandProfile.Deviation
  kpt : List KPT.Entry
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
    (state.evidenceSpecs.all fun spec =>
      spec.commandProfile.all fun selected =>
        state.commandProfiles.any fun profile =>
          profile.ref == selected &&
            match profile.authority with
            | .proposed => false
            | .acceptedByCaller _ => true) &&
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
    state.commandProfiles.all CommandProfile.Profile.wellFormed &&
    (state.commandProfiles.map (·.ref)).Nodup &&
    (state.commandProfiles.all fun profile =>
      (match profile.scope with
      | .project => true
      | .work key =>
          state.work.any (·.ref.key == key)) &&
      profile.predecessor.all fun selected =>
        state.commandProfiles.any fun prior =>
          prior.ref == selected && prior.scope == profile.scope) &&
    state.commandDeviations.all CommandProfile.Deviation.wellFormed &&
    (state.commandDeviations.all fun deviation =>
      state.commandProfiles.any fun profile =>
        profile.ref == deviation.profile &&
          profile.disposition == .recommended &&
          match profile.authority with
          | .proposed => false
          | .acceptedByCaller _ => true) &&
    (state.commandDeviations.all fun deviation =>
      deviation.evidence.all fun selected =>
        state.evidenceSpecs.any fun spec =>
          spec.ref == selected &&
            spec.commandProfile == some deviation.profile) &&
    (state.commandDeviations.all fun deviation =>
      deviation.evidence.all fun selected =>
        !state.evidenceResults.any (·.spec.ref == selected)) &&
    state.kpt.all KPT.Entry.wellFormed &&
    (state.kpt.map (·.ref)).Nodup &&
    (state.kpt.all fun entry =>
      (match entry.scope with
      | .project => true
      | .work key =>
          state.work.any (·.ref.key == key)) &&
      (entry.relation.all fun relation =>
        match relation with
        | .commandProfile ref =>
            state.commandProfiles.any (·.ref == ref)
        | .design ref =>
            state.design.designItems.any (·.ref == ref)
        | .task ref =>
            state.tasks.any (·.ref == ref)
        | .reviewObservation ref =>
            state.reviewResults.any fun result =>
              result.review == ref.review &&
                result.observations.any (·.key == ref.observation)
        | .evidenceResult ref =>
            state.evidenceResults.any fun result =>
              result.spec.ref == ref.evidence &&
                result.observedValue == ref.observedValue &&
                result.passed == ref.passed) &&
      entry.predecessor.all fun selected =>
        state.kpt.any fun prior =>
          prior.ref == selected && prior.scope == entry.scope) &&
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

private def formalDesignSelectable (state : State) (assuranceKey : String)
    (design : DesignRef) : Bool :=
  (currentDesignRefs state).contains design ||
    (state.design.designItems.reverse.find? (·.ref.key == design.key)
      |>.any fun item =>
        item.ref == design &&
          item.authority == .unaccepted &&
          item.assurance.obligations.any fun obligation =>
              obligation.key == assuranceKey &&
              obligation.method == .formal)

def formalSpecCurrent (state : State) (spec : Evidence.FormalSpec) : Bool :=
  state.formalSpecs.reverse.find? (fun candidate =>
    candidate.key == spec.key && candidate.design == spec.design)
    |>.any (· == spec)

def selectedFormalSpecs (state : State) (key : String) :
    List Evidence.FormalSpec :=
  state.formalSpecs.filter fun spec =>
    spec.key == key &&
      formalSpecCurrent state spec &&
      formalDesignSelectable state key spec.design

def selectedFormalSpecsForDesign (state : State) (key designKey : String) :
    List Evidence.FormalSpec :=
  let candidates :=
    (selectedFormalSpecs state key).filter (·.design.key == designKey)
  let selectedAccepted :=
    state.work.find? (·.ref == state.focus.work) |>.bind fun work =>
      work.completionBoundary.findSome? fun member =>
        match member.target, member.basis with
        | .assurance selectedKey, .design [accepted] =>
            if selectedKey == key && accepted.ref.key == designKey then
              some accepted.ref
            else
              none
        | _, _ => none
  match selectedAccepted with
  | some design => candidates.filter (·.design == design)
  | none =>
      let accepted := candidates.filter fun spec =>
        (currentDesignRefs state).contains spec.design
      if !accepted.isEmpty then accepted
      else
        let latestUnaccepted :=
          state.design.designItems.reverse.find? fun item =>
            item.ref.key == designKey &&
              item.authority == .unaccepted &&
              item.assurance.obligations.any fun obligation =>
                obligation.key == key && obligation.method == .formal
        match latestUnaccepted with
        | some item => candidates.filter (·.design == item.ref)
        | none => []

def selectedFormalSpecsForCompletion (state : State) (key : String) :
    List Evidence.FormalSpec :=
  match state.work.find? (·.ref == state.focus.work) with
  | none => []
  | some work =>
      work.completionBoundary.filterMap fun member =>
        match member.target, member.basis with
        | .assurance selectedKey, .design [accepted] =>
            if selectedKey != key then none
            else
              state.formalSpecs.reverse.find? fun spec =>
                spec.key == key && spec.design == accepted.ref &&
                  formalSpecCurrent state spec &&
                  formalDesignSelectable state key spec.design
        | _, _ => none

def selectedFormalSpecsForPreview (state : State) (key designKey : String) :
    List Evidence.FormalSpec :=
  let candidates :=
    (selectedFormalSpecs state key).filter (·.design.key == designKey)
  let latestUnaccepted :=
    state.design.designItems.reverse.find? fun item =>
      item.ref.key == designKey &&
        item.authority == .unaccepted &&
        item.assurance.obligations.any fun obligation =>
          obligation.key == key && obligation.method == .formal
  match latestUnaccepted with
  | some item => candidates.filter (·.design == item.ref)
  | none => candidates

def latestFormalResultForSpec? (state : State)
    (spec : Evidence.FormalSpec) : Option Evidence.FormalResult :=
  state.formalResults.reverse.find? fun result =>
    result.currentFor spec [spec.design]

def formalResultsRequiringVerification (state : State) :
  List Evidence.FormalResult :=
  state.formalSpecs.filterMap fun spec =>
    if formalSpecCurrent state spec &&
        formalDesignSelectable state spec.key spec.design then
      latestFormalResultForSpec? state spec
    else
      none

def workCurrent (state : State) (work : WorkRef) : Bool :=
  state.work.any (·.ref == work) &&
    !state.work.any fun candidate =>
      candidate.ref.key == work.key && candidate.ref.version > work.version

def commandProfileCurrent (state : State)
    (profile : CommandProfile.Profile) : Bool :=
  state.commandProfiles.any (·.ref == profile.ref) &&
    (match profile.authority with
    | .proposed => false
    | .acceptedByCaller _ => true) &&
    !state.commandProfiles.any fun candidate =>
      candidate.ref.key == profile.ref.key &&
        candidate.scope == profile.scope &&
        candidate.ref.version > profile.ref.version &&
        match candidate.authority with
        | .proposed => false
        | .acceptedByCaller _ => true

def memoryScopeApplicable (state : State) : MemoryScope → Bool
  | .project => true
  | .work key => key == state.focus.work.key

def commandProfileApplicable (state : State)
    (profile : CommandProfile.Profile) : Bool :=
  commandProfileCurrent state profile &&
    memoryScopeApplicable state profile.scope

def applicableCommandProfiles (state : State) (purpose : String) :
    List CommandProfile.Profile :=
  state.commandProfiles.filter fun profile =>
    profile.purpose == purpose && commandProfileApplicable state profile

def currentCallerKPT (state : State) : List KPT.Entry :=
  state.kpt.filter fun entry =>
    (match entry.authority with
    | .nonAuthoritative => false
    | .callerOwned _ => true) &&
    !state.kpt.any fun successor =>
      successor.ref.key == entry.ref.key &&
        successor.scope == entry.scope &&
        successor.ref.version > entry.ref.version &&
        match successor.authority with
        | .nonAuthoritative => false
        | .callerOwned _ => true

def currentKPT (state : State) : List KPT.Entry :=
  let caller := currentCallerKPT state
  let authored := state.kpt.filter fun entry =>
    entry.authority == .nonAuthoritative &&
      !(caller.any fun owned =>
        owned.ref.key == entry.ref.key && owned.scope == entry.scope) &&
      !(state.kpt.any fun successor =>
          successor.ref.key == entry.ref.key &&
          successor.scope == entry.scope &&
          successor.author == entry.author &&
          successor.authority == .nonAuthoritative &&
          successor.ref.version > entry.ref.version)
  caller ++ authored

def relevantKPT (state : State) : List KPT.Entry :=
  (currentKPT state).filter fun entry =>
    memoryScopeApplicable state entry.scope

def pendingCommandProfileProposals (state : State) :
    List CommandProfile.Profile :=
  state.commandProfiles.filter fun profile =>
    profile.authority == .proposed &&
      memoryScopeApplicable state profile.scope &&
      !state.commandProfiles.any fun successor =>
        successor.ref.key == profile.ref.key &&
          successor.scope == profile.scope &&
          successor.ref.version > profile.ref.version

def kptCandidateExactForCurrentCaller (state : State)
    (entry : KPT.Entry) : Bool :=
  entry.authority == .nonAuthoritative &&
    (currentCallerKPT state).any fun owned =>
      owned.ref.key == entry.ref.key &&
        owned.scope == entry.scope &&
        entry.predecessor == some owned.ref

def pendingKPTCandidates (state : State) : List KPT.Entry :=
  state.kpt.filter fun entry =>
    kptCandidateExactForCurrentCaller state entry &&
      memoryScopeApplicable state entry.scope &&
      !(state.kpt.any fun successor =>
          successor.ref.key == entry.ref.key &&
          successor.scope == entry.scope &&
          successor.author == entry.author &&
          successor.authority == .nonAuthoritative &&
          successor.ref.version > entry.ref.version) &&
      !(state.kpt.any fun successor =>
        successor.predecessor == some entry.ref &&
          match successor.authority with
          | .nonAuthoritative => false
          | .callerOwned _ => true)

def taskCurrent (state : State) (task : Work.Task) : Bool :=
  state.tasks.any (·.ref == task.ref) &&
    !(state.tasks.any fun candidate =>
      candidate.ref.key == task.ref.key &&
        candidate.ref.version > task.ref.version) &&
    task.designScope.all fun item =>
      (currentDesignRefs state).contains item.ref

def sameEvidenceBasisLineage (prior next : Work.DerivationBasis) : Bool :=
  match prior, next with
  | .design prior, .design next =>
      prior.map (·.ref.key) == next.map (·.ref.key)
  | .workBoundary prior, .workBoundary next => prior.key == next.key
  | _, _ => false

def evidenceSpecLatestInBasis (state : State) (spec : Evidence.Spec) : Bool :=
  state.evidenceSpecs.any (·.ref == spec.ref) &&
    !(state.evidenceSpecs.any fun candidate =>
      candidate.ref.key == spec.ref.key &&
        sameEvidenceBasisLineage spec.basis candidate.basis &&
        candidate.ref.version > spec.ref.version)

def evidenceSpecCurrent (state : State) (spec : Evidence.Spec) : Bool :=
  evidenceSpecLatestInBasis state spec &&
    spec.commandProfile.all fun selected =>
      state.commandProfiles.find? (·.ref == selected)
        |>.any (commandProfileCurrent state)

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
        candidate.scope.work.key == request.scope.work.key &&
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

def selectedAssuranceForBasis? (state : State) (key : String)
    (basis : Work.DerivationBasis) : Option Evidence.AssuranceSpec :=
  (Evidence.selectedAssurance (currentDesignItems state)).find? fun assurance =>
    assurance.key == key && assurance.basis == basis

def assuranceSatisfiedForBasis (state : State) (key : String)
    (basis : Work.DerivationBasis)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) : Bool :=
  (selectedAssuranceForBasis? state key basis).any fun assurance =>
    basis.wellFormed &&
      match assurance.method with
      | .formal =>
          match basis with
          | .design [selected] =>
              (state.formalSpecs.reverse.find? fun spec =>
                spec.key == key && spec.design == selected.ref &&
                  formalSpecCurrent state spec)
                |>.bind (latestFormalResultForSpec? state)
                |>.any fun result =>
                  !staleFormalResultIdentities.contains result.identity &&
                    result.conformsFor result.spec (currentDesignRefs state)
          | _ => false
      | .evidence =>
          state.evidenceResults.any fun result =>
            result.spec.ref.key == key &&
              result.spec.basis == basis &&
              result.passed &&
              evidenceResultCurrent state result

def assuranceSatisfied (state : State) (key : String)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) : Bool :=
  (Evidence.selectedAssurance (currentDesignItems state)).any fun assurance =>
    assurance.key == key &&
      assuranceSatisfiedForBasis state key assurance.basis
        staleFormalResultIdentities

def reviewResolved (state : State) (review : ReviewRef) : Bool :=
  match state.reviewRequests.find? (·.ref == review) with
  | none => false
  | some request =>
      reviewRequestCurrent state request &&
        state.reviewResults.any fun result =>
          result.resolvedBy request state.reviewDispositions

def completionMemberSatisfied (state : State) (work : WorkRef)
    (member : Work.CompletionMember)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) : Bool :=
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
    | .assurance key =>
        assuranceSatisfiedForBasis state key member.basis
          staleFormalResultIdentities
    | .reviewResolved review => reviewResolved state review
    | .externalObservation evidence =>
        state.evidenceResults.any fun result =>
          result.spec.ref == evidence &&
            result.passed &&
            evidenceResultCurrent state result

def missingCompletion (state : State) (work : WorkRef)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) :
    List Work.CompletionMember :=
  match state.work.find? (·.ref == work) with
  | none => []
  | some selected =>
      if workCurrent state selected.ref && selected.wellFormed then
        selected.completionBoundary.filter fun member =>
          !completionMemberSatisfied state selected.ref member
            staleFormalResultIdentities
      else
        []

def currentlyComplete (state : State) (work : WorkRef)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) : Bool :=
  workCurrent state work &&
    (state.work.find? (·.ref == work)).any fun selected =>
      selected.wellFormed &&
        (missingCompletion state work staleFormalResultIdentities).isEmpty

def rebaseBoundary (prior next : WorkRef)
    (member : Work.CompletionMember) : Work.CompletionMember :=
  match member.basis with
  | .workBoundary selected =>
      if selected == prior then
        { member with basis := .workBoundary next }
      else
        member
  | .design _ => member

def reviseFocusedWork (state : State)
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

def completionMemberReplacedByDesigns
    (selected : List Design.AcceptedRef)
    (member : Work.CompletionMember) : Bool :=
  match member.target, member.basis with
  | .assurance _, .design current =>
      current.any fun item =>
        selected.any (·.ref.key == item.ref.key)
  | _, _ => false

def retainUnreplacedCompletionMembers
    (current : List Work.CompletionMember)
    (selected : List Design.AcceptedRef) :
    List Work.CompletionMember :=
  current.filter fun member =>
    !completionMemberReplacedByDesigns selected member

def completionBoundaryForAddedTask (state : State) (selected : WorkRef)
    (boundary : List Work.CompletionMember) (taskRef : TaskRef)
    (designScope : List Design.AcceptedRef)
    (basis : Work.DerivationBasis) : List Work.CompletionMember :=
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
  let retained :=
    retainUnreplacedCompletionMembers currentBoundary designScope
  { target := .taskSatisfied taskRef
    basis :=
      match basis with
      | .workBoundary _ => .workBoundary selected
      | .design items => .design items } :: assuranceMembers ++ retained

def reviseFocusedWorkForTask (state : State) (taskRef : TaskRef)
    (designScope : List Design.AcceptedRef)
    (basis : Work.DerivationBasis) : Except String (State × WorkRef) :=
  reviseFocusedWork state fun selected boundary =>
    completionBoundaryForAddedTask state selected boundary taskRef
      designScope basis

def taskForAddedDesign (taskRef : TaskRef) (work : WorkRef)
    (description : String) (basis : Work.DerivationBasis)
    (designScope : List Design.AcceptedRef) : Work.Task :=
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

def addTaskForDesign (state : State) (description : String)
    (designKeys : List String) : Except String State :=
  let key := s!"task-{state.tasks.length + 1}"
  let taskRef : TaskRef := { key, version := 0 }
  let designScope :=
    (currentDesignItems state).filterMap fun item =>
      if designKeys.contains item.ref.key then item.acceptedRef? else none
  let basis : Work.DerivationBasis :=
    if designScope.isEmpty then .workBoundary state.focus.work
    else .design designScope
  if description.isEmpty then
    .error "A task description is required."
  else if designScope.length != designKeys.eraseDups.length then
    .error "One or more selected design statements are not accepted and current."
  else
    match reviseFocusedWorkForTask state taskRef designScope basis with
    | .error message => .error message
    | .ok (revised, work) =>
        let task :=
          taskForAddedDesign taskRef work description basis designScope
        if !task.wellFormed then
          .error "The task is not valid for the current outcome."
        else
          .ok
            { revised with
              tasks := revised.tasks ++ [task]
              focus := { revised.focus with task := some taskRef } }

def addTask (state : State) (description : String) : Except String State :=
  addTaskForDesign state description []

def pendingTaskForRef? (state : State) (selected : TaskRef) :
    Option Work.Task :=
  state.tasks.find? fun task =>
    task.work.key == state.focus.work.key &&
      task.state == .pending &&
      taskCurrent state task &&
      task.ref == selected

def taskSelectedForFinish? (state : State) : Option Work.Task :=
  let selectedMissingTask :=
    (missingCompletion state state.focus.work).findSome? fun member =>
      match member.target with
      | .taskSatisfied task => some task
      | _ => none
  selectedMissingTask.bind fun selected =>
    pendingTaskForRef? state selected

def finishCurrentTask (state : State) : Except String State :=
  match taskSelectedForFinish? state with
  | none => .error "No task is pending for the current outcome."
  | some selected =>
      .ok
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

def nextCommandProfileRef (state : State) (key : String) :
    CommandProfileRef :=
  { key
    version :=
      (state.commandProfiles.filter (·.ref.key == key)).foldl
        (fun next profile => max next (profile.ref.version + 1)) 0 }

def latestAcceptedCommandProfile? (state : State) (key : String)
    (scope : MemoryScope) : Option CommandProfile.Profile :=
  state.commandProfiles.reverse.find? fun profile =>
    profile.ref.key == key && profile.scope == scope &&
      commandProfileCurrent state profile

def recordCommandProfile (state : State) (source : Source)
    (decision : Option CallerDecision) (key purpose : String)
    (scope : MemoryScope) (argv : List String) (cwd : Option String)
    (disposition : CommandProfile.Disposition) : Except String State := do
  let authority ← match decision with
    | some accepted =>
        if !accepted.wellFormed then
          throw "A valid caller decision is required."
        pure (CommandProfile.Authority.acceptedByCaller accepted)
    | none =>
        if source.kind == .caller then
          throw "A caller decision is required for an authoritative Command Profile."
        pure .proposed
  let predecessor :=
    (latestAcceptedCommandProfile? state key scope).map (·.ref)
  let profile : CommandProfile.Profile :=
    { ref := nextCommandProfileRef state key
      predecessor
      purpose
      scope
      argv
      cwd
      disposition
      source
      authority }
  if !profile.wellFormed then
    throw "The Command Profile is invalid."
  pure { state with commandProfiles := state.commandProfiles ++ [profile] }

def acceptCommandProfile (state : State) (key : String)
    (scope : MemoryScope) (decision : CallerDecision) : Except String State := do
  if !decision.wellFormed then
    throw "A valid caller decision is required."
  let candidate ← match
      (pendingCommandProfileProposals state).filter fun profile =>
        profile.ref.key == key && profile.scope == scope with
    | [profile] => pure profile
    | [] => throw "No proposed Command Profile matches that key and scope."
    | _ => throw "The proposed Command Profile key and scope are ambiguous."
  let accepted : CommandProfile.Profile :=
    { candidate with
      ref := nextCommandProfileRef state key
      predecessor :=
        (latestAcceptedCommandProfile? state key scope).map (·.ref)
          |>.orElse (fun _ => some candidate.ref)
      authority := .acceptedByCaller decision }
  if !accepted.wellFormed then
    throw "The accepted Command Profile is invalid."
  pure { state with commandProfiles := state.commandProfiles ++ [accepted] }

def recordCommandDeviation (state : State) (profileKey : String)
    (actualArgv : List String) (actualCwd : Option String)
    (reason : String) (source : Source)
    (evidenceKey : Option String := none)
    (profileScope : Option MemoryScope := none) : Except String State := do
  let candidates := state.commandProfiles.filter fun profile =>
    profile.ref.key == profileKey &&
      profileScope.all (· == profile.scope) &&
      commandProfileApplicable state profile
  let profile ← match candidates with
    | [selected] => pure selected
    | [] => throw "No accepted current Command Profile matches that key."
    | _ =>
        throw "The Command Profile is ambiguous; select an exact scoped profile."
  if profile.disposition != .recommended then
    throw "Only a recommended Command Profile records an agent-reasoned deviation."
  let pendingEvidence := state.evidenceSpecs.filter fun spec =>
    evidenceSpecCurrent state spec &&
      spec.commandProfile == some profile.ref &&
      evidenceKey.all (· == spec.ref.key) &&
      (match spec.basis with
      | .workBoundary selected => selected.key == state.focus.work.key
      | .design selected =>
          state.work.find? (·.ref == state.focus.work) |>.any fun work =>
            work.completionBoundary.any fun member =>
              member.target == .assurance spec.ref.key &&
                member.basis == .design selected) &&
      !state.evidenceResults.any fun result =>
        result.spec == spec && evidenceResultCurrent state result
  let selectedEvidence ← match pendingEvidence with
    | [] =>
        if evidenceKey.isSome then
          throw "No pending EvidenceSpec matches that Command Profile."
        pure none
    | [spec] => pure (some spec.ref)
    | _ =>
        throw "More than one pending EvidenceSpec uses that profile; select its Evidence key."
  let deviation : CommandProfile.Deviation :=
    { profile := profile.ref
      evidence := selectedEvidence
      actualArgv
      actualCwd
      reason
      source }
  if !deviation.wellFormed then
    throw "The Command Profile deviation is invalid."
  pure
    { state with
      commandDeviations := state.commandDeviations ++ [deviation] }

def nextKPTRef (state : State) (key : String) : KPTRef :=
  { key
    version :=
      (state.kpt.filter (·.ref.key == key)).foldl
        (fun next entry => max next (entry.ref.version + 1)) 0 }

def kptRelationProfileScope (state : State) :
    KPT.ProfileScopeSelector → MemoryScope
  | .project => .project
  | .focusedWork => .work state.focus.work.key

def resolveKPTRelation (state : State)
    (selector : KPT.RelationSelector) : Except String KPT.Relation :=
  match selector with
  | .commandProfile key selectedScope =>
      let scope := kptRelationProfileScope state selectedScope
      match state.commandProfiles.filter fun profile =>
          profile.ref.key == key && profile.scope == scope &&
            commandProfileCurrent state profile with
      | [profile] => .ok (.commandProfile profile.ref)
      | [] => .error "No current applicable Command Profile matches the KPT relation."
      | _ => .error "The KPT Command Profile relation is ambiguous."
  | .design key selectedAuthority =>
      let candidates : List Design.Item := match selectedAuthority with
        | .accepted =>
            (currentDesignItems state).filter fun item => item.ref.key == key
        | .candidate =>
            state.design.designItems.filter fun item =>
              item.ref.key == key &&
                !(state.design.designItems.any fun successor =>
                  successor.ref.key == key &&
                    successor.ref.version > item.ref.version) &&
                match item.authority with
                | .unaccepted => true
                | _ => false
      match candidates with
      | [item] => .ok (.design item.ref)
      | [] => .error "No current DesignItem matches the KPT relation."
      | _ => .error "The KPT DesignItem relation is ambiguous."
  | .task description =>
      match state.tasks.filter fun task =>
          task.description == description &&
            task.work.key == state.focus.work.key && taskCurrent state task with
      | [task] => .ok (.task task.ref)
      | [] => .error "No current Task matches the KPT relation."
      | _ => .error "The KPT Task relation is ambiguous."
  | .reviewObservation reviewKey observationKey =>
      let candidates := state.reviewResults.filterMap fun result =>
        let currentRequest :=
          state.reviewRequests.find? fun request =>
            request.ref == result.review && reviewRequestCurrent state request
        if result.review.key == reviewKey &&
            currentRequest.any
              (·.scope.work.key == state.focus.work.key) &&
            result.observations.any (·.key == observationKey) then
          some
            (KPT.Relation.reviewObservation
              { review := result.review, observation := observationKey })
        else
          none
      match candidates with
      | [relation] => .ok relation
      | [] => .error "No current Review observation matches the KPT relation."
      | _ => .error "The KPT Review observation relation is ambiguous."
  | .evidenceResult key selectedBasis =>
      let candidates := state.evidenceResults.filter fun result =>
        let basisMatches := match selectedBasis, result.spec.basis with
          | .focusedWork, .workBoundary work =>
              work.key == state.focus.work.key
          | .design designKey, .design items =>
              items.any (·.ref.key == designKey)
          | _, _ => false
        result.spec.ref.key == key && basisMatches &&
          evidenceResultCurrent state result
      match candidates with
      | [result] =>
          .ok <| .evidenceResult
            { evidence := result.spec.ref
              observedValue := result.observedValue
              passed := result.passed }
      | [] => .error "No current Evidence result matches the KPT relation."
      | _ => .error "The KPT Evidence result relation is ambiguous."

def selectKPTPredecessor (state : State) (author key : String)
    (scope : MemoryScope) (decision : Option CallerDecision)
    (predecessorAuthor : Option String) : Except String (Option KPTRef) :=
  match decision with
  | some _ =>
      let candidates := (currentKPT state).filter fun entry =>
        entry.ref.key == key && entry.scope == scope &&
          predecessorAuthor.all (· == entry.author)
      match candidates with
      | [] =>
          if predecessorAuthor.isSome then
            .error "No current KPT entry matches that predecessor author."
          else
            .ok none
      | [current] => .ok (some current.ref)
      | _ =>
          .error
            "The current KPT key and scope are ambiguous; select one exact predecessor author."
  | none =>
      match (currentCallerKPT state).reverse.find? fun entry =>
          entry.ref.key == key && entry.scope == scope with
      | some caller => .ok (some caller.ref)
      | none =>
          .ok <|
            state.kpt.reverse.find? (fun entry =>
              entry.ref.key == key && entry.scope == scope &&
                entry.author == author)
              |>.map (·.ref)

def authorityForKPTRecording (source : Source)
    (decision : Option CallerDecision)
    (predecessorAuthor : Option String) : Except String KPT.Authority :=
  match decision with
  | some accepted =>
      if !accepted.wellFormed then
        .error "A valid caller decision is required."
      else
        .ok (.callerOwned accepted)
  | none =>
      if source.kind == .caller then
        .error "A caller decision is required for caller-owned KPT."
      else if predecessorAuthor.isSome then
        .error "Only a caller-owned KPT may select a predecessor author."
      else
        .ok .nonAuthoritative

def resolveOptionalKPTRelation (state : State)
    (selector : Option KPT.RelationSelector) :
    Except String (Option KPT.Relation) :=
  match selector with
  | none => .ok none
  | some selected => some <$> resolveKPTRelation state selected

def recordKPTResolved (state : State) (source : Source) (author : String)
    (decision : Option CallerDecision) (key : String)
    (category : KPT.Category) (scope : MemoryScope) (statement : String)
    (relation : Option KPT.Relation)
    (predecessorAuthor : Option String := none) : Except String State := do
  let authority ←
    authorityForKPTRecording source decision predecessorAuthor
  let predecessor ←
    selectKPTPredecessor state author key scope decision predecessorAuthor
  let entry : KPT.Entry :=
    { ref := nextKPTRef state key
      predecessor
      category
      scope
      statement
      source
      author
      relation
      authority }
  if !entry.wellFormed then
    throw "The KPT entry is invalid."
  pure { state with kpt := state.kpt ++ [entry] }

def recordKPT (state : State) (source : Source) (author : String)
    (decision : Option CallerDecision) (key : String)
    (category : KPT.Category) (scope : MemoryScope) (statement : String)
    (relationSelector : Option KPT.RelationSelector := none)
    (predecessorAuthor : Option String := none) : Except String State := do
  let relation ← resolveOptionalKPTRelation state relationSelector
  recordKPTResolved state source author decision key category scope statement
    relation predecessorAuthor

def acceptKPT
    (state : State) (key : String) (scope : MemoryScope)
    (author : String)
    (decision : CallerDecision) : Except String State := do
  if !decision.wellFormed then
    throw "A valid caller decision is required."
  if author.isEmpty then
    throw "A stable KPT author is required."
  let candidate ← match (pendingKPTCandidates state).filter fun entry =>
      entry.ref.key == key && entry.scope == scope && entry.author == author with
    | [entry] => pure entry
    | [] =>
        throw
          "No agent-authored KPT candidate matches that key, scope, and author."
    | _ =>
        throw
          "The agent-authored KPT key, scope, and author are ambiguous."
  let accepted : KPT.Entry :=
    { candidate with
      ref := nextKPTRef state key
      predecessor := some candidate.ref
      authority := .callerOwned decision }
  if !accepted.wellFormed then
    throw "The adopted KPT entry is invalid."
  pure { state with kpt := state.kpt ++ [accepted] }

structure EvidenceRecordingSelection where
  revised : State
  ref : EvidenceRef
  basis : Work.DerivationBasis
  commandProfile : Option CommandProfileRef

def requiredProfileReplacementSelected (state : State) (key : String)
    (basis : Work.DerivationBasis)
    (commandProfile : Option CommandProfileRef) : Bool :=
  !(state.evidenceSpecs.any fun prior =>
      prior.ref.key == key &&
        sameEvidenceBasisLineage prior.basis basis &&
        evidenceSpecLatestInBasis state prior &&
        prior.commandProfile.any fun selected =>
          state.commandProfiles.find? (·.ref == selected)
            |>.any (·.disposition == .required)) ||
    commandProfile.isSome

def selectEvidenceRecordingCandidate
    (state : State) (key observation method environment : String)
    (acceptanceCondition trustedBoundary artifactIdentity : String)
    (designKey : Option String := none)
    (commandProfileKey : Option String := none)
    (commandProfileScope : Option MemoryScope := none)
    (commandProfileDecision : Option CallerDecision := none) :
    Except String EvidenceRecordingSelection := do
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
  let selectedAssurances := selectedWork.completionBoundary.filterMap fun member =>
    match member.target, member.basis with
    | .assurance selectedKey, .design accepted =>
        if selectedKey == key &&
            designKey.all fun selected =>
              accepted.any (·.ref.key == selected) then
          some accepted
        else
          none
    | _, _ => none
  let selectedAssurance ← match selectedAssurances with
    | [accepted] => pure (some accepted)
    | [] =>
        if designKey.isSome then
          throw "No selected Evidence obligation matches that Design."
        else
          pure none
    | _ =>
        throw "The Evidence obligation is ambiguous; select its Design key."
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
  let commandProfile ← match commandProfileKey, commandProfileDecision with
    | none, none => pure none
    | none, some _ =>
        throw "A Command Profile selection decision requires an exact profile."
    | some _, none =>
        throw "Selecting a Command Profile requires an explicit caller decision."
    | some selectedKey, some decision =>
        if !decision.wellFormed then
          throw "A valid caller decision is required to select a Command Profile."
        match revised.commandProfiles.filter fun profile =>
            profile.ref.key == selectedKey &&
              commandProfileScope.all (· == profile.scope) &&
              commandProfileApplicable revised profile with
        | [profile] => pure (some profile.ref)
        | [] =>
            throw "No accepted current Command Profile matches that key."
        | _ =>
            throw "The Command Profile is ambiguous; select an exact scoped profile."
  pure { revised, ref := evidenceRef, basis, commandProfile }

def selectEvidenceRecording
    (state : State) (key observation method environment : String)
    (acceptanceCondition trustedBoundary artifactIdentity : String)
    (designKey : Option String := none)
    (commandProfileKey : Option String := none)
    (commandProfileScope : Option MemoryScope := none)
    (commandProfileDecision : Option CallerDecision := none) :
    Except String EvidenceRecordingSelection := do
  let selected ←
    selectEvidenceRecordingCandidate state key observation method environment
      acceptanceCondition trustedBoundary artifactIdentity designKey
      commandProfileKey commandProfileScope commandProfileDecision
  if !requiredProfileReplacementSelected selected.revised key selected.basis
      selected.commandProfile then
    throw
      "A required Command Profile binding needs an explicit caller-selected exact replacement."
  pure selected

def evidenceSpecForRecording (selected : EvidenceRecordingSelection)
    (observation method environment : String) (inputs : List String)
    (acceptanceCondition trustedBoundary artifactIdentity : String)
    (commandProfileDecision : Option CallerDecision) : Evidence.Spec :=
  { ref := selected.ref
    observation := observation
    method := method
    environment := environment
    inputs := inputs
    acceptanceCondition := acceptanceCondition
    trustedBoundary := trustedBoundary
    artifactIdentity := artifactIdentity
    basis := selected.basis
    commandProfile := selected.commandProfile
    commandProfileDecision := commandProfileDecision }

def addEvidenceAfterSelectionValidation
    (state : State) (key observation method environment : String)
    (inputs : List String)
    (acceptanceCondition trustedBoundary artifactIdentity : String)
    (designKey : Option String := none)
    (commandProfileKey : Option String := none)
    (commandProfileScope : Option MemoryScope := none)
    (commandProfileDecision : Option CallerDecision := none) :
    Except String State := do
  let selected ←
    selectEvidenceRecording state key observation method environment
      acceptanceCondition trustedBoundary artifactIdentity designKey
      commandProfileKey commandProfileScope commandProfileDecision
  let spec :=
    evidenceSpecForRecording selected observation method environment inputs
      acceptanceCondition trustedBoundary artifactIdentity
      commandProfileDecision
  if !spec.wellFormed then
    throw "The evidence description is invalid."
  else
    pure
      { selected.revised with
        evidenceSpecs := selected.revised.evidenceSpecs ++ [spec] }

def addEvidence (state : State) (key observation method environment : String)
    (inputs : List String)
    (acceptanceCondition trustedBoundary artifactIdentity : String)
    (designKey : Option String := none)
    (commandProfileKey : Option String := none)
    (commandProfileScope : Option MemoryScope := none)
    (commandProfileDecision : Option CallerDecision := none) :
    Except String State :=
  if commandProfileKey.isSome != commandProfileDecision.isSome then
    .error
      "An exact Command Profile and its explicit caller selection are required together."
  else
    addEvidenceAfterSelectionValidation state key observation method
      environment inputs acceptanceCondition trustedBoundary artifactIdentity
      designKey commandProfileKey commandProfileScope commandProfileDecision

def evidenceSpecsSelectedByFocusedBoundary (state : State) (key : String)
    (designKey : Option String := none) : List Evidence.Spec :=
  let selectedWork := state.work.find? fun work =>
    work.ref == state.focus.work && workCurrent state work.ref
  selectedWork.toList.flatMap fun selectedWork =>
  let selectedEvidenceRefs :=
    selectedWork.completionBoundary.filterMap fun member =>
      match member.target with
      | .externalObservation evidence =>
          if evidence.key == key && designKey.isNone then some evidence else none
      | _ => none
  let selectedEvidenceBases :=
    selectedWork.completionBoundary.filterMap fun member =>
      match member.target with
      | .assurance selectedKey =>
          if (designKey.isSome || selectedEvidenceRefs.isEmpty) &&
              selectedKey == key &&
              designKey.all fun selected =>
                match member.basis with
                | .design accepted =>
                    accepted.any (·.ref.key == selected)
                | .workBoundary _ => false then
            match selectedAssuranceForBasis? state key member.basis with
            | some assurance =>
                if assurance.method == .evidence then some member.basis else none
            | none => none
          else
            none
      | _ => none
  state.evidenceSpecs.filter fun spec =>
    spec.ref.key == key &&
      evidenceSpecCurrent state spec &&
      (selectedEvidenceRefs.contains spec.ref ||
        selectedEvidenceBases.contains spec.basis)

def recordEvidence (state : State) (key observedValue : String)
    (passed : Bool) (designKey : Option String := none) : Except String State :=
  if observedValue.isEmpty then
    .error "An observed value is required."
  else if !(state.work.any fun work =>
      work.ref == state.focus.work && workCurrent state work.ref) then
    .error "The current outcome does not exist or has changed."
  else
    match evidenceSpecsSelectedByFocusedBoundary state key designKey with
    | [spec] =>
        if state.commandDeviations.any (·.evidence == some spec.ref) then
          .error
            "The recorded actual route differs from this EvidenceSpec's Command Profile; select a caller-accepted exact alternate profile."
        else
        let result : Evidence.Result := { spec, observedValue, passed }
        if !result.wellFormed then
          .error "The evidence result is invalid."
        else
          .ok
            { state with
              evidenceResults := state.evidenceResults ++ [result] }
    | [] => .error "No current evidence description has that name."
    | _ => .error "The evidence name is ambiguous."

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
  if state.formalSpecs.any fun selected =>
      selected.key == spec.key && selected.design == spec.design then
    throw "That exact Design already has a different formal selection; record a successor Design to change its formal scope."
  return { state with formalSpecs := state.formalSpecs ++ [spec] }

def formalSpecsSelectedForRecording (state : State) (key : String)
    (designKey : Option String := none)
    (designVersion : Option Nat := none) : List Evidence.FormalSpec :=
  match designKey, designVersion with
  | some selected, some version =>
      state.formalSpecs.filter fun spec =>
        spec.key == key &&
          spec.design == ({ key := selected, version } : DesignRef) &&
          formalSpecCurrent state spec &&
          formalDesignSelectable state key spec.design
  | some selected, none => selectedFormalSpecsForDesign state key selected
  | none, _ => selectedFormalSpecs state key

def formalSpecForRecording (state : State) (key : String)
    (designKey : Option String := none)
    (designVersion : Option Nat := none) :
    Except String Evidence.FormalSpec :=
  match formalSpecsSelectedForRecording state key designKey designVersion with
  | [spec] => .ok spec
  | [] => .error "No current formal assurance has that name."
  | _ => .error "The formal assurance name is ambiguous."

def validateFormalResultForRecording
    (result : Evidence.FormalResult) : Except String Unit := do
  let spec := result.spec
  if !spec.wellFormed then
    throw "The selected formal scope is invalid."
  if result.toolIdentity.isEmpty then
    throw "The formal tool identity is missing."
  if result.semanticPreview.isEmpty || result.previewIdentity.isEmpty then
    throw "The formal meaning preview identity is missing."
  if !spec.modules.all result.checkedClosure.contains then
    throw "The checked module closure does not contain every selected module."
  if result.checkedArtifacts.isEmpty ||
      !result.checkedArtifacts.all (fun artifact => !artifact.isEmpty) then
    throw "The checked formal artifact identity is incomplete."
  match spec.oracle, result.oracleArtifact, spec.adapter,
      result.conformancePassed with
  | none, none, none, none => pure ()
  | some _, some artifact, none, none =>
      if artifact.isEmpty then throw "The checked oracle identity is missing."
  | some _, some artifact, some _, some _ =>
      if artifact.isEmpty then
        throw "The checked oracle identity is missing."
  | some _, some artifact, some _, none =>
      if artifact.isEmpty then
        throw "The checked oracle identity is missing."
  | _, _, _, _ =>
      throw "The formal result does not match the selected assurance method."

def appendFormalResult (state : State)
    (result : Evidence.FormalResult) : State :=
  -- A repeated observation is still a new verification event. Move an
  -- identical historical value to the end so latest-result selection cannot
  -- leave a newer counterexample or execution failure authoritative.
  {
    state with
    formalResults := state.formalResults.filter (· != result) ++ [result]
  }

def recordFormalResult (state : State) (key toolIdentity : String)
    (oracleArtifact : Option String)
    (checkedClosure checkedArtifacts : List String)
    (conformancePassed : Option Bool)
    (semanticPreview previewIdentity : String)
    (designKey : Option String := none)
    (designVersion : Option Nat := none) : Except String State :=
  match formalSpecForRecording state key designKey designVersion with
  | .error message => .error message
  | .ok spec =>
      let result : Evidence.FormalResult :=
        { spec
          toolIdentity
          checkedClosure
          checkedArtifacts
          oracleArtifact
          conformancePassed
          semanticPreview
          previewIdentity }
      match validateFormalResultForRecording result with
      | .error message => .error message
      | .ok _ => .ok (appendFormalResult state result)

private def designReviewArtifacts? (state : State)
    (candidate : Design.Item)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) :
    Option (List String) :=
  let formalKeys := candidate.assurance.obligations.filterMap fun obligation =>
    if obligation.method == .formal then some obligation.key else none
  if formalKeys.isEmpty then
    some [candidate.statement]
  else
    let results := formalKeys.filterMap fun key =>
      (state.formalSpecs.find? fun spec =>
        spec.key == key && spec.design == candidate.ref)
        |>.bind (latestFormalResultForSpec? state)
        |>.filter fun result =>
          !staleFormalResultIdentities.contains result.identity
    if results.length != formalKeys.length then
      none
    else
      some <| results.flatMap fun result =>
        [result.previewIdentity, result.toolIdentity] ++
          result.oracleArtifact.toList ++ result.checkedArtifacts

def recordDesign
    (state : State) (source : Source) (key statement : String)
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

def designCandidateForAcceptance (state : State) (key : String)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) :
    Except String Design.Item := do
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
    (designReviewArtifacts? state candidate
      staleFormalResultIdentities).any fun artifacts =>
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
        (state.formalSpecs.find? fun spec =>
          spec.key == obligation.key && spec.design == candidate.ref)
          |>.bind (latestFormalResultForSpec? state)
          |>.any fun result =>
            !staleFormalResultIdentities.contains result.identity
      else
        true
  if !formalReady then
    throw "The proposed formal design requires a current verified formal result before caller acceptance."
  pure candidate

def stateWithAcceptedDesign (state : State) (candidate accepted : Design.Item) :
    State :=
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

def acceptDesign (state : State) (key : String)
    (decision : CallerDecision)
    (complexity : Option Design.ComplexityRationale := none)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) :
    Except String State := do
  let candidate ←
    designCandidateForAcceptance state key staleFormalResultIdentities
  let accepted : Design.Item :=
    { candidate with
      complexityRationale := complexity
      authority := .acceptedByCaller decision }
  if !accepted.wellFormed then
    throw "The caller acceptance is invalid."
  pure (stateWithAcceptedDesign state candidate accepted)

def acceptDesignWithKPT
    (state : State) (designKey : String)
    (decision : CallerDecision) (kptAuthor kptKey : String)
    (category : KPT.Category) (scope : MemoryScope) (statement : String)
    (relation : Option KPT.RelationSelector := none)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) : Except String State := do
  let accepted ←
    acceptDesign state designKey decision none staleFormalResultIdentities
  recordKPT accepted decision.source kptAuthor (some decision) kptKey category scope
    statement relation

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

def kptProfileScopeSelector : MemoryScope → KPT.ProfileScopeSelector
  | .project => .project
  | .work _ => .focusedWork

def resolveAtomicCommandProfileKPTRelation (state : State)
    (profileKey : String) (profileScope : MemoryScope)
    (selector : Option KPT.RelationSelector) :
    Except String (Option KPT.Relation) :=
  let selectedScope := kptProfileScopeSelector profileScope
  match selector with
  | some (.commandProfile selectedKey selectedProfileScope) =>
      if selectedKey = profileKey && selectedProfileScope = selectedScope then
        match state.commandProfiles.reverse.head? with
        | some candidate =>
            if candidate.ref.key = profileKey &&
                candidate.scope = profileScope then
              .ok (some (.commandProfile candidate.ref))
            else
              .error
                "The atomic Command Profile candidate identity was not retained."
        | none =>
            .error "The atomic Command Profile candidate identity is missing."
      else
        resolveOptionalKPTRelation state selector
  | _ => resolveOptionalKPTRelation state selector

def recordKPTWithCommandProfile
    (state : State) (source : Source)
    (kptAuthor : String)
    (decision : Option CallerDecision) (kptKey : String)
    (category : KPT.Category) (scope : MemoryScope) (statement : String)
    (relation : Option KPT.RelationSelector) (profileKey purpose : String)
    (argv : List String) (cwd : Option String)
    (disposition : CommandProfile.Disposition) : Except String State := do
  let withProfile ←
    recordCommandProfile state source decision profileKey purpose scope argv cwd
      disposition
  let resolvedRelation ←
    resolveAtomicCommandProfileKPTRelation withProfile profileKey scope relation
  recordKPTResolved withProfile source kptAuthor decision kptKey category scope
    statement resolvedRelation

def recordKPTWithInstruction
    (state : State) (decision : CallerDecision)
    (kptAuthor : String)
    (key : String) (category : KPT.Category) (scope : MemoryScope)
    (statement : String) (relation : Option KPT.RelationSelector)
    (instruction : String) : Except String State := do
  let withKPT ←
    recordKPT state decision.source kptAuthor (some decision) key category scope
      statement relation
  recordInstruction withKPT decision instruction

def resolveAtomicDesignKPTRelation (state : State)
    (designKey : String) (selector : Option KPT.RelationSelector) :
    Except String (Option KPT.Relation) :=
  match selector with
  | some (.design selectedKey .candidate) =>
      if selectedKey == designKey then
        match state.design.designItems.reverse.head? with
        | some candidate =>
            if candidate.ref.key == designKey then
              .ok (some (.design candidate.ref))
            else
              .error "The atomic Design candidate identity was not retained."
        | none => .error "The atomic Design candidate identity is missing."
      else
        resolveOptionalKPTRelation state selector
  | _ => resolveOptionalKPTRelation state selector

def recordKPTWithDesignCandidate
    (state : State) (source : Source)
    (kptAuthor : String)
    (decision : Option CallerDecision) (kptKey : String)
    (category : KPT.Category) (scope : MemoryScope) (statement : String)
    (relation : Option KPT.RelationSelector) (designKey designStatement : String)
    (role : Design.Role) (assurance : Design.AssuranceSelection)
    (dependencyKeys : List String := [])
    (addsComplexity : Bool := false) : Except String State := do
  let withDesign ←
    recordDesign state source designKey designStatement role assurance
      dependencyKeys addsComplexity
  let resolvedRelation ←
    resolveAtomicDesignKPTRelation withDesign designKey relation
  recordKPTResolved withDesign source kptAuthor decision kptKey category scope
    statement resolvedRelation

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

def requestDesignReview (state : State) (key designKey : String)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) :
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
  let artifacts ← match
      designReviewArtifacts? state candidate staleFormalResultIdentities with
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

def selectedReviewRequests (state : State) (reviewKey : String) :
    List Review.Request :=
  match state.work.find? (·.ref == state.focus.work) with
  | none => []
  | some work =>
      if !workCurrent state work.ref then []
      else
        work.completionBoundary.filterMap fun member =>
          match member.target with
          | .reviewResolved review =>
              if review.key != reviewKey then none
              else
                state.reviewRequests.find? fun request =>
                  request.ref == review && reviewRequestCurrent state request
          | _ => none

def recordReviewResult (state : State) (reviewKey reviewer : String)
    (observation : Review.Observation) : Except String State :=
  match selectedReviewRequests state reviewKey with
  | [] => .error "No current review has that name."
  | _ :: _ :: _ => .error "The review name is ambiguous."
  | [request] =>
      let existing := state.reviewResults.find? (·.review == request.ref)
      let result : Except String Review.Result :=
        match existing with
        | none =>
            .ok
              { review := request.ref
                scope := request.scope
                reviewer
                observations := [observation] }
        | some result =>
            if result.reviewer != reviewer then
              .error "Additional observations must come from the same reviewer."
            else if result.observations.any (·.key == observation.key) then
              .error "That review observation already exists."
            else
              .ok
                { result with
                  observations := result.observations ++ [observation] }
      match result with
      | .error message => .error message
      | .ok checked =>
          if !checked.exactFor request then
            .error "The review result does not match its requested scope."
          else
            let results :=
              match existing with
              | none => state.reviewResults ++ [checked]
              | some prior =>
                  state.reviewResults.map fun candidate =>
                    if candidate == prior then checked else candidate
            .ok { state with reviewResults := results }

def recordCleanReview (state : State) (reviewKey reviewer : String) :
    Except String State :=
  match selectedReviewRequests state reviewKey with
  | [request] =>
      if state.reviewResults.any (·.review == request.ref) then
        .error "A result already exists for that review scope."
      else
        let result : Review.Result :=
          { review := request.ref
            scope := request.scope
            reviewer
            observations := [] }
        if !result.exactFor request then
          .error "The clean review result does not match its requested scope."
        else
          .ok { state with reviewResults := state.reviewResults ++ [result] }
  | [] => .error "No current review has that name."
  | _ => .error "The review name is ambiguous."

def recordReviewDisposition (state : State) (reviewKey observationKey : String)
    (decision : Review.Decision) (caller : CallerDecision)
    (successor : Option Design.AcceptedRef := none)
    (complexity : Option Review.ComplexityRationale := none) :
    Except String State := do
  let results := state.reviewResults.filter fun result =>
    result.review.key == reviewKey &&
      (selectedReviewRequests state reviewKey).any fun request =>
        request.ref == result.review
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
    (complexity : Option Review.ComplexityRationale := none)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) :
    Except String State := do
  let state ←
    acceptDesign state successorKey caller complexity
      staleFormalResultIdentities
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
          "Return after finishing the interrupting outcome, or use replan-return with the caller's selected outcome and reason to replace the return plan."
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

def returnFromInterruption (state : State)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) : ReturnResult :=
  match state.focus.returnPoint with
  | none => .invalid "No interruption return point is pending."
  | some point =>
      if !currentlyComplete state state.focus.work
          staleFormalResultIdentities then
        .invalid
          "The interrupting outcome is not complete; finish its selected boundary before returning."
      else
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
  let nextRef : WorkRef :=
    { key := selected.ref.key, version := selected.ref.version + 1 }
  let successor : Work.Unit :=
    { selected with
      ref := nextRef
      completionBoundary :=
        selected.completionBoundary.map (rebaseBoundary selected.ref nextRef)
      authority := caller }
  if !successor.wellFormed then
    throw "The caller-selected return outcome is invalid."
  pure
    { state with
      work := state.work ++ [successor]
      focus :=
        { work := nextRef
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
          { work with
            ref := nextRef
            completionBoundary := boundary
            authority := correction.caller }
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

def nextAction (state : State)
    (staleFormalResultIdentities :
      List Evidence.FormalResultIdentity := []) : Option NextAction :=
  if !state.wellFormed then
    some (.cannotAdvance "The recorded project state is invalid.")
  else
    match (missingCompletion state state.focus.work
      staleFormalResultIdentities).head? with
    | some member => some (.satisfy member)
    | none =>
        if !currentlyComplete state state.focus.work
            staleFormalResultIdentities then
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
