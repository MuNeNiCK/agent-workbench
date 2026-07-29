# Agent Workbench

Agent Workbench gives a coding agent durable project memory for accepted
design, implementation work, assurance, review decisions, interruption, and
completion.

Its Lean support is for the project using the Skill. A project may express selected
acceptance-critical behavior as project-domain Lean contracts, proofs, and an
executable oracle. When design changes locally, declared dependencies identify
the affected scope; unrelated accepted assurance does not need a whole-project
review again.

Non-formal requirements remain first-class. Performance, deployment, UX,
third-party behavior, and human judgement can use explicit external Evidence
instead of being forced into propositions.

The Skill keeps its state under `.agent-workbench` and stays outside product
code. SQLite is only the private atomic persistence mechanism; it does not
define the user workflow.

Continue with [Installation](installation.md) and [Workflow](workflow.md).
