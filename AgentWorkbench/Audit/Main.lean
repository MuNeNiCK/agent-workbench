import Lean
import AgentWorkbench.Audit.Expected
import AgentWorkbench.Cli.Program

open Lean

namespace AgentWorkbench.Audit

structure ModuleRule where
  module : String
  imports : Array String

structure TheoremRule where
  declaration : String
  expectedType : String

structure ManifestPolicy where
  theorems : Array String
  modules : Array String
  permittedAxioms : Array String
  forbiddenAxioms : Array String
  unsafeFfiModules : Array String

def designModuleMapPath : System.FilePath :=
  ".agent-workbench/designs/lean-first-core/05-building-blocks.md"

def designMapTokens (content : String) : Except String (List String) :=
  let marker := "The initial module/interface map is normative:\n\n```text\n"
  match content.splitOn marker with
  | _ :: after :: _ =>
      match after.splitOn "\n```" with
      | block :: _ =>
          .ok <| ((block.replace "\n" " ").replace "," " ").splitOn " " |>
            List.filter (fun token => !token.isEmpty)
      | _ => .error "normative module map closing fence is missing"
  | _ => .error "normative module map opening marker is missing"

def tokenSection (start stop : String) (tokens : List String) : List String :=
  ((tokens.dropWhile (· != start)).drop 1).takeWhile (· != stop)

def isQualifiedDesignToken (token : String) : Bool :=
  token.contains '.'

def qualifyDesignModule (token : String) : String :=
  "AgentWorkbench." ++ token

def resolveDesignImports (allModules l0 l1 l2 l3 : List String)
    (tokens : List String) : List String :=
  (tokens.flatMap fun token =>
    if token = "L0" then l0
    else if token = "L1" then l1
    else if token = "L2" then l2
    else if token = "L3" then l3
    else
      match allModules.find? (fun module => module.endsWith ("." ++ token)) with
      | some module => [module]
      | none => []).eraseDups

partial def parseDesignArrowRules (allModules l0 l1 l2 l3 : List String) :
    List String → Except String (Array ModuleRule)
  | [] => .ok #[]
  | module :: "->" :: rest => do
      unless isQualifiedDesignToken module do
        throw s!"invalid normative module token: {module}"
      let imports := rest.takeWhile (fun token => !isQualifiedDesignToken token)
      let remaining := rest.drop imports.length
      let tail ← parseDesignArrowRules allModules l0 l1 l2 l3 remaining
      return #[⟨qualifyDesignModule module,
        (resolveDesignImports allModules l0 l1 l2 l3 imports).toArray⟩] ++ tail
  | token :: _ => .error s!"invalid normative module rule near {token}"

def parseDesignModuleRules (content : String) : Except String (Array ModuleRule) := do
  let tokens ← designMapTokens content
  let l0Tokens := tokenSection "L0" "L1" tokens
  let l1Section := tokenSection "L1" "L2" tokens
  let l2Section := tokenSection "L2" "L3" tokens
  let l3Section := tokenSection "L3" "L4" tokens
  let l4Section := tokenSection "L4" "L5" tokens
  let l0 := (l0Tokens.filter isQualifiedDesignToken).map qualifyDesignModule
  let l1 := ((l1Section.takeWhile (· != "->")).filter isQualifiedDesignToken).map
    qualifyDesignModule
  let l2 := (l2Section.filter isQualifiedDesignToken).map qualifyDesignModule
  let l3 := (l3Section.filter isQualifiedDesignToken).map qualifyDesignModule
  let l4 := (l4Section.filter isQualifiedDesignToken).map qualifyDesignModule
  let allModules := l0 ++ l1 ++ l2 ++ l3 ++ l4
  unless !l0.isEmpty && !l1.isEmpty && !l2.isEmpty && !l3.isEmpty && !l4.isEmpty do
    throw "normative module map has an empty product layer"
  let l0Rules := l0.toArray.map fun module => ⟨module, #[]⟩
  let l1Rules := l1.toArray.map fun module => ⟨module, l0.toArray⟩
  let l2Rules ← parseDesignArrowRules allModules l0 l1 l2 l3 l2Section
  let l3Rules ← parseDesignArrowRules allModules l0 l1 l2 l3 l3Section
  let l4Rules ← parseDesignArrowRules allModules l0 l1 l2 l3 l4Section
  return l0Rules ++ l1Rules ++ l2Rules ++ l3Rules ++ l4Rules

