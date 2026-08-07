import AgentWorkbench.Application.Common

namespace AgentWorkbenchProof

open AgentWorkbench

theorem validated_success
    (candidate result : ProjectState)
    (success : validated candidate = .ok result) :
    result = candidate ∧ AgentWorkbench.ValidProjectState result := by
  cases checked : validateState candidate with
  | error message => simp [validated, checked] at success
  | ok value =>
      cases value
      have equal : candidate = result := by simpa [validated, checked] using success
      subst result
      exact ⟨rfl, validProjectState_of_validation candidate (by simpa using checked)⟩

theorem validated_preserves
    (candidate result : ProjectState)
    (success : validated candidate = .ok result) :
    AgentWorkbench.ValidProjectState result :=
  (validated_success candidate result success).2

end AgentWorkbenchProof
