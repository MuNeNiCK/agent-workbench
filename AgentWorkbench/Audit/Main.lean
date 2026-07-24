import Lean
import AgentWorkbench.Audit.Expected
import AgentWorkbench.Cli.Program

open Lean

namespace AgentWorkbench.Audit

structure ModuleRule where
  module : String
  imports : Array String

structure TheoremRule where
  declaration : String
  expectedType : String

structure ManifestPolicy where
  theorems : Array String
  modules : Array String
  sourceAuthorities : Array String
  permittedAxioms : Array String
  forbiddenAxioms : Array String
  unsafeFfiModules : Array String

structure PublicDefinitionInventory where
  module : String
  path : String
  mutationCount : Nat
  mutationDigest : String
  outsideCount : Nat
  outsideDigest : String
deriving DecidableEq, Repr

def identityModules : Array String := #[
  "AgentWorkbench.Domain.Identity",
  "AgentWorkbench.Domain.Facts"]

def domainModules : Array String := #[
  "AgentWorkbench.Domain.Work",
  "AgentWorkbench.Domain.Design",
  "AgentWorkbench.Domain.Review",
  "AgentWorkbench.Domain.Evidence",
  "AgentWorkbench.Domain.ExternalOperation"]

def policyModules : Array String := #[
  "AgentWorkbench.Policy.Traceability",
  "AgentWorkbench.Policy.Authority",
  "AgentWorkbench.Policy.Completion",
  "AgentWorkbench.Policy.Update"]

def kernelModules : Array String := #[
  "AgentWorkbench.Kernel.Replay",
  "AgentWorkbench.Kernel.Decide",
  "AgentWorkbench.Kernel.Gates",
  "AgentWorkbench.Kernel.Resolver"]

def productModuleRules : Array ModuleRule :=
  let domainImports := identityModules
  let policyImports := identityModules ++ domainModules
  identityModules.map (⟨·, #[]⟩) ++
    domainModules.map (⟨·, domainImports⟩) ++
    #[
      ⟨"AgentWorkbench.Policy.Traceability", policyImports⟩,
      ⟨"AgentWorkbench.Policy.Authority", policyImports⟩,
      ⟨"AgentWorkbench.Policy.Completion",
        policyImports ++ #[
          "AgentWorkbench.Policy.Traceability",
          "AgentWorkbench.Policy.Authority"]⟩,
      ⟨"AgentWorkbench.Policy.Update", policyImports⟩,
      ⟨"AgentWorkbench.Kernel.Replay", identityModules ++ domainModules⟩,
      ⟨"AgentWorkbench.Kernel.Decide",
        identityModules ++ domainModules ++ policyModules ++
          #["AgentWorkbench.Kernel.Replay"]⟩,
      ⟨"AgentWorkbench.Kernel.Gates",
        identityModules ++ domainModules ++ policyModules ++
          #["AgentWorkbench.Kernel.Replay"]⟩,
      ⟨"AgentWorkbench.Kernel.Resolver",
        identityModules ++ domainModules ++ policyModules ++ kernelModules⟩,
      ⟨"AgentWorkbench.Application.Service",
        identityModules ++ domainModules ++ policyModules ++ kernelModules⟩]

def publicDefinitionModuleRules : Array ModuleRule :=
  productModuleRules ++ #[
    ⟨"AgentWorkbench.Adapter.Codec", #[]⟩,
    ⟨"AgentWorkbench.Adapter.DurableFilesystem", #[]⟩,
    ⟨"AgentWorkbench.Adapter.SQLite", #[]⟩,
    ⟨"AgentWorkbench.Adapter.Update", #[]⟩,
    ⟨"AgentWorkbench.Cli.Program",
      #["AgentWorkbench.Application.Service"]⟩,
    ⟨"AgentWorkbench",
      #["AgentWorkbench.Application.Service"]⟩]

def sourcePathForModule (moduleName : String) : String :=
  String.intercalate "/" (moduleName.splitOn ".") ++ ".lean"

def compiledBoundaryLeanPaths : Array String := #[
  "Main.lean",
  "lakefile.lean"
]

def theoremRules : Array TheoremRule := #[
  ⟨"AgentWorkbench.Kernel.Replay.replay_deterministic", "AgentWorkbench.Audit.Expected.replay_deterministic"⟩,
  ⟨"AgentWorkbench.Kernel.Replay.replay_preserves_valid", "AgentWorkbench.Audit.Expected.replay_preserves_valid"⟩,
  ⟨"AgentWorkbench.Kernel.Replay.work_completed_event_exact", "AgentWorkbench.Audit.Expected.work_completed_event_exact"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.decide_preserves_valid", "AgentWorkbench.Audit.Expected.decide_preserves_valid"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.decide_emits_only_derived_events", "AgentWorkbench.Audit.Expected.decide_emits_only_derived_events"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.decide_rejection_has_no_effect", "AgentWorkbench.Audit.Expected.decide_rejection_has_no_effect"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.close_work_preserves_valid", "AgentWorkbench.Audit.Expected.close_work_preserves_valid"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.close_work_emits_atomic_event", "AgentWorkbench.Audit.Expected.close_work_emits_atomic_event"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.decide_complete_requires_closeable", "AgentWorkbench.Audit.Expected.decide_complete_requires_closeable"⟩,
  ⟨"AgentWorkbench.Kernel.Decide.replay_completion_applicability_matches_policy", "AgentWorkbench.Audit.Expected.replay_completion_applicability_matches_policy"⟩,
  ⟨"AgentWorkbench.Domain.Work.single_active_activation", "AgentWorkbench.Audit.Expected.single_active_activation"⟩,
  ⟨"AgentWorkbench.Domain.Work.resume_requires_readiness", "AgentWorkbench.Audit.Expected.resume_requires_readiness"⟩,
  ⟨"AgentWorkbench.Policy.Authority.review_claim_has_no_authority", "AgentWorkbench.Audit.Expected.review_claim_has_no_authority"⟩,
  ⟨"AgentWorkbench.Kernel.Gates.gate_is_read_only", "AgentWorkbench.Audit.Expected.gate_is_read_only"⟩,
  ⟨"AgentWorkbench.Kernel.Gates.all_gates_are_read_only", "AgentWorkbench.Audit.Expected.gates_all_read_only"⟩,
  ⟨"AgentWorkbench.Kernel.Resolver.next_is_allowed", "AgentWorkbench.Audit.Expected.next_is_allowed"⟩,
  ⟨"AgentWorkbench.Application.Service.status_is_read_only", "AgentWorkbench.Audit.Expected.status_is_read_only"⟩,
  ⟨"AgentWorkbench.Application.Service.next_is_read_only", "AgentWorkbench.Audit.Expected.next_is_read_only"⟩,
  ⟨"AgentWorkbench.Application.Service.every_gate_is_read_only", "AgentWorkbench.Audit.Expected.every_gate_is_read_only"⟩,
  ⟨"AgentWorkbench.Kernel.Projection.verified_stage_matches_replay", "AgentWorkbench.Audit.Expected.verified_stage_matches_replay"⟩,
  ⟨"AgentWorkbench.Kernel.Projection.adoption_is_atomic", "AgentWorkbench.Audit.Expected.adoption_is_atomic"⟩,
  ⟨"AgentWorkbench.Policy.Completion.completion_requires_current_obligations", "AgentWorkbench.Audit.Expected.completion_requires_current_obligations"⟩,
  ⟨"AgentWorkbench.Policy.Completion.completion_requires_authoritative_lifecycle", "AgentWorkbench.Audit.Expected.completion_requires_authoritative_lifecycle"⟩,
  ⟨"AgentWorkbench.Policy.Completion.completion_requires_active_target", "AgentWorkbench.Audit.Expected.completion_requires_active_target"⟩,
  ⟨"AgentWorkbench.Policy.Update.exact_retry_returns_same_receipt", "AgentWorkbench.Audit.Expected.exact_retry_returns_same_receipt"⟩
]

def expectedPermittedAxioms : Array String :=
  #["propext", "Quot.sound", "Classical.choice"]

def expectedForbiddenAxioms : Array String := #["sorryAx"]

def expectedUnsafeFfiModules : Array String := #[
  "AgentWorkbench.Adapter.SQLite",
  "AgentWorkbench.Adapter.DurableFilesystem",
  "AgentWorkbench.Adapter.Process",
  "AgentWorkbench.Adapter.Git"
]

def expectedSourceAuthorityPaths : Array String := #[
  "AgentWorkbench/Audit/Expected.lean",
  "AgentWorkbench/Application/Service.lean",
  "AgentWorkbench/Audit/Main.lean"
]

def expectedTrackedPaths : Array String := #[
  ".gitignore",
  "AgentWorkbench.lean",
  "AgentWorkbench/Adapter/Codec.lean",
  "AgentWorkbench/Adapter/DurableFilesystem.lean",
  "AgentWorkbench/Adapter/SQLite.lean",
  "AgentWorkbench/Adapter/Update.lean",
  "AgentWorkbench/Application/Service.lean",
  "AgentWorkbench/Audit/Expected.lean",
  "AgentWorkbench/Audit/Main.lean",
  "AgentWorkbench/Cli/Program.lean",
  "AgentWorkbench/Domain/Design.lean",
  "AgentWorkbench/Domain/Evidence.lean",
  "AgentWorkbench/Domain/ExternalOperation.lean",
  "AgentWorkbench/Domain/Facts.lean",
  "AgentWorkbench/Domain/Identity.lean",
  "AgentWorkbench/Domain/Review.lean",
  "AgentWorkbench/Domain/Work.lean",
  "AgentWorkbench/Kernel/Decide.lean",
  "AgentWorkbench/Kernel/Gates.lean",
  "AgentWorkbench/Kernel/Replay.lean",
  "AgentWorkbench/Kernel/Resolver.lean",
  "AgentWorkbench/Policy/Authority.lean",
  "AgentWorkbench/Policy/Completion.lean",
  "AgentWorkbench/Policy/Traceability.lean",
  "AgentWorkbench/Policy/Update.lean",
  "AgentWorkbench/Tests/KernelLaws.lean",
  "AgentWorkbench/Tests/StorageLaws.lean",
  "AgentWorkbench/Tests/WorkflowLaws.lean",
  "Main.lean",
  "bindings/durable_filesystem.c",
  "lake-manifest.json",
  "lakefile.lean",
  "lean-toolchain",
  "proof-manifest.toml"
]

def verificationAndBoundaryPaths : Array String := #[
  ".gitignore",
  "AgentWorkbench/Audit/Expected.lean",
  "AgentWorkbench/Audit/Main.lean",
  "AgentWorkbench/Tests/KernelLaws.lean",
  "AgentWorkbench/Tests/StorageLaws.lean",
  "AgentWorkbench/Tests/WorkflowLaws.lean",
  "proof-manifest.toml"
]

def publicProductPaths : Array String :=
  expectedTrackedPaths.filter fun path => !verificationAndBoundaryPaths.contains path

def expectedInventory (moduleName : String) (mutationCount : Nat)
    (mutationDigest : String) (outsideCount : Nat)
    (outsideDigest : String) : PublicDefinitionInventory := {
  module := moduleName
  path := sourcePathForModule moduleName
  mutationCount
  mutationDigest
  outsideCount
  outsideDigest
}

def emptyDeclarationDigest : String :=
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

