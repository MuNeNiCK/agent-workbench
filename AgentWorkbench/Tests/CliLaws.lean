import Lean.Data.Json

open Lean

namespace AgentWorkbench.Tests.CliLaws

def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw <| IO.userError message

def jsonObject (fields : List (String × Json)) : String :=
  (Json.mkObj fields).compress

def request (revision : Nat) (operation command : String)
    (fields : List (String × Json)) : String :=
  jsonObject <| [
    ("operation", toJson operation),
    ("expectedRevision", toJson revision),
    ("command", toJson command)
  ] ++ fields

def scopeFields (purpose snapshot artifact : String) : List (String × Json) := [
  ("design", toJson (some 1 : Option Nat)),
  ("work", toJson 1),
  ("repositorySnapshot", toJson snapshot),
  ("artifactDigest", toJson artifact),
  ("purpose", toJson purpose)
]

def lifecycleRequests : List String := [
  request 1 "import-design" "import-design" [
    ("design", toJson 1),
    ("designRevision", toJson 1),
    ("predecessor", toJson (none : Option Nat)),
    ("owner", toJson "owner"),
    ("contentDigest", toJson "sha256:design"),
    ("requirements", toJson ["capability"]),
    ("decisions", toJson ["use the verified transition kernel"]),
    ("validationGates", toJson ["positive-lifecycle"])
  ],
  request 2 "plan-design-review" "record-review-plan" <| [
    ("plan", toJson 1),
    ("owner", toJson "owner"),
    ("reviewer", toJson "design-reviewer"),
    ("adjudicator", toJson "owner"),
    ("caller", toJson "owner")
  ] ++ scopeFields "design" "snapshot:design" "sha256:design",
  request 3 "claim-design-clean" "record-review-claim" <| [
    ("review", toJson 1),
    ("plan", toJson 1),
    ("epoch", toJson 0),
    ("claim", toJson "clean"),
    ("reviewer", toJson "design-reviewer")
  ] ++ scopeFields "design" "snapshot:design" "sha256:design",
  request 4 "accept-design-review" "record-review-adjudication" [
    ("review", toJson 1),
    ("decision", toJson "accepted"),
    ("adjudicator", toJson "owner"),
    ("reason", toJson "the clean claim matches the frozen design")
  ],
  request 5 "approve-design" "approve-design" [
    ("design", toJson 1)
  ],
  request 6 "plan-decomposition-review" "record-review-plan" <| [
    ("plan", toJson 2),
    ("owner", toJson "owner"),
    ("reviewer", toJson "planner"),
    ("adjudicator", toJson "owner"),
    ("caller", toJson "owner")
  ] ++ scopeFields "decomposition" "snapshot:decomposition" "sha256:decomposition",
  request 7 "claim-decomposition-clean" "record-review-claim" <| [
    ("review", toJson 2),
    ("plan", toJson 2),
    ("epoch", toJson 0),
    ("claim", toJson "clean"),
    ("reviewer", toJson "planner")
  ] ++ scopeFields "decomposition" "snapshot:decomposition" "sha256:decomposition",
  request 8 "accept-decomposition-review" "record-review-adjudication" [
    ("review", toJson 2),
    ("decision", toJson "accepted"),
    ("adjudicator", toJson "owner"),
    ("reason", toJson "the decomposition covers the approved design")
  ],
  request 9 "record-decomposition" "record-decomposition" [
    ("key", toJson "implementation"),
    ("design", toJson 1),
    ("work", toJson 1),
    ("designRevision", toJson 1),
    ("contentDigest", toJson "sha256:decomposition"),
    ("requirements", toJson ["capability"]),
    ("implementationWork", toJson ["work:1"]),
    ("tasks", toJson ["deliver"]),
    ("completionChecks", toJson ["native lifecycle completes"]),
    ("checklists", toJson ["positive native lifecycle"]),
    ("validationGates", toJson ["positive-lifecycle"]),
    ("reviewer", toJson "planner"),
    ("adjudicator", toJson "owner")
  ],
  request 10 "plan-conformance-review" "record-review-plan" <| [
    ("plan", toJson 3),
    ("owner", toJson "owner"),
    ("reviewer", toJson "conformance-reviewer"),
    ("adjudicator", toJson "owner"),
    ("caller", toJson "owner")
  ] ++ scopeFields "design-conformance" "snapshot:current" "sha256:artifact",
  request 11 "plan-quality-review" "record-review-plan" <| [
    ("plan", toJson 4),
    ("owner", toJson "owner"),
    ("reviewer", toJson "quality-reviewer"),
    ("adjudicator", toJson "owner"),
    ("caller", toJson "owner")
  ] ++ scopeFields "implementation-quality" "snapshot:current" "sha256:artifact",
  request 12 "plan-completion" "plan-completion" [
    ("work", toJson 1),
    ("decomposition", toJson (some "implementation" : Option String)),
    ("relatedWork", toJson ([] : List Json)),
    ("phases", toJson ([] : List String)),
    ("tasks", toJson ([] : List String)),
    ("checklists", toJson ([] : List String)),
    ("reviews", toJson ([] : List Nat)),
    ("findings", toJson ([] : List String)),
    ("validations", toJson ([] : List String)),
    ("repositories", toJson ([] : List String)),
    ("corrections", toJson ([] : List String)),
    ("workRecords", toJson ([] : List String))
  ],
  request 13 "claim-conformance-clean" "record-review-claim" <| [
    ("review", toJson 3),
    ("plan", toJson 3),
    ("epoch", toJson 0),
    ("claim", toJson "clean"),
    ("reviewer", toJson "conformance-reviewer")
  ] ++ scopeFields "design-conformance" "snapshot:current" "sha256:artifact",
  request 14 "accept-conformance-review" "record-review-adjudication" [
    ("review", toJson 3),
    ("decision", toJson "accepted"),
    ("adjudicator", toJson "owner"),
    ("reason", toJson "the implementation matches the frozen design")
  ],
  request 15 "claim-quality-clean" "record-review-claim" <| [
    ("review", toJson 4),
    ("plan", toJson 4),
    ("epoch", toJson 0),
    ("claim", toJson "clean"),
    ("reviewer", toJson "quality-reviewer")
  ] ++ scopeFields "implementation-quality" "snapshot:current" "sha256:artifact",
  request 16 "accept-quality-review" "record-review-adjudication" [
    ("review", toJson 4),
    ("decision", toJson "accepted"),
    ("adjudicator", toJson "owner"),
    ("reason", toJson "the implementation is ready at the frozen scope")
  ],
  request 17 "record-external-operation" "record-external-operation" [
    ("externalOperation", toJson "publish-fixture"),
    ("work", toJson (some 1 : Option Nat)),
    ("kind", toJson "release"),
    ("target", toJson "remote-fixture"),
    ("expectedRemoteArtifactDigest", toJson (none : Option String)),
    ("artifactDigest", toJson "sha256:artifact")
  ],
  request 18 "dispatch-external-operation" "advance-external-operation" [
    ("externalOperation", toJson "publish-fixture"),
    ("work", toJson (some 1 : Option Nat)),
    ("kind", toJson "release"),
    ("target", toJson "remote-fixture"),
    ("expectedRemoteArtifactDigest", toJson (none : Option String)),
    ("artifactDigest", toJson "sha256:artifact"),
    ("state", toJson "dispatched"),
    ("observationIdentity", toJson (none : Option String)),
    ("observedArtifactDigest", toJson (none : Option String)),
    ("disposition", toJson (none : Option String))
  ],
  request 19 "uncertain-external-operation" "advance-external-operation" [
    ("externalOperation", toJson "publish-fixture"),
    ("work", toJson (some 1 : Option Nat)),
    ("kind", toJson "release"),
    ("target", toJson "remote-fixture"),
    ("expectedRemoteArtifactDigest", toJson (none : Option String)),
    ("artifactDigest", toJson "sha256:artifact"),
    ("state", toJson "uncertain"),
    ("observationIdentity", toJson (none : Option String)),
    ("observedArtifactDigest", toJson (none : Option String)),
    ("disposition", toJson (none : Option String))
  ],
  request 20 "reconcile-external-operation" "advance-external-operation" [
    ("externalOperation", toJson "publish-fixture"),
    ("work", toJson (some 1 : Option Nat)),
    ("kind", toJson "release"),
    ("target", toJson "remote-fixture"),
    ("expectedRemoteArtifactDigest", toJson (none : Option String)),
    ("artifactDigest", toJson "sha256:artifact"),
    ("state", toJson "succeeded"),
    ("observationIdentity", toJson (some "remote-fixture" : Option String)),
    ("observedArtifactDigest", toJson (some "sha256:artifact" : Option String)),
    ("disposition", toJson (none : Option String))
  ],
  request 21 "record-obligation" "record-obligation" [
    ("work", toJson 1),
    ("key", toJson "positive-lifecycle"),
    ("commandProfile", toJson "native-cli"),
    ("invocation", toJson "agent-workbench lifecycle"),
    ("repository", toJson "fixture"),
    ("snapshot", toJson "snapshot:current"),
    ("artifactDigest", toJson "sha256:artifact"),
    ("kind", toJson "test"),
    ("requirements", toJson ["capability"]),
    ("expectedProducer", toJson "native-cli-laws"),
    ("expectedObservation", toJson "fresh-process completion"),
    ("design", toJson 1),
    ("designRevision", toJson 1)
  ],
  request 22 "record-evidence" "record-evidence" [
    ("evidence", toJson 1),
    ("work", toJson 1),
    ("obligation", toJson "positive-lifecycle"),
    ("observedRevision", toJson 21),
    ("commandProfile", toJson "native-cli"),
    ("invocation", toJson "agent-workbench lifecycle"),
    ("exitCode", toJson (0 : Int)),
    ("repository", toJson "fixture"),
    ("snapshot", toJson "snapshot:current"),
    ("artifactDigest", toJson "sha256:artifact"),
    ("kind", toJson "test"),
    ("requirements", toJson ["capability"]),
    ("producer", toJson "native-cli-laws"),
    ("observedAt", toJson "fresh-process completion"),
    ("design", toJson 1),
    ("designRevision", toJson 1)
  ],
  request 23 "complete-work" "complete-work" [
    ("work", toJson 1)
  ]
]

