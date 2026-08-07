import AgentWorkbench.Decision.ProofReuse
import AgentWorkbench.Adapter.ContentDigest
import AgentWorkbench.Adapter.Runtime

namespace AgentWorkbench.ProofInput

structure Material where
  claimInput : ClaimInput
  elaboratedPropositionDigest : String
  propositionDependencies : List String
  sources : List ProofSourceDigest
  deriving Lean.ToJson

private def withoutCommonPrefix : List String → List String → List String × List String
  | leftHead :: leftTail, rightHead :: rightTail =>
      if leftHead == rightHead then withoutCommonPrefix leftTail rightTail
      else (leftHead :: leftTail, rightHead :: rightTail)
  | left, right => (left, right)

def pathIdentity (proofRoot source : System.FilePath) : String :=
  let (rootTail, sourceTail) := withoutCommonPrefix proofRoot.components source.components
  let parts := List.replicate rootTail.length ".." ++ sourceTail
  if parts.isEmpty then "." else String.intercalate "/" parts

private def sourcePath
    (projectRoot : System.FilePath) (claim : LeanClaim) (source : SourceInput) : System.FilePath :=
  let proofRoot : System.FilePath := claim.input.proofRoot
  let base := if proofRoot.isAbsolute then proofRoot else projectRoot / proofRoot
  let configured : System.FilePath := source.path
  if configured.isAbsolute then configured else base / configured

private def proofRootPath (projectRoot : System.FilePath) (claim : LeanClaim) : System.FilePath :=
  let configured : System.FilePath := claim.input.proofRoot
  if configured.isAbsolute then configured else projectRoot / configured

private def sourceDependencies
    (projectRoot : System.FilePath) (runtime : Runtime.Layout) (claim : LeanClaim)
    (source : System.FilePath) : IO (List System.FilePath) := do
  let result ← Process.executeWithOverrides projectRoot {
    executable := runtime.elanExecutable.toString
    arguments := #["run", claim.input.toolchain, "lake", "env", "lean", "--src-deps",
      source.toString]
    workingDirectory := some (proofRootPath projectRoot claim).toString }
    #[("ELAN_HOME", runtime.elanHome.toString)]
  if result.exitCode != 0 then
    throw (IO.userError s!"cannot resolve Lean source dependencies for {source}: {result.stderr}")
  pure (result.stdout.splitOn "\n" |>.filterMap (fun line =>
    let path := line.trimAscii.toString
    if path.isEmpty then none else some (System.FilePath.mk path)))

def pathWithin (root path : System.FilePath) : Bool :=
  root.components.isPrefixOf path.components

private def isToolchainSource (runtime : Runtime.Layout) (path : System.FilePath) : Bool :=
  pathWithin runtime.elanHome path

private partial def collectSources
    (projectRoot : System.FilePath) (runtime : Runtime.Layout) (claim : LeanClaim)
    (pending : List System.FilePath) (seen : List String) (ordered : List System.FilePath) :
    IO (List String × List System.FilePath) := do
  match pending with
  | [] => pure (seen, ordered)
  | source :: remaining =>
      let canonical ← IO.FS.realPath source
      let identity := canonical.toString
      if seen.contains identity || isToolchainSource runtime canonical then
        collectSources projectRoot runtime claim remaining seen ordered
      else
        let dependencies ← sourceDependencies projectRoot runtime claim canonical
        let projectDependencies := dependencies.filter (fun path =>
          path.toString.endsWith ".lean" && !isToolchainSource runtime path)
        let (seenAfterDependencies, orderedAfterDependencies) ←
          collectSources projectRoot runtime claim projectDependencies (seen ++ [identity]) ordered
        collectSources projectRoot runtime claim remaining seenAfterDependencies
          (orderedAfterDependencies ++ [canonical])

def resolveDeclaredSourcePaths
    (projectRoot : System.FilePath) (claim : LeanClaim) : IO (List System.FilePath) := do
  let mut declared := []
  for source in claim.input.declaredSources do
    let path := sourcePath projectRoot claim source
    if !(← path.pathExists) then
      throw (IO.userError s!"declared Lean source is missing: {source.path}")
    declared := declared ++ [path]
  pure declared

def declaredSourcePaths
    (projectRoot : System.FilePath) (claim : LeanClaim) : IO (List System.FilePath) := do
  let declared ← resolveDeclaredSourcePaths projectRoot claim
  for (source, path) in claim.input.declaredSources.zip declared do
    let digest ← ContentDigest.file path
    if let some expected := source.expectedDigest then
      if digest != expected then
        throw (IO.userError s!"declared Lean source digest changed: {source.path}")
  pure declared

def sourceFiles
    (projectRoot : System.FilePath) (runtime : Runtime.Layout)
    (claim : LeanClaim) : IO (List System.FilePath) := do
  let declared ← declaredSourcePaths projectRoot claim
  pure (← collectSources projectRoot runtime claim declared [] []).2

def sourceClosurePaths
    (projectRoot : System.FilePath) (runtime : Runtime.Layout)
    (claim : LeanClaim) : IO (List System.FilePath) := do
  let declared ← resolveDeclaredSourcePaths projectRoot claim
  pure (← collectSources projectRoot runtime claim declared [] []).2

private def configurationSources (root : System.FilePath) : IO (List System.FilePath) := do
  let candidates := ["lean-toolchain", "lakefile.lean", "lakefile.toml", "lake-manifest.json"]
  let mut present := []
  for name in candidates do
    let path := root / name
    if ← path.pathExists then present := present ++ [path]
  pure present

partial def packageConfigurationSources
    (source : System.FilePath) : IO (List System.FilePath) := do
  let rec find (directory : System.FilePath) : IO (List System.FilePath) := do
    let present ← configurationSources directory
    if !present.isEmpty then return present
    match directory.parent with
    | some parent => if parent == directory then pure [] else find parent
    | none => pure []
  match source.parent with
  | some directory => find directory
  | none => pure []

def evaluate
    (projectRoot : System.FilePath) (runtime : Runtime.Layout)
    (claim : LeanClaim) : IO (CurrentClaimDigest × List ProofSourceDigest) := do
  let declared ← declaredSourcePaths projectRoot claim
  let root ← IO.FS.realPath (proofRootPath projectRoot claim)
  let closure := (← collectSources projectRoot runtime claim declared [] []).2
  let mut configurations ← configurationSources root
  for source in closure do
    configurations := configurations ++ (← packageConfigurationSources source)
  let allSources := (closure ++ configurations).foldl (fun unique path =>
    if unique.any (fun prior => prior.toString == path.toString) then unique else unique ++ [path]) []
  let mut digests := []
  for path in allSources do
    let canonical ← IO.FS.realPath path
    let digest ← ContentDigest.file canonical
    digests := digests ++ [{ path := pathIdentity root canonical, digest }]
  let sortedDigests := digests.mergeSort (fun left right => left.path < right.path)
  let material : Material := {
    claimInput := claim.input
    elaboratedPropositionDigest := claim.elaboratedPropositionDigest
    propositionDependencies := claim.propositionDependencies
    sources := sortedDigests }
  let inputDigest := ContentDigest.string (Lean.toJson material).compress
  pure ({
    claimId := claim.id
    claimInput := claim.input
    elaboratedPropositionDigest := claim.elaboratedPropositionDigest
    propositionDependencies := claim.propositionDependencies
    sourceDigests := sortedDigests
    inputDigest }, sortedDigests)

end AgentWorkbench.ProofInput