def expectedPublicDefinitions : Array PublicDefinitionInventory := #[
  expectedInventory "AgentWorkbench.Domain.Identity" 0 emptyDeclarationDigest 97
    "87dd11c24eea43789a656a50d722af4e70502a6905afcd1c24cd8f08536a302c",
  expectedInventory "AgentWorkbench.Domain.Facts" 0 emptyDeclarationDigest 37
    "56d9c8b8b7a8f0a49f0fc9e6d80d1a7cf317624a5e487e85a20dfccaec3c46b6",
  expectedInventory "AgentWorkbench.Domain.Work" 0 emptyDeclarationDigest 63
    "ea09d29fb20293c15be638369f52eaa863549dd815d67fa5f64252241034e428",
  expectedInventory "AgentWorkbench.Domain.Design" 0 emptyDeclarationDigest 226
    "ddc71a85811bfb2a71cf3a5668d907a5c7b03ae2c87c4e496c824c985df6b295",
  expectedInventory "AgentWorkbench.Domain.Review" 0 emptyDeclarationDigest 113
    "b5b306e9c1b39870e21da40a656fbff93b2dc6a9424a8d3d96b00ae188c085af",
  expectedInventory "AgentWorkbench.Domain.Evidence" 0 emptyDeclarationDigest 57
    "05c872cf356dba7255a93a74fb8797e2c6ee243e6991fe427862f9f46395173c",
  expectedInventory "AgentWorkbench.Domain.ExternalOperation" 0 emptyDeclarationDigest 55
    "5460ed55eabf067e9505b077b54d83e86954da18f4df19ea2609216288baa069",
  expectedInventory "AgentWorkbench.Policy.Traceability" 0 emptyDeclarationDigest 13
    "058989c9b6a12ed1432a59d7f49d3e605ec7c51571b6d6a99d153567374bb2ed",
  expectedInventory "AgentWorkbench.Policy.Authority" 0 emptyDeclarationDigest 12
    "94a01515155893bc391f9d27a97c8b14295c17d4fabeccb8a4f4703cb37856cd",
  expectedInventory "AgentWorkbench.Policy.Completion" 0 emptyDeclarationDigest 15
    "d0fb4de3879d5578e60bd8341434304d7de920c34c7b3c72c88353c96e9ae73c",
  expectedInventory "AgentWorkbench.Policy.Update" 0 emptyDeclarationDigest 13
    "db1c26c0195afe78005771c79cf7425a7cc30094a1cd68f80f62e3305b1d67be",
  expectedInventory "AgentWorkbench.Kernel.Replay" 0 emptyDeclarationDigest 190
    "4fea7d43ad1c5ca22b653172738d1d0b24c284e2e444413d6411dd01776dd799",
  expectedInventory "AgentWorkbench.Kernel.Decide" 0 emptyDeclarationDigest 18
    "8dd996b4bbfad62ddbaba3265af7967f5f785ca5a6410b245f20aa2bbb07ae25",
  expectedInventory "AgentWorkbench.Kernel.Gates" 0 emptyDeclarationDigest 14
    "b1809af11d5f59de753f6e2507886b3aac693c7122b7767f8eebdef73189aa8d",
  expectedInventory "AgentWorkbench.Kernel.Resolver" 0 emptyDeclarationDigest 20
    "5cf1fcf72239a6faed8c568f2abc2ed5d02971628f4c63fd03362766ccd9aec1",
  expectedInventory "AgentWorkbench.Application.Service" 6
    "4ecaa9b6807063d57cfc6049a335a876b1dd7d6056aa1375af468084f93ad36c" 23
    "9e1fa50367bd5ce24382e8682add73441d48f52866affc869408a52663bc2963",
  expectedInventory "AgentWorkbench.Adapter.Codec" 0 emptyDeclarationDigest 284
    "cffb5f5dc387432d6dc4e963ce5687a143a359e06303a08a6d0f3fa94ca21049",
  expectedInventory "AgentWorkbench.Adapter.DurableFilesystem" 2
    "70c5d35673c2a7de0f0e5f5eae9a6756a048170609126747b6358de604ec4736" 21
    "95dfd9064de24675f4de9689d55d3bc9129255419231bbc7939606fde2445157",
  expectedInventory "AgentWorkbench.Adapter.SQLite" 8
    "82d90d178b57b2375740d74ef9a031b65cdef922b265a3d959f919aad8803597" 47
    "8435c3d97171190d09d31822bdaf450fc3b7ddfd8ab36aafc1ac9c159a9e9721",
  expectedInventory "AgentWorkbench.Adapter.Update" 6
    "fec4d16ac64810ad3727d320205ea6f4bff37a5108a357bf824db44805324e27" 31
    "f0ae7b33ed816a23d885f55f4f461ae7f932766960f698cd5184bfec9afa2006",
  expectedInventory "AgentWorkbench.Cli.Program" 3
    "231be32dca19471a2473244788b8c9876465c972dd76025b14fb35b0705f39b6" 4
    "6bb025900d2d164c2153d54dc40f08d7d20aa935452c9308a62fc658cd04f593",
  expectedInventory "AgentWorkbench" 0 emptyDeclarationDigest 0
    emptyDeclarationDigest
]

def expectedMutationSurfaces : Array String := #[
  "AgentWorkbench.Adapter.DurableFilesystem.stage",
  "AgentWorkbench.Adapter.DurableFilesystem.replace",
  "AgentWorkbench.Adapter.SQLite.withWriterLock",
  "AgentWorkbench.Adapter.SQLite.initializeStore",
  "AgentWorkbench.Adapter.SQLite.repairProjectionWithLockHook",
  "AgentWorkbench.Adapter.SQLite.repairProjectionWithHook",
  "AgentWorkbench.Adapter.SQLite.repairProjection",
  "AgentWorkbench.Adapter.SQLite.mutateWithLockHook",
  "AgentWorkbench.Adapter.SQLite.mutateWithHook",
  "AgentWorkbench.Adapter.SQLite.mutate",
  "AgentWorkbench.Adapter.Update.applyWithLockHook",
  "AgentWorkbench.Adapter.Update.applyWithHook",
  "AgentWorkbench.Adapter.Update.apply",
  "AgentWorkbench.Adapter.Update.restoreWithLockHook",
  "AgentWorkbench.Adapter.Update.restoreWithHook",
  "AgentWorkbench.Adapter.Update.restore",
  "AgentWorkbench.Application.Service.execute",
  "AgentWorkbench.Application.Service.complete",
  "AgentWorkbench.Application.Service.repairProjection",
  "AgentWorkbench.Application.Service.executeRecovery",
  "AgentWorkbench.Application.Service.executeAction",
  "AgentWorkbench.Application.Service.executeRequest",
  "AgentWorkbench.Cli.Program.executeRequest",
  "AgentWorkbench.Cli.Program.executeBootstrap",
  "AgentWorkbench.Cli.Program.run"
]

def expectedCliEntrypoint : String :=
  "import AgentWorkbench.Cli.Program\n\ndef main : IO Unit :=\n  AgentWorkbench.Cli.Program.run\n"

def fail (message : String) : IO α :=
  throw <| IO.userError s!"verified-core audit failed: {message}"

def lines (content : String) : Array String :=
  (content.splitOn "\n").toArray

def validateTrackedPaths (actual : Array String) : Except String Unit := do
  unless actual = expectedTrackedPaths do
    throw "tracked repository paths differ from the exhaustive inventory"

def isPublicDefinition : ConstantInfo → Bool
  | .defnInfo _ | .opaqueInfo _ => true
  | _ => false

def hasDeclarationRange (env : Environment) (name : Name) : Bool :=
  (declRangeExt.find? (level := .exported) env name).isSome ||
    (declRangeExt.find? (level := .server) env name).isSome

def publicDefinitionsInModule (env : Environment)
    (moduleName : String) : Array String :=
  (env.constants.toList.filterMap fun (name, info) =>
    if isPublicDefinition info && !isPrivateName name &&
        hasDeclarationRange env name then
      match env.getModuleIdxFor? name with
      | none => none
      | some moduleIdx =>
          if env.header.moduleNames[moduleIdx.toNat]! == moduleName.toName then
            some name.toString
          else
            none
    else
      none).toArray.qsort (· < ·)

def publicDefinitionsInNamespace (env : Environment)
    (namespaceName : String) : Array String :=
  let namePrefix := namespaceName ++ "."
  (env.constants.toList.filterMap fun (name, info) =>
    let rendered := name.toString
    if isPublicDefinition info && rendered.startsWith namePrefix then
      let suffix := (rendered.drop namePrefix.length).toString
      if !suffix.isEmpty && !suffix.contains '.' then some suffix else none
    else
      none).toArray.qsort (· < ·)

def declarationDigest (definitions : Array String) : IO String :=
  IO.FS.withTempFile fun handle path => do
    handle.putStr (String.intercalate "\n" definitions.toList)
    handle.flush
    let output ← IO.Process.output {
      cmd := "sha256sum"
      args := #[path.toString]
    }
    unless output.exitCode = 0 do
      fail s!"compiled declaration digest failed: {output.stderr}"
    match output.stdout.splitOn " " with
    | digest :: _ =>
        unless digest.length = 64 do
          fail "compiled declaration digest has an invalid length"
        pure digest
    | _ => fail "compiled declaration digest is missing"

def actualInventory (expected : PublicDefinitionInventory)
    (definitions : Array String) : IO PublicDefinitionInventory := do
  let mutations := definitions.filter expectedMutationSurfaces.contains
  let outside := definitions.filter fun name =>
    !expectedMutationSurfaces.contains name
  return {
    module := expected.module
    path := sourcePathForModule expected.module
    mutationCount := mutations.size
    mutationDigest := ← declarationDigest mutations
    outsideCount := outside.size
    outsideDigest := ← declarationDigest outside
  }

def validatePublicDefinitions
    (actual : Array PublicDefinitionInventory) : Except String Unit := do
  unless actual = expectedPublicDefinitions do
    throw s!"public definition surfaces differ from the exhaustive inventory: {repr actual}"

def validateMutationSurfaces (actual : Array String) : Except String Unit := do
  unless actual = expectedMutationSurfaces.qsort (· < ·) do
    throw s!"mutation and update surfaces differ from the exhaustive inventory: {repr actual}"

structure PrivateIdentifierFixture where
  marker : String
  value : String

def privateIdentifierLabels : Array String := #[
  "acceptance" ++ "_record_id",
  "activation" ++ "_id",
  "application" ++ "_id",
  "attempt" ++ "_id",
  "authority" ++ "_event_id",
  "authority" ++ "_id",
  "checklist" ++ "_id",
  "checklist" ++ "_item_id",
  "child" ++ "_activation_id",
  "child" ++ "_work_unit_id",
  "closure" ++ "_id",
  "command" ++ "_deviation_id",
  "command" ++ "_profile_id",
  "command" ++ "_usage_id",
  "correction" ++ "_session_id",
  "corrected" ++ "_design_version_id",
  "decision" ++ "_id",
  "decomposition" ++ "_plan_id",
  "dependency" ++ "_id",
  "design" ++ "_package_id",
  "design" ++ "_requirement_id",
  "design" ++ "_version_id",
  "finding" ++ "_id",
  "finding" ++ "_verification_id",
  "fork" ++ "_id",
  "git" ++ "_commit_id",
  "git" ++ "_file_change_id",
  "invocation" ++ "_id",
  "kpt" ++ "_item_conversion_id",
  "kpt" ++ "_item_id",
  "kpt" ++ "_review_id",
  "kpt" ++ "_rule_id",
  "next" ++ "_phase_id",
  "parent" ++ "_activation_id",
  "parent" ++ "_suspend_snapshot_id",
  "parent" ++ "_work_unit_id",
  "phase" ++ "_id",
  "predecessor" ++ "_plan_id",
  "previous" ++ "_checklist_item_id",
  "repair" ++ "_run_id",
  "repository" ++ "_id",
  "repository" ++ "_dirty_entry_id",
  "repository" ++ "_snapshot_comparison_id",
  "repository" ++ "_snapshot_id",
  "repository" ++ "_state_classification_id",
  "resume" ++ "_check_id",
  "retirement" ++ "_id",
  "review" ++ "_plan_id",
  "review" ++ "_plan_target_id",
  "review" ++ "_policy_id",
  "review" ++ "_run_id",
  "review" ++ "_scope_id",
  "selected" ++ "_work_unit_id",
  "source" ++ "_attempt_id",
  "source" ++ "_closure_id",
  "source" ++ "_session_id",
  "source" ++ "_work_unit_id",
  "successor" ++ "_attempt_id",
  "successor" ++ "_closure_id",
  "successor" ++ "_plan_id",
  "successor" ++ "_session_id",
  "superseded" ++ "_closure_id",
  "suspend" ++ "_snapshot_id",
  "target" ++ "_work_unit_id",
  "task" ++ "_derivation_id",
  "task" ++ "_id",
  "user" ++ "_correction_id",
  "validation" ++ "_gate_id",
  "validation" ++ "_gate_template_id",
  "validation" ++ "_run_id",
  "work" ++ "_record_command_id",
  "work" ++ "_record_commit_id",
  "work" ++ "_record_file_id",
  "work" ++ "_record_id",
  "work" ++ "_unit_id"
]