def invoke (cli : System.FilePath) (arguments : Array String) : IO IO.Process.Output := do
  let result ← IO.Process.output { cmd := cli.toString, args := arguments }
  unless result.exitCode == 0 do
    throw <| IO.userError s!"native CLI failed: {arguments.toList}\n{result.stderr}"
  return result

def applySources (cli root state : System.FilePath) (sources : List String) :
    IO Unit := do
  IO.FS.createDirAll root
  for (source, index) in sources.zipIdx do
    let requestPath := root / s!"focused-request-{index}.json"
    IO.FS.writeFile requestPath source
    let _ ← invoke cli #["--state", state.toString, "apply", requestPath.toString]

def testAggregateLifecycle (cli root : System.FilePath) : IO Unit := do
  let state := root / "aggregate-lifecycle.sqlite3"
  let _ ← invoke cli #[
    "--state", state.toString, "init", "owner",
    "deliver an aggregate outcome", "both ordered phases and release complete"]
  let phaseA := Json.mkObj [
    ("key", toJson "phase-a"), ("group", toJson "delivery"),
    ("order", toJson 1), ("dependencies", toJson ([] : List String)),
    ("tasks", toJson ["task-a"]), ("reviews", toJson ([] : List Nat))]
  let phaseB := Json.mkObj [
    ("key", toJson "phase-b"), ("group", toJson "delivery"),
    ("order", toJson 2), ("dependencies", toJson ["phase-a"]),
    ("tasks", toJson ["task-b"]), ("reviews", toJson [8001])]
  let phaseScope : List (String × Json) := [
    ("design", toJson (none : Option Nat)),
    ("work", toJson 1),
    ("phase", toJson (some "phase-b" : Option String)),
    ("repositorySnapshot", toJson "snapshot:aggregate"),
    ("artifactDigest", toJson "sha256:aggregate"),
    ("purpose", toJson "implementation-quality")]
  let resultingScope (key : String) (work : Nat) (outcome boundary : String) :=
    Json.mkObj [
      ("key", toJson key), ("work", toJson work), ("owner", toJson "owner"),
      ("outcome", toJson outcome), ("completionBoundary", toJson boundary)]
  let externalFields : List (String × Json) := [
    ("externalOperation", toJson "release-aggregate"),
    ("work", toJson (some 1 : Option Nat)),
    ("kind", toJson "release"),
    ("target", toJson "release:aggregate"),
    ("expectedRemoteArtifactDigest", toJson (none : Option String)),
    ("artifactDigest", toJson "sha256:aggregate")]
  applySources cli root state [
    request 1 "aggregate-plan" "plan-completion" [
      ("work", toJson 1), ("relatedWork", toJson ([] : List Json)),
      ("phases", Json.arr #[phaseA, phaseB]),
      ("tasks", toJson ["task-a", "task-b"]),
      ("checklists", toJson ([] : List String)),
      ("reviews", toJson ([] : List Nat)),
      ("findings", toJson ([] : List String)),
      ("validations", toJson ([] : List String)),
      ("repositories", toJson ([] : List String)),
      ("corrections", toJson ([] : List String)),
      ("workRecords", toJson ([] : List String))],
    request 2 "aggregate-rescope" "record-scope-change" [
      ("key", toJson "rescope-outcome"), ("work", toJson 1),
      ("kind", toJson "rescope"), ("cause", toJson "outcome"),
      ("principal", toJson "owner"),
      ("reason", toJson "the accepted outcome is narrower"),
      ("sharedRecords", toJson ["task-a", "task-b"]),
      ("dependencies", toJson ["phase-a"]),
      ("dispositions", toJson ["retain both task records"]),
      ("resultingScopes", Json.arr #[
        resultingScope "narrow-delivery" 1 "deliver the narrower outcome"
          "both ordered phases and release complete"])],
    request 3 "aggregate-split" "record-scope-change" [
      ("key", toJson "split-lifecycle"), ("work", toJson 1),
      ("kind", toJson "split"), ("cause", toJson "independent-lifecycle"),
      ("principal", toJson "owner"),
      ("reason", toJson "the deliveries now have independent lifecycles"),
      ("sharedRecords", toJson ["task-a", "task-b"]),
      ("dependencies", toJson ["phase-a"]),
      ("dispositions", toJson ["retain phase dependency"]),
      ("resultingScopes", Json.arr #[
        resultingScope "delivery-a" 2 "deliver independent result a"
          "result a reaches its own terminal state",
        resultingScope "delivery-b" 3 "deliver independent result b"
          "result b reaches its own terminal state"])],
    request 4 "aggregate-task-a" "complete-task" [
      ("work", toJson 1), ("key", toJson "task-a")],
    request 5 "aggregate-phase-a" "complete-phase" [
      ("work", toJson 1), ("key", toJson "phase-a")],
    request 6 "aggregate-task-b" "complete-task" [
      ("work", toJson 1), ("key", toJson "task-b")],
    request 7 "aggregate-phase-review-plan" "record-review-plan" <| [
      ("plan", toJson 8001), ("owner", toJson "owner"),
      ("reviewer", toJson "phase-reviewer"), ("adjudicator", toJson "owner"),
      ("caller", toJson "owner")
    ] ++ phaseScope,
    request 8 "aggregate-phase-review-claim" "record-review-claim" <| [
      ("review", toJson 8001), ("plan", toJson 8001), ("epoch", toJson 5),
      ("claim", toJson "clean"), ("reviewer", toJson "phase-reviewer")
    ] ++ phaseScope,
    request 9 "aggregate-phase-review-adjudication"
      "record-review-adjudication" [
      ("review", toJson 8001), ("decision", toJson "accepted"),
      ("adjudicator", toJson "owner"),
      ("reason", toJson "the phase result meets its accepted scope")],
    request 10 "aggregate-phase-b" "complete-phase" [
      ("work", toJson 1), ("key", toJson "phase-b")],
    request 11 "aggregate-release-prepare" "record-external-operation"
      externalFields,
    request 12 "aggregate-release-dispatch" "advance-external-operation" <|
      externalFields ++ [
        ("state", toJson "dispatched"),
        ("observationIdentity", toJson (none : Option String)),
        ("observedArtifactDigest", toJson (none : Option String)),
        ("disposition", toJson (none : Option String))],
    request 13 "aggregate-release-uncertain" "advance-external-operation" <|
      externalFields ++ [
        ("state", toJson "uncertain"),
        ("observationIdentity", toJson (none : Option String)),
        ("observedArtifactDigest", toJson (none : Option String)),
        ("disposition", toJson (none : Option String))],
    request 14 "aggregate-release-succeeded" "advance-external-operation" <|
      externalFields ++ [
        ("state", toJson "succeeded"),
        ("observationIdentity", toJson (some "release:aggregate" : Option String)),
        ("observedArtifactDigest", toJson (some "sha256:aggregate" : Option String)),
        ("disposition", toJson (none : Option String))]
  ]
  let status ← invoke cli #["--state", state.toString, "status"]
  expect (status.stdout.contains "state: current" &&
      status.stdout.contains "revision: 15")
    "native aggregate lifecycle did not retain its exact current revision"

