import AgentWorkbench.Adapter.DesignSource
import AgentWorkbench.Adapter.ProofInput
import AgentWorkbench.Adapter.Runtime
import AgentWorkbench.Adapter.ProofBuild
import AgentWorkbench.Adapter.ProofElaboration
import AgentWorkbench.Adapter.PathPolicy

namespace AgentWorkbench.DesignClaim

structure Prepared where
  claims : List LeanClaim
  sources : List DesignSource.Captured

private def fail (message : String) : IO α :=
  throw (IO.userError message)

private def pathWithin (root path : System.FilePath) : Bool :=
  root.normalize.components.isPrefixOf path.normalize.components

private def proofRootPath
    (projectRoot : System.FilePath) (claim : LeanClaim) : IO System.FilePath := do
  let configured : System.FilePath := claim.input.proofRoot
  if configured.isAbsolute || configured.components.any (· == "..") then
    fail s!"Claim {claim.id} proof root must be project-relative"
  let requiredRoot ← IO.FS.realPath (projectRoot / ".agent-workbench" / "design" / "proofs")
  let path := projectRoot / configured
  unless ← path.pathExists do fail s!"Claim {claim.id} proof root does not exist"
  let canonical ← IO.FS.realPath path
  if ← PathPolicy.containsSymlinkBelow projectRoot configured then
    fail s!"Claim {claim.id} proof root contains a symlink"
  unless pathWithin requiredRoot canonical do
    fail s!"Claim {claim.id} proof root is outside .agent-workbench/design/proofs"
  pure canonical

private def sourcePath
    (proofRoot : System.FilePath) (claimId : String) (source : SourceInput) : IO System.FilePath := do
  let configured : System.FilePath := source.path
  if configured.isAbsolute || configured.components.any (· == "..") ||
      configured.extension != some "lean" then
    fail s!"Claim {claimId} source must be a relative .lean file below its proof root: {source.path}"
  let path := proofRoot / configured
  unless ← path.pathExists do fail s!"Claim {claimId} source does not exist: {source.path}"
  let metadata ← path.symlinkMetadata
  unless metadata.type == .file do
    fail s!"Claim {claimId} source is not a regular non-symlink file: {source.path}"
  let canonical ← IO.FS.realPath path
  if ← PathPolicy.containsSymlinkBelow proofRoot configured then
    fail s!"Claim {claimId} source path contains a symlink: {source.path}"
  unless pathWithin proofRoot canonical do
    fail s!"Claim {claimId} source escapes its proof root: {source.path}"
  pure canonical

private def targetFor (projectRoot path : System.FilePath) : String :=
  "file:" ++ ProofInput.pathIdentity projectRoot.normalize path.normalize

private def ensureCompleteClosure
    (projectRoot : System.FilePath) (runtime : Runtime.Layout)
    (proofRoot : System.FilePath) (claim : LeanClaim)
    (declared : List System.FilePath) : IO Unit := do
  let closure ← ProofInput.sourceClosurePaths projectRoot runtime claim
  let canonicalProjectRoot ← IO.FS.realPath projectRoot
  let packagesRoot := proofRoot / ".lake" / "packages"
  for dependency in closure do
    let canonical ← IO.FS.realPath dependency
    if pathWithin canonicalProjectRoot canonical && !pathWithin packagesRoot canonical &&
        canonical.extension == some "lean" && !declared.contains canonical then
      fail s!"Claim {claim.id} omits local Lean dependency from declaredSources: {canonical}"

