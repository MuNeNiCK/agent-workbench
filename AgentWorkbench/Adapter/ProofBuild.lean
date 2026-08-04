import AgentWorkbench.Adapter.ProofInput

namespace AgentWorkbench.ProofBuild

structure OutputBaseline where
  directory : System.FilePath
  existed : Bool
  parentExisted : Bool

private structure ManifestPackage where
  type : String
  name : String
  dir : Option String := none
  subDir : Option String := none
  deriving Lean.FromJson

private structure LakeManifest where
  packagesDir : String := ".lake/packages"
  packages : Array ManifestPackage := #[]
  deriving Lean.FromJson

private structure OutputLayout where
  original : System.FilePath
  existed : Bool
  parentExisted : Bool
  backup : System.FilePath
  isolated : System.FilePath
  wasBackedUp : System.FilePath
  wasAbsent : System.FilePath

private def moduleOleanComponents : List String → Except String (List String)
  | [] => throw "Lean module name is empty"
  | [name] => pure [name ++ ".olean"]
  | name :: rest => do pure (name :: (← moduleOleanComponents rest))

private def buildDirectoryFromOlean
    (path : System.FilePath) (moduleName : String) : Except String System.FilePath := do
  let moduleComponents ← moduleOleanComponents (moduleName.splitOn ".")
  let suffix := ["lib", "lean"] ++ moduleComponents
  let components := path.components
  if components.length < suffix.length then
    throw s!"Lean output is not the queried module output for {moduleName}: {path}"
  let prefixLength := components.length - suffix.length
  if components.drop prefixLength != suffix then
    throw s!"Lean output is not the queried module output for {moduleName}: {path}"
  let directory := System.mkFilePath (components.take prefixLength)
  if directory.toString.isEmpty then
    throw s!"Lean output has no package build directory: {path}"
  pure directory

private structure ModuleSetup where
  name : String
  importArts : Std.TreeMap String (Array String)
  deriving Lean.FromJson

