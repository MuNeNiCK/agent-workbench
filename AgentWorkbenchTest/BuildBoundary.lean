import AgentWorkbenchTest.Fixture

namespace AgentWorkbenchTest.BuildBoundary

private def containsText (text fragment : String) : Bool :=
  (text.splitOn fragment).length > 1

private def normalized (path : String) : String :=
  path.replace "\\\\" "\\" |>.replace "\\" "/"

private def readRequired (path : System.FilePath) : IO String := do
  unless ← path.pathExists do
    throw (IO.userError s!"missing build-boundary input: {path}")
  IO.FS.readFile path

private def firstExisting : List System.FilePath → IO (Option System.FilePath)
  | [] => pure none
  | path :: rest => do
      if ← path.pathExists then pure (some path) else firstExisting rest

private def findExecutable (names : List String) : IO (Option System.FilePath) := do
  let some value ← IO.getEnv "PATH" | pure none
  let separator := if System.Platform.isWindows then ";" else ":"
  let candidates := value.splitOn separator |>.flatMap fun directory =>
    names.map fun name => System.FilePath.mk directory / System.FilePath.mk name
  firstExisting candidates

private def findElanExecutable (names : List String) : IO (Option System.FilePath) := do
  let elanHome := (← IO.getEnv "ELAN_HOME").map System.FilePath.mk
  let home := (← IO.getEnv "HOME").map fun value => System.FilePath.mk value / ".elan"
  let profile := (← IO.getEnv "USERPROFILE").map fun value => System.FilePath.mk value / ".elan"
  let roots := [elanHome, home, profile].filterMap id
  let candidates := roots.flatMap fun root =>
    names.map fun name => root / "bin" / System.FilePath.mk name
  firstExisting candidates

