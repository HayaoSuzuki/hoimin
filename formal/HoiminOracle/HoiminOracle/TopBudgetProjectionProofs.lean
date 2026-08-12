import HoiminOracle.TopBudgetProjectionModel

namespace HoiminOracle.TopBudgetProjection

theorem waves_zero (jobs : Nat) : waves 0 jobs = 0 := by
  simp [waves]

set_option maxHeartbeats 100000 in
theorem waves_cover (selected jobs : Nat) (positive : 0 < jobs) :
    selected ≤ waves selected jobs * jobs := by
  have division := Nat.div_add_mod selected jobs
  have remainderLt := Nat.mod_lt selected positive
  by_cases divides : selected % jobs = 0
  · calc
      selected = jobs * (selected / jobs) + selected % jobs := division.symm
      _ = (selected / jobs) * jobs := by simp [divides, Nat.mul_comm]
      _ ≤ waves selected jobs * jobs := by simp [waves, divides]
  · calc
      selected = jobs * (selected / jobs) + selected % jobs := division.symm
      _ ≤ jobs * (selected / jobs) + jobs :=
        Nat.add_le_add_left (Nat.le_of_lt remainderLt) _
      _ = (selected / jobs + 1) * jobs := by
        simp [Nat.mul_add, Nat.mul_comm]
      _ = waves selected jobs * jobs := by simp [waves, divides]

set_option maxHeartbeats 100000 in
theorem one_fewer_wave_does_not_cover
    (selected jobs : Nat) (positiveJobs : 0 < jobs)
    (positiveSelected : 0 < selected) :
    (waves selected jobs - 1) * jobs < selected := by
  have division := Nat.div_add_mod selected jobs
  have remainderLt := Nat.mod_lt selected positiveJobs
  by_cases divides : selected % jobs = 0
  · have quotientPositive : 0 < selected / jobs := by
      apply Nat.pos_of_ne_zero
      intro quotientZero
      simp [divides, quotientZero] at division
      omega
    have quotientOne : 1 ≤ selected / jobs := quotientPositive
    calc
      (waves selected jobs - 1) * jobs = (selected / jobs - 1) * jobs := by
        simp [waves, divides]
      _ < ((selected / jobs - 1) + 1) * jobs := by
        simp [Nat.add_mul, positiveJobs]
      _ = (selected / jobs) * jobs := by rw [Nat.sub_add_cancel quotientOne]
      _ = selected := by simpa [divides, Nat.mul_comm] using division
  · have remainderPositive : 0 < selected % jobs := Nat.pos_of_ne_zero divides
    calc
      (waves selected jobs - 1) * jobs = (selected / jobs) * jobs := by
        simp [waves, divides]
      _ < (selected / jobs) * jobs + selected % jobs :=
        Nat.lt_add_of_pos_right remainderPositive
      _ = selected := by simpa [Nat.mul_comm] using division

set_option maxHeartbeats 100000 in
theorem waves_eq_zero_iff (selected jobs : Nat) (positive : 0 < jobs) :
    waves selected jobs = 0 ↔ selected = 0 := by
  constructor
  · intro noWaves
    have covered := waves_cover selected jobs positive
    simp [noWaves] at covered
    exact covered
  · intro empty
    subst selected
    exact waves_zero jobs

theorem projected_capacity_eq_capped_product (input : Input) :
    (project input).projectedCapacity =
      min input.durationMax
        ((effectiveTimeout input) * waves input.selected input.jobs) := by
  rfl

theorem equality_is_not_shortfall (input : Input)
    (equal : (project input).projectedCapacity = input.remaining) :
    (project input).shortfall = false := by
  unfold project at equal ⊢
  dsimp at equal ⊢
  rw [decide_eq_false_iff_not]
  omega

theorem greater_capacity_is_shortfall (input : Input)
    (greater : input.remaining < (project input).projectedCapacity) :
    (project input).shortfall = true := by
  unfold project at greater ⊢
  dsimp at greater ⊢
  rw [decide_eq_true_iff]
  exact greater

theorem fixed_timeout_is_preserved (input : Input) (ticks : Nat)
    (fixed : input.timeoutMode = .fixed ticks) :
    effectiveTimeout input = ticks := by
  simp [effectiveTimeout, fixed]

theorem auto_timeout_uses_saturated_rule (input : Input)
    (automatic : input.timeoutMode = .auto) :
    effectiveTimeout input =
      max (min input.durationMax (5 * input.ticksPerSecond))
        (min input.durationMax
          (min input.durationMax (input.baseline * 2) + input.ticksPerSecond)) := by
  simp [effectiveTimeout, automatic, autoTimeout, saturatingAdd, saturatingMul]

end HoiminOracle.TopBudgetProjection