private def bindClaim
    (projectRoot : System.FilePath) (runtime : Runtime.Layout)
    (claim : LeanClaim) (known : List DesignSource.Captured) :
    IO (LeanClaim × List DesignSource.Captured) := do
  let proofRoot ← proofRootPath projectRoot claim
  let mut paths := []
  for source in claim.input.declaredSources do
    paths := paths ++ [← sourcePath proofRoot claim.id source]
  if !(paths.all fun path => paths.count path == 1) then
    fail s!"Claim {claim.id} contains duplicate declared Lean sources"

  -- Dependency discovery uses the request shape only. Immutable digests are derived below from
  -- the single captured byte arrays, never accepted from request input.
  let unbound : LeanClaim := { claim with input := { claim.input with
    declaredSources := claim.input.declaredSources.map fun source =>
      { source with expectedDigest := none } } }
  ensureCompleteClosure projectRoot runtime proofRoot unbound paths

  let mut captured := known
  let mut boundSources := []
  for (source, path) in claim.input.declaredSources.zip paths do
    let target := targetFor projectRoot path
    let capture ← match captured.find? (·.target == target) with
      | some value => pure value
      | none => do
          let content ← IO.FS.readBinFile path
          let value : DesignSource.Captured := {
            target, mediaKind := "lean", digest := ContentDigest.bytes content
            content, units := [] }
          captured := captured ++ [value]
          pure value
    boundSources := boundSources ++ [{ source with expectedDigest := some capture.digest }]
  let bound := { claim with input := { claim.input with declaredSources := boundSources } }
  ensureCompleteClosure projectRoot runtime proofRoot bound paths
  pure (bound, captured)

def prepareWithRuntime
    (projectRoot : System.FilePath) (runtime : Runtime.Layout)
    (claims : List LeanClaim) : IO Prepared := do
  let mut prepared := []
  let mut sources := []
  for claim in claims do
    let (bound, nextSources) ← bindClaim projectRoot runtime claim sources
    prepared := prepared ++ [bound]
    sources := nextSources
  pure { claims := prepared, sources }

def prepare (projectRoot : System.FilePath) (claims : List LeanClaim) : IO Prepared :=
  prepareWithRuntime projectRoot (Runtime.layout projectRoot) claims

private def buildSourcesCommand
    (runtime : Runtime.Layout) (claim : LeanClaim) : CommandSpec :=
  { executable := runtime.elanExecutable.toString
    arguments := #["run", ProofToolchain.identifier, "lake", "-H", "-R", "--no-cache",
      "build"] ++ claim.input.declaredSources.toArray.map (·.path)
    environment := #[("ELAN_HOME", runtime.elanHome.toString)] }

def elaborateWithRuntime
    (projectRoot : System.FilePath) (runtime : Runtime.Layout) (claim : LeanClaim)
    (layouts : List ProofBuild.OutputLayout) : IO LeanClaim := do
  let proofRoot ← proofRootPath projectRoot claim
  let declared ← ProofInput.declaredSourcePaths projectRoot claim
  ensureCompleteClosure projectRoot runtime proofRoot claim declared
  let (buildOutput, elaboration?) ← ProofBuild.withFreshOutputs layouts
    (do
      let before ← ProofInput.evaluate projectRoot runtime claim
      let result ← Process.execute projectRoot {
        (buildSourcesCommand runtime claim) with workingDirectory := some proofRoot.toString }
      pure (result, before.1))
    (fun before leanPaths => do
      let built ← ProofInput.evaluate projectRoot runtime claim
      if built.1 != before then
        fail s!"Claim {claim.id} input changed while elaborating its proposition"
      let result ← ProofElaboration.run projectRoot proofRoot runtime claim leanPaths
      let checked ← ProofInput.evaluate projectRoot runtime claim
      if checked.1 != before then
        fail s!"Claim {claim.id} input changed during proposition elaboration"
      pure result)
  if buildOutput.1.exitCode != 0 then
    fail s!"cannot freshly build Claim {claim.id}: {buildOutput.1.stderr}"
  let result ← match elaboration? with
    | some value => pure value
    | none => fail s!"Claim {claim.id} proposition was not elaborated"
  pure { claim with
    elaboratedPropositionDigest := result.elaboratedPropositionDigest
    propositionDependencies := result.propositionDependencies }

def elaborate
    (projectRoot : System.FilePath) (claim : LeanClaim)
    (layouts : List ProofBuild.OutputLayout) : IO LeanClaim :=
  elaborateWithRuntime projectRoot (Runtime.layout projectRoot) claim layouts

end AgentWorkbench.DesignClaim