def privateIdentifierLabelFixtures : Array PrivateIdentifierFixture :=
  privateIdentifierLabels.foldl
    (fun fixtures label =>
      (fixtures.push ⟨label ++ ":", label ++ ":91"⟩).push
        ⟨label ++ "=", label ++ "=91"⟩)
    #[]

def repositoryInstanceLabels : Array String := #[
  "phase",
  "finding",
  "work" ++ "_unit",
  "task",
  "checklist",
  "review" ++ "_run",
  "repository" ++ "_snapshot"
]

def repositoryInstanceSeparators : Array String := #[":", "="]

def repositoryInstanceFixtures : Array PrivateIdentifierFixture :=
  repositoryInstanceLabels.foldl
    (fun fixtures label =>
      repositoryInstanceSeparators.foldl
        (fun fixtures separator =>
          fixtures.push
            ⟨label ++ separator, label ++ separator ++ "91"⟩)
        fixtures)
    #[]

def opaqueHandlePrefixes : Array String := #[
  "decision" ++ "_",
  "plan" ++ "_review" ++ "_"
]

def opaqueHandleFixtures : Array PrivateIdentifierFixture :=
  opaqueHandlePrefixes.map fun handlePrefix =>
    ⟨handlePrefix, handlePrefix ++ "0123456789abcdef0123456789abcdef"⟩

def reportedPrivateIdentifierFixtures : Array PrivateIdentifierFixture := #[
  ⟨"phase" ++ "=", "phase" ++ "=9"⟩,
  ⟨"finding" ++ "=", "finding" ++ "=91"⟩,
  ⟨"work" ++ "_unit" ++ "=", "work" ++ "_unit" ++ "=3"⟩,
  ⟨"task" ++ "=", "task" ++ "=27"⟩,
  ⟨"checklist" ++ "=", "checklist" ++ "=8"⟩,
  ⟨"review" ++ "_run" ++ "=", "review" ++ "_run" ++ "=168"⟩,
  ⟨"repository" ++ "_snapshot" ++ "=",
    "repository" ++ "_snapshot" ++ "=57"⟩,
  ⟨"decision" ++ "_",
    "decision" ++ "_0123456789abcdef0123456789abcdef"⟩,
  ⟨"plan" ++ "_review" ++ "_",
    "plan" ++ "_review" ++ "_0123456789abcdef0123456789abcdef"⟩
]

def privateRecipeFixtures : Array PrivateIdentifierFixture := #[
  ⟨"R" ++ "EQ-", "R" ++ "EQ-104"⟩,
  ⟨"G" ++ "ATE-", "G" ++ "ATE-22"⟩,
  ⟨"D" ++ "EC-", "D" ++ "EC-91"⟩,
  ⟨"gover" ++ "ned", "gover" ++ "ned-branch"⟩,
  ⟨"leg" ++ "acy", "leg" ++ "acy-compatibility-route"⟩,
  ⟨".agent-" ++ "workbench", ".agent-" ++ "workbench/ledger.sqlite"⟩,
  ⟨"phase" ++ ":", "phase" ++ ":3"⟩,
  ⟨"finding" ++ ":", "finding" ++ ":91"⟩,
  ⟨"review-" ++ "run:", "review-" ++ "run:168"⟩,
  ⟨"review-" ++ "plan:", "review-" ++ "plan:47"⟩,
  ⟨"review-" ++ "context:", "review-" ++ "context:design-implementation-diff"⟩,
  ⟨"authority" ++ "_event:", "authority" ++ "_event:48"⟩,
  ⟨"closure" ++ "_attempt:", "closure" ++ "_attempt:134"⟩,
  ⟨"work" ++ "_unit:", "work" ++ "_unit:3"⟩,
  ⟨"design" ++ "_review", "design" ++ "_review"⟩,
  ⟨"design" ++ "_task_decomposition", "design" ++ "_task_decomposition"⟩,
  ⟨"design" ++ "_implementation_diff", "design" ++ "_implementation_diff"⟩,
  ⟨"implementation" ++ "_review", "implementation" ++ "_review"⟩,
  ⟨"work-" ++ "unit:", "work-" ++ "unit:3"⟩,
  ⟨"design-" ++ "version:", "design-" ++ "version:14"⟩,
  ⟨"repository-" ++ "snapshot:", "repository-" ++ "snapshot:64"⟩,
  ⟨"closure" ++ ":", "closure" ++ ":112"⟩,
  ⟨"task" ++ ":", "task" ++ ":27"⟩,
  ⟨"checklist" ++ ":", "checklist" ++ ":8"⟩,
  ⟨"validation-" ++ "run:", "validation-" ++ "run:19"⟩
]

def privateIdentifierFixtures : Array PrivateIdentifierFixture :=
  privateRecipeFixtures ++ privateIdentifierLabelFixtures ++
    repositoryInstanceFixtures ++ opaqueHandleFixtures ++
    reportedPrivateIdentifierFixtures

def forbiddenPublicMarkers : Array String :=
  privateIdentifierFixtures.map (·.marker)

def validatePublicOutputGeneratorInventory : Except String Unit := do
  let expectedLabels := #[
    "phase",
    "finding",
    "work" ++ "_unit",
    "task",
    "checklist",
    "review" ++ "_run",
    "repository" ++ "_snapshot"
  ]
  let expectedSeparators := #[":", "="]
  let expectedOpaquePrefixes := #[
    "decision" ++ "_",
    "plan" ++ "_review" ++ "_"
  ]
  unless repositoryInstanceLabels = expectedLabels do
    throw "repository-instance output labels differ from the exhaustive inventory"
  unless repositoryInstanceSeparators = expectedSeparators do
    throw "repository-instance output separators differ from the exhaustive inventory"
  unless opaqueHandlePrefixes = expectedOpaquePrefixes do
    throw "opaque output handle prefixes differ from the exhaustive inventory"

def expectedPublicOutputGenerators : Array String := #[
  "AgentWorkbench.Application.Service.actionErrorOutput",
  "AgentWorkbench.Application.Service.actionOutput",
  "AgentWorkbench.Application.Service.executeAction",
  "AgentWorkbench.Application.Service.executeRequest",
  "AgentWorkbench.Application.Service.gateOutput",
  "AgentWorkbench.Application.Service.inspectionOutput",
  "AgentWorkbench.Application.Service.renderBootstrap",
  "AgentWorkbench.Application.Service.renderDecision",
  "AgentWorkbench.Application.Service.resolutionOutput"
]

def expectedGeneratedResponseProjections : Array String := #[
  "AgentWorkbench.Application.Service.Response.output",
  "AgentWorkbench.Application.Service.Response.store"
]

def contentHasMarker (content : String) (markers : Array String) : Bool :=
  markers.any fun marker => content.contains marker

def validateTrackedProductContent (path content : String) : Except String Unit := do
  for marker in forbiddenPublicMarkers do
    if content.contains marker then
      throw s!"tracked product surface {path} contains private planning marker {marker}"

def auditRepositoryInventory : IO Unit := do
  let output ← IO.Process.output { cmd := "git", args := #["ls-files"] }
  unless output.exitCode = 0 do
    fail s!"tracked path inventory command failed: {output.stderr}"
  let actual := lines output.stdout |>.filter (· != "")
  match validateTrackedPaths actual with
  | .error error => fail error
  | .ok _ => pure ()
  unless expectedTrackedPaths.all fun path =>
      publicProductPaths.contains path || verificationAndBoundaryPaths.contains path do
    fail "tracked path inventory contains an unclassified path"
  unless publicProductPaths.all fun path => !verificationAndBoundaryPaths.contains path do
    fail "public and verification path inventories overlap"
  let inventoriedLeanPaths :=
    publicDefinitionModuleRules.map (sourcePathForModule ·.module)
  let publicLeanPaths := publicProductPaths.filter (·.endsWith ".lean")
  let classifiedLeanPaths :=
    (inventoriedLeanPaths ++ compiledBoundaryLeanPaths).qsort (· < ·)
  unless publicLeanPaths.qsort (· < ·) = classifiedLeanPaths do
    fail "public Lean module paths differ from compiled inventory and boundary wrappers"
  let unregistered := actual.push "unregistered-product-surface.lean"
  if (validateTrackedPaths unregistered).isOk then
    fail "negative unregistered tracked-path fixture was accepted"

def auditPublicProductSurfaces : IO Unit := do
  match validatePublicOutputGeneratorInventory with
  | .error error => fail error
  | .ok _ => pure ()
  for fixture in privateIdentifierFixtures do
    unless contentHasMarker fixture.value forbiddenPublicMarkers do
      fail s!"negative private-identifier fixture was not rejected for {fixture.value}"
  let ordinaryProductLanguage := #[
    "a decision records an accepted product outcome",
    "a phase groups coherent work",
    "a finding describes an observed product defect",
    "a review checks the completed product boundary"
  ]
  for path in expectedTrackedPaths do
    for fixture in privateIdentifierFixtures do
      if (validateTrackedProductContent path fixture.value).isOk then
        fail s!"negative tracked artifact fixture {fixture.value} was accepted for {path}"
    for content in ordinaryProductLanguage do
      if !(validateTrackedProductContent path content).isOk then
        fail s!"ordinary product language was rejected for {path}: {content}"
  for path in expectedTrackedPaths do
    let content ← IO.FS.readFile path
    match validateTrackedProductContent path content with
    | .error error => fail error
    | .ok _ => pure ()

def auditPublicDefinitions (env : Environment) : IO Unit := do
  let expectedModules := expectedPublicDefinitions.map (·.module)
  let derivedModules := publicDefinitionModuleRules.map (·.module)
  unless expectedModules = derivedModules do
    fail "compiled declaration inventory does not derive from the product module map"
  let mut actualDefinitions : Array PublicDefinitionInventory := #[]
  let mut allDefinitions : Array String := #[]
  for expected in expectedPublicDefinitions do
    let definitions := publicDefinitionsInModule env expected.module
    allDefinitions := allDefinitions ++ definitions
    actualDefinitions := actualDefinitions.push
      (← actualInventory expected definitions)
  match validatePublicDefinitions actualDefinitions with
  | .error error => fail error
  | .ok _ => pure ()
  let mutationSurfaces :=
    allDefinitions.filter expectedMutationSurfaces.contains |>.qsort (· < ·)
  match validateMutationSurfaces mutationSurfaces with
  | .error error => fail error
  | .ok _ => pure ()
  let unregisteredModule := actualDefinitions.push <|
    expectedInventory "AgentWorkbench.Adapter.Unregistered" 0
      emptyDeclarationDigest 0 emptyDeclarationDigest
  if (validatePublicDefinitions unregisteredModule).isOk then
    fail "negative unregistered public-module fixture was accepted"
  let alteredDefinitions := actualDefinitions.map fun inventory =>
    if inventory.module = "AgentWorkbench.Adapter.Codec" then
      { inventory with
        outsideCount := inventory.outsideCount + 1
        outsideDigest :=
          "0000000000000000000000000000000000000000000000000000000000000000" }
    else
      inventory
  if (validatePublicDefinitions alteredDefinitions).isOk then
    fail "negative unregistered public-definition fixture was accepted"
  let alteredMutations := mutationSurfaces.push
    "AgentWorkbench.Adapter.SQLite.alternateMutation"
  if (validateMutationSurfaces alteredMutations).isOk then
    fail "negative unregistered mutation-surface fixture was accepted"
  let codecDefinitions :=
    publicDefinitionsInModule env "AgentWorkbench.Adapter.Codec"
  unless codecDefinitions.contains
      "AgentWorkbench.Adapter.Codec.instToBinaryWorkUnit" do
    fail "compiled Codec declaration fixture was not enumerated by module provenance"
  let fixtureNamespace :=
    "AgentWorkbench.Audit.Expected.PublicDeclarationFixtures"
  let fixtureDefinitions := publicDefinitionsInNamespace env fixtureNamespace
  let expectedFixtures := #[
    "attributedMutation", "indentedMutation", "opaqueMutation", "unsafeMutation"]
  unless fixtureDefinitions = expectedFixtures do
    fail s!"compiled declaration-form fixtures were not exhaustively enumerated: {repr fixtureDefinitions}"