def testRecoveryDetails (cli root : System.FilePath) : IO Unit := do
  let state := root / "recovery-details.sqlite3"
  let _ ← invoke cli #[
    "--state", state.toString, "init", "owner",
    "review recovery details", "the scoped finding is independently verified"]
  applySources cli root state [
    request 1 "details-import-design" "import-design" [
      ("design", toJson 1),
      ("designRevision", toJson 1),
      ("predecessor", toJson (none : Option Nat)),
      ("owner", toJson "owner"),
      ("contentDigest", toJson "sha256:details-design"),
      ("requirements", toJson ["capability"]),
      ("decisions", toJson ["review the accepted capability"]),
      ("validationGates", toJson ["review-recovery"])
    ],
    request 2 "details-review-plan" "record-review-plan" <| [
      ("plan", toJson 1),
      ("owner", toJson "owner"),
      ("reviewer", toJson "reviewer"),
      ("adjudicator", toJson "owner"),
      ("caller", toJson "owner")
    ] ++ scopeFields "design" "snapshot:review" "sha256:details-design",
    request 3 "details-review-claim" "record-review-claim" <| [
      ("review", toJson 1),
      ("plan", toJson 1),
      ("epoch", toJson 0),
      ("claim", toJson "findings"),
      ("reviewer", toJson "reviewer")
    ] ++ scopeFields "design" "snapshot:review" "sha256:details-design",
    request 4 "details-finding" "record-review-finding" [
      ("key", toJson "finding-1"),
      ("review", toJson 1),
      ("blocking", toJson true),
      ("authority", toJson "capability"),
      ("failureAccount", toJson "the accepted capability is not yet demonstrated"),
      ("invariant", toJson "the capability is demonstrated"),
      ("remediationSurfaces", toJson ["fixture"])
    ]
  ]
  let unadjudicated ← invoke cli #["--state", state.toString, "status"]
  expect (unadjudicated.stdout.contains "open-findings: 1" &&
      unadjudicated.stdout.contains "finding: key=finding-1")
    "unadjudicated blocking finding disappeared from recovered status"
  applySources cli (root / "finding-actions") state [
    request 5 "details-adjudicate-finding" "adjudicate-review-finding" [
      ("key", toJson "finding-1"),
      ("adjudicator", toJson "owner"),
      ("reason", toJson "the finding is within the accepted capability"),
      ("accepted", toJson true)
    ],
    request 6 "details-close-finding" "close-review-finding" [
      ("key", toJson "finding-1"),
      ("attempt", toJson 1),
      ("evidenceDigest", toJson "sha256:fixed-evidence"),
      ("repositorySnapshot", toJson "snapshot:fixed")
    ],
    request 7 "details-verify-finding" "verify-review-finding" <| [
      ("finding", toJson "finding-1"),
      ("attempt", toJson 1),
      ("verifier", toJson "independent-verifier"),
      ("evidenceDigest", toJson "sha256:fixed-evidence"),
      ("result", toJson "verified")
    ] ++ scopeFields "design" "snapshot:fixed" "sha256:fixed-artifact",
    request 8 "details-adjudicate-verification"
      "adjudicate-finding-verification" [
      ("finding", toJson "finding-1"),
      ("attempt", toJson 1),
      ("adjudicator", toJson "owner")
    ],
    request 9 "details-record-correction" "record-user-correction" [
      ("key", toJson "correction-1"),
      ("scope", toJson "fixture behavior"),
      ("statement", toJson "preserve the accepted fixture behavior"),
      ("work", toJson (some 1 : Option Nat)),
      ("design", toJson (some 1 : Option Nat))
    ]
  ]
  let recovered ← invoke cli #["--state", state.toString, "status"]
  expect (recovered.stdout.contains "open-findings: 0" &&
      recovered.stdout.contains
        "correction: key=correction-1 scope=fixture behavior work=some 1 design=some 1")
    "verified finding or scoped correction was not recovered exactly"