def theoremRules : Array TheoremRule := #[
  ⟨"AgentWorkbench.Kernel.Replay.replay_deterministic", "AgentWorkbench.Audit.Expected.replay_deterministic"⟩,
  ⟨"AgentWorkbench.Kernel.Replay.replay_preserves_valid", "AgentWorkbench.Audit.Expected.replay_preserves_valid"⟩,
  ⟨"AgentWorkbench.Kernel.Replay.work_completed_event_exact", "AgentWorkbench.Audit.Expected.work_completed_event_exact"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.decide_preserves_valid", "AgentWorkbench.Audit.Expected.decide_preserves_valid"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.decide_emits_only_derived_events", "AgentWorkbench.Audit.Expected.decide_emits_only_derived_events"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.decide_rejection_has_no_effect", "AgentWorkbench.Audit.Expected.decide_rejection_has_no_effect"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.close_work_preserves_valid", "AgentWorkbench.Audit.Expected.close_work_preserves_valid"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.close_work_emits_atomic_event", "AgentWorkbench.Audit.Expected.close_work_emits_atomic_event"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.decide_complete_requires_closeable", "AgentWorkbench.Audit.Expected.decide_complete_requires_closeable"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.replay_completion_applicability_matches_policy", "AgentWorkbench.Audit.Expected.replay_completion_applicability_matches_policy"⟩,
  ⟨"AgentWorkbench.Domain.Work.single_active_activation", "AgentWorkbench.Audit.Expected.single_active_activation"⟩,
  ⟨"AgentWorkbench.Domain.Work.resume_requires_readiness", "AgentWorkbench.Audit.Expected.resume_requires_readiness"⟩,
  ⟨"AgentWorkbench.Policy.Authority.review_claim_has_no_authority", "AgentWorkbench.Audit.Expected.review_claim_has_no_authority"⟩,
  ⟨"AgentWorkbench.Kernel.Gates.gate_is_read_only", "AgentWorkbench.Audit.Expected.gate_is_read_only"⟩,
  ⟨"AgentWorkbench.Kernel.Gates.all_gates_are_read_only", "AgentWorkbench.Audit.Expected.gates_all_read_only"⟩,
  ⟨"AgentWorkbench.Kernel.Resolver.next_is_allowed", "AgentWorkbench.Audit.Expected.next_is_allowed"⟩,
  ⟨"AgentWorkbench.Application.Service.status_is_read_only", "AgentWorkbench.Audit.Expected.status_is_read_only"⟩,
  ⟨"AgentWorkbench.Application.Service.next_is_read_only", "AgentWorkbench.Audit.Expected.next_is_read_only"⟩,
  ⟨"AgentWorkbench.Application.Service.every_gate_is_read_only", "AgentWorkbench.Audit.Expected.every_gate_is_read_only"⟩,
  ⟨"AgentWorkbench.Kernel.Projection.verified_stage_matches_replay", "AgentWorkbench.Audit.Expected.verified_stage_matches_replay"⟩,
  ⟨"AgentWorkbench.Kernel.Projection.adoption_is_atomic", "AgentWorkbench.Audit.Expected.adoption_is_atomic"⟩,
  ⟨"AgentWorkbench.Policy.Completion.completion_requires_current_obligations", "AgentWorkbench.Audit.Expected.completion_requires_current_obligations"⟩,
  ⟨"AgentWorkbench.Policy.Completion.completion_requires_authoritative_lifecycle", "AgentWorkbench.Audit.Expected.completion_requires_authoritative_lifecycle"⟩,
  ⟨"AgentWorkbench.Policy.Completion.completion_requires_active_target", "AgentWorkbench.Audit.Expected.completion_requires_active_target"⟩,
  ⟨"AgentWorkbench.Policy.Update.exact_retry_returns_same_receipt", "AgentWorkbench.Audit.Expected.exact_retry_returns_same_receipt"⟩
]

def expectedPermittedAxioms : Array String :=
  #["propext", "Quot.sound", "Classical.choice"]

def expectedForbiddenAxioms : Array String := #["sorryAx"]

def expectedUnsafeFfiModules : Array String := #[
  "AgentWorkbench.Adapter.SQLite",
  "AgentWorkbench.Adapter.DurableFilesystem",
  "AgentWorkbench.Adapter.Process",
  "AgentWorkbench.Adapter.Git"
]