def parseEntries (key : String) : List String → Array String → Except String (Array String)
  | [], _ => .error s!"unterminated manifest array: {key}"
  | line :: rest, values =>
      let trimmed := line.trimAscii.toString
      if trimmed = "]" then
        .ok values
      else if trimmed.isEmpty then
        parseEntries key rest values
      else
        let value := if trimmed.endsWith "," then (trimmed.dropEnd 1).toString else trimmed
        if value.startsWith "\"" && value.endsWith "\"" && value.length ≥ 2 then
          parseEntries key rest (values.push <| ((value.drop 1).dropEnd 1).toString)
        else
          .error s!"invalid manifest value in {key}: {trimmed}"

def parseManifestArray (key content : String) : Except String (Array String) :=
  let marker := s!"{key} = ["
  match (content.splitOn "\n").dropWhile (·.trimAscii.toString != marker) with
  | [] => .error s!"missing manifest array: {key}"
  | _ :: rest => parseEntries key rest #[]

def parseManifestPolicy (content : String) : Except String ManifestPolicy := do
  unless (content.splitOn "\n").any (·.trimAscii.toString = "version = 1") do
    throw "manifest version must be exactly 1"
  return {
    theorems := ← parseManifestArray "theorems" content
    modules := ← parseManifestArray "modules" content
    sourceAuthorities := ← parseManifestArray "source_authorities" content
    permittedAxioms := ← parseManifestArray "permitted_axioms" content
    forbiddenAxioms := ← parseManifestArray "forbidden_axioms" content
    unsafeFfiModules := ← parseManifestArray "unsafe_ffi_modules" content }

def validateManifestPolicy (designRules : Array ModuleRule)
    (policy : ManifestPolicy) : Except String Unit := do
  unless policy.theorems = theoremRules.map (·.declaration) do
    throw "theorem manifest differs from immutable Lean policy"
  unless policy.modules = designRules.map (·.module) do
    throw "module manifest differs from the approved Design Package module map"
  let authorityPaths := policy.sourceAuthorities.map fun entry =>
    (entry.splitOn "=").head?.getD ""
  unless authorityPaths = expectedSourceAuthorityPaths do
    throw "source authority paths differ from immutable Lean policy"
  unless policy.sourceAuthorities.all fun entry =>
      match entry.splitOn "=" with
      | [_path, digest] => digest.length = 64 && digest.all fun char =>
          char.isDigit || ('a' ≤ char && char ≤ 'f')
      | _ => false do
    throw "source authority entry must contain one lowercase SHA256 digest"
  unless policy.permittedAxioms = expectedPermittedAxioms do
    throw "permitted axiom manifest differs from immutable Lean policy"
  unless policy.forbiddenAxioms = expectedForbiddenAxioms do
    throw "forbidden axiom manifest differs from immutable Lean policy"
  unless policy.unsafeFfiModules = expectedUnsafeFfiModules do
    throw "unsafe/FFI boundary differs from immutable Lean policy"

def validateManifestContent (designRules : Array ModuleRule)
    (content : String) : Except String ManifestPolicy := do
  let policy ← parseManifestPolicy content
  validateManifestPolicy designRules policy
  return policy

def sha256File (path : String) : IO String := do
  let output ← IO.Process.output { cmd := "sha256sum", args := #[path] }
  unless output.exitCode = 0 do
    fail s!"sha256 authority command failed for {path}: {output.stderr}"
  match output.stdout.trimAscii.toString.splitOn " " with
  | digest :: _ =>
      unless digest.length = 64 do fail s!"invalid SHA256 output for {path}"
      pure digest
  | _ => fail s!"missing SHA256 output for {path}"

def actualSourceAuthorities : IO (Array String) := do
  let mut entries := #[]
  for path in expectedSourceAuthorityPaths do
    entries := entries.push s!"{path}={← sha256File path}"
  pure entries

def sourceAuthoritiesMatch (policy : ManifestPolicy) : IO Bool := do
  return policy.sourceAuthorities = (← actualSourceAuthorities)

def auditSourceAuthorities (policy : ManifestPolicy) : IO Unit := do
  unless ← sourceAuthoritiesMatch policy do
    fail "versioned source authority SHA256 differs from reviewed theorem/service boundary"

def auditNegativeFixtures (designRules : Array ModuleRule) (manifest : String) : IO Unit := do
  let fixtures : Array (String × String) := #[
    ("missing-theorem", manifest.replace
      "AgentWorkbench.Kernel.Replay.replay_deterministic"
      "AgentWorkbench.Kernel.Replay.unapproved_replacement"),
    ("expanded-axioms", manifest.replace
      "  \"Classical.choice\"," "  \"Classical.choice\",\n  \"sorryAx\","),
    ("unsafe-boundary", manifest.replace
      "AgentWorkbench.Adapter.SQLite" "AgentWorkbench.Application.Service")
  ]
  for (name, fixture) in fixtures do
    if fixture = manifest then fail s!"negative fixture did not mutate manifest: {name}"
    match validateManifestContent designRules fixture with
    | .error _ => pure ()
    | .ok _ => fail s!"negative manifest fixture was accepted: {name}"
  let actual ← actualSourceAuthorities
  let some first := actual.toList.head? | fail "source authority fixture is missing"
  let authorityFixture := manifest.replace first
    "AgentWorkbench/Audit/Expected.lean=0000000000000000000000000000000000000000000000000000000000000000"
  if authorityFixture = manifest then fail "source authority negative fixture did not mutate manifest"
  let fixturePolicy ← match validateManifestContent designRules authorityFixture with
    | .ok policy => pure policy
    | .error error => fail s!"source authority fixture was structurally invalid: {error}"
  if ← sourceAuthoritiesMatch fixturePolicy then
    fail "synchronized source-authority weakening fixture was accepted"

def modulePath (module : String) : System.FilePath :=
  System.FilePath.mk <| (module.replace "." "/") ++ ".lean"

def sourceImports (module : String) : IO (Array String) := do
  let source ← IO.FS.readFile (modulePath module)
  return (source.splitOn "\n").foldl (init := #[]) fun imports line =>
    let trimmed := line.trimAscii.toString
    if trimmed.startsWith "import " then imports.push (trimmed.drop 7).trimAscii.toString
    else imports

def directImports (rules : Array ModuleRule) (module : String) : List String :=
  match rules.find? (·.module = module) with
  | some rule => rule.imports.toList
  | none => []

def directDependents (rules : Array ModuleRule) (module : String) : List String :=
  (rules.filterMap fun rule =>
    if rule.imports.contains module then some rule.module else none).toList

partial def transitiveVisit (next : String → List String) : List String → List String → List String
  | [], visited => visited
  | module :: remaining, visited =>
      if visited.contains module then transitiveVisit next remaining visited
      else transitiveVisit next (next module ++ remaining) (module :: visited)

def importClosure (rules : Array ModuleRule) (module : String) : List String :=
  transitiveVisit (directImports rules) (directImports rules module) []

def dependentClosure (rules : Array ModuleRule) (module : String) : List String :=
  transitiveVisit (directDependents rules) (directDependents rules module) []

def auditReverificationBounds (actual : Array ModuleRule) : IO Unit := do
  for rule in actual do
    if (importClosure actual rule.module).contains rule.module then
      fail s!"module dependency cycle reaches {rule.module}"
  let workDependents := dependentClosure actual "AgentWorkbench.Domain.Work"
  for sibling in #["AgentWorkbench.Domain.Design", "AgentWorkbench.Domain.Review",
      "AgentWorkbench.Domain.Evidence", "AgentWorkbench.Domain.ExternalOperation"] do
    if workDependents.contains sibling then
      fail s!"Domain.Work change reaches sibling module {sibling}"
  let completionDependents := dependentClosure actual "AgentWorkbench.Policy.Completion"
  for lower in actual.filter fun rule => rule.module.startsWith "AgentWorkbench.Domain." do
    if completionDependents.contains lower.module then
      fail s!"Policy.Completion change reaches lower-level module {lower.module}"
  unless workDependents.contains "AgentWorkbench.Application.Service" do
    fail "Domain.Work dependent closure does not reach the application boundary"
  unless completionDependents.contains "AgentWorkbench.Application.Service" do
    fail "Policy.Completion dependent closure does not reach the application boundary"

def importsWithinDesign (designRules actualRules : Array ModuleRule) : Bool :=
  actualRules.all fun actual =>
    match designRules.find? (·.module = actual.module) with
    | none => false
    | some allowed => actual.imports.all allowed.imports.contains

def auditArchitecture (designRules : Array ModuleRule) : IO Unit := do
  let mut actualRules : Array ModuleRule := #[]
  for rule in designRules do
    unless ← (modulePath rule.module).pathExists do
      fail s!"missing normative module {rule.module}"
    let actual ← sourceImports rule.module
    actualRules := actualRules.push ⟨rule.module, actual⟩
    for imported in actual do
      unless rule.imports.contains imported do
        fail s!"{rule.module} imports forbidden dependency {imported}"
  unless importsWithinDesign designRules actualRules do
    fail "actual product imports are not a subset of Design Package authority"
  let invalidFixture := actualRules.map fun rule =>
    if rule.module = "AgentWorkbench.Domain.Work" then
      { rule with imports := rule.imports.push "AgentWorkbench.Domain.Review" }
    else rule
  if importsWithinDesign designRules invalidFixture then
    fail "negative out-of-map import fixture was accepted"
  auditReverificationBounds actualRules
  let cliImports ← sourceImports "Main"
  unless cliImports = #["AgentWorkbench.Cli.Program"] do
    fail "executable entrypoint must import only AgentWorkbench.Cli.Program"
  let cli ← IO.FS.readFile "Main.lean"
  unless cli = expectedCliEntrypoint do
    fail "executable entrypoint differs from the immutable compiled CLI wrapper"
  let programImports ← sourceImports "AgentWorkbench.Cli.Program"
  unless programImports = #["AgentWorkbench.Application.Service"] do
    fail "CLI program must import only AgentWorkbench.Application.Service"
  let program ← IO.FS.readFile (modulePath "AgentWorkbench.Cli.Program")
  for forbidden in #["AgentWorkbench.Domain", "AgentWorkbench.Policy", "AgentWorkbench.Kernel",
      "Domain.", "Policy.", "Kernel."] do
    if program.contains forbidden then
      fail s!"CLI program bypasses Application.Service through {forbidden}"
  let spoofed :=
    "import AgentWorkbench.Cli.Program\n\n-- Application.Service.execute Application.Service.bootstrapCommand\ndef main : IO Unit := pure ()\n"
  if spoofed = expectedCliEntrypoint then fail "negative CLI source fixture was accepted"

def declarationDependencies (info : ConstantInfo) : List Name :=
  let used (expr : Expr) := expr.getUsedConstants.toList
  match info with
  | .axiomInfo value => used value.type
  | .defnInfo value => used value.type ++ used value.value
  | .thmInfo value => used value.type ++ used value.value
  | .opaqueInfo value => used value.type ++ used value.value
  | .quotInfo _ => []
  | .inductInfo value => used value.type ++ value.ctors
  | .ctorInfo value => used value.type
  | .recInfo value => used value.type

partial def declarationResultType : Expr → Expr
  | .forallE _ _ body _ => declarationResultType body
  | result => result