def testProjectionRepair (cli root : System.FilePath) : IO Unit := do
  let state := root / "projection-repair.sqlite3"
  let _ ← invoke cli #[
    "--state", state.toString, "init", "owner",
    "recover projection state", "native recovery returns current status"]
  let injected ← IO.Process.output {
    cmd := "sqlite3"
    args := #[state.toString, "DELETE FROM projection;"]
  }
  expect (injected.exitCode == 0)
    s!"projection fault injection failed: {injected.stderr}"
  let next ← invoke cli #["--state", state.toString, "next"]
  let commandLine ← match next.stdout.splitOn "\n" |>.find?
      (·.startsWith "command: ") with
    | some line => pure line
    | none => throw <| IO.userError s!"projection damage did not return a repair command: {next.stdout}"
  let tokens := ((commandLine.drop 9).replace "\"" "").splitOn " "
  let arguments ← match tokens with
    | "agent-workbench" :: arguments => pure arguments.toArray
    | _ => throw <| IO.userError s!"repair command was not directly executable: {commandLine}"
  let _ ← invoke cli arguments
  let status ← invoke cli #["--state", state.toString, "status"]
  expect (status.stdout.contains "state: current" &&
      status.stdout.contains "revision: 1")
    "revision-bound native repair did not restore current status"

