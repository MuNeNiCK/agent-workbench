# Getting started

## What you provide

Tell the coding agent the project outcome in ordinary language and ask it to use
`$agent-workbench`:

```text
Use $agent-workbench for this repository. I want <outcome>.
Research the existing project, construct the design, implement it, and verify completion.
```

You do not have to translate the request into Tasks, state records, JSON, or Lean. Constructing the
project design and selecting an appropriate verification method are agent responsibilities.

## What the agent should do

On first use, the installed Skill sets up the project-local runtime. The agent then reads only the
current project context and discovers which actions are valid in that state. For a new project it
constructs a design, explains material choices when needed, and starts work on your outcome.

For an existing project, ask it to continue instead:

```text
Use $agent-workbench. Read the current project context and continue the focused work.
Report any real blocker that requires my decision.
```

## What a useful status report contains

A useful report describes the project in user terms:

- the requested outcome;
- what has been implemented;
- what evidence has passed or become stale;
- any unresolved correction or accepted review finding;
- the remaining result, or why completion is ready.

Internal identifiers may be included when they help diagnosis, but an identifier alone is not a
status report.

## When the agent asks you a question

Workbench does not require routine approval between every step. A user decision is needed when the
accepted outcome is genuinely ambiguous, authority must change, an external action needs permission,
or a trade-off would materially change the result. Ordinary state transitions, validation, and
evidence collection remain agent work.

## Correcting the work

State the correction directly:

```text
The accepted behavior is <correct behavior>, not <previous interpretation>.
Record this correction and update the design before continuing.
```

An open correction blocks completion until later work incorporates or otherwise resolves it. The old
interpretation remains history but is not treated as current merely because similar text is found.