def hasPublicOutputType (info : ConstantInfo) : Bool :=
  let dependencies := (declarationResultType info.type).getUsedConstants.toList
  dependencies.contains ``String ||
    dependencies.contains `AgentWorkbench.Application.Service.Response

def declarationValue? : ConstantInfo → Option Expr
  | .defnInfo value => some value.value
  | .thmInfo value => some value.value
  | .opaqueInfo value => some value.value
  | _ => none

partial def auditUnsafeReachability (env : Environment) : List Name → List Name → IO Unit
  | [], _ => pure ()
  | name :: rest, visited => do
      if visited.contains name then
        auditUnsafeReachability env rest visited
      else
        match env.find? name with
        | none => auditUnsafeReachability env rest (name :: visited)
        | some info =>
            if info.isUnsafe then fail s!"unsafe declaration reachable from verified roots: {name}"
            auditUnsafeReachability env (declarationDependencies info ++ rest) (name :: visited)

partial def declarationReaches (env : Environment) (target : Name) :
    List Name → List Name → Bool
  | [], _ => false
  | name :: rest, visited =>
      if name = target then true
      else if visited.contains name then declarationReaches env target rest visited
      else
        match env.find? name with
        | none => declarationReaches env target rest (name :: visited)
        | some info =>
            declarationReaches env target (declarationDependencies info ++ rest) (name :: visited)

def auditPublicOutputGenerators (env : Environment) : IO Unit := do
  let serviceDefinitions := publicDefinitionsInModule env
    "AgentWorkbench.Application.Service"
  let responseProjections := serviceDefinitions.filter
    (·.startsWith "AgentWorkbench.Application.Service.Response.")
  unless responseProjections = expectedGeneratedResponseProjections do
    fail s!"generated Response projections differ from the exact exclusion inventory: {repr responseProjections}"
  let actual := serviceDefinitions.filter fun declaration =>
      !expectedGeneratedResponseProjections.contains declaration &&
        match env.find? declaration.toName with
        | some info => hasPublicOutputType info
        | none => false
  unless actual = expectedPublicOutputGenerators do
    fail s!"compiled public output generators differ from the exhaustive inventory: {repr actual}"
  let publicRoots := [
    `AgentWorkbench.Cli.Program.executeRequest,
    `AgentWorkbench.Cli.Program.renderDecision,
    `AgentWorkbench.Cli.Program.renderBootstrap,
    `AgentWorkbench.Cli.Program.run
  ]
  for generator in expectedPublicOutputGenerators do
    let generatorName := generator.toName
    unless (env.find? generatorName).isSome do
      fail s!"compiled public output generator is missing: {generator}"
    unless declarationReaches env generatorName publicRoots [] do
      fail s!"compiled public output generator is not reachable from a public CLI renderer: {generator}"

def auditDeclarations (env : Environment) : IO Unit := do
  auditPublicOutputGenerators env
  for (name, info) in env.constants.toList do
    let rendered := name.toString
    if rendered.startsWith "AgentWorkbench." &&
        !rendered.startsWith "AgentWorkbench.Audit." &&
        !rendered.startsWith "AgentWorkbench.Tests." &&
        !expectedUnsafeFfiModules.any (fun moduleName =>
          rendered.startsWith (moduleName ++ ".")) &&
        info.isUnsafe then
      fail s!"unsafe declaration entered normative implementation: {rendered}"
  let roots := theoremRules.toList.map (·.declaration.toName) ++
    [`AgentWorkbench.Application.Service.execute,
     `AgentWorkbench.Application.Service.executeAction,
     `AgentWorkbench.Application.Service.bootstrapCommand,
     `AgentWorkbench.Cli.Program.executeBootstrap,
     `AgentWorkbench.Cli.Program.executeRequest,
     `AgentWorkbench.Cli.Program.run]
  auditUnsafeReachability env roots []
  unless declarationReaches env `AgentWorkbench.Application.Service.execute
      [`AgentWorkbench.Cli.Program.run] [] do
    let runDeps := (env.find? `AgentWorkbench.Cli.Program.run).map declarationDependencies
    let bootstrapDeps := (env.find? `AgentWorkbench.Cli.Program.executeBootstrap).map declarationDependencies
    fail s!"compiled CLI program does not reach Application.Service.execute; run={repr runDeps}; bootstrap={repr bootstrapDeps}"
  unless declarationReaches env `AgentWorkbench.Application.Service.executeRequest
      [`AgentWorkbench.Cli.Program.executeRequest] [] do
    fail "compiled CLI request path does not reach Application.Service.executeRequest"
  unless declarationReaches env `AgentWorkbench.Application.Service.executeAction
      [`AgentWorkbench.Cli.Program.executeRequest] [] do
    fail "compiled CLI request path does not reach resolver action execution"
  unless declarationReaches env `AgentWorkbench.Application.Service.execute
      [`AgentWorkbench.Application.Service.executeAction] [] do
    fail "resolver action execution does not reach an authoritative mutation"
  unless declarationReaches env `AgentWorkbench.Kernel.Decide.decide
      [`AgentWorkbench.Application.Service.execute] [] do
    fail "public Service.execute does not reach authoritative Decide.decide"
  unless declarationReaches env `AgentWorkbench.Kernel.Decide.closeWork
      [`AgentWorkbench.Application.Service.complete] [] do
    fail "public Service.complete does not reach authoritative Decide.closeWork"
  unless declarationReaches env `AgentWorkbench.Kernel.Gates.run
      [`AgentWorkbench.Application.Service.queryGate] [] do
    fail "public Service.queryGate does not reach authoritative Gates.run"
  unless declarationReaches env `AgentWorkbench.Kernel.Resolver.next
      [`AgentWorkbench.Application.Service.resolve] [] do
    fail "public Service.resolve does not reach authoritative Resolver.next"
  unless declarationReaches env `AgentWorkbench.Kernel.Projection.repair
      [`AgentWorkbench.Application.Service.repairProjection] [] do
    fail "public Service.repairProjection does not reach authoritative Projection.repair"
  unless declarationReaches env `AgentWorkbench.Application.Service.repairProjection
      [`AgentWorkbench.Cli.Program.executeRequest] [] do
    fail "compiled CLI request path does not reach the explicit projection repair mutation"
  if declarationReaches env `AgentWorkbench.Application.Service.execute
      [`AgentWorkbench.Audit.Expected.cliWithoutMutationFixture] [] then
    fail "negative compiled CLI dependency fixture unexpectedly reaches Service.execute"
  unless declarationReaches env `AgentWorkbench.Application.Service.execute
      [`AgentWorkbench.Audit.Expected.cliConditionalBypassFixture] [] do
    fail "conditional CLI bypass fixture did not retain its dead Service.execute dependency"
  let context : Core.Context := {
    fileName := "<verified-core-cli-audit>", fileMap := FileMap.ofString "" }
  let state : Core.State := { env := env }
  let compareCompiled (actualName expectedName : Name) : IO Bool := do
    let some actual := env.find? actualName | fail s!"compiled declaration missing: {actualName}"
    let some expected := env.find? expectedName | fail s!"expected declaration missing: {expectedName}"
    let some actualValue := declarationValue? actual |
      fail s!"compiled declaration has no inspectable value: {actualName}"
    let some expectedValue := declarationValue? expected |
      fail s!"expected declaration has no inspectable value: {expectedName}"
    let (sameType, _, _) ← (Meta.withTransparency .all <|
      Meta.isDefEq actual.type expected.type).toIO context state
    let (sameValue, _, _) ← (Meta.withTransparency .all <|
      Meta.isDefEq actualValue expectedValue).toIO context state
    return sameType && sameValue
  unless ← compareCompiled `AgentWorkbench.Cli.Program.executeBootstrap
      `AgentWorkbench.Audit.Expected.expectedExecuteBootstrap do
    fail "compiled CLI bootstrap differs from immutable expected implementation"
  unless ← compareCompiled `AgentWorkbench.Cli.Program.run
      `AgentWorkbench.Audit.Expected.expectedCliRun do
    fail "compiled CLI control flow differs from immutable expected implementation"
  if ← compareCompiled `AgentWorkbench.Audit.Expected.cliConditionalBypassFixture
      `AgentWorkbench.Audit.Expected.expectedCliRun then
    fail "conditional no-mutation CLI fixture matched expected CLI behavior"

def auditTheorems (env : Environment) : IO Unit := do
  let context : Core.Context := { fileName := "<verified-core-audit>", fileMap := FileMap.ofString "" }
  let state : Core.State := { env := env }
  for rule in theoremRules do
    let name := rule.declaration.toName
    let expectedName := rule.expectedType.toName
    let some info := env.find? name | fail s!"required declaration is absent: {rule.declaration}"
    let some expected := env.find? expectedName |
      fail s!"internal expected signature is absent: {rule.expectedType}"
    match info with
    | .thmInfo _ => pure ()
    | _ => fail s!"required declaration is not a theorem: {rule.declaration}"
    let (sameType, _, _) ← (Meta.isDefEq info.type expected.type).toIO context state
    unless sameType do
      fail s!"theorem type differs from immutable Lean signature: {rule.declaration}"
    if info.isUnsafe then fail s!"required theorem is unsafe: {rule.declaration}"
    let collect : CoreM (Array Name) := collectAxioms name
    let axioms ← collect.toIO' context state
    for axiomName in axioms do
      let rendered := toString axiomName
      if expectedForbiddenAxioms.contains rendered then
        fail s!"forbidden axiom {rendered} reached {rule.declaration}"
      unless expectedPermittedAxioms.contains rendered do
        fail s!"unpermitted axiom {rendered} reached {rule.declaration}"

def auditCliMutation : IO Unit := do
  let initial := Application.Service.initialStore
  match Cli.Program.executeBootstrap with
  | .error error => fail s!"CLI mutation fixture was rejected: {repr error}"
  | .ok transaction =>
      unless transaction.accepted.events.length = 1 do
        fail "CLI mutation did not emit exactly one event"
      unless transaction.accepted.result.state.revision = initial.ledger.storedHead.next do
        fail "CLI mutation did not advance exactly one revision"
      unless (Application.Service.queryValidity transaction.result).value = .pass do
        fail "CLI mutation result is not valid"

def auditResponseOutput : IO Unit := do
  let initial := Application.Service.initialStore
  let dynamicMarkers := forbiddenPublicMarkers ++ #[
    "AgentWorkbench.", "ledger-main", "owner-private-value",
    "outcome-private-value", "completion-private-value",
    "work-private-value", "activation-private-value",
    "/private/project/state", "sha3-256:", "991", "992"
  ]
  let checkOutput (label value : String) : IO Unit := do
    if contentHasMarker value dynamicMarkers then
      fail s!"{label} exposes an internal or dynamic value: {value}"
  let requireResponse (label : String)
      (result : Except String Application.Service.Response) :
      IO Application.Service.Response := do
    match result with
    | .error error =>
        checkOutput s!"{label} error" error
        fail s!"{label} was unexpectedly rejected: {error}"
    | .ok response =>
        checkOutput label response.output
        pure response
  let checkCliResult (label : String) (result : Except String String) : IO Unit := do
    match result with
    | .error error => checkOutput s!"{label} error" error
    | .ok output => checkOutput label output
  let hostilePoint : Domain.Projection.LedgerPoint := {
    ledger := ⟨"ledger-main"⟩
    revision := ⟨991⟩
    historyDigest := ⟨"sha3-256:history-private-value"⟩
  }
  let requests : Array Application.Service.Request := #[
    .status,
    .next,
    .gate .validState,
    .gate (.completion ⟨991⟩),
    .gate (.designReady ⟨991⟩),
    .gate (.traceReady ⟨991⟩ ⟨991⟩),
    .gate (.resumeReady ⟨991⟩ ⟨992⟩),
    .gate (.reviewReady ⟨991⟩),
    .gate (.evidenceExact ⟨991⟩ "outcome-private-value"),
    .gate (.correctionsReady "owner-private-value")
  ]
  let formatterActions : Array Kernel.Resolver.Action := #[
    .initializeWork hostilePoint,
    .continueActiveWork hostilePoint ⟨991⟩ ⟨992⟩,
    .resumeSuspendedWork hostilePoint ⟨991⟩ ⟨992⟩
  ]
  for action in formatterActions do
    checkOutput "action formatter" (Application.Service.actionOutput action)
    checkOutput "action error formatter"
      (Application.Service.actionErrorOutput action)
    checkOutput "resolution formatter"
      (Application.Service.resolutionOutput (.action action))
    match Application.Service.executeRequest (.action action) initial with
    | .ok _ => fail "adversarial stale action unexpectedly succeeded"
    | .error error => checkOutput "stale action rejection" error
  checkOutput "blocked resolution formatter" <|
    Application.Service.resolutionOutput <|
      .blocked (.noResumableActivation hostilePoint [⟨992⟩])
  let initializeAction ← match (Application.Service.resolve initial).value with
    | .action action@(.initializeWork _) => pure action
    | _ => fail "initialization success fixture did not produce an action"
  discard <| requireResponse "initialization success response"
    (Application.Service.executeRequest (.action initializeAction) initial)
  let hostileResponse : Application.Service.Response := {
    store := initial
    output := "owner-private-value /private/project/state sha3-256: 991"
  }
  checkCliResult "CLI bootstrap rejection" <|
    Cli.Program.renderBootstrap <|
      .error (.invalidTransition
        "owner-private-value /private/project/state sha3-256: 991")
  checkCliResult "CLI validity rejection" <|
    Cli.Program.renderDecision
      (.blocked "owner-private-value /private/project/state")
      (.action (.initializeWork hostilePoint))
      (fun _ => .ok hostileResponse)
  checkCliResult "CLI resolver rejection" <|
    Cli.Program.renderDecision .pass
      (.blocked (.noResumableActivation hostilePoint [⟨992⟩]))
      (fun _ => .ok hostileResponse)
  checkCliResult "CLI action rejection" <|
    Cli.Program.renderDecision .pass (.action (.initializeWork hostilePoint))
      (fun _ => .error
        "owner-private-value /private/project/state sha3-256: 991")
  checkCliResult "CLI success response" <|
    Cli.Program.renderDecision .pass (.action (.initializeWork hostilePoint))
      (fun _ => .ok hostileResponse)
  for request in requests do
    discard <| requireResponse "initial public response"
      (Application.Service.executeRequest request initial)
  let hostileCommand : Kernel.Decide.Command :=
    .initializeWork initial.ledger.storedHead
      { id := ⟨991⟩, status := .open, owner := "owner-private-value"
        outcome := "outcome-private-value /private/project/state"
        completionBoundary := "completion-private-value sha3-256:" }
      { id := ⟨992⟩, work := ⟨991⟩, status := .active
        readyToResume := false }
  let hostileStore ← match Application.Service.execute hostileCommand initial with
    | .error error => fail s!"hostile output fixture setup failed: {repr error}"
    | .ok transaction => pure transaction.result
  for request in requests do
    discard <| requireResponse "hostile public response"
      (Application.Service.executeRequest request hostileStore)
  let hostileAction ← match (Application.Service.resolve hostileStore).value with
    | .blocked blocker => fail s!"hostile action fixture was blocked: {repr blocker}"
    | .action action => pure action
  discard <| requireResponse "hostile action response"
    (Application.Service.executeRequest (.action hostileAction) hostileStore)
  let missingStore := { hostileStore with active := none }
  let repairCommand ← match (Application.Service.status missingStore).value.repairCommand? with
    | none => fail "repair output fixture did not produce a repair command"
    | some command => pure command
  discard <| requireResponse "repair request response"
    (Application.Service.executeRequest (.repairProjection repairCommand) missingStore)
  let repairAction ← match (Application.Service.resolve missingStore).value with
    | .action action@(.repairProjection _) => pure action
    | _ => fail "repair output fixture did not produce a repair action"
  checkOutput "repair error formatter"
    (Application.Service.actionErrorOutput repairAction)
  discard <| requireResponse "repair action response"
    (Application.Service.executeRequest (.action repairAction) missingStore)
  match Application.Service.executeRequest (.repairProjection repairCommand) hostileStore with
  | .ok _ => fail "mismatched repair output fixture unexpectedly succeeded"
  | .error error => checkOutput "repair rejection" error
  match Cli.Program.executeBootstrap with
  | .error error => fail s!"public action fixture bootstrap was rejected: {repr error}"
  | .ok transaction =>
      match (Application.Service.resolve transaction.result).value with
      | .blocked blocker => fail s!"public action fixture was blocked: {repr blocker}"
      | .action action =>
          match Application.Service.executeRequest (.action action) transaction.result with
          | .error error => fail s!"public action fixture was rejected: {error}"
          | .ok response =>
              checkOutput "bootstrap action response" response.output

inductive ManagedProjectStage
  | build
  | test
  | audit
  | behavior
deriving DecidableEq, Repr

inductive ManagedWorkflowStage
  | initialize
  | design
  | designReview
  | decompositionReview
  | decomposition
  | conformanceReview
  | qualityReview
  | planning
  | phase
  | task
  | checklist
  | workRecord
  | validation
  | repository
  | evidence
  | completion
deriving DecidableEq, Repr

structure ManagedProjectCommand where
  stage : ManagedProjectStage
  command : String
  args : Array String
deriving DecidableEq, Repr

structure ManagedProjectObservation where
  stage : ManagedProjectStage
  invocation : String
  exitCode : Int
  stdout : String
  stderr : String
deriving DecidableEq, Repr

structure ManagedWorkflowResult where
  stages : Array ManagedWorkflowStage
  evidenceCount : Nat
  stateDigest : String
  closed : Bool
deriving DecidableEq, Repr

structure ManagedProjectFixture where
  name : String
  language : String
  buildSystem : String
  validationTool : String
  publicFiles : Array (String × String)
  commands : Array ManagedProjectCommand
  behavior : ManagedProjectCommand
  privateIdentity : String

def cMakefile : String :=
  ".PHONY: build test audit\n" ++
  "build:\n\tmkdir -p build\n\t$(CC) -std=c11 -Wall -Wextra -Werror -o build/app src/main.c\n" ++
  "test: build\n\ttest \"$$(./build/app)\" = \"c-project-ok\"\n\t@printf 'c-test-ok\\n'\n" ++
  "audit:\n\t$(CC) -std=c11 -Wall -Wextra -Werror -fsyntax-only src/main.c\n\t@printf 'c-audit-ok\\n'\n"

def pythonBuild : String :=
  "import compileall\n" ++
  "if not compileall.compile_dir('src', quiet=1):\n" ++
  "    raise SystemExit(1)\n" ++
  "print('python-build-ok')\n"

def pythonTest : String :=
  "from src.app import project_result\n" ++
  "assert project_result() == 'python-project-ok'\n" ++
  "print('python-test-ok')\n"

def managedProjectFixtures : Array ManagedProjectFixture := #[
  {
    name := "c-make-project"
    language := "C"
    buildSystem := "Make"
    validationTool := "native executable assertion"
    publicFiles := #[
      ("src/main.c",
        "#include <stdio.h>\nint main(void) { puts(\"c-project-ok\"); return 0; }\n"),
      ("Makefile", cMakefile),
      ("README.md",
        "# C project\n\nBuild: `make build`\nTest: `make test`\nAudit: `make audit`\n")]
    commands := #[
      ⟨.build, "make", #["build"]⟩,
      ⟨.test, "make", #["test"]⟩,
      ⟨.audit, "make", #["audit"]⟩]
    behavior := ⟨.behavior, "./build/app", #[]⟩
    privateIdentity := "c-project-private-identity"
  },
  {
    name := "python-project"
    language := "Python"
    buildSystem := "compileall build program"
    validationTool := "Python assertion runner"
    publicFiles := #[
      ("src/__init__.py", ""),
      ("src/app.py",
        "def project_result():\n    return 'python-project-ok'\n\n" ++
        "if __name__ == '__main__':\n    print(project_result())\n"),
      ("build.py", pythonBuild),
      ("test_project.py", pythonTest),
      ("README.md",
        "# Python project\n\nBuild: `python3 build.py`\n" ++
        "Test: `python3 test_project.py`\n" ++
        "Audit: `python3 -m py_compile src/app.py test_project.py`\n")]
    commands := #[
      ⟨.build, "python3", #["build.py"]⟩,
      ⟨.test, "python3", #["test_project.py"]⟩,
      ⟨.audit, "python3", #["-m", "py_compile", "src/app.py", "test_project.py"]⟩]
    behavior := ⟨.behavior, "python3", #["src/app.py"]⟩
    privateIdentity := "python-project-private-identity"
  }
]

def writeFixtureFiles (project : System.FilePath)
    (files : Array (String × String)) : IO Unit := do
  for (relative, content) in files do
    let path := project / relative
    if let some parent := path.parent then IO.FS.createDirAll parent
    IO.FS.writeFile path content

def readFixtureFiles (project : System.FilePath)
    (files : Array (String × String)) : IO (Array (String × String)) := do
  let mut result := #[]
  for (relative, _) in files do
    result := result.push (relative, ← IO.FS.readFile (project / relative))
  pure result

def observeManagedCommand (project : System.FilePath)
    (command : ManagedProjectCommand) : IO ManagedProjectObservation := do
  let output ← IO.Process.output {
    cmd := command.command, args := command.args, cwd := some project }
  pure {
    stage := command.stage
    invocation := String.intercalate " " (command.command :: command.args.toList)
    exitCode := Int.ofNat output.exitCode.toNat
    stdout := output.stdout
    stderr := output.stderr
  }

def observeManagedProject (project : System.FilePath)
    (fixture : ManagedProjectFixture) : IO (Array ManagedProjectObservation) := do
  let mut observations := #[]
  for command in fixture.commands do
    observations := observations.push (← observeManagedCommand project command)
  observations := observations.push (← observeManagedCommand project fixture.behavior)
  pure observations

def requiredManagedStages : Array ManagedProjectStage :=
  #[.build, .test, .audit, .behavior]

def requiredManagedWorkflowStages : Array ManagedWorkflowStage :=
  #[.initialize, .design, .designReview, .decompositionReview,
    .decomposition, .conformanceReview, .qualityReview, .planning,
    .phase, .task, .checklist, .workRecord, .validation, .repository,
    .evidence, .completion]

def validateManagedFixtureSet
    (fixtures : Array ManagedProjectFixture) : Except String Unit := do
  unless fixtures.size = 2 do
    throw "managed-project acceptance requires exactly two fixtures"
  let some first := fixtures[0]? | throw "first managed-project fixture is missing"
  let some second := fixtures[1]? | throw "second managed-project fixture is missing"
  unless first.language != second.language &&
      first.buildSystem != second.buildSystem &&
      first.validationTool != second.validationTool do
    throw "managed-project fixtures do not use unrelated language and toolchain identities"
  let firstTools := (first.commands.map (·.command)).push first.behavior.command
  let secondTools := (second.commands.map (·.command)).push second.behavior.command
  unless firstTools.all fun tool => !secondTools.contains tool do
    throw "managed-project fixtures share executable project toolchains"
  for fixture in fixtures do
    unless fixture.commands.map (·.stage) = #[.build, .test, .audit] do
      throw s!"managed-project fixture {fixture.name} lacks documented build, test, or audit execution"

def validateManagedProjectResult (fixture : ManagedProjectFixture)
    (beforeFiles afterFiles : Array (String × String))
    (before after : Array ManagedProjectObservation)
    (privateState movedState : System.FilePath) : Except String Unit := do
  unless beforeFiles = afterFiles do
    throw s!"managed-project fixture {fixture.name} changed public project bytes"
  unless before = after do
    throw s!"managed-project fixture {fixture.name} depends on private state presence"
  unless requiredManagedStages.all fun stage =>
      before.any (·.stage == stage) do
    throw s!"managed-project fixture {fixture.name} omitted a required workflow stage"
  for observation in before do
    unless observation.exitCode = 0 do
      throw s!"managed-project fixture {fixture.name} failed during {repr observation.stage}"
  let privateMarkers := forbiddenPublicMarkers ++
    #[fixture.privateIdentity, privateState.toString, movedState.toString]
  for observation in before do
    if contentHasMarker observation.stdout privateMarkers ||
        contentHasMarker observation.stderr privateMarkers then
      throw s!"managed-project fixture {fixture.name} exposed private identity or path"

def executeManagedCommand (fixture : ManagedProjectFixture)
    (command : Kernel.Decide.Command) (store : Kernel.Projection.Store)
    (stage : String) : IO Kernel.Projection.Store := do
  match Application.Service.execute command store with
  | .error error =>
      fail s!"managed-project fixture {fixture.name} rejected {stage}: {repr error}"
  | .ok transaction => pure transaction.result

def managedObservationDigest
    (observations : Array ManagedProjectObservation) : IO String :=
  IO.FS.withTempFile fun handle path => do
    for observation in observations do
      handle.putStr s!"{repr observation.stage}\n{observation.invocation}\n"
      handle.putStr s!"{observation.exitCode}\n{observation.stdout}\n{observation.stderr}\n"
    handle.flush
    sha256File path.toString

def managedScope (design : Domain.DesignId) (work : Domain.WorkId)
    (snapshot artifact : String) (purpose : Domain.Review.Purpose) :
    Domain.Review.FrozenScope := {
  design := some design
  work
  repositorySnapshot := snapshot
  artifactDigest := artifact
  purpose
}

def recordManagedReview (fixture : ManagedProjectFixture)
    (id : Nat) (purpose : Domain.Review.Purpose)
    (design : Domain.DesignId) (work : Domain.WorkId) (snapshot artifact : String)
    (claimEpoch : Domain.CompletionEpoch) (store : Kernel.Projection.Store) :
    IO Kernel.Projection.Store := do
  let plan : Domain.Review.Plan := {
    id := ⟨id⟩
    owner := "bootstrap-owner"
    reviewer := s!"independent-reviewer-{id}"
    adjudicator := "bootstrap-owner"
    scope := managedScope design work snapshot artifact purpose
  }
  let store ← executeManagedCommand fixture
    (.recordReviewPlan store.ledger.storedHead plan) store
    s!"{repr purpose} review plan"
  let claim : Domain.Review.Claim := {
    id := ⟨id⟩
    plan := plan.id
    work
    epoch := claimEpoch
    claim := .clean
    reviewer := plan.reviewer
    scope := some plan.scope
  }
  let store ← executeManagedCommand fixture
    (.recordReviewClaim store.ledger.storedHead claim) store
    s!"{repr purpose} review claim"
  executeManagedCommand fixture
    (.recordReviewAdjudication store.ledger.storedHead {
      review := claim.id
      decision := .accepted
      adjudicator := plan.adjudicator
    }) store s!"{repr purpose} review adjudication"

def runManagedWorkflow (fixture : ManagedProjectFixture)
    (observations : Array ManagedProjectObservation) : IO ManagedWorkflowResult := do
  let transaction ← match Application.Service.execute
      Application.Service.bootstrapCommand Application.Service.initialStore with
    | .error error =>
        fail s!"managed-project fixture {fixture.name} could not initialize its workflow: {repr error}"
    | .ok transaction => pure transaction
  let mut stages : Array ManagedWorkflowStage := #[.initialize]
  let work : Domain.WorkId := ⟨1⟩
  let designId : Domain.DesignId := ⟨1⟩
  let design : Domain.Design.DesignVersion := {
    id := designId
    revision := ⟨1⟩
    owner := "bootstrap-owner"
    contentDigest := s!"design:{fixture.name}"
    requirements := [{ key := "managed-project-independence", active := true }]
    decisions := ["project tools enter through typed observations"]
    validationGates := ["project build, test, audit, and state independence"]
  }
  let snapshot := s!"fixture:{fixture.name}"
  let artifact ← managedObservationDigest observations
  let mut store ← executeManagedCommand fixture
    (.importDesign transaction.result.ledger.storedHead design)
    transaction.result "design import"
  stages := stages.push .design
  store ← recordManagedReview fixture 100 .design designId work
    snapshot design.contentDigest ⟨0⟩ store
  store ← executeManagedCommand fixture
    (.approveDesign store.ledger.storedHead designId) store "design approval"
  stages := stages.push .designReview
  let decompositionArtifact := s!"decomposition:{fixture.name}"
  store ← recordManagedReview fixture 200 .decomposition designId work
    snapshot decompositionArtifact ⟨0⟩ store
  stages := stages.push .decompositionReview
  let decomposition : Domain.Design.Decomposition := {
    key := decompositionArtifact
    design := designId
    work
    designRevision := design.revision
    contentDigest := decompositionArtifact
    items := [{
      key := "project-lifecycle"
      requirements := ["managed-project-independence"]
      implementationWork := ["run project toolchain"]
      tasks := ["build, test, and audit"]
      completionChecks := ["behavior is independent of private state"]
      checklists := ["typed evidence is exact"]
      validationGates := ["complete project workflow"]
    }]
    reviewer := "independent-reviewer-200"
    adjudicator := "bootstrap-owner"
    accepted := true
  }
  store ← executeManagedCommand fixture
    (.recordDecomposition store.ledger.storedHead decomposition)
    store "reviewed decomposition"
  stages := stages.push .decomposition
  let completionEpoch : Domain.CompletionEpoch := ⟨4⟩
  store ← recordManagedReview fixture 300 .designConformance designId work
    snapshot artifact completionEpoch store
  stages := stages.push .conformanceReview
  store ← recordManagedReview fixture 400 .implementationQuality designId work
    snapshot artifact completionEpoch store
  stages := stages.push .qualityReview
  let completionPlan : Domain.Lifecycle.CompletionPlan := {
    work
    relatedWork := []
    phases := ["project-execution"]
    tasks := ["build-test-audit"]
    checklists := ["state-independence"]
    reviews := [⟨300⟩, ⟨400⟩]
    findings := []
    validations := ["project-validation"]
    repositories := ["project-snapshot"]
    corrections := []
    workRecords := ["project-observations"]
  }
  store ← executeManagedCommand fixture
    (.planCompletion store.ledger.storedHead completionPlan)
    store "completion planning"
  stages := stages.push .planning
  store ← executeManagedCommand fixture
    (.completePhase store.ledger.storedHead work "project-execution")
    store "phase completion"
  stages := stages.push .phase
  store ← executeManagedCommand fixture
    (.completeTask store.ledger.storedHead work "build-test-audit")
    store "task completion"
  stages := stages.push .task
  store ← executeManagedCommand fixture
    (.completeChecklist store.ledger.storedHead work "state-independence")
    store "checklist completion"
  stages := stages.push .checklist
  store ← executeManagedCommand fixture
    (.linkWorkRecord store.ledger.storedHead work
      "project-observations" s!"observations:{artifact}")
    store "work-record linkage"
  stages := stages.push .workRecord
  store ← executeManagedCommand fixture
    (.passValidation store.ledger.storedHead work "project-validation" artifact)
    store "validation recording"
  stages := stages.push .validation
  store ← executeManagedCommand fixture
    (.classifyRepository store.ledger.storedHead work "project-snapshot" snapshot)
    store "repository classification"
  stages := stages.push .repository
  for (observation, index) in observations.zipIdx do
    let key := s!"project-observation-{index}"
    let obligation : Domain.Evidence.Obligation := {
      work
      key
      revision := store.ledger.storedHead
      commandProfile := s!"{repr observation.stage}"
      invocation := observation.invocation
      repository := fixture.name
      snapshot
      artifactDigest := artifact
      current := true
      kind := if observation.stage == .build then .build else .test
      requirements := ["managed-project-independence"]
      expectedProducer := s!"{fixture.language}:{fixture.buildSystem}:{fixture.validationTool}"
      expectedObservation := s!"project-observation:{index}"
      design := designId
      designRevision := design.revision
    }
    store ← executeManagedCommand fixture
      (.recordObligation store.ledger.storedHead obligation)
      store s!"typed obligation {index}"
    let evidence : Domain.Evidence.Evidence := {
      id := ⟨1000 + index⟩
      work
      obligation := obligation.key
      revision := obligation.revision
      commandProfile := obligation.commandProfile
      invocation := obligation.invocation
      exitCode := observation.exitCode
      repository := obligation.repository
      snapshot := obligation.snapshot
      artifactDigest := obligation.artifactDigest
      current := true
      kind := obligation.kind
      requirements := obligation.requirements
      producer := obligation.expectedProducer
      observedAt := obligation.expectedObservation
      design := obligation.design
      designRevision := obligation.designRevision
    }
    store ← executeManagedCommand fixture
      (.recordEvidence store.ledger.storedHead evidence)
      store s!"typed evidence {index}"
    unless (Application.Service.queryGate
        (.evidenceExact work key) store).value == .pass do
      fail s!"managed-project fixture {fixture.name} evidence gate rejected {key}"
  stages := stages.push .evidence
  let state ← match (Application.Service.status store).value.currentState? with
    | none => fail s!"managed-project fixture {fixture.name} lost its current workflow state"
    | some state => pure state
  unless state.evidence.length = observations.size &&
      state.obligations.length = observations.size do
    fail s!"managed-project fixture {fixture.name} did not persist every typed observation"
  unless (Application.Service.queryGate (.completion work) store).value == .pass do
    fail s!"managed-project fixture {fixture.name} did not reach completion readiness"
  let completed ← match Application.Service.complete store.ledger.storedHead work store with
    | .error error =>
        fail s!"managed-project fixture {fixture.name} completion was rejected: {repr error}"
    | .ok transaction => pure transaction.result
  let completedState ← match (Application.Service.status completed).value.currentState? with
    | none => fail s!"managed-project fixture {fixture.name} lost completed state"
    | some state => pure state
  unless completedState.work.any fun unit =>
      unit.id == work && unit.status == .closed do
    fail s!"managed-project fixture {fixture.name} did not complete its workflow"
  stages := stages.push .completion
  pure {
    stages
    evidenceCount := state.evidence.length
    stateDigest := (Kernel.Replay.stateDigest completedState).value
    closed := true
  }

def validateManagedWorkflowResult (expectedEvidence : Nat)
    (result : ManagedWorkflowResult) : Except String Unit := do
  unless result.stages = requiredManagedWorkflowStages do
    throw "managed-project workflow omitted, duplicated, or reordered a required lifecycle stage"
  unless result.evidenceCount = expectedEvidence do
    throw "managed-project workflow did not bind every project observation as exact evidence"
  unless result.closed && !result.stateDigest.isEmpty do
    throw "managed-project workflow did not reach a deterministic closed state"

def auditManagedProjectNegativeFixtures : IO Unit := do
  match validateManagedFixtureSet managedProjectFixtures with
  | .error error => fail error
  | .ok _ => pure ()
  let first ← match managedProjectFixtures[0]? with
    | some fixture => pure fixture
    | none => fail "first managed-project negative fixture is missing"
  let coupled := managedProjectFixtures.mapIdx fun index fixture =>
    if index = 1 then
      { fixture with
        language := first.language
        buildSystem := first.buildSystem
        validationTool := first.validationTool
        commands := first.commands
        behavior := first.behavior }
    else fixture
  if (validateManagedFixtureSet coupled).isOk then
    fail "coupled managed-project toolchain fixture was accepted"
  let fixture := first
  let files := fixture.publicFiles
  let privateState := System.FilePath.mk (".private-" ++ "state")
  let movedState := System.FilePath.mk "moved-private-state"
  let observations : Array ManagedProjectObservation := requiredManagedStages.map fun stage =>
    { stage, invocation := "fixture-command", exitCode := 0, stdout := "", stderr := "" }
  for stage in requiredManagedStages do
    let incomplete := observations.filter (·.stage != stage)
    if (validateManagedProjectResult fixture files files incomplete incomplete
        privateState movedState).isOk then
      fail s!"missing managed-project command stage fixture was accepted: {repr stage}"
  if (validateManagedProjectResult fixture files
      (files.push ("changed", "bytes")) observations observations
      privateState movedState).isOk then
    fail "altered managed-project bytes fixture was accepted"
  let changedObservations := observations.map fun observation =>
    if observation.stage == .test then
      { observation with stdout := "state-dependent" }
    else observation
  if (validateManagedProjectResult fixture files files observations
      changedObservations privateState movedState).isOk then
    fail "private-state-dependent managed-project behavior fixture was accepted"
  let leaked := observations.map fun observation =>
    if observation.stage == .behavior then
      { observation with stdout := fixture.privateIdentity }
    else observation
  if (validateManagedProjectResult fixture files files leaked leaked
      privateState movedState).isOk then
    fail "managed-project identity leak fixture was accepted"
  let completeWorkflow : ManagedWorkflowResult := {
    stages := requiredManagedWorkflowStages
    evidenceCount := observations.size
    stateDigest := "deterministic-state"
    closed := true
  }
  for stage in requiredManagedWorkflowStages do
    let incomplete := {
      completeWorkflow with
      stages := completeWorkflow.stages.filter (· != stage)
    }
    if (validateManagedWorkflowResult observations.size incomplete).isOk then
      fail s!"missing managed-project lifecycle stage fixture was accepted: {repr stage}"
  if (validateManagedWorkflowResult observations.size
      { completeWorkflow with evidenceCount := observations.size - 1 }).isOk then
    fail "incomplete managed-project evidence fixture was accepted"

def runManagedProjectFixture (root : System.FilePath)
    (fixture : ManagedProjectFixture) : IO Unit := do
  let project := root / fixture.name
  IO.FS.createDirAll project
  writeFixtureFiles project fixture.publicFiles
  let privateState := project / (".agent-" ++ "workbench")
  IO.FS.createDirAll privateState
  IO.FS.writeFile (privateState / "identity") fixture.privateIdentity
  let beforeFiles ← readFixtureFiles project fixture.publicFiles
  let before ← observeManagedProject project fixture
  let beforeWorkflow ← runManagedWorkflow fixture before
  let movedState := root / s!"{fixture.name}-private-state"
  IO.FS.rename privateState movedState
  let after ← observeManagedProject project fixture
  let afterWorkflow ← runManagedWorkflow fixture after
  let afterFiles ← readFixtureFiles project fixture.publicFiles
  match validateManagedProjectResult fixture beforeFiles afterFiles before after
      privateState movedState with
  | .error error => fail error
  | .ok _ => pure ()
  match validateManagedWorkflowResult before.size beforeWorkflow,
      validateManagedWorkflowResult after.size afterWorkflow with
  | .error error, _ | _, .error error => fail error
  | .ok _, .ok _ => pure ()
  unless beforeWorkflow = afterWorkflow do
    fail s!"managed-project fixture {fixture.name} complete workflow depends on private state presence"

def auditManagedProjectIndependence : IO Unit := do
  auditManagedProjectNegativeFixtures
  IO.FS.withTempDir fun root => do
    for fixture in managedProjectFixtures do
      runManagedProjectFixture root fixture

def traceModuleRules (designRules : Array ModuleRule) : Array ModuleRule := designRules ++ #[
  ⟨"AgentWorkbench", #["AgentWorkbench.Application.Service"]⟩,
  ⟨"AgentWorkbench.Cli.Program", #["AgentWorkbench.Application.Service"]⟩,
  ⟨"Main", #["AgentWorkbench.Cli.Program"]⟩,
  ⟨"AgentWorkbench.Tests.KernelLaws", #["AgentWorkbench.Cli.Program"]⟩,
  ⟨"AgentWorkbench.Tests.WorkflowLaws", #["AgentWorkbench.Application.Service"]⟩,
  ⟨"AgentWorkbench.Audit.Expected", #["AgentWorkbench.Application.Service"]⟩,
  ⟨"AgentWorkbench.Audit.Main", #["AgentWorkbench.Audit.Expected", "AgentWorkbench.Cli.Program"]⟩
]

def traceProjectFiles (designRules : Array ModuleRule) : Array String :=
  designRules.map (fun rule => (rule.module.replace "." "/") ++ ".lean") ++ #[
  "AgentWorkbench.lean",
  "AgentWorkbench/Adapter/Codec.lean",
  "AgentWorkbench/Adapter/DurableFilesystem.lean",
  "AgentWorkbench/Adapter/SQLite.lean",
  "AgentWorkbench/Adapter/Update.lean",
  "AgentWorkbench/Audit/Expected.lean",
  "AgentWorkbench/Audit/Main.lean",
  "AgentWorkbench/Cli/Program.lean",
  "AgentWorkbench/Tests/KernelLaws.lean",
  "AgentWorkbench/Tests/StorageLaws.lean",
  "AgentWorkbench/Tests/WorkflowLaws.lean",
  "bindings/durable_filesystem.c",
  "Main.lean",
  "lake-manifest.json",
  "lakefile.lean",
  "lean-toolchain",
  "proof-manifest.toml"
]

structure RebuildTraceCase where
  key : String
  module : String
  file : String
  privateProof : Bool := false

def rebuildTraceCases : Array RebuildTraceCase := #[
  ⟨"private-proof", "AgentWorkbench.Domain.Work", "AgentWorkbench/Domain/Work.lean", true⟩,
  ⟨"domain-work", "AgentWorkbench.Domain.Work", "AgentWorkbench/Domain/Work.lean", false⟩,
  ⟨"policy-completion", "AgentWorkbench.Policy.Completion", "AgentWorkbench/Policy/Completion.lean", false⟩,
  ⟨"domain-identity", "AgentWorkbench.Domain.Identity", "AgentWorkbench/Domain/Identity.lean", false⟩
]

def copyTraceProject (target : System.FilePath) (designRules : Array ModuleRule) : IO Unit := do
  for relative in traceProjectFiles designRules do
    let source := System.FilePath.mk relative
    let destination := target / relative
    if let some parent := destination.parent then IO.FS.createDirAll parent
    IO.FS.writeFile destination (← IO.FS.readFile source)

def runLakeBuild (lake project : System.FilePath) (targets : Array String := #[])
    (oldMode : Bool := false) : IO String := do
  let argsPrefix := if oldMode then #["--old", "build"] else #["build"]
  let output ← IO.Process.output {
    cmd := lake.toString, args := argsPrefix ++ targets, cwd := some project }
  unless output.exitCode = 0 do
    fail s!"representative Lake rebuild failed:\n{output.stdout}\n{output.stderr}"
  return output.stdout ++ "\n" ++ output.stderr

def builtModules (output : String) : List String :=
  ((output.splitOn "\n").filterMap fun line =>
    match line.splitOn "Built " with
    | _ :: built :: _ =>
        match built.trimAscii.toString.splitOn " " with
        | token :: _ =>
            -- Reverification bounds concern Lean environment checks. Native
            -- code generation/link artifacts (`:c.o`, `:dynlib`) are not a
            -- semantic recheck of the named module.
            if token.contains ':' then none
            else if token = "Main" || token = "AgentWorkbench" ||
                token.startsWith "AgentWorkbench." then some token else none
        | _ => none
    | _ => none).eraseDups

def mutateTraceSource (case : RebuildTraceCase) (source : String) : Except String String :=
  if case.privateProof then
    let before :=
      "theorem single_active_activation {activations : List Activation}\n    (valid : AtMostOneActive activations) :\n    (activeActivations activations).length ≤ 1 := by\n  exact valid"
    let after :=
      "theorem single_active_activation {activations : List Activation}\n    (valid : AtMostOneActive activations) :\n    (activeActivations activations).length ≤ 1 := by\n  assumption "
    let changed := source.replace before after
    if changed = source then .error "private-proof trace mutation anchor was not found" else .ok changed
  else
    .ok <| source ++ s!"\n\ndef agentWorkbenchTraceProbe_{case.key.replace "-" "_"} : Unit := ()\n"

def allowedTraceModules (normativeRules : Array ModuleRule)
    (case : RebuildTraceCase) : List String :=
  if case.privateProof then [case.module]
  else case.module :: dependentClosure normativeRules case.module

def requiredTraceModules (actualRules : Array ModuleRule)
    (case : RebuildTraceCase) : List String :=
  if case.privateProof then [case.module]
  else case.module :: dependentClosure actualRules case.module

def traceSatisfiesBounds (actualRules : Array ModuleRule)
    (normativeRules : Array ModuleRule) (case : RebuildTraceCase)
    (trace : List String) : Bool :=
  let allowed := allowedTraceModules normativeRules case
  let required := requiredTraceModules actualRules case
  required.all trace.contains && trace.all allowed.contains

def auditIncompleteTraceFixture (actualRules : Array ModuleRule)
    (normativeRules : Array ModuleRule) (case : RebuildTraceCase) : IO Unit := do
  if !case.privateProof then
    let required := requiredTraceModules actualRules case
    match required.reverse with
    | [] => fail s!"public trace {case.key} has no required modules"
    | missing :: _ =>
        let incomplete := required.filter (· != missing)
        if traceSatisfiesBounds actualRules normativeRules case incomplete then
          fail s!"incomplete public trace fixture was accepted for {case.key}; missing {missing}"

def auditRebuildTrace (lake project : System.FilePath) (actualRules : Array ModuleRule)
    (normativeRules : Array ModuleRule) (case : RebuildTraceCase) : IO Unit := do
  unless normativeRules.any (fun rule => rule.module = case.module) do
    fail s!"trace owner {case.module} is outside the normative module map"
  if case.privateProof && (dependentClosure normativeRules case.module).isEmpty then
    fail s!"private-proof trace owner {case.module} has no normative importers"
  if case.privateProof && !theoremRules.any (fun rule =>
      rule.declaration = "AgentWorkbench.Domain.Work.single_active_activation") then
    fail "private-proof trace theorem is not protected by the immutable theorem policy"
  let path := project / case.file
  let original ← IO.FS.readFile path
  let changed ← match mutateTraceSource case original with
    | .ok changed => pure changed
    | .error error => fail error
  IO.FS.writeFile path changed
  -- `--old` is sound only for this manifest-locked theorem proof body: its
  -- exported type is immutable and Lean proof irrelevance hides its value.
  let targets := if case.privateProof then #[] else
    #["AgentWorkbench", "agent-workbench", "kernel-laws", "workflow-laws",
      "verified-core-audit"]
  let trace := builtModules (← runLakeBuild lake project targets (oldMode := case.privateProof))
  if trace.isEmpty then fail s!"Lake emitted no rebuild trace for {case.key}"
  unless trace.contains case.module do
    fail s!"Lake did not rebuild changed module {case.module} for {case.key}"
  let allowed := allowedTraceModules normativeRules case
  for rebuilt in trace do
    unless allowed.contains rebuilt do
      fail s!"{case.key} rebuilt {rebuilt} outside its normative reverse closure"
  let required := requiredTraceModules actualRules case
  for dependent in required do
    unless trace.contains dependent do
      fail s!"{case.key} omitted required reverse-dependent module {dependent}"
  unless traceSatisfiesBounds actualRules normativeRules case trace do
    fail s!"{case.key} rebuild trace does not equal its required bounded closure"
  auditIncompleteTraceFixture actualRules normativeRules case
  IO.println s!"verified-core trace {case.key}: required={String.intercalate "," required}; rebuilt={String.intercalate "," trace}"
  IO.FS.writeFile path original
  discard <| runLakeBuild lake project

def auditRepresentativeRebuilds (designRules : Array ModuleRule) : IO Unit := do
  let lake := (← findSysroot) / "bin" / "lake"
  unless ← lake.pathExists do fail s!"Lake executable is absent: {lake}"
  let normativeRules := traceModuleRules designRules
  let mut actualRules : Array ModuleRule := #[]
  for expected in normativeRules do
    let imports := (← sourceImports expected.module).filter fun imported =>
      normativeRules.any (·.module = imported)
    for imported in imports do
      unless expected.imports.contains imported do
        fail s!"trace module {expected.module} imports {imported} outside its normative bound"
    actualRules := actualRules.push ⟨expected.module, imports⟩
  IO.FS.withTempDir fun project => do
    copyTraceProject project designRules
    discard <| runLakeBuild lake project
    for case in rebuildTraceCases do
      auditRebuildTrace lake project actualRules normativeRules case

def main : IO Unit := do
  let designRules := productModuleRules
  auditRepositoryInventory
  auditPublicProductSurfaces
  let manifest ← IO.FS.readFile "proof-manifest.toml"
  let policy ← match validateManifestContent designRules manifest with
    | .error error => fail error
    | .ok policy => pure policy
  auditSourceAuthorities policy
  auditNegativeFixtures designRules manifest
  auditArchitecture designRules
  initSearchPath (← findSysroot) [".lake/build/lib/lean"]
  let env ← importModules #[
    { module := `AgentWorkbench.Audit.Expected },
    { module := `AgentWorkbench.Cli.Program },
    { module := `AgentWorkbench },
    { module := `AgentWorkbench.Adapter.Codec },
    { module := `AgentWorkbench.Adapter.DurableFilesystem },
    { module := `AgentWorkbench.Adapter.SQLite },
    { module := `AgentWorkbench.Adapter.Update }] {}
  auditPublicDefinitions env
  auditTheorems env
  auditDeclarations env
  auditCliMutation
  auditResponseOutput
  auditManagedProjectIndependence
  auditRepresentativeRebuilds designRules
  IO.println "verified-core audit: pass"

end AgentWorkbench.Audit

def main : IO Unit :=
  AgentWorkbench.Audit.main
