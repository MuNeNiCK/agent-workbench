import AgentWorkbench.Domain.Lookup

namespace AgentWorkbench

def entryIsSuperseded (state : ProjectState) (entry : LedgerEntry) : Bool :=
  state.ledgerEntries.any (fun replacement =>
    replacement.order > entry.order && replacement.supersedes.contains entry.id)

def entryAppliesTo
    (state : ProjectState) (design : DesignRevision) (work : Work) (entry : LedgerEntry) : Bool :=
  let designApplies :=
    match entry.designRevision with
    | none => true
    | some entryDesign =>
        entryDesign == design.id ||
        match entry.payload with
        | .userCorrection correction =>
            let incorporatedByCurrent :=
              match correction.incorporatedIn with
              | none => false
              | some incorporated =>
                  incorporated == design.id || state.designDescendsFrom incorporated design.id
            correction.resolvedByEntryId.isNone && state.designDescendsFrom entryDesign design.id &&
              !incorporatedByCurrent
        | _ => false
  entry.scope == work.scope &&
  (entry.workId == none || entry.workId == some work.id) &&
  designApplies

def effectiveEntries
    (state : ProjectState) (design : DesignRevision) (work : Work) : List LedgerEntry :=
  state.ledgerEntries.filter (fun entry =>
    entryAppliesTo state design work entry && !entryIsSuperseded state entry)

structure CurrentProjection where
  design : DesignRevision
  work : Work
  entries : List LedgerEntry
  deriving Repr, DecidableEq, Lean.ToJson, Lean.FromJson

def currentProjection? (state : ProjectState) : Option CurrentProjection := do
  let design ← state.currentDesign?
  let work ← state.currentWork?
  if work.designRevision != some design.id then none else
  pure { design, work, entries := effectiveEntries state design work }

end AgentWorkbench
