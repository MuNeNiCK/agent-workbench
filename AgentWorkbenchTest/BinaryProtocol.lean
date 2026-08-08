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

private partial def injectUnknownIntoArrays (arrayKeys : List String) : Lean.Json → Lean.Json
  | .obj fields => Lean.Json.mkObj <| fields.toList.map fun (key, value) =>
      let nested := injectUnknownIntoArrays arrayKeys value
      if arrayKeys.contains key then
        let injected := match nested with
          | .arr items => .arr <| items.map fun item => match item with
            | .obj itemFields => Lean.Json.mkObj
                (("inventedSystemField", Lean.Json.bool true) :: itemFields.toList)
            | other => other
          | other => other
        (key, injected)
      else (key, nested)
  | .arr items => .arr (items.map (injectUnknownIntoArrays arrayKeys))
  | value => value

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
  for (operation, arrayKeys) in [
      (Operation.designPropose, ["sourceUnitDispositions", "assumptions", "statements",
        "statementCoverage", "removedStatements", "acceptanceCriteria", "leanClaims",
        "declaredSources"]),
      (Operation.planPropose,
        ["sourceUnitDispositions", "statementDispositions", "steps"])] do
    IO.FS.withTempDir fun root => do
      let schema ← match AgentWorkbench.Cli.operationInputSchema? operation.name with
        | some value => pure value
        | none => throw (IO.userError s!"operation has no strict input schema: {operation.name}")
      let output ← invoke root operation (some (injectUnknownIntoArrays arrayKeys schema))
      expect (output.exitCode != 0 && containsText output.stderr s!"unknown fields for {operation.name}")
        s!"public binary accepted nested uncontracted fields for {operation.name}: {output.stderr}"
      for key in arrayKeys do
        expect (containsText output.stderr s!"{key}[0].inventedSystemField")
          s!"strict schema omitted nested object array {operation.name}.{key}: {output.stderr}"
      expect (!(← (root / ".agent-workbench" / "state.db").pathExists))
        s!"nested unknown fields created authoritative state for {operation.name}"

end AgentWorkbenchTest.BinaryProtocol