def testCapabilityDispositionRoutes (cli root : System.FilePath) : IO Unit := do
  let state := root / "capability-disposition.sqlite3"
  let _ ← invoke cli #[
    "--state", state.toString, "init", "owner",
    "exercise public capability routes", "the selected public outcomes are durable"]
  applySources cli (root / "capability-requests") state [
    request 1 "capability-plan" "plan-completion" [
      ("work", toJson 1),
      ("relatedWork", toJson ([] : List Json)),
      ("phases", toJson ([] : List String)),
      ("tasks", toJson ([] : List String)),
      ("checklists", toJson ([] : List String)),
      ("reviews", toJson ([] : List Nat)),
      ("findings", toJson ([] : List String)),
      ("validations", toJson ["capability-validation"]),
      ("repositories", toJson ["capability-repository"]),
      ("corrections", toJson ([] : List String)),
      ("workRecords", toJson ["repository-change"])
    ],
    request 2 "repository-evidence" "record-repository-evidence" [
      ("work", toJson 1),
      ("key", toJson "repository-change"),
      ("repository", toJson "agent-workbench"),
      ("snapshot", toJson "tree:phase-34"),
      ("commit", toJson "commit:phase-34"),
      ("changedFiles", toJson [
        "AgentWorkbench/Cli/Program.lean",
        "skills/agent-workbench/SKILL.md"
      ])
    ],
    request 3 "capability-validation" "pass-validation" [
      ("work", toJson 1),
      ("key", toJson "capability-validation"),
      ("artifactDigest", toJson "sha256:capability-validation")
    ],
    request 4 "capability-repository" "classify-repository" [
      ("work", toJson 1),
      ("key", toJson "capability-repository"),
      ("snapshotDigest", toJson "tree:phase-34")
    ],
    request 5 "kpt-context" "record-kpt" [
      ("key", toJson "kpt-context"),
      ("work", toJson 1),
      ("keep", toJson ["retain caller adjudication"]),
      ("problem", toJson ["public capability route was missing"]),
      ("try", toJson ["add one bounded route"]),
      ("learningCandidate", toJson (none : Option String))
    ],
    request 6 "kpt-candidate" "record-kpt" [
      ("key", toJson "kpt-candidate"),
      ("work", toJson 1),
      ("keep", toJson ["retain focused tests"]),
      ("problem", toJson ([] : List String)),
      ("try", toJson ["record explicit exports"]),
      ("learningCandidate", toJson
        (some "one selected class per export" : Option String))
    ],
    request 7 "capability-complete" "complete-work" [
      ("work", toJson 1)
    ]
  ]
  let status ← invoke cli #["--state", state.toString, "status"]
  expect (status.stdout.contains "revision: 8" &&
      status.stdout.contains "active: none" &&
      status.stdout.contains "open-corrections: 0" &&
      !status.stdout.contains "correction: key=kpt")
    "KPT context changed authority, freshness, or completion readiness"
  let correctionExport := root / "correction-export.txt"
  let reviewExport := root / "review-export.txt"
  let ledgerExport := root / "ledger-export.txt"
  let _ ← invoke cli #[
    "--state", state.toString, "export", "phase-34",
    "correction", correctionExport.toString]
  let _ ← invoke cli #[
    "--state", state.toString, "export", "phase-34",
    "review", reviewExport.toString]
  let _ ← invoke cli #[
    "--state", state.toString, "export", "phase-34",
    "ledger", ledgerExport.toString]
  let correctionText ← IO.FS.readFile correctionExport
  let reviewText ← IO.FS.readFile reviewExport
  let ledgerText ← IO.FS.readFile ledgerExport
  expect (correctionText.contains "kpt-context" &&
      correctionText.contains "kpt-candidate" &&
      !reviewText.contains "kpt-context" &&
      !ledgerText.contains "kpt-context" &&
      ledgerText.contains "history-digest=sha3-256:")
    s!"focused export included an unselected private class or lost selected data\ncorrection={correctionText}\nreview={reviewText}\nledger={ledgerText}"
  let repeatedExport ← IO.Process.output {
    cmd := cli.toString
    args := #[
      "--state", state.toString, "export", "phase-34",
      "correction", correctionExport.toString]
  }
  expect (repeatedExport.exitCode != 0 &&
      (← IO.FS.readFile correctionExport) == correctionText)
    "exclusive export overwrote an existing output"
  let uncertainExport := root / "uncertain-export.txt"
  let uncertainResult ← IO.Process.output {
    cmd := cli.toString
    args := #[
      "--state", state.toString, "export", "phase-34",
      "correction", uncertainExport.toString]
    env := #[("AW_TEST_FAIL_EXPORT_PARENT_FSYNC", some "1")]
  }
  expect (uncertainResult.exitCode == 0 &&
      uncertainResult.stdout.contains "durability=uncertain" &&
      (← IO.FS.readFile uncertainExport) == correctionText)
    "published export did not report uncertain directory durability exactly"
  let diagnosis ← invoke cli #["--state", state.toString, "doctor"]
  expect (diagnosis.stdout.contains "diagnosis: healthy" &&
      diagnosis.stdout.contains "revision: 8")
    "public read-only diagnosis did not report the current ledger"

  let blockedState := root / "blocked-work.sqlite3"
  let _ ← invoke cli #[
    "--state", blockedState.toString, "init", "owner",
    "wait for an external decision", "the decision is available"]
  applySources cli (root / "blocked-request") blockedState [
    request 1 "block-by-suspension" "suspend-work" [
      ("work", toJson 1),
      ("activation", toJson 1),
      ("reason", toJson "external decision is missing"),
      ("returnPoint", toJson "resume decision handling"),
      ("assumptions", toJson ["the decision remains external"]),
      ("resumeConditions", toJson ["the decision is recorded"])
    ]
  ]
  let blockedNext ← invoke cli #["--state", blockedState.toString, "next"]
  expect (blockedNext.stdout.contains "next: blocked" &&
      blockedNext.stdout.contains "no activation is ready to resume")
    "blocked work remained executable without satisfying resume conditions"

  let followUpState := root / "follow-up.sqlite3"
  let _ ← invoke cli #[
    "--state", followUpState.toString, "init", "owner",
    "deliver the first outcome", "the first outcome is complete"]
  applySources cli (root / "follow-up-requests") followUpState [
    request 1 "first-plan" "plan-completion" [
      ("work", toJson 1),
      ("relatedWork", toJson ([] : List Json)),
      ("phases", toJson ([] : List String)),
      ("tasks", toJson ([] : List String)),
      ("checklists", toJson ([] : List String)),
      ("reviews", toJson ([] : List Nat)),
      ("findings", toJson ([] : List String)),
      ("validations", toJson ([] : List String)),
      ("repositories", toJson ([] : List String)),
      ("corrections", toJson ([] : List String)),
      ("workRecords", toJson ([] : List String))
    ],
    request 2 "first-complete" "complete-work" [
      ("work", toJson 1)
    ],
    request 3 "reopen-as-follow-up" "register-follow-up" [
      ("sourceWork", toJson 1),
      ("work", toJson 2),
      ("activation", toJson 2),
      ("owner", toJson "owner"),
      ("outcome", toJson "continue the completed outcome"),
      ("completionBoundary", toJson "the follow-up is independently complete")
    ]
  ]
  let followUpStatus ← invoke cli #["--state", followUpState.toString, "status"]
  let followUpNext ← invoke cli #["--state", followUpState.toString, "next"]
  expect (followUpStatus.stdout.contains "revision: 4" &&
      followUpStatus.stdout.contains "work=2 activation=2" &&
      followUpNext.stdout.contains "next: executable" &&
      followUpNext.stdout.contains "action: continue" &&
      followUpNext.stdout.contains "continue 4 2 2")
    "terminal predecessor and successor follow-up were not committed atomically"

