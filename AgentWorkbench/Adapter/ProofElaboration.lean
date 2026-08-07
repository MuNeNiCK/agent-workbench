import AgentWorkbench.Adapter.ContentDigest
import AgentWorkbench.Adapter.Process
import AgentWorkbench.Adapter.Runtime
import AgentWorkbench.Domain.Design

namespace AgentWorkbench.ProofElaboration

structure Result where
  elaboratedPropositionDigest : String
  propositionDependencies : List String
  deriving Repr, DecidableEq, Lean.FromJson, Lean.ToJson

private def sourceModule (source : SourceInput) : String :=
  source.path.replace "\\" "/" |>.dropEnd 5 |>.toString |>.replace "/" "."

private def quoted (value : String) : String :=
  (Lean.toJson value).compress

def analyzerSource (input : ClaimInput) : String :=
  let imports := input.declaredSources.map (fun source => s!"import {sourceModule source}")
  String.intercalate "\n" imports ++ "\n" ++
  "import Lean.Data.Json\nimport Lean.Elab.Command\nimport Lean.Meta\n\n" ++
  "open Lean\n\nrun_cmd do\n" ++
  s!"  let propositionName := ({quoted input.proposition} : String).toName\n" ++
  "  let env ← getEnv\n" ++
  "  unless env.contains propositionName do\n" ++
  "    throwError m!\"missing proposition declaration {propositionName}\"\n" ++
  "  let proposition := mkConst propositionName\n" ++
  "  let canonical ← Lean.Elab.Command.liftTermElabM do\n" ++
  "    let inferred ← Lean.Meta.inferType proposition\n" ++
  "    unless (← Lean.Meta.isDefEq inferred (mkSort Level.zero)) do\n" ++
  "      throwError m!\"Claim declaration {propositionName} is not Prop-valued\"\n" ++
  "    Lean.Meta.whnf proposition\n" ++
  "  let dependencies := canonical.getUsedConstantsAsSet.toList.map toString\n" ++
  "    |>.mergeSort (· < ·)\n" ++
  "  let output := Lean.Json.mkObj\n" ++
  "    [(\"elaboratedPropositionDigest\", toJson " ++
    "(\"blake3-payload:\" ++ reprStr canonical)),\n" ++
  "     (\"propositionDependencies\", toJson dependencies)]\n" ++
  "  IO.println output.compress\n"

private def parseResult (output : String) : Except String Result := do
  let lines := output.splitOn "\n" |>.filter (fun line => !line.trimAscii.isEmpty)
  let line ← match lines.getLast? with
    | some value => pure value
    | none => throw "pinned proposition analyzer returned no result"
  let json ← Lean.Json.parse line
  let raw ← (Lean.fromJson? json : Except String Result)
  if !raw.elaboratedPropositionDigest.startsWith "blake3-payload:" then
    throw "pinned proposition analyzer returned no elaborated expression"
  let payload := raw.elaboratedPropositionDigest.drop 15 |>.toString
  pure { raw with elaboratedPropositionDigest := ContentDigest.string payload }

def run
    (projectRoot proofRoot : System.FilePath) (runtime : Runtime.Layout)
    (claim : LeanClaim) (leanPaths : List System.FilePath) : IO Result := do
  IO.FS.withTempDir fun temporary => do
    let analyzer := temporary / "Proposition.lean"
    IO.FS.writeFile analyzer (analyzerSource claim.input)
    let result ← Process.execute projectRoot {
      executable := runtime.elanExecutable.toString
      arguments := #["run", ProofToolchain.identifier, "lean", analyzer.toString]
      workingDirectory := some proofRoot.toString
      environment := #[
        ("ELAN_HOME", runtime.elanHome.toString),
        ("LEAN_PATH", System.SearchPath.toString leanPaths)] }
    if result.exitCode != 0 then
      throw (IO.userError s!"cannot elaborate Claim {claim.id}:\n{result.stderr}")
    match parseResult result.stdout with
    | .ok value => pure value
    | .error message => throw (IO.userError s!"invalid proposition analyzer result: {message}")

end AgentWorkbench.ProofElaboration