private def pinnedLeanRoot : IO System.FilePath := do
  let lean ← match ← IO.getEnv "LEAN" with
    | some configured => pure <| System.FilePath.mk configured
    | none => match ← findElanExecutable ["lean.exe", "lean"] with
      | some path => pure path
      | none => match ← findExecutable ["lean.exe", "lean"] with
        | some path => pure path
        | none => throw (IO.userError "cannot find the pinned Lean executable")
  let result ← try
      IO.Process.output { cmd := lean.toString, args := #["--print-prefix"] }
    catch error =>
      throw (IO.userError s!"cannot start the pinned Lean executable {lean}: {error}")
  unless result.exitCode == 0 do
    throw (IO.userError s!"cannot locate the pinned Lean toolchain with {lean}")
  pure <| System.FilePath.mk result.stdout.trimAscii.toString

private def productionSources : IO (Array System.FilePath) := do
  let nested ← (System.FilePath.mk "AgentWorkbench").walkDir
  pure <| #[System.FilePath.mk "AgentWorkbench.lean", System.FilePath.mk "Main.lean"] ++
    nested.filter (fun path => path.extension == some "lean")

private def verifyProductionSourceBoundary : IO Unit := do
  for path in ← productionSources do
    let source ← IO.FS.readFile path
    expect (!containsText source "import AgentWorkbenchProof")
      s!"production imports the private proof library: {path}"
    expect (!containsText source "@[extern")
      s!"production declares a repository-owned FFI boundary: {path}"

private structure RootManifest where
  executable : String
  allowedGeneratedLeanObjects : List String := []
  deriving Lean.FromJson

private def reviewedRootManifest : IO (List RootManifest) := do
  let source ← readRequired "tests/executable-capabilities.json"
  let json ← match Lean.Json.parse source with
    | .ok value => pure value
    | .error message => throw (IO.userError s!"invalid executable capability manifest: {message}")
  match (Lean.fromJson? json : Except String (List RootManifest)) with
  | .ok value => pure value
  | .error message => throw (IO.userError s!"invalid executable capability manifest: {message}")

private def verifyExecutableRoots : IO Unit := do
  let lakefile ← readRequired "lakefile.lean"
  let declared := lakefile.splitOn "\n" |>.filterMap fun line =>
    let line := line.trimAscii.toString
    if line.startsWith "lean_exe " then
      let rest := (line.drop "lean_exe ".length).trimAscii.toString
      some <| (rest.splitOn " ").head?.getD ""
        |>.replace "«" "" |>.replace "»" ""
    else none
  let reviewed := (← reviewedRootManifest).map (·.executable)
  expect (declared == reviewed)
    s!"unreviewed executable root(s): declared={declared}, reviewed={reviewed}"

private def verifyDeclarationCapabilityGraph : IO Unit := do
  let lakeName := if System.Platform.isWindows then "lake.exe" else "lake"
  let lake := (← pinnedLeanRoot) / "bin" / lakeName
  unless ← lake.pathExists do
    throw (IO.userError s!"missing pinned Lake executable for capability graph: {lake}")
  let result ← IO.Process.output {
    cmd := lake.toString, args := #["env", "lean", "tests/CapabilityGraph.lean"] }
  unless result.exitCode == 0 do
    throw (IO.userError s!"executable declaration capability graph failed: {result.stdout}{result.stderr}")

private def verifyReleaseAuthorizationBoundary : IO Unit := do
  let workflow ← readRequired ".github/workflows/release.yml"
  let verifier ← readRequired ".github/verify-release-authorization.py"
  let signer ← readRequired ".github/release-signers/munenick.asc"
  for required in [
      "Verify signed Workbench release authorization",
      "90D71F220DD653AA1C66FA23F8195A7A5BD1D5AF",
      "git verify-tag --raw",
      "agent-workbench release authorization v1",
      "ready-digest",
      "design-review-conclusion-entry-id",
      "design-review-target-snapshot",
      "design-review-clean",
      "implementation-review-conclusion-entry-id",
      "implementation-review-target-snapshot",
      "implementation-review-clean",
      "refs/notes/agent-workbench-release",
      "--authorization-record-file",
      "prepare"] do
    expect (containsText (workflow ++ verifier) required)
      s!"release workflow omits authorization binding {required}"
  expect (containsText signer "BEGIN PGP PUBLIC KEY BLOCK")
    "release authorization signer key is missing"
  let result ← IO.Process.output {
    cmd := "python3", args := #[".github/verify-release-authorization.py", "self-test"] }
  unless result.exitCode == 0 do
    throw (IO.userError s!"release authorization fixture matrix failed: {result.stdout}{result.stderr}")

private def verifyOrdinaryCiEvidenceBoundary : IO Unit := do
  for path in [".github/workflows/ci.yml", ".github/workflows/docs.yml"] do
    let workflow ← readRequired path
    for forbidden in ["tested-commit", "Attest tested commit", "RUNNER_TEMP/tested-commit.txt"] do
      expect (!containsText workflow forbidden)
        s!"ordinary CI contains Workbench-only evidence transport {forbidden}: {path}"
  let release ← readRequired ".github/workflows/release.yml"
  expect (containsText release "actions/upload-artifact" &&
    containsText release "agent-workbench-${{ matrix.target }}")
    "release artifact publication was removed with ordinary Workbench evidence transport"

private def generatedLeanObject? (path : String) : Option String := do
  let [_, packageInput] := normalized path |>.splitOn "/.lake/packages/" | none
  let [package, object] := packageInput.splitOn "/.lake/build/ir/" | none
  let module ← if object.endsWith ".c.o.export" then
      some (object.dropEnd ".c.o.export".length).toString
    else if object.endsWith ".c.o.noexport" then
      some (object.dropEnd ".c.o.noexport".length).toString
    else none
  some s!"{package}/{module}"

private def isReviewedNativeDependency (path : String) : Bool :=
  (containsText path "/.lake/packages/Blake3/.lake/build/lib/" &&
      path.endsWith "blake3_c.a") ||
  containsText path "/.lake/packages/MD4Lean/.lake/build/md4c/" ||
  containsText path "/.lake/packages/MD4Lean/.lake/build/wrapper/" ||
  (containsText path "/.lake/packages/leansqlite/.lake/build/lib/" &&
      path.endsWith "leansqlite.a")

private def archivePath? (response marker : String) : Option String :=
  response.splitOn "\n" |>.findSome? fun line =>
    let path := normalized line |>.trimAscii.toString |>.replace "\"" ""
    if containsText path marker then some path else none

private def archiveMemberStem (member : String) : String :=
  let name := normalized member |>.splitOn "/" |>.getLast!
  if name.endsWith ".obj" then (name.dropEnd 4).toString
  else if name.endsWith ".o" then (name.dropEnd 2).toString
  else name

private def verifyArchiveMembers
    (response marker : String) (expected : List String) : IO Unit := do
  let archive ← match archivePath? response marker with
    | some path => pure path
    | none => throw (IO.userError s!"missing reviewed native archive {marker}")
  let bin := (← pinnedLeanRoot) / "bin"
  let executable := bin / "llvm-ar.exe"
  let extensionless := bin / "llvm-ar"
  let ar ← match ← IO.getEnv "LEAN_AR" with
    | some configured => pure <| System.FilePath.mk configured
    | none => do
      if ← executable.pathExists then
        pure executable
      else if ← extensionless.pathExists then
        pure extensionless
      else
        match ← IO.getEnv "AR" with
          | some configured => pure <| System.FilePath.mk configured
          | none => pure <| System.FilePath.mk "ar"
  let listing ← try
      IO.Process.output { cmd := ar.toString, args := #["t", archive] }
    catch error =>
      throw (IO.userError s!"cannot start reviewed native archive inspector {ar}: {error}")
  unless listing.exitCode == 0 do
    throw (IO.userError s!"cannot inspect reviewed native archive {archive}: {listing.stderr}")
  let members := listing.stdout.splitOn "\n" |>.filterMap fun line =>
    let member := line.trimAscii.toString
    if member.isEmpty then none else some (archiveMemberStem member)
  expect (members.length == expected.length && expected.all members.contains)
    s!"native archive membership changed for {archive}: actual={members}, reviewed={expected}"

private def verifyProductLinkResponse : IO Unit := do
  let suffix := if System.Platform.isWindows then ".exe.rsp" else ".rsp"
  let responsePath : System.FilePath := s!".lake/build/bin/agent-workbench{suffix}"
  let response := normalized (← readRequired responsePath)
  expect (!containsText response "/AgentWorkbenchProof")
    "product link response reaches the private proof library"
  let objectSuffix :=
    if System.Platform.isWindows then ".c.o.noexport" else ".c.o.export"
  for module in ["/AgentWorkbench/Cli/Main", "/Blake3/C", "/MD4Lean/FFI", "/SQLite/FFI"] do
    let required := module ++ objectSuffix
    unless containsText response required do
      let leaf := module.splitOn "/" |>.getLast!
      let candidates := response.splitOn "\n" |>.filter fun line =>
        containsText line leaf
      throw (IO.userError
        s!"product link response is missing reviewed dependency {required}; inputs={candidates}")
  let packageInputs := response.splitOn "\n" |>.map normalized |>.filter fun line =>
    containsText line "/.lake/packages/"
  let generated := packageInputs.filterMap fun input =>
    generatedLeanObject? (input.trimAscii.toString.replace "\"" "")
  let manifests ← reviewedRootManifest
  let expectedGenerated := (manifests.find? (·.executable == "agent-workbench")).map
    (·.allowedGeneratedLeanObjects) |>.getD []
  expect (generated.mergeSort (· < ·) == expectedGenerated.mergeSort (· < ·) &&
    generated.eraseDups.length == generated.length &&
    expectedGenerated.eraseDups.length == expectedGenerated.length)
    s!"generated Lean package objects changed: actual={generated}, reviewed={expectedGenerated}"
  for input in packageInputs do
    let path := input.trimAscii.toString.replace "\"" ""
    expect ((generatedLeanObject? path).isSome || isReviewedNativeDependency path)
      s!"unreviewed package-native link input: {path}"
  verifyArchiveMembers response "blake3_c.a" ["blake3", "blake3_dispatch", "blake3_portable", "ffi_c"]
  verifyArchiveMembers response "leansqlite.a" ["sqlite3", "leansqlite", "shathree"]

private def verifyQueryStoreCapability : IO Unit :=
  IO.FS.withTempDir fun root => do
    let source := root / "QueryCapability.lean"
    IO.FS.writeFile source
      "import AgentWorkbench.Cli.Query\n#check AgentWorkbench.Store.openReadOnly\n#check AgentWorkbench.Store.open\n#check AgentWorkbench.Store.writeConnection\n#check AgentWorkbench.Store.commitOperation\n"
    let lakeName := if System.Platform.isWindows then "lake.exe" else "lake"
    let lake := (← pinnedLeanRoot) / "bin" / lakeName
    unless ← lake.pathExists do
      throw (IO.userError s!"missing pinned Lake executable for the query boundary check: {lake}")
    let result ← try
        IO.Process.output { cmd := lake.toString, args := #["env", "lean", source.toString] }
      catch error =>
        throw (IO.userError s!"cannot run the query boundary check with {lake}: {error}")
    let diagnostics := result.stdout ++ result.stderr
    expect (result.exitCode != 0 &&
      containsText diagnostics "AgentWorkbench.Store.open" &&
      containsText diagnostics "AgentWorkbench.Store.writeConnection" &&
      containsText diagnostics "AgentWorkbench.Store.commitOperation")
      "query import can name a Store write opener, connection, or commit primitive"

def run : IO Unit := do
  verifyProductionSourceBoundary
  verifyExecutableRoots
  verifyDeclarationCapabilityGraph
  verifyReleaseAuthorizationBoundary
  verifyOrdinaryCiEvidenceBoundary
  verifyProductLinkResponse
  verifyQueryStoreCapability

end AgentWorkbenchTest.BuildBoundary
