import AgentWorkbenchTest.Fixture
import AgentWorkbench.Cli.Describe

namespace AgentWorkbenchTest.BinaryProtocol

open AgentWorkbench AgentWorkbenchTest

private def executablePath : System.FilePath :=
  if System.Platform.isWindows then ".lake/build/bin/agent-workbench.exe"
  else ".lake/build/bin/agent-workbench"

private def containsText (text fragment : String) : Bool :=
  (text.splitOn fragment).length > 1

private def invoke
    (root : System.FilePath) (operation : Operation)
    (input : Option Lean.Json) : IO IO.Process.Output :=
  IO.Process.output {
    cmd := executablePath.toString
    args := #["--project", root.toString] ++ (operation.name.splitOn " ").toArray }
    (input.map (·.compress))

private def rejectedBeforeSemanticDispatch (stderr : String) : Bool :=
  ["unknown command", "invalid JSON input", "invalid input for", "unknown fields",
    "missing native contract"].any (containsText stderr)

def run : IO Unit := do
  let mutations := Operation.all.filter (·.kind == .mutation)
  for operation in mutations do
    let contract ← match AgentWorkbench.Cli.operationContract? operation.name with
      | some value => pure value
      | none => throw (IO.userError s!"mutation has no public binary contract: {operation.name}")
    IO.FS.withTempDir fun root => do
      let output ← invoke root operation contract.inputExample
      expect (!rejectedBeforeSemanticDispatch output.stderr)
        s!"public binary did not decode and dispatch mutation {operation.name}: {output.stderr}"
  IO.FS.withTempDir fun root => do
    let output ← IO.Process.output {
      cmd := executablePath.toString
      args := #["--project", root.toString, "work", "start"] }
      (some "{\"id\":\"work-1\",\"outcome\":\"x\",\"scope\":\"project\",\"responsibleAgentRun\":\"a\",\"invented\":true}")
    expect (output.exitCode != 0 && containsText output.stderr "unknown fields for work start")
      "public binary accepted an uncontracted mutation field"

end AgentWorkbenchTest.BinaryProtocol
