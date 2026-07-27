# Agent Workbench v0.2.2

`v0.2.2` makes remote publication and release attempts honor their prepared
preconditions before dispatch.

- Reject a remote predecessor that violates the prepared precondition, even
  when the target and artifact digest otherwise match.
- After an uncertain response, reconcile only the exact target and artifact
  digest that could represent the attempted successor.
- Cover precondition conflicts and adapter failure boundaries through the
  executable storage laws.
- Keep the one-Skill installation and static Linux x86_64 release contract from
  `v0.2.1`.
