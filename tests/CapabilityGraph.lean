import Lean.Util.FoldConsts
import AgentWorkbench.Cli.Main
import AgentWorkbenchTest
import AgentWorkbenchTest.ProofBuild
import AgentWorkbenchProof

open Lean

namespace AgentWorkbenchCapabilityGraph

private structure RootManifest where
  executable : String
  released : Bool
  entryDeclarations : List String
  allowedCapabilities : List String
  deriving Lean.FromJson

private partial def reachableFrom
    (environment : Environment) (pending visited : List Name) : List Name :=
  match pending with
  | [] => visited
  | name :: rest =>
      if visited.contains name then reachableFrom environment rest visited
      else
        let direct := match environment.find? name with
          | some info => match info.value? true with
            | some value => value.getUsedConstants.toList
            | none => []
          | none => []
        reachableFrom environment (direct ++ rest) (name :: visited)

private def allowedModulePrefixes : List String :=
  ["AgentWorkbench", "AgentWorkbenchTest", "AgentWorkbenchProof", "SQLite", "Blake3", "MD4Lean",
    "Init", "Std", "Lean"]

private def declarationModule? (environment : Environment) (name : Name) : Option Name := do
  let index ← environment.getModuleIdxFor? name
  environment.header.moduleNames[index.toNat]?

private def declarationClassified (environment : Environment) (name : Name) : Bool :=
  match declarationModule? environment name with
  | none => false
  | some moduleName =>
      let value := moduleName.toString
      allowedModulePrefixes.any fun allowed =>
        value == allowed || value.startsWith (allowed ++ ".")

private def rejectUnclassified
    (environment : Environment) (root : String) (graph : List Name) : CoreM Unit := do
  let unknown := graph.filter (!declarationClassified environment ·)
  unless unknown.isEmpty do
    throwError "{root} reaches declarations outside the reviewed module namespaces: {unknown.take 20}"

private def requireReachable (root : Name) (reachable : List Name) (capability : Name) : CoreM Unit :=
  unless reachable.contains capability do
    throwError "{root} does not reach required capability {capability}"

private def forbidReachable (root : Name) (reachable : List Name) (capability : Name) : CoreM Unit :=
  if reachable.contains capability then
    throwError "{root} reaches forbidden capability {capability}"
  else pure ()

private def hasNameFragment (graph : List Name) (fragment : String) : Bool :=
  graph.any fun name => (name.toString.splitOn fragment).length > 1

private def capabilities (graph : List Name) : List String :=
  ([
    if graph.contains ``AgentWorkbench.Store.executeMutation then some "store-mutation" else none,
    if hasNameFragment graph "AgentWorkbench.Store" then some "store-adapter" else none,
    if hasNameFragment graph "AgentWorkbenchProof" then some "proof-library" else none,
    if hasNameFragment graph "SQLite" then some "sqlite" else none,
    if hasNameFragment graph "Blake3" then some "blake3-ffi" else none,
    if hasNameFragment graph "MD4Lean" then some "markdown-ffi" else none,
    if hasNameFragment graph "IO.FS" then some "filesystem" else none,
    if hasNameFragment graph "IO.Process" then some "process" else none
  ].filterMap id).mergeSort (· < ·)

private def readManifest : CoreM (List RootManifest) := do
  let source ← IO.FS.readFile "tests/executable-capabilities.json"
  let json ← match Lean.Json.parse source with
    | .ok value => pure value
    | .error message => throwError "invalid executable capability manifest: {message}"
  match Lean.fromJson? json with
  | .ok value => pure value
  | .error message => throwError "invalid executable capability manifest: {message}"

private def rootRunDeclarations (path : String) : CoreM (List String) := do
  let source ← IO.FS.readFile path
  pure <| source.splitOn "\n" |>.filterMap fun line =>
    let value := line.trimAscii.toString
    if value.startsWith "AgentWorkbenchTest." && value.endsWith ".run" then some value else none

namespace UnreviewedBridge

def delegate := AgentWorkbench.Store.executeMutation

end UnreviewedBridge

run_meta do
  let environment ← getEnv
  let manifest ← readManifest
  let executableNames := manifest.map (·.executable)
  if executableNames != ["agent-workbench", "agent-workbench-tests", "agent-workbench-proof-tests"] ||
      executableNames.eraseDups.length != executableNames.length then
    throwError "executable capability manifest differs from the reviewed Lake roots"
  else pure ()
  let testEntries := (manifest.find? (·.executable == "agent-workbench-tests")).map
    (·.entryDeclarations) |>.getD []
  let proofEntries := (manifest.find? (·.executable == "agent-workbench-proof-tests")).map
    (·.entryDeclarations) |>.getD []
  if (← rootRunDeclarations "AgentWorkbenchTests.lean") != testEntries then
    throwError "agent-workbench-tests root calls differ from the reviewed manifest"
  else pure ()
  if (← rootRunDeclarations "AgentWorkbenchProofTests.lean") != proofEntries then
    throwError "agent-workbench-proof-tests root calls differ from the reviewed manifest"
  else pure ()
  let productSource ← IO.FS.readFile "Main.lean"
  if (productSource.splitOn "AgentWorkbench.Cli.main").length != 2 then
    throwError "product executable root differs from the reviewed declaration"
  else pure ()
  for root in manifest do
    let entries := root.entryDeclarations.map String.toName
    if entries.isEmpty || entries.eraseDups.length != entries.length then
      throwError "{root.executable} has an empty or duplicate entry declaration"
    else pure ()
    for entry in entries do
      if (environment.find? entry).isNone then
        throwError "{root.executable} names missing entry declaration {entry}"
      else pure ()
    let graph := reachableFrom environment entries []
    rejectUnclassified environment root.executable graph
    let actual := capabilities graph
    let allowed := root.allowedCapabilities.mergeSort (· < ·)
    if actual != allowed || root.allowedCapabilities.eraseDups.length != root.allowedCapabilities.length then
      throwError "{root.executable} capability graph differs: actual={actual}, reviewed={allowed}"
    else pure ()
    if root.released && root.executable != "agent-workbench" then
      throwError "unreviewed released executable root {root.executable}"
    else pure ()
  let productRoot := ``AgentWorkbench.Cli.main
  let productGraph := reachableFrom environment [productRoot] []
  let testGraph := reachableFrom environment
    ((manifest.find? (·.executable == "agent-workbench-tests")).map
      (·.entryDeclarations.map String.toName) |>.getD []) []
  let proofRoot := ``AgentWorkbenchTest.ProofBuild.run
  let proofGraph := reachableFrom environment [proofRoot] []
  let bridgeGraph := reachableFrom environment [``UnreviewedBridge.delegate] []
  let mutationExecutor := ``AgentWorkbench.Store.executeMutation
  requireReachable productRoot productGraph mutationExecutor
  requireReachable ``AgentWorkbenchTest.Atomicity.run testGraph mutationExecutor
  forbidReachable proofRoot proofGraph mutationExecutor
  if declarationClassified environment ``UnreviewedBridge.delegate ||
      !bridgeGraph.contains mutationExecutor then
    throwError "unknown-namespace bridge counterexample does not cross the reviewed capability boundary"
  else pure ()
  if productGraph.any (fun name => name.toString.startsWith "AgentWorkbenchProof") then
    throwError "product declaration graph reaches the private proof library"
  else pure ()

end AgentWorkbenchCapabilityGraph
