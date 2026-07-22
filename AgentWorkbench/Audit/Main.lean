import Lean

open Lean

namespace AgentWorkbench.Audit

structure ModuleRule where
  module : String
  imports : Array String

def moduleRules : Array ModuleRule := #[
  ⟨"AgentWorkbench.Domain.Identity", #[]⟩,
  ⟨"AgentWorkbench.Domain.Facts", #[]⟩,
  ⟨"AgentWorkbench.Domain.Work", #["AgentWorkbench.Domain.Identity", "AgentWorkbench.Domain.Facts"]⟩,
  ⟨"AgentWorkbench.Domain.Design", #["AgentWorkbench.Domain.Identity", "AgentWorkbench.Domain.Facts"]⟩,
  ⟨"AgentWorkbench.Domain.Review", #["AgentWorkbench.Domain.Identity", "AgentWorkbench.Domain.Facts"]⟩,
  ⟨"AgentWorkbench.Domain.Evidence", #["AgentWorkbench.Domain.Identity", "AgentWorkbench.Domain.Facts"]⟩,
  ⟨"AgentWorkbench.Domain.ExternalOperation", #["AgentWorkbench.Domain.Identity", "AgentWorkbench.Domain.Facts"]⟩,
  ⟨"AgentWorkbench.Policy.Traceability", #["AgentWorkbench.Domain.Work", "AgentWorkbench.Domain.Design", "AgentWorkbench.Domain.Evidence"]⟩,
  ⟨"AgentWorkbench.Policy.Authority", #["AgentWorkbench.Domain.Review", "AgentWorkbench.Domain.Facts"]⟩,
  ⟨"AgentWorkbench.Policy.Completion", #["AgentWorkbench.Domain.Work", "AgentWorkbench.Domain.Design", "AgentWorkbench.Domain.Review", "AgentWorkbench.Domain.Evidence", "AgentWorkbench.Domain.ExternalOperation", "AgentWorkbench.Policy.Traceability", "AgentWorkbench.Policy.Authority"]⟩,
  ⟨"AgentWorkbench.Policy.Update", #["AgentWorkbench.Domain.Identity", "AgentWorkbench.Domain.Facts"]⟩,
  ⟨"AgentWorkbench.Kernel.Replay", #["AgentWorkbench.Domain.Work", "AgentWorkbench.Domain.Design", "AgentWorkbench.Domain.Review", "AgentWorkbench.Domain.Evidence", "AgentWorkbench.Domain.ExternalOperation"]⟩,
  ⟨"AgentWorkbench.Kernel.Decide", #["AgentWorkbench.Kernel.Replay", "AgentWorkbench.Policy.Traceability", "AgentWorkbench.Policy.Authority", "AgentWorkbench.Policy.Completion", "AgentWorkbench.Policy.Update"]⟩,
  ⟨"AgentWorkbench.Kernel.Gates", #["AgentWorkbench.Kernel.Replay", "AgentWorkbench.Policy.Completion"]⟩,
  ⟨"AgentWorkbench.Kernel.Resolver", #["AgentWorkbench.Kernel.Gates", "AgentWorkbench.Domain.Work"]⟩,
  ⟨"AgentWorkbench.Application.Service", #["AgentWorkbench.Kernel.Decide", "AgentWorkbench.Kernel.Gates", "AgentWorkbench.Kernel.Resolver"]⟩
]

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

def modulePath (module : String) : System.FilePath :=
  System.FilePath.mk <| (module.replace "." "/") ++ ".lean"

def sourceImports (module : String) : IO (Array String) := do
  let source ← IO.FS.readFile (modulePath module)
  return (source.splitOn "\n").foldl (init := #[]) fun imports line =>
    let trimmed := line.trimAscii.toString
    if trimmed.startsWith "import " then
      imports.push (trimmed.drop 7).trimAscii.toString
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
      if visited.contains module then
        transitiveVisit next remaining visited
      else
        transitiveVisit next (next module ++ remaining) (module :: visited)

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
  for lower in actual.filter fun rule =>
      rule.module.startsWith "AgentWorkbench.Domain." do
    if completionDependents.contains lower.module then
      fail s!"Policy.Completion change reaches lower-level module {lower.module}"
  unless workDependents.contains "AgentWorkbench.Application.Service" do
    fail "Domain.Work dependent closure does not reach the application boundary"
  unless completionDependents.contains "AgentWorkbench.Application.Service" do
    fail "Policy.Completion dependent closure does not reach the application boundary"

def auditArchitecture (manifestModules : Array String) : IO Unit := do
  let normativeModules := moduleRules.map (·.module)
  unless manifestModules.qsort (· < ·) = normativeModules.qsort (· < ·) do
    fail "proof manifest module closure differs from the normative module map"
  let mut actualRules : Array ModuleRule := #[]
  for rule in moduleRules do
    unless ← (modulePath rule.module).pathExists do
      fail s!"missing normative module {rule.module}"
    let actual ← sourceImports rule.module
    actualRules := actualRules.push ⟨rule.module, actual⟩
    for imported in actual do
      unless rule.imports.contains imported do
        fail s!"{rule.module} imports forbidden dependency {imported}"
  auditReverificationBounds actualRules
  let cliImports ← sourceImports "Main"
  unless cliImports = #["AgentWorkbench.Application.Service"] do
    fail "CLI must import only AgentWorkbench.Application.Service"
  let cli ← IO.FS.readFile "Main.lean"
  for forbidden in #["AgentWorkbench.Domain", "AgentWorkbench.Policy", "AgentWorkbench.Kernel",
      "Domain.", "Policy.", "Kernel."] do
    if cli.contains forbidden then
      fail s!"CLI bypasses Application.Service through {forbidden}"

def auditTheorems (theorems permitted forbidden : Array String) : IO Unit := do
  initSearchPath (← findSysroot) [".lake/build/lib/lean"]
  let env ← importModules #[{ module := `AgentWorkbench.Application.Service }] {}
  let context : Core.Context := {
    fileName := "<verified-core-audit>"
    fileMap := FileMap.ofString ""
  }
  let state : Core.State := { env := env }
  for theoremName in theorems do
    let name := theoremName.toName
    unless env.contains name do
      fail s!"manifest declaration is absent: {theoremName}"
    let collect : CoreM (Array Name) := collectAxioms name
    let axioms ← collect.toIO' context state
    for axiomName in axioms do
      let rendered := toString axiomName
      if forbidden.contains rendered then
        fail s!"forbidden axiom {rendered} reached {theoremName}"
      unless permitted.contains rendered do
        fail s!"unpermitted axiom {rendered} reached {theoremName}"

def main : IO Unit := do
  let manifest ← IO.FS.readFile "proof-manifest.toml"
  let parse (key : String) : IO (Array String) :=
    match parseManifestArray key manifest with
    | .ok values => pure values
    | .error error => fail error
  let theorems ← parse "theorems"
  let modules ← parse "modules"
  let permitted ← parse "permitted_axioms"
  let forbidden ← parse "forbidden_axioms"
  let unsafeModules ← parse "unsafe_ffi_modules"
  if theorems.isEmpty then fail "theorem manifest is empty"
  for unsafeModule in unsafeModules do
    if modules.contains unsafeModule then
      fail s!"unsafe/FFI module entered proof closure: {unsafeModule}"
  auditArchitecture modules
  auditTheorems theorems permitted forbidden
  IO.println "verified-core audit: pass"

end AgentWorkbench.Audit

def main : IO Unit :=
  AgentWorkbench.Audit.main
