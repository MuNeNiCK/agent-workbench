import AgentWorkbench.Adapter.ProofBuild

namespace AgentWorkbenchTest.ProofBuild

open AgentWorkbench

private def expect (condition : Bool) (message : String) : IO Unit :=
  unless condition do throw (IO.userError message)

private def result (exitCode : Nat) : Process.Result :=
  { exitCode, stdout := "", stderr := "", stdoutDigest := "", stderrDigest := "" }

private def baseline (directory : System.FilePath) (existed : Bool) :
    AgentWorkbench.ProofBuild.OutputBaseline :=
  { directory, existed, parentExisted := true }

private def directoryEntries (path : System.FilePath) : IO (List String) := do
  let entries ← path.readDir
  pure (entries.toList.map (·.fileName) |>.mergeSort (· < ·))

private def exerciseExistingOutput (parent : System.FilePath) : IO Unit := do
  let output := parent / "existing"
  IO.FS.createDirAll output
  IO.FS.writeFile (output / "preserved") "original output"
  let before ← directoryEntries parent
  let (buildOutput, checkResult?) ← AgentWorkbench.ProofBuild.withFreshOutputs [baseline output true]
    (do
      expect (!(← output.pathExists)) "existing output remained visible during rebuild"
      IO.FS.createDirAll (output / "lib" / "lean")
      IO.FS.writeFile (output / "lib" / "lean" / "Fresh.olean") "fresh output"
      pure (result 0, ()))
    (fun _ paths => do
      expect (!(← output.pathExists)) "normal output became visible during isolated check"
      let some leanPath := paths.head?
        | throw (IO.userError "isolated check received no Lean output path")
      expect ((← (leanPath / "Fresh.olean").pathExists))
        "isolated check did not receive the fresh output"
      pure (result 0))
  expect (buildOutput.1.exitCode == 0 && checkResult?.map (·.exitCode) == some 0)
    "successful isolated build did not return both results"
  expect ((← IO.FS.readFile (output / "preserved")) == "original output")
    "existing output content was not restored"
  expect ((← directoryEntries parent) == before)
    "successful isolated build changed its parent directory"

private def exerciseFailedBuild (parent : System.FilePath) : IO Unit := do
  let output := parent / "failed"
  IO.FS.createDirAll output
  IO.FS.writeFile (output / "preserved") "original output"
  let before ← directoryEntries parent
  let (buildOutput, checkResult?) ← AgentWorkbench.ProofBuild.withFreshOutputs [baseline output true]
    (do
      IO.FS.createDirAll output
      IO.FS.writeFile (output / "partial") "failed build output"
      pure (result 1, ()))
    (fun _ _ => (throw (IO.userError "check ran after a failed build") : IO Process.Result))
  expect (buildOutput.1.exitCode == 1 && checkResult?.isNone)
    "failed build did not skip its isolated check"
  expect ((← IO.FS.readFile (output / "preserved")) == "original output")
    "failed build did not restore existing output"
  expect (!(← (output / "partial").pathExists))
    "failed build left partial output"
  expect ((← directoryEntries parent) == before)
    "failed build changed its parent directory"

private def exerciseAbsentOutput (parent : System.FilePath) : IO Unit := do
  let output := parent / "absent"
  let before ← directoryEntries parent
  let (_, checkResult?) ← AgentWorkbench.ProofBuild.withFreshOutputs [baseline output false]
    (do
      IO.FS.createDirAll (output / "lib" / "lean")
      IO.FS.writeFile (output / "lib" / "lean" / "Fresh.olean") "fresh output"
      pure (result 0, ()))
    (fun _ _ => pure (result 0))
  expect (checkResult?.map (·.exitCode) == some 0)
    "absent-output check did not run"
  expect (!(← output.pathExists)) "initially absent output remained after proof check"
  expect ((← directoryEntries parent) == before)
    "initially absent output changed its parent directory"

private def exerciseCallbackFailure (parent : System.FilePath) : IO Unit := do
  let output := parent / "failed-callback"
  IO.FS.createDirAll output
  IO.FS.writeFile (output / "preserved") "original output"
  let before ← directoryEntries parent
  let mut failed := false
  try
    let _ ← AgentWorkbench.ProofBuild.withFreshOutputs [baseline output true]
      (do
        IO.FS.createDirAll (output / "lib" / "lean")
        IO.FS.writeFile (output / "lib" / "lean" / "Fresh.olean") "fresh output"
        pure (result 0, ()))
      (fun _ _ => (throw (IO.userError "isolated callback failed") : IO Unit))
  catch _ =>
    failed := true
  expect failed "isolated callback failure did not propagate"
  expect ((← IO.FS.readFile (output / "preserved")) == "original output")
    "isolated callback failure did not restore existing output"
  expect ((← directoryEntries parent) == before)
    "isolated callback failure changed its parent directory"

private def exerciseAbsentOutputParent (root : System.FilePath) : IO Unit := do
  let package := root / "package-without-lake-state"
  IO.FS.createDirAll package
  let output := package / ".lake" / "build"
  let (_, checkResult?) ← AgentWorkbench.ProofBuild.withFreshOutputs
    [{ directory := output, existed := false, parentExisted := false }]
    (do
      IO.FS.createDirAll (output / "lib" / "lean")
      IO.FS.writeFile (output / "lib" / "lean" / "Fresh.olean") "fresh output"
      pure (result 0, ()))
    (fun _ _ => pure (result 0))
  expect (checkResult?.map (·.exitCode) == some 0)
    "absent-parent check did not run"
  expect (!(← (package / ".lake").pathExists))
    "proof operation left a Lake state parent that was initially absent"

def run : IO Unit :=
  IO.FS.withTempDir fun root => do
    let parent := root / "outputs"
    IO.FS.createDirAll parent
    exerciseExistingOutput parent
    exerciseFailedBuild parent
    exerciseAbsentOutput parent
    exerciseCallbackFailure parent
    exerciseAbsentOutputParent root

end AgentWorkbenchTest.ProofBuild