def expectedCliEntrypoint : String :=
  "import AgentWorkbench.Cli.Program\n\ndef main : IO Unit :=\n  AgentWorkbench.Cli.Program.run\n"

def fail (message : String) : IO α :=
  throw <| IO.userError s!"verified-core audit failed: {message}"

def parseEntries (key : String) : List String → Array String → Except String (Array String)
  | [], _ => .error s!"unterminated manifest array: {key}"
  | line :: rest, values =>
      let trimmed := line.trimAscii.toString
      if trimmed = "]" then
        .ok values
      else if trimmed.isEmpty then
        parseEntries key rest values
      else
        let value := if trimmed.endsWith "," then (trimmed.dropEnd 1).toString else trimmed
        if value.startsWith "\"" && value.endsWith "\"" && value.length ≥ 2 then
          parseEntries key rest (values.push <| ((value.drop 1).dropEnd 1).toString)
        else
          .error s!"invalid manifest value in {key}: {trimmed}"

def parseManifestArray (key content : String) : Except String (Array String) :=
  let marker := s!"{key} = ["
  match (content.splitOn "\n").dropWhile (·.trimAscii.toString != marker) with
  | [] => .error s!"missing manifest array: {key}"
  | _ :: rest => parseEntries key rest #[]

def parseManifestPolicy (content : String) : Except String ManifestPolicy := do
  unless (content.splitOn "\n").any (·.trimAscii.toString = "version = 1") do
    throw "manifest version must be exactly 1"
  return {
    theorems := ← parseManifestArray "theorems" content
    modules := ← parseManifestArray "modules" content
    permittedAxioms := ← parseManifestArray "permitted_axioms" content
    forbiddenAxioms := ← parseManifestArray "forbidden_axioms" content
    unsafeFfiModules := ← parseManifestArray "unsafe_ffi_modules" content }

def validateManifestPolicy (designRules : Array ModuleRule)
    (policy : ManifestPolicy) : Except String Unit := do
  unless policy.theorems = theoremRules.map (·.declaration) do
    throw "theorem manifest differs from immutable Lean policy"
  unless policy.modules = designRules.map (·.module) do
    throw "module manifest differs from the approved Design Package module map"
  unless policy.permittedAxioms = expectedPermittedAxioms do
    throw "permitted axiom manifest differs from immutable Lean policy"
  unless policy.forbiddenAxioms = expectedForbiddenAxioms do
    throw "forbidden axiom manifest differs from immutable Lean policy"
  unless policy.unsafeFfiModules = expectedUnsafeFfiModules do
    throw "unsafe/FFI boundary differs from immutable Lean policy"

def validateManifestContent (designRules : Array ModuleRule)
    (content : String) : Except String ManifestPolicy := do
  let policy ← parseManifestPolicy content
  validateManifestPolicy designRules policy
  return policy

def auditNegativeFixtures (designRules : Array ModuleRule) (manifest : String) : IO Unit := do
  let fixtures : Array (String × String) := #[
    ("missing-theorem", manifest.replace
      "AgentWorkbench.Kernel.Replay.replay_deterministic"
      "AgentWorkbench.Kernel.Replay.unapproved_replacement"),
    ("expanded-axioms", manifest.replace
      "  \"Classical.choice\"," "  \"Classical.choice\",\n  \"sorryAx\","),
    ("unsafe-boundary", manifest.replace
      "AgentWorkbench.Adapter.SQLite" "AgentWorkbench.Application.Service")
  ]
  for (name, fixture) in fixtures do
    if fixture = manifest then fail s!"negative fixture did not mutate manifest: {name}"
    match validateManifestContent designRules fixture with
    | .error _ => pure ()
    | .ok _ => fail s!"negative manifest fixture was accepted: {name}"

def modulePath (module : String) : System.FilePath :=
  System.FilePath.mk <| (module.replace "." "/") ++ ".lean"

