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
| `superseded` | Former accepted Design or amended candidate retained as immutable history | Derived by successor acceptance or candidate amendment |
| `rejected` | Candidate explicitly rejected without changing accepted authority | `design reject` |
| `replaced` | Legacy decoded status | No new v0.2.8 transition creates it |

`design propose` and `design amend` require a focused Work and capture exact declared Markdown bytes
from its private Design workspace. The accepted parent, Work binding, identity, digest, and candidate
amendment status are derived. Initial acceptance keeps that Work focused and binds it to the initial
Design. Successor acceptance requires focus to be cleared; ancestor-bound Work remains suspended
until explicit adoption. Editing or deleting a live draft after proposal cannot change, invalidate,
or silently replace the immutable candidate stored in SQLite.

## Work states

| State | Meaning | Entry transitions |
|---|---|---|
| `active` | Work is eligible to be focused; `focusedWorkId` identifies the one currently performed | `work start`, `work focus`, or `work resume` |
| `suspended` | Work retained with an explicit return condition | `work suspend` |
| `completed` | Readiness passed and exactly one matching completion record was committed | `work complete` |
| `withdrawn` | Outcome ended unsuccessfully under an effective User Correction | `work withdraw` |

Important transitions:

| Situation | Required operation | Result | Common rejection |
|---|---|---|---|
| Start a new outcome | `work start` | New Work becomes focused; it binds the accepted Design or retains an empty baseline before initial Design | Another Work is focused |
| Pause current work | `work suspend` | Work becomes suspended and records a non-empty resume condition | Named Work is not focused |
| Continue retained work | `work resume` or `work focus` | Suspended/active Work becomes focused | Another Work is focused or successor adoption is required |
| Move retained Work to a successor design | `work adopt-design` | Design binding changes; Work remains unfocused | Work was not suspended, successor is not a descendant, or requester is not responsible |
| Transfer agent responsibility | `work handoff` | Responsible agent run changes; Work and evidence boundary remain | Work is not focused or successor is already responsible |
| Complete | `work complete` | Work becomes completed, focus clears, and the exact completion-input digest is recorded atomically | Derived readiness is false |
| End without success | `work withdraw` | Work becomes withdrawn and focus clears | No effective same-Work User Correction authorizes withdrawal |

## Implementation Plan states

| State | Meaning |
|---|---|
| `candidate` | Immutable proposed Plan with no Task authority yet |
| `current` | The one materialized Plan for the Work's adopted Design |
| `superseded` | Historical Plan replaced atomically during materialization |

`plan propose/replace` captures exact private Plan-source bytes and the complete Design delta.
`plan materialize` requires current selected Claim receipts, makes only the candidate head current,
and creates or reopens the exact derived Task graph in the same state transition. A candidate alone
does not change productive authority.

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

Command evidence is current only while its Profile, resolved command, target, and every declared
input still match the observations recorded by the run. Evidence without recorded input
observations is retained as history but is not current.

Superseded entries and superseded designs remain in `history` but do not re-enter Current Context by
text similarity. Repeated categories are bounded; use paginated history or entity lookup for older
records.

## Readiness gaps

`ready` is false if any of these current conditions holds:

- the accepted Design or current Plan/source archive is incomplete or inconsistent;
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