def makePredecessorV2 (ledger : System.FilePath) : IO Unit := do
  let changed ← IO.Process.output {
    cmd := "sqlite3"
    args := #[ledger.toString, "
      ALTER TABLE projection_repairs RENAME TO current_projection_repairs;
      CREATE TABLE projection_repairs (
        observed_digest TEXT NOT NULL,
        head_revision TEXT NOT NULL,
        history_digest TEXT NOT NULL,
        adopted_digest TEXT NOT NULL,
        PRIMARY KEY (observed_digest, head_revision, history_digest)
      );
      INSERT INTO projection_repairs
        (observed_digest, head_revision, history_digest, adopted_digest)
      SELECT observed_digest, head_revision, history_digest, adopted_digest
      FROM current_projection_repairs;
      DROP TABLE current_projection_repairs;
      INSERT INTO update_provenance
        (singleton, source_schema, source_digest, backup_digest, backup_size)
      VALUES (1, '1', 'historical-source', 'historical-backup', '0');
      UPDATE metadata SET schema_version = '2' WHERE singleton = 1;"]
  }
  expect (changed.exitCode == 0)
    s!"public update predecessor setup failed: {changed.stderr}"

def argumentsAfter (marker output : String) : IO (Array String) :=
  match output.splitOn "\n" |>.find? (·.startsWith marker) with
  | some line =>
      pure ((line.drop marker.length).toString.splitOn " ").toArray
  | none => throw <| IO.userError s!"missing command arguments: {marker}"