def sourceImports (module : String) : IO (Array String) := do
  let source ← IO.FS.readFile (modulePath module)
  return (source.splitOn "\n").foldl (init := #[]) fun imports line =>
    let trimmed := line.trimAscii.toString
    if trimmed.startsWith "import " then imports.push (trimmed.drop 7).trimAscii.toString
    else imports

def directImports (rules : Array ModuleRule) (module : String) : List String :=
  match rules.find? (·.module = module) with
  | some rule => rule.imports.toList
  | none => []

def directDependents (rules : Array ModuleRule) (module : String) : List String :=
  (rules.filterMap fun rule =>
    if rule.imports.contains module then some rule.module else none).toList

partial def transitiveVisit (next : String → List String) : List String → List String → List String
  | [], visited => visited
  | module :: remaining, visited =>
      if visited.contains module then transitiveVisit next remaining visited
      else transitiveVisit next (next module ++ remaining) (module :: visited)

def importClosure (rules : Array ModuleRule) (module : String) : List String :=
  transitiveVisit (directImports rules) (directImports rules module) []

def dependentClosure (rules : Array ModuleRule) (module : String) : List String :=
  transitiveVisit (directDependents rules) (directDependents rules module) []

def auditReverificationBounds (actual : Array ModuleRule) : IO Unit := do
  for rule in actual do
    if (importClosure actual rule.module).contains rule.module then
      fail s!"module dependency cycle reaches {rule.module}"
  let workDependents := dependentClosure actual "AgentWorkbench.Domain.Work"
  for sibling in #["AgentWorkbench.Domain.Design", "AgentWorkbench.Domain.Review",
      "AgentWorkbench.Domain.Evidence", "AgentWorkbench.Domain.ExternalOperation"] do
    if workDependents.contains sibling then
      fail s!"Domain.Work change reaches sibling module {sibling}"
  let completionDependents := dependentClosure actual "AgentWorkbench.Policy.Completion"
  for lower in actual.filter fun rule => rule.module.startsWith "AgentWorkbench.Domain." do
    if completionDependents.contains lower.module then
      fail s!"Policy.Completion change reaches lower-level module {lower.module}"
  unless workDependents.contains "AgentWorkbench.Application.Service" do
    fail "Domain.Work dependent closure does not reach the application boundary"
  unless completionDependents.contains "AgentWorkbench.Application.Service" do
    fail "Policy.Completion dependent closure does not reach the application boundary"

def importsWithinDesign (designRules actualRules : Array ModuleRule) : Bool :=
  actualRules.all fun actual =>
    match designRules.find? (·.module = actual.module) with
    | none => false
    | some allowed => actual.imports.all allowed.imports.contains

def auditArchitecture (designRules : Array ModuleRule) : IO Unit := do
  let mut actualRules : Array ModuleRule := #[]
  for rule in designRules do
    unless ← (modulePath rule.module).pathExists do
      fail s!"missing normative module {rule.module}"
    let actual ← sourceImports rule.module
    actualRules := actualRules.push ⟨rule.module, actual⟩
    for imported in actual do
      unless rule.imports.contains imported do
        fail s!"{rule.module} imports forbidden dependency {imported}"
  unless importsWithinDesign designRules actualRules do
    fail "actual product imports are not a subset of Design Package authority"
  let invalidFixture := actualRules.map fun rule =>
    if rule.module = "AgentWorkbench.Domain.Work" then
      { rule with imports := rule.imports.push "AgentWorkbench.Domain.Review" }
    else rule
  if importsWithinDesign designRules invalidFixture then
    fail "negative out-of-map import fixture was accepted"
  auditReverificationBounds actualRules
  let cliImports ← sourceImports "Main"
  unless cliImports = #["AgentWorkbench.Cli.Program"] do
    fail "executable entrypoint must import only AgentWorkbench.Cli.Program"
  let cli ← IO.FS.readFile "Main.lean"
  unless cli = expectedCliEntrypoint do
    fail "executable entrypoint differs from the immutable compiled CLI wrapper"
  let programImports ← sourceImports "AgentWorkbench.Cli.Program"
  unless programImports = #["AgentWorkbench.Application.Service"] do
    fail "CLI program must import only AgentWorkbench.Application.Service"
  let program ← IO.FS.readFile (modulePath "AgentWorkbench.Cli.Program")
  for forbidden in #["AgentWorkbench.Domain", "AgentWorkbench.Policy", "AgentWorkbench.Kernel",
      "Domain.", "Policy.", "Kernel."] do
    if program.contains forbidden then
      fail s!"CLI program bypasses Application.Service through {forbidden}"
  let spoofed :=
    "import AgentWorkbench.Cli.Program\n\n-- Application.Service.execute Application.Service.bootstrapCommand\ndef main : IO Unit := pure ()\n"
  if spoofed = expectedCliEntrypoint then fail "negative CLI source fixture was accepted"