private def moduleSetup
    (projectRoot : System.FilePath) (proofRoot : System.FilePath)
    (runtime : Runtime.Layout) (source : System.FilePath) : IO ModuleSetup := do
  let result ← Process.execute projectRoot {
    executable := runtime.elanExecutable.toString
    arguments := #["run", ProofToolchain.identifier, "lake", "setup-file", source.toString]
    workingDirectory := some proofRoot.toString
    environment := #[("ELAN_HOME", runtime.elanHome.toString)] }
  if result.exitCode != 0 then
    throw (IO.userError s!"cannot resolve Lean module name for {source}: {result.stderr}")
  let json ← match Lean.Json.parse result.stdout with
    | .ok value => pure value
    | .error message => throw (IO.userError s!"invalid Lake setup for {source}: {message}")
  match Lean.fromJson? json with
  | .ok (setup : ModuleSetup) => pure setup
  | .error message => throw (IO.userError s!"incomplete Lake setup for {source}: {message}")

private def queryOutput
    (projectRoot : System.FilePath) (proofRoot : System.FilePath)
    (runtime : Runtime.Layout) (source : System.FilePath)
    (querySource : String) : IO System.FilePath := do
  let result ← Process.execute projectRoot {
    executable := runtime.elanExecutable.toString
    arguments := #["run", ProofToolchain.identifier, "lake", "query",
      s!"{querySource}:olean", "--text"]
    workingDirectory := some proofRoot.toString
    environment := #[("ELAN_HOME", runtime.elanHome.toString)] }
  if result.exitCode != 0 then
    throw (IO.userError s!"cannot resolve isolated Lean output for {source}: {result.stderr}")
  let outputs := result.stdout.splitOn "\n" |>.filterMap (fun line =>
    let value := line.trimAscii.toString
    if value.endsWith ".olean" then some (System.FilePath.mk value) else none)
  match outputs.getLast? with
  | some output => pure <| if output.isAbsolute then output else proofRoot / output
  | none => throw (IO.userError s!"Lake returned no .olean output for {source}")

def buildDirectories
    (projectRoot : System.FilePath) (runtime : Runtime.Layout)
    (claim : LeanClaim) : IO (List System.FilePath) := do
  let proofRoot :=
    let configured : System.FilePath := claim.input.proofRoot
    if configured.isAbsolute then configured else projectRoot / configured
  let sources ← ProofInput.declaredSourcePaths projectRoot claim
  let mut directories := []
  for (source, declaredSource) in sources.zip claim.input.declaredSources do
    let setup ← moduleSetup projectRoot proofRoot runtime source
    let output ← queryOutput projectRoot proofRoot runtime source declaredSource.path
    let directory ← match buildDirectoryFromOlean output setup.name with
      | .ok value => pure value
      | .error message => throw (IO.userError message)
    if !directories.any (fun prior => prior.toString == directory.toString) then
      directories := directories ++ [directory]
    for (name, outputs) in setup.importArts.toList do
      for output in outputs do
        let configured : System.FilePath := output
        let path := if configured.isAbsolute then configured else proofRoot / configured
        if !ProofInput.pathWithin runtime.elanHome path then
          let directory ← match buildDirectoryFromOlean path name with
            | .ok value => pure value
            | .error message => throw (IO.userError message)
          if !directories.any (fun prior => prior.toString == directory.toString) then
            directories := directories ++ [directory]
  pure directories

private def removeIfPresent (path : System.FilePath) : IO Unit := do
  if ← path.pathExists then IO.FS.removeDirAll path

private def canonicalRoot (path : System.FilePath) : IO System.FilePath := do
  if ← path.pathExists then IO.FS.realPath path else pure path

def captureBaselines
    (projectRoot : System.FilePath) (claim : LeanClaim) : IO (List OutputBaseline) := do
  let configured : System.FilePath := claim.input.proofRoot
  let proofRoot ← canonicalRoot <|
    if configured.isAbsolute then configured else projectRoot / configured
  let mut directories := [proofRoot / ".lake" / "build"]
  let manifestPath := proofRoot / "lake-manifest.json"
  if ← manifestPath.pathExists then
    let json ← match Lean.Json.parse (← IO.FS.readFile manifestPath) with
      | .ok value => pure value
      | .error message => throw (IO.userError s!"invalid Lake manifest: {message}")
    let manifest ← match Lean.fromJson? json with
      | .ok (value : LakeManifest) => pure value
      | .error message => throw (IO.userError s!"incomplete Lake manifest: {message}")
    for package in manifest.packages do
      let packageRoot ← if package.type == "path" then
          match package.dir with
          | some directory => canonicalRoot (proofRoot / directory)
          | none => throw (IO.userError s!"path package {package.name} has no directory")
        else
          canonicalRoot (proofRoot / manifest.packagesDir / package.name)
      let packageRoot := match package.subDir with
        | some directory => packageRoot / directory
        | none => packageRoot
      let output := packageRoot / ".lake" / "build"
      if !directories.any (fun prior => prior.toString == output.toString) then
        directories := directories ++ [output]
  let mut baselines := []
  for directory in directories do
    let parentExisted ← match directory.parent with
      | some parent => parent.pathExists
      | none => pure false
    baselines := baselines ++ [{
      directory, existed := (← directory.pathExists), parentExisted }]
  pure baselines

def validateDiscoveredOutputs
    (baselines : List OutputBaseline) (directories : List System.FilePath) : IO Unit := do
  for directory in directories do
    unless baselines.any (fun baseline => baseline.directory.toString == directory.toString) do
      throw (IO.userError s!"Lean output is outside the pre-operation Lake manifest: {directory}")

private def restore (layouts : List OutputLayout) : IO Unit := do
  for layout in layouts.reverse do
    if ← layout.wasBackedUp.pathExists then
      removeIfPresent layout.original
      removeIfPresent layout.isolated
      if ← layout.backup.pathExists then
        IO.FS.rename layout.backup layout.original
      else
        throw (IO.userError s!"proof output backup disappeared before restoration: {layout.original}")
    else if ← layout.wasAbsent.pathExists then
      removeIfPresent layout.original
      removeIfPresent layout.isolated
      if !layout.parentExisted then
        if let some parent := layout.original.parent then removeIfPresent parent

def withFreshOutputs {α β : Type}
    (baselines : List OutputBaseline) (build : IO (Process.Result × β))
    (check : β → List System.FilePath → IO α) : IO ((Process.Result × β) × Option α) :=
  IO.FS.withTempDir fun marker => do
    let token := marker.components.getLast?.getD "isolated"
    let mut layouts := []
    for (baseline, index) in baselines.zipIdx do
      let directory := baseline.directory
      let parent ← match directory.parent with
        | some value => pure value
        | none => throw (IO.userError s!"Lean build directory has no parent: {directory}")
      layouts := layouts ++ [{
        original := directory
        existed := baseline.existed
        parentExisted := baseline.parentExisted
        backup := parent / s!".agent-workbench-old-{token}-{index}"
        isolated := parent / s!".agent-workbench-proof-{token}-{index}"
        wasBackedUp := marker / s!"backed-up-{index}"
        wasAbsent := marker / s!"absent-{index}" }]
    try
      for layout in layouts do
        if ← layout.backup.pathExists then
          throw (IO.userError s!"proof backup path already exists: {layout.backup}")
        if ← layout.isolated.pathExists then
          throw (IO.userError s!"isolated proof output path already exists: {layout.isolated}")
        if layout.existed then
          if !(← layout.original.pathExists) then
            throw (IO.userError s!"pre-existing proof output disappeared during discovery: {layout.original}")
          IO.FS.rename layout.original layout.backup
          try
            IO.FS.writeFile layout.wasBackedUp ""
          catch error =>
            IO.FS.rename layout.backup layout.original
            throw error
        else
          removeIfPresent layout.original
          IO.FS.writeFile layout.wasAbsent ""
      let buildOutput ← build
      let buildResult := buildOutput.1
      if buildResult.exitCode != 0 then return (buildOutput, none)
      let mut isolated := []
      for layout in layouts do
        if ← layout.original.pathExists then
          IO.FS.rename layout.original layout.isolated
          isolated := isolated ++ [layout]
      if isolated.isEmpty then
        throw (IO.userError "fresh Lean build produced no package output directory")
      let leanPaths := isolated.map (fun layout => layout.isolated / "lib" / "lean")
      pure (buildOutput, some (← check buildOutput.2 leanPaths))
    finally
      restore layouts

end AgentWorkbench.ProofBuild
