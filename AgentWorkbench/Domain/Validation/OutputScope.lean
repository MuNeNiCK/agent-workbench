import Lean.Data.Json

namespace AgentWorkbench
namespace Validation

private def pathPrefix : List String → List String → Bool
  | [], _ => true
  | _, [] => false
  | left :: leftRest, right :: rightRest =>
      left == right && pathPrefix leftRest rightRest

private def hasWindowsDrivePrefix (path : String) : Bool :=
  match path.toList with
  | first :: ':' :: _ => first.isAlpha
  | _ => false

private def protectedRoots : List (List String) :=
  [[".agent-workbench"], [".agents"], [".git"]]

/-- Validate an output identity before it can become a managed mutation boundary.
The protected roots contain Workbench authority, installed guidance, and Git
authority; neither they nor an ancestor/descendant scope may be managed output. -/
def validateManagedOutputScope (identity : String) : Except String Unit := do
  let source ← if identity.startsWith "file:" then
      pure (identity.drop 5).toString
    else if identity.startsWith "tree:" then
      pure (identity.drop 5).toString
    else
      throw s!"managed output has unsupported identity: {identity}"
  let normalized := source.replace "\\" "/"
  let configured : System.FilePath := normalized
  if source.isEmpty || configured.isAbsolute || normalized.startsWith "/" ||
      hasWindowsDrivePrefix normalized || configured.components.any (· == "..") then
    throw s!"managed output must be a project-relative path: {identity}"
  let components := configured.components.filter fun component =>
    !component.isEmpty && component != "."
  if components.isEmpty then
    throw s!"managed output cannot be the project root: {identity}"
  if protectedRoots.any fun rootComponents =>
      pathPrefix components rootComponents || pathPrefix rootComponents components then
    throw s!"managed output overlaps a protected project root: {identity}"

end Validation
end AgentWorkbench