def declarationDependencies (info : ConstantInfo) : List Name :=
  let used (expr : Expr) := expr.getUsedConstants.toList
  match info with
  | .axiomInfo value => used value.type
  | .defnInfo value => used value.type ++ used value.value
  | .thmInfo value => used value.type ++ used value.value
  | .opaqueInfo value => used value.type ++ used value.value
  | .quotInfo _ => []
  | .inductInfo value => used value.type ++ value.ctors
  | .ctorInfo value => used value.type
  | .recInfo value => used value.type

def declarationValue? : ConstantInfo → Option Expr
  | .defnInfo value => some value.value
  | .thmInfo value => some value.value
  | .opaqueInfo value => some value.value
  | _ => none

partial def auditUnsafeReachability (env : Environment) : List Name → List Name → IO Unit
  | [], _ => pure ()
  | name :: rest, visited => do
      if visited.contains name then
        auditUnsafeReachability env rest visited
      else
        match env.find? name with
        | none => auditUnsafeReachability env rest (name :: visited)
        | some info =>
            if info.isUnsafe then fail s!"unsafe declaration reachable from verified roots: {name}"
            auditUnsafeReachability env (declarationDependencies info ++ rest) (name :: visited)

partial def declarationReaches (env : Environment) (target : Name) :
    List Name → List Name → Bool
  | [], _ => false
  | name :: rest, visited =>
      if name = target then true
      else if visited.contains name then declarationReaches env target rest visited
      else
        match env.find? name with
        | none => declarationReaches env target rest (name :: visited)
        | some info =>
            declarationReaches env target (declarationDependencies info ++ rest) (name :: visited)

def auditDeclarations (env : Environment) : IO Unit := do
  for (name, info) in env.constants.toList do
    let rendered := name.toString
    if rendered.startsWith "AgentWorkbench." &&
        !rendered.startsWith "AgentWorkbench.Audit." &&
        !rendered.startsWith "AgentWorkbench.Tests." && info.isUnsafe then
      fail s!"unsafe declaration entered normative implementation: {rendered}"
  let roots := theoremRules.toList.map (·.declaration.toName) ++
    [`AgentWorkbench.Application.Service.execute,
     `AgentWorkbench.Application.Service.executeAction,
     `AgentWorkbench.Application.Service.bootstrapCommand,
     `AgentWorkbench.Cli.Program.executeBootstrap,
     `AgentWorkbench.Cli.Program.executeRequest,
     `AgentWorkbench.Cli.Program.run]
  auditUnsafeReachability env roots []
  unless declarationReaches env `AgentWorkbench.Application.Service.execute
      [`AgentWorkbench.Cli.Program.run] [] do
    let runDeps := (env.find? `AgentWorkbench.Cli.Program.run).map declarationDependencies
    let bootstrapDeps := (env.find? `AgentWorkbench.Cli.Program.executeBootstrap).map declarationDependencies
    fail s!"compiled CLI program does not reach Application.Service.execute; run={repr runDeps}; bootstrap={repr bootstrapDeps}"
  unless declarationReaches env `AgentWorkbench.Application.Service.executeRequest
      [`AgentWorkbench.Cli.Program.executeRequest] [] do
    fail "compiled CLI request path does not reach Application.Service.executeRequest"
  unless declarationReaches env `AgentWorkbench.Application.Service.executeAction
      [`AgentWorkbench.Cli.Program.executeRequest] [] do
    fail "compiled CLI request path does not reach resolver action execution"
  unless declarationReaches env `AgentWorkbench.Application.Service.execute
      [`AgentWorkbench.Application.Service.executeAction] [] do
    fail "resolver action execution does not reach governed mutation"
  unless declarationReaches env `AgentWorkbench.Application.Service.repairProjection
      [`AgentWorkbench.Cli.Program.executeRequest] [] do
    fail "compiled CLI request path does not reach the explicit projection repair mutation"
  if declarationReaches env `AgentWorkbench.Application.Service.execute
      [`AgentWorkbench.Audit.Expected.cliWithoutMutationFixture] [] then
    fail "negative compiled CLI dependency fixture unexpectedly reaches Service.execute"
  unless declarationReaches env `AgentWorkbench.Application.Service.execute
      [`AgentWorkbench.Audit.Expected.cliConditionalBypassFixture] [] do
    fail "conditional CLI bypass fixture did not retain its dead Service.execute dependency"
  let context : Core.Context := {
    fileName := "<verified-core-cli-audit>", fileMap := FileMap.ofString "" }
  let state : Core.State := { env := env }
  let compareCompiled (actualName expectedName : Name) : IO Bool := do
    let some actual := env.find? actualName | fail s!"compiled declaration missing: {actualName}"
    let some expected := env.find? expectedName | fail s!"expected declaration missing: {expectedName}"
    let some actualValue := declarationValue? actual |
      fail s!"compiled declaration has no inspectable value: {actualName}"
    let some expectedValue := declarationValue? expected |
      fail s!"expected declaration has no inspectable value: {expectedName}"
    let (sameType, _, _) ← (Meta.withTransparency .all <|
      Meta.isDefEq actual.type expected.type).toIO context state
    let (sameValue, _, _) ← (Meta.withTransparency .all <|
      Meta.isDefEq actualValue expectedValue).toIO context state
    return sameType && sameValue
  unless ← compareCompiled `AgentWorkbench.Cli.Program.executeBootstrap
      `AgentWorkbench.Audit.Expected.expectedExecuteBootstrap do
    fail "compiled CLI bootstrap differs from immutable expected implementation"
  unless ← compareCompiled `AgentWorkbench.Cli.Program.run
      `AgentWorkbench.Audit.Expected.expectedCliRun do
    fail "compiled CLI control flow differs from immutable expected implementation"
  if ← compareCompiled `AgentWorkbench.Audit.Expected.cliConditionalBypassFixture
      `AgentWorkbench.Audit.Expected.expectedCliRun then
    fail "conditional no-mutation CLI fixture matched expected CLI behavior"

