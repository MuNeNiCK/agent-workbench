# Native operation reference

This is an advanced user/support reference for diagnosing rejected actions and auditing the Skill's
public route without reading product source. During ordinary use, the coding agent invokes these
operations through the installed Skill; the user does not construct JSON requests.

## Discover the current contract

After setup, the project-local executable supports:

```text
.agent-workbench/bin/agent-workbench --project <project-root> context
.agent-workbench/bin/agent-workbench --project <project-root> describe
.agent-workbench/bin/agent-workbench --project <project-root> describe <operation>
```

`describe` returns the finite operation catalogue and the subset applicable to current state.
`describe <operation>` returns its applicability, summary, and current input example. This is the
distributed request contract; documentation intentionally does not duplicate every JSON field.

Mutation input is JSON on standard input as machine transport. It is not the persisted database
format. Unknown top-level and nested fields are rejected. A request cannot provide ledger order,
scope/Work/Design binding, supersession, lifecycle status, generated design identity, parent, or
source snapshots unless that field belongs to the returned intent contract.

## Operation catalogue

| Area | Operations |
|---|---|
| Setup and discovery | `init`, `describe` |
| Design | `design propose`, `design amend`, `design accept`, `design reject`, `design get`, `design inspect-sources`, `design source`, `design diff`, `design export` |
| Work | `work start`, `work get`, `work focus`, `work resume`, `work suspend`, `work handoff`, `work adoption-impact`, `work adopt-design`, `work withdraw`, `work complete` |
| Implementation Plan | `plan propose`, `plan replace`, `plan materialize`, `plan get`, `plan inspect-sources`, `plan source`, `plan diff`, `plan export` |
| Task | `task close` (Tasks are materialized from the current Plan; there is no manual Task creation operation) |
| Command Profile | `profile define`, `profile replace`, `command show`, `command run` |
| Artifact evidence | `artifact observe` |
| User correction | `correction record`, `correction supersede`, `correction resolve`, `correction incorporate` |
| KPT | `kpt record`, `kpt apply` |
| Review | `review start`, `review resume`, `review handoff`, `review finding`, `review disposition`, `review conclude`, `review verify`, `review context`, `review inspect` |
| Lean proof | `proof digest`, `proof run` |
| Read models | `entry get`, `history`, `context`, `ready` |

There is no public generic entry append, whole-state replacement, arbitrary transition, or generic
`done` operation. Mutations express one domain intent and derive authoritative relationships inside
Workbench.

## Safe invocation sequence

1. Read `context`.
2. Read the current applicable set from `describe`.
3. Before an unfamiliar mutation, inspect `describe <operation>`.
4. Require `applicable: true` and use only fields returned by that contract.
5. For a Command Profile, call `command show` before `command run`.
6. Use `ready` as the completion decision.

Declare every file or other observable input on which a Command Profile's result depends. A
successful `command run` binds its evidence to the observed state of those inputs as well as the
resolved command and target. Changing any declared input makes that evidence stale. Records created
by an older Workbench version without input observations remain readable, but cannot satisfy current
evidence requirements until the command is run again.

Applicability shown by `describe` is guidance, not the transaction boundary. Every mutation acquires
project ownership and rechecks applicability against authoritative state before commit. A concurrent
operation may therefore make a previously displayed request inapplicable; the rejected request does
not advance state revision.

An applicable mutation has at least one current set of state-owned referents that satisfies its
semantic preconditions; it does not mean arbitrary request-authored content will succeed. For
example, `review finding` remains inapplicable until the current Work has a compatible fresh root
Review. Its target provenance is derived from that Review rather than supplied by the request.

## Read operations

- `context` returns the bounded current projection.
- `ready` returns the derived completion decision and current gaps.
- `design get`, `plan get`, `work get`, and `entry get` return one entity by stable ID.
- `design source/diff/export` and `plan source/diff/export` read immutable SQLite archives, never
  later draft files.
- `work adoption-impact` derives the exact consequences of adopting the accepted successor before
  the binding changes.
- `review context` returns isolated fresh or resumed reviewer input.
- `history` returns entries after an order and accepts a limit from 1 through 100.
- `command show` resolves one applicable profile without executing it.
- `proof digest` derives the current complete identity of a selected claim input.

Read-only operations do not acquire the project mutation lock. `describe` reads state only to report
applicability.

## Mutation response and failure

Mutation responses report the committed authoritative result or current context. Invalid JSON,
unknown fields, duplicate IDs, incorrect binding, stale source content, or inapplicable state is an
error. Errors do not authorize a user or agent to edit `.agent-workbench/state.db`; use the
reported contract and [state reference](state-reference.md) to select the valid next operation.
