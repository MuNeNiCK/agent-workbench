# State and transition reference

Use this reference when an agent cannot continue, an operation is rejected, or reported state does
not match the expected project situation. It describes the public state returned by Workbench and
the transitions available through its native operations.

## Project selectors

The project has two current selectors:

- `acceptedDesignId` identifies the one accepted DesignRevision, when one exists.
- `focusedWorkId` identifies the one focused Work, when one exists.

No Work is focused while accepting a successor design or adopting that design into suspended work.
This is an intentional consistency boundary, not a request for the user to edit state.

## DesignRevision states

| State | Meaning | Public transition |
|---|---|---|
| `candidate` | Proposed immutable design awaiting acceptance | `design propose` creates it |
| `accepted` | Current normative design | `design accept` selects it |
| `replaced` | Former accepted design displaced by its accepted successor | Derived by `design accept` |
| `rejected` | Reserved schema state | No current native operation enters this state |

`design propose` is available only when no Work is focused. The candidate's parent is derived from
the current accepted design. `design accept` is also available only with no focused Work and accepts
only a candidate whose parent is still the current accepted design. If a declared design-source file
changed after proposal, acceptance is rejected and a new candidate must be proposed.

## Work states

| State | Meaning | Entry transitions |
|---|---|---|
| `focused` | The one Work currently being performed | `work start`, `work focus`, or `work resume` |
| `suspended` | Work retained with an explicit return condition | `work suspend` |
| `blocked` | Resumable schema state | No current native operation enters this state |
| `completed` | Readiness passed and Work was completed | `work complete` |

Important transitions:

| Situation | Required operation | Result | Common rejection |
|---|---|---|---|
| Start a new outcome | `work start` | New Work becomes focused and binds the accepted design | No accepted design or another focused Work |
| Pause current work | `work suspend` | Work becomes suspended and records a non-empty resume condition | Named Work is not focused |
| Continue retained work | `work resume` or `work focus` | Suspended/blocked Work becomes focused | Another Work is focused or its design is no longer accepted |
| Move retained Work to a successor design | `work adopt-design` | Design binding changes; Work remains unfocused | Work was not suspended, successor is not a descendant, or requester is not responsible |
| Transfer agent responsibility | `work handoff` | Responsible agent run changes; Work and evidence boundary remain | Work is not focused or successor is already responsible |
| Complete | `work complete` | Work becomes completed and focus clears | Derived readiness is false |

## Current Context

`context` returns a bounded projection for the next action. When Work is focused it can contain:

- the accepted DesignRevision and focused Work;
- required unfinished Tasks;
- applicable Command Profiles;
- effective user corrections;
- relevant KPT entries;
- unresolved accepted findings;
- missing or stale acceptance evidence;
- missing or stale Lean receipts; and
- changed declared design sources.

Superseded entries and replaced designs remain in `history` but do not re-enter Current Context by
text similarity. Repeated categories are bounded; use paginated history or entity lookup for older
records.

## Readiness gaps

`ready` is false if any of these current conditions holds:

- a declared design-source snapshot changed;
- a required Task remains open;
- an acceptance criterion lacks current evidence of the declared kind;
- a selected Lean claim lacks a current kernel-accepted receipt;
- a user correction remains effective; or
- an accepted finding lacks current resumed-review verification.

The response identifies the corresponding gaps. Correct the underlying project result or use the
applicable semantic operation; never change private database rows to remove a gap.

## Ledger entry currentness

Tasks, profiles, executions, artifact observations, corrections, KPT, reviews, findings,
dispositions, verifications, proof receipts, handoffs, and design adoptions are typed ledger entries.
Workbench generates their order and Work/Design/scope binding.

An entry superseded by a later same-kind, same-bound entry remains history but is excluded from the
current projection. Evidence also stops counting when its target snapshot changes, its producing
profile is replaced, its Work/Design binding is no longer current, or its proof input changes.