def auditTheorems (env : Environment) : IO Unit := do
  let context : Core.Context := { fileName := "<verified-core-audit>", fileMap := FileMap.ofString "" }
  let state : Core.State := { env := env }
  for rule in theoremRules do
    let name := rule.declaration.toName
    let expectedName := rule.expectedType.toName
    let some info := env.find? name | fail s!"required declaration is absent: {rule.declaration}"
    let some expected := env.find? expectedName |
      fail s!"internal expected signature is absent: {rule.expectedType}"
    match info with
    | .thmInfo _ => pure ()
    | _ => fail s!"required declaration is not a theorem: {rule.declaration}"
    let (sameType, _, _) ← (Meta.isDefEq info.type expected.type).toIO context state
    unless sameType do
      fail s!"theorem type differs from immutable Lean signature: {rule.declaration}"
    if info.isUnsafe then fail s!"required theorem is unsafe: {rule.declaration}"
    let collect : CoreM (Array Name) := collectAxioms name
    let axioms ← collect.toIO' context state
    for axiomName in axioms do
      let rendered := toString axiomName
      if expectedForbiddenAxioms.contains rendered then
        fail s!"forbidden axiom {rendered} reached {rule.declaration}"
      unless expectedPermittedAxioms.contains rendered do
        fail s!"unpermitted axiom {rendered} reached {rule.declaration}"

def auditCliMutation : IO Unit := do
  let initial := Application.Service.initialStore
  match Cli.Program.executeBootstrap with
  | .error error => fail s!"CLI mutation fixture was rejected: {repr error}"
  | .ok transaction =>
      unless transaction.accepted.events.length = 1 do
        fail "CLI mutation did not emit exactly one event"
      unless transaction.accepted.result.state.revision = initial.ledger.storedHead.next do
        fail "CLI mutation did not advance exactly one revision"
      unless (Application.Service.queryValidity transaction.result).value = .pass do
        fail "CLI mutation result is not valid"

def traceModuleRules (designRules : Array ModuleRule) : Array ModuleRule := designRules ++ #[
  ⟨"AgentWorkbench", #["AgentWorkbench.Application.Service"]⟩,
  ⟨"AgentWorkbench.Cli.Program", #["AgentWorkbench.Application.Service"]⟩,
  ⟨"Main", #["AgentWorkbench.Cli.Program"]⟩,
  ⟨"AgentWorkbench.Tests.KernelLaws", #["AgentWorkbench.Cli.Program"]⟩,
  ⟨"AgentWorkbench.Audit.Expected", #["AgentWorkbench.Application.Service"]⟩,
  ⟨"AgentWorkbench.Audit.Main", #["AgentWorkbench.Audit.Expected", "AgentWorkbench.Cli.Program"]⟩
]