def testPublicUpdateAndRestore (cli root : System.FilePath) : IO Unit := do
  let state := root / "public-update.sqlite3"
  let _ ← invoke cli #[
    "--state", state.toString, "init", "owner",
    "exercise update recovery", "the original state is restorable"]
  makePredecessorV2 state
  let inspection ← invoke cli #["--state", state.toString, "update", "inspect"]
  let applyArgs ← argumentsAfter "arguments: " inspection.stdout
  let updated ← invoke cli <| #["--state", state.toString] ++ applyArgs
  expect (updated.stdout.contains "updated: true" &&
      updated.stdout.contains "backup-digest:")
    "public update did not report its content-addressed backup"
  let restoreArgs ← argumentsAfter "restore-arguments: " updated.stdout
  let restored ← invoke cli <| #["--state", state.toString] ++ restoreArgs
  expect (restored.stdout.contains "restored: true" &&
      restored.stdout.contains "schema: 2")
    "public restore did not recover the exact predecessor"
  let requiredAgain ← invoke cli #["--state", state.toString, "update", "inspect"]
  expect (requiredAgain.stdout.contains "update: required")
    "restored predecessor was not recoverable through the same dry-run path"

def runFixture (cli root project state : System.FilePath)
    (sourceName source tool : String) : IO Unit := do
  IO.FS.createDirAll project
  let sourcePath := project / sourceName
  IO.FS.writeFile sourcePath source
  let before ← IO.FS.readFile sourcePath
  let toolBefore ← IO.Process.output {
    cmd := tool
    args := #[sourceName]
    cwd := some project
  }
  expect (toolBefore.exitCode == 0)
    s!"managed-project toolchain failed before Workbench use: {toolBefore.stderr}"
  let _ ← invoke cli #[
    "--state", state.toString, "init", "owner",
    "deliver the fixture", "complete the accepted fixture work"]
  let initialNext ← invoke cli #["--state", state.toString, "next"]
  let expectedContinue :=
    s!"command: agent-workbench --state {repr state.toString} continue 1 1 1"
  expect (initialNext.stdout.contains "action: continue" &&
      initialNext.stdout.contains expectedContinue)
    "initialized state did not return the exact revision-bound continue command"
  let _ ← invoke cli #[
    "--state", state.toString, "continue", "1", "1", "1"]
  for (source, index) in lifecycleRequests.zipIdx do
    let requestPath := root / s!"request-{state.fileName.getD "state"}-{index}.json"
    IO.FS.writeFile requestPath source
    let _ ← invoke cli #["--state", state.toString, "apply", requestPath.toString]
  let status ← invoke cli #["--state", state.toString, "status"]
  expect (status.stdout.contains "state: current" &&
      status.stdout.contains "revision: 24" &&
      status.stdout.contains "active: none")
    "native CLI lifecycle did not durably complete"
  let recovered ← invoke cli #["--state", state.toString, "status"]
  expect (recovered.stdout == status.stdout)
    "fresh native process did not recover the same authoritative state"
  let finalNext ← invoke cli #["--state", state.toString, "next"]
  expect (finalNext.stdout.contains "next: blocked" &&
      finalNext.stdout.contains "no activation is ready to resume")
    s!"completed lifecycle did not return its exact terminal blocker: {finalNext.stdout}"
  expect ((← IO.FS.readFile sourcePath) == before)
    "managed-project source changed while using a relocated Workbench state area"
  let toolAfter ← IO.Process.output {
    cmd := tool
    args := #[sourceName]
    cwd := some project
  }
  expect (toolAfter.exitCode == 0 && toolAfter.stdout == toolBefore.stdout)
    "managed-project toolchain behavior changed after Workbench use"

def run : IO Unit := IO.FS.withTempDir fun root => do
  let cli : System.FilePath := ".lake/build/bin/agent-workbench"
  let version ← invoke cli #["--version"]
  expect (version.stdout.trimAscii.toString == "agent-workbench 0.2.1")
    "native CLI did not report the release version"
  let nodeProject := root / "node-project"
  let pythonProject := root / "python-project"
  runFixture cli root nodeProject
    (nodeProject / ".agent-workbench" / "state.sqlite3")
    "test.js" "console.log('node-fixture')\n" "node"
  runFixture cli root pythonProject
    (root / "detached-state" / "python.sqlite3")
    "main.py" "print('python-fixture')\n" "python3"
  testAggregateLifecycle cli root
  testRecoveryDetails cli root
  testProjectionRepair cli root
  testCapabilityDispositionRoutes cli root
  testPublicUpdateAndRestore cli root
  IO.println "cli laws: pass"

end AgentWorkbench.Tests.CliLaws

def main : IO Unit :=
  AgentWorkbench.Tests.CliLaws.run
