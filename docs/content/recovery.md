# Recovery and interruption

Run `status` and `next` through the Skill after process loss or a new session.
The same accepted design, current Work and Task, caller review decisions,
assurance, and interruption return point are recovered.

The wrapper privately retains one pending semantic intention. If a response is
lost, repeat the same project action unchanged. The exact retry returns the
already committed outcome without duplicating work. A different mutation is
rejected until that intention is reconciled.
Argument-vector identity is injective even when caller text contains control
characters; no free-text delimiter is treated as an argument boundary.
Stale formal identities stream through a private file. Producer, inspection,
sort, and finalization failures all stop before the public operation runs, and
the completed projection is validated before any mutation commits. Post-commit
recovery rendering reuses that validated snapshot, so projection transport
cannot turn a committed mutation into an ordinary failure that discards its
retry identity.
This boundary lasts while the wrapper invocation remains pending; delivery
after the wrapper has returned success belongs to the calling agent surface.

Do not inspect or edit the SQLite state file. An unreadable state stops before
caller authority is inferred.

`interrupt <urgent-outcome> <first-task>` preserves one return point and starts
the urgent Work atomically. Finish the urgent boundary, then run `return`;
an unfinished urgent Work is rejected by `return`. If
accepted design or the saved Work changed, the Skill asks for the minimum
caller replan decision instead of silently retargeting the Task. Use
`replan-return <outcome> <reason>` to record that decision. The same explicit
operation can replace a pending return plan when the caller chooses to stop
pursuing unfinished interrupting Work; an ordinary `return` cannot.
