import HoiminOracle.TimeoutLimitModel

namespace HoiminOracle.TimeoutLimit

theorem accepted_analyzer_is_bounded (input : Input) (accepted : accepts input = true) :
    0 < input.analyzer ∧ input.analyzer ≤ input.maximum := by
  simp [accepts, durationValid] at accepted
  omega

theorem accepted_baseline_is_bounded (input : Input) (accepted : accepts input = true) :
    0 < input.baseline ∧ input.baseline ≤ input.maximum := by
  simp [accepts, durationValid] at accepted
  omega

theorem accepted_total_is_bounded (input : Input) (accepted : accepts input = true) :
    0 < input.total ∧ input.total ≤ input.maximum := by
  simp [accepts, durationValid] at accepted
  omega

theorem accepted_effective_mutant_is_bounded
    (input : Input) (accepted : accepts input = true) :
    0 < effectiveMutantTimeout input ∧
      effectiveMutantTimeout input ≤ input.maximum := by
  cases mode : input.mutant <;>
    simp [accepts, durationValid, effectiveMutantTimeout, mode] at accepted ⊢ <;>
    omega

theorem accepted_bounds_every_effective_deadline
    (input : Input) (accepted : accepts input = true) :
    (0 < input.analyzer ∧ input.analyzer ≤ input.maximum) ∧
    (0 < input.baseline ∧ input.baseline ≤ input.maximum) ∧
    (0 < effectiveMutantTimeout input ∧
      effectiveMutantTimeout input ≤ input.maximum) ∧
    (0 < input.total ∧ input.total ≤ input.maximum) := by
  exact ⟨accepted_analyzer_is_bounded input accepted,
    accepted_baseline_is_bounded input accepted,
    accepted_effective_mutant_is_bounded input accepted,
    accepted_total_is_bounded input accepted⟩

end HoiminOracle.TimeoutLimit
