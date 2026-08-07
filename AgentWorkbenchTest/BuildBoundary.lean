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

private def reviewedExecutableNames : List String :=
  ["agent-workbench", "agent-workbench-tests", "agent-workbench-proof-tests"]

private def verifyExecutableRoots : IO Unit := do
  let lakefile ← readRequired "lakefile.lean"
  let declared := lakefile.splitOn "\n" |>.filterMap fun line =>
    let line := line.trimAscii.toString
    if line.startsWith "lean_exe " then
      let rest := (line.drop "lean_exe ".length).trimAscii.toString
      some <| (rest.splitOn " ").head?.getD ""
        |>.replace "«" "" |>.replace "»" ""
    else none
  expect (declared == reviewedExecutableNames)
    s!"unreviewed executable root(s): declared={declared}, reviewed={reviewedExecutableNames}"

private def isGeneratedLeanObject (path : String) : Bool :=
  containsText path "/.lake/build/ir/" &&
    (path.endsWith ".c.o.export" || path.endsWith ".c.o.noexport")

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
  for input in packageInputs do
    let path := input.trimAscii.toString.replace "\"" ""
    expect (isGeneratedLeanObject path || isReviewedNativeDependency path)
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
  verifyProductLinkResponse
  verifyQueryStoreCapability

end AgentWorkbenchTest.BuildBoundary