def traceProjectFiles (designRules : Array ModuleRule) : Array String :=
  designRules.map (fun rule => (rule.module.replace "." "/") ++ ".lean") ++ #[
  "AgentWorkbench.lean",
  "AgentWorkbench/Audit/Expected.lean",
  "AgentWorkbench/Audit/Main.lean",
  "AgentWorkbench/Cli/Program.lean",
  "AgentWorkbench/Tests/KernelLaws.lean",
  "Main.lean",
  "lake-manifest.json",
  "lakefile.toml",
  "lean-toolchain",
  "proof-manifest.toml"
]

structure RebuildTraceCase where
  key : String
  module : String
  file : String
  privateProof : Bool := false

def rebuildTraceCases : Array RebuildTraceCase := #[
  ⟨"private-proof", "AgentWorkbench.Domain.Work", "AgentWorkbench/Domain/Work.lean", true⟩,
  ⟨"domain-work", "AgentWorkbench.Domain.Work", "AgentWorkbench/Domain/Work.lean", false⟩,
  ⟨"policy-completion", "AgentWorkbench.Policy.Completion", "AgentWorkbench/Policy/Completion.lean", false⟩,
  ⟨"domain-identity", "AgentWorkbench.Domain.Identity", "AgentWorkbench/Domain/Identity.lean", false⟩
]

def copyTraceProject (target : System.FilePath) (designRules : Array ModuleRule) : IO Unit := do
  for relative in traceProjectFiles designRules do
    let source := System.FilePath.mk relative
    let destination := target / relative
    if let some parent := destination.parent then IO.FS.createDirAll parent
    IO.FS.writeFile destination (← IO.FS.readFile source)

