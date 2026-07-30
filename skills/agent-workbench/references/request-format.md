# Project action reference

All actions are positional and run through the installed Skill wrapper.
Comma-separated values are used only where shown; `-` means no optional value.

```text
init <outcome> <first-task>
status
next
complete

start-work <outcome> <first-task>
switch-work <outcome>
add-task <description>
add-task-for-design <description> <design-key>...
finish-task

record-design <key> <role> <assurance> <statement> [dependency-key...]
propose-design <ordinary|complexity> <key> <role> <assurance>
  <statement> [dependency-key...]
request-design-review <review> <design-key>
accept-design <key> <reason>
accept-design-with-kpt <design-key> <reason> <author> <kpt-key>
  <keep|problem|try> <project|work> <statement>
  <relation-kind|-> <relation-key|-> <relation-member|->
accept-complex-design <key> <reason> <necessity>
  <why-simpler-is-insufficient> <bounded-scope> <maintenance-cost>
retire-design <key> <reason>
record-instruction <statement>
record-question <statement>
reject-proposal <target> <reason>
record-source-effects <design-key|-> <role> <assurance>
  <design-statement|-> <instruction|-> <question|->
  <outcome|-> <first-task|-> [dependency-key...]

record-command-profile <key> <purpose> <project|work>
  <required|recommended|discouraged> <cwd|-> <reason> <argv>...
propose-command-profile <key> <purpose> <project|work>
  <required|recommended|discouraged> <cwd|-> <argv>...
accept-command-profile <key> <project|work> <reason>
record-command-deviation <key> <project|work> <evidence-key|-> <cwd|-> <reason>
  <actual-argv>...

record-kpt <author> <key> <keep|problem|try> <project|work>
  <statement> <relation-kind|-> <relation-key|-> <relation-member|->
  <predecessor-author|->
propose-kpt <author> <key> <keep|problem|try> <project|work>
  <statement> <relation-kind|-> <relation-key|-> <relation-member|->
accept-kpt <author> <key> <project|work> <reason>
record-kpt-command-profile <author> <kpt-key> <keep|problem|try> <project|work>
  <statement> <relation-kind|-> <relation-key|-> <relation-member|->
  <profile-key> <purpose>
  <required|recommended|discouraged> <cwd|-> <argv>...
record-kpt-instruction <author> <kpt-key> <keep|problem|try> <project|work>
  <statement> <relation-kind|-> <relation-key|-> <relation-member|->
  <instruction>
record-kpt-design <author> <kpt-key> <keep|problem|try> <project|work>
  <statement> <relation-kind|-> <relation-key|-> <relation-member|->
  <design-key> <role> <assurance>
  <design-statement> [dependency-key...]
kpt-history <key> <project|work>

KPT relation kinds are `command-profile`, `design`, `task`,
`review-observation`, and `evidence-result`. The relation key is respectively a
profile key, Design key, Task description, Review key, or Evidence key.
The `relation-member` is the profile scope (`project|work`), Design authority
(`accepted|candidate`), `work` for a Task, the observation key for a Review, or
the Evidence basis (`work|design:<Design-key>`), respectively. `work` always
selects the focused Work. Generated
immutable identities remain internal. The Kernel
resolves one exact current target and rejects missing or ambiguous relations.
`predecessor-author` selects one exact current standalone-author entry for
caller supersession; it is not an adoption route.

add-evidence <key> <observation> <method> <environment> <input,...|->
  <acceptance-condition> <trusted-boundary> <artifact>
  [design-key|- command-profile-key project|work caller-selection-reason]
record-evidence <key> <observed-value> <pass|fail> [design-key]

preview-formal <assurance-key> <design-key> <oracle-module> <module,...>
  <implementation-surface,...|-> <adapter|-> <case,...|->
formal-check <assurance-key> [design-key]

request-review <key> <design|implementation|reuse> <artifact>
record-review <review> <reviewer> <observation> <risk|proposal>
  <ordinary|complexity> <summary> <evidence>
record-clean-review <review> <reviewer>
resolve-review <review> <observation>
  <accepted|rejected|rescoped|deferred|needs-evidence> <reason>
adopt-review-proposal <review> <observation> <successor-design> <reason>
adopt-complex-review-proposal <review> <observation> <successor-design> <reason>
  <necessity> <why-simpler-is-insufficient> <bounded-scope> <maintenance-cost>
correct-review <mistaken-review> <intended-outcome> <intended-task>
  <intended-artifact> <reason>

interrupt <urgent-outcome> <first-task>
return
replan-return <outcome> <reason>

assign-phase <task-description> <phase-name> <display-order>
rename-phase <current-name> <new-name>
order-phase <phase-name> <display-order>
```

Roles are `goal`, `functional`, `non-functional`, `constraint`, `decision`,
`structure`, `fact`, and `boundary`. Assurance is `formal`, `evidence`,
`mixed`, or `none`.

The wrapper owns operation identity and exact retry while an invocation remains
pending. A different mutation is rejected while that result is uncertain;
retry the same action unchanged. Delivery after the wrapper has returned
success is outside this boundary.