def runLakeBuild (lake project : System.FilePath) (targets : Array String := #[])
    (oldMode : Bool := false) : IO String := do
  let argsPrefix := if oldMode then #["--old", "build"] else #["build"]
  let output ← IO.Process.output {
    cmd := lake.toString, args := argsPrefix ++ targets, cwd := some project }
  unless output.exitCode = 0 do
    fail s!"representative Lake rebuild failed:\n{output.stdout}\n{output.stderr}"
  return output.stdout ++ "\n" ++ output.stderr

def builtModules (output : String) : List String :=
  ((output.splitOn "\n").filterMap fun line =>
    match line.splitOn "Built " with
    | _ :: built :: _ =>
        match built.trimAscii.toString.splitOn " " with
        | token :: _ =>
            let module := (token.splitOn ":").head?.getD token
            if module = "Main" || module = "AgentWorkbench" ||
                module.startsWith "AgentWorkbench." then some module else none
        | _ => none
    | _ => none).eraseDups

def mutateTraceSource (case : RebuildTraceCase) (source : String) : Except String String :=
  if case.privateProof then
    let before :=
      "theorem single_active_activation {activations : List Activation}\n    (valid : AtMostOneActive activations) :\n    (activeActivations activations).length ≤ 1 := by\n  exact valid"
    let after :=
      "theorem single_active_activation {activations : List Activation}\n    (valid : AtMostOneActive activations) :\n    (activeActivations activations).length ≤ 1 := by\n  assumption "
    let changed := source.replace before after
    if changed = source then .error "private-proof trace mutation anchor was not found" else .ok changed
  else
    .ok <| source ++ s!"\n\ndef agentWorkbenchTraceProbe_{case.key.replace "-" "_"} : Unit := ()\n"

def allowedTraceModules (normativeRules : Array ModuleRule)
    (case : RebuildTraceCase) : List String :=
  if case.privateProof then [case.module]
  else case.module :: dependentClosure normativeRules case.module

def requiredTraceModules (actualRules : Array ModuleRule)
    (case : RebuildTraceCase) : List String :=
  if case.privateProof then [case.module]
  else case.module :: dependentClosure actualRules case.module

def traceSatisfiesBounds (actualRules : Array ModuleRule)
    (normativeRules : Array ModuleRule) (case : RebuildTraceCase)
    (trace : List String) : Bool :=
  let allowed := allowedTraceModules normativeRules case
  let required := requiredTraceModules actualRules case
  required.all trace.contains && trace.all allowed.contains

def auditIncompleteTraceFixture (actualRules : Array ModuleRule)
    (normativeRules : Array ModuleRule) (case : RebuildTraceCase) : IO Unit := do
  if !case.privateProof then
    let required := requiredTraceModules actualRules case
    match required.reverse with
    | [] => fail s!"public trace {case.key} has no required modules"
    | missing :: _ =>
        let incomplete := required.filter (· != missing)
        if traceSatisfiesBounds actualRules normativeRules case incomplete then
          fail s!"incomplete public trace fixture was accepted for {case.key}; missing {missing}"

def auditRebuildTrace (lake project : System.FilePath) (actualRules : Array ModuleRule)
    (normativeRules : Array ModuleRule) (case : RebuildTraceCase) : IO Unit := do
  unless normativeRules.any (fun rule => rule.module = case.module) do
    fail s!"trace owner {case.module} is outside the normative module map"
  if case.privateProof && (dependentClosure normativeRules case.module).isEmpty then
    fail s!"private-proof trace owner {case.module} has no normative importers"
  if case.privateProof && !theoremRules.any (fun rule =>
      rule.declaration = "AgentWorkbench.Domain.Work.single_active_activation") then
    fail "private-proof trace theorem is not protected by the immutable theorem policy"
  let path := project / case.file
  let original ← IO.FS.readFile path
  let changed ← match mutateTraceSource case original with
    | .ok changed => pure changed
    | .error error => fail error
  IO.FS.writeFile path changed
  -- `--old` is sound only for this manifest-locked theorem proof body: its
  -- exported type is immutable and Lean proof irrelevance hides its value.
  let targets := if case.privateProof then #[] else
    #["AgentWorkbench", "agent-workbench", "kernel-laws", "verified-core-audit"]
  let trace := builtModules (← runLakeBuild lake project targets (oldMode := case.privateProof))
  if trace.isEmpty then fail s!"Lake emitted no rebuild trace for {case.key}"
  unless trace.contains case.module do
    fail s!"Lake did not rebuild changed module {case.module} for {case.key}"
  let allowed := allowedTraceModules normativeRules case
  for rebuilt in trace do
    unless allowed.contains rebuilt do
      fail s!"{case.key} rebuilt {rebuilt} outside its normative reverse closure"
  let required := requiredTraceModules actualRules case
  for dependent in required do
    unless trace.contains dependent do
      fail s!"{case.key} omitted required reverse-dependent module {dependent}"
  unless traceSatisfiesBounds actualRules normativeRules case trace do
    fail s!"{case.key} rebuild trace does not equal its required bounded closure"
  auditIncompleteTraceFixture actualRules normativeRules case
  IO.println s!"verified-core trace {case.key}: required={String.intercalate "," required}; rebuilt={String.intercalate "," trace}"
  IO.FS.writeFile path original
  discard <| runLakeBuild lake project

def auditRepresentativeRebuilds (designRules : Array ModuleRule) : IO Unit := do
  let lake := (← findSysroot) / "bin" / "lake"
  unless ← lake.pathExists do fail s!"Lake executable is absent: {lake}"
  let normativeRules := traceModuleRules designRules
  let mut actualRules : Array ModuleRule := #[]
  for expected in normativeRules do
    let imports := (← sourceImports expected.module).filter fun imported =>
      normativeRules.any (·.module = imported)
    for imported in imports do
      unless expected.imports.contains imported do
        fail s!"trace module {expected.module} imports {imported} outside its normative bound"
    actualRules := actualRules.push ⟨expected.module, imports⟩
  IO.FS.withTempDir fun project => do
    copyTraceProject project designRules
    discard <| runLakeBuild lake project
    for case in rebuildTraceCases do
      auditRebuildTrace lake project actualRules normativeRules case

def main : IO Unit := do
  let designMap ← IO.FS.readFile designModuleMapPath
  let designRules ← match parseDesignModuleRules designMap with
    | .ok rules => pure rules
    | .error error => fail error
  let manifest ← IO.FS.readFile "proof-manifest.toml"
  match validateManifestContent designRules manifest with
  | .error error => fail error
  | .ok _ => pure ()
  auditNegativeFixtures designRules manifest
  auditArchitecture designRules
  initSearchPath (← findSysroot) [".lake/build/lib/lean"]
  let env ← importModules #[
    { module := `AgentWorkbench.Audit.Expected },
    { module := `AgentWorkbench.Cli.Program }] {}
  auditTheorems env
  auditDeclarations env
  auditCliMutation
  auditRepresentativeRebuilds designRules
  IO.println "verified-core audit: pass"

end AgentWorkbench.Audit

def main : IO Unit :=
  AgentWorkbench.Audit.main
