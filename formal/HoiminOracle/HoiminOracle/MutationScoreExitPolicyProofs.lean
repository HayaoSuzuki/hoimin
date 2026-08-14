import HoiminOracle.MutationScoreExitPolicyModel

namespace HoiminOracle.MutationScoreExitPolicy

theorem record_commutes (counts : Counts) (left right : Status) :
    record (record counts left) right = record (record counts right) left := by
  cases left <;> cases right <;> simp [record] <;> omega

set_option maxHeartbeats 100000 in
theorem summarize_permutation_invariant
    {left right : List Status} (permutation : left.Perm right) :
    summarize left = summarize right := by
  apply permutation.foldl_eq'
  intro first _ second _ counts
  exact record_commutes counts first second

theorem record_increments_exactly_one (counts : Counts) (status : Status) :
    total (record counts status) = total counts + 1 := by
  cases status <;> simp [record, total] <;> omega

private theorem fold_count_status
    (statuses : List Status) (initial : Counts) (status : Status) :
    countStatus (statuses.foldl record initial) status =
      countStatus initial status + statuses.count status := by
  induction statuses generalizing initial with
  | nil => simp
  | cons head tail induction =>
      simp only [List.foldl_cons, List.count_cons]
      rw [induction]
      cases head <;> cases status <;> simp [record, countStatus] <;> omega

theorem summarize_counts_each_status (statuses : List Status) (status : Status) :
    countStatus (summarize statuses) status = statuses.count status := by
  cases status with
  | killed => simpa [summarize, countStatus] using fold_count_status statuses {} Status.killed
  | survived => simpa [summarize, countStatus] using fold_count_status statuses {} Status.survived
  | timeout => simpa [summarize, countStatus] using fold_count_status statuses {} Status.timeout
  | outOfMemory => simpa [summarize, countStatus] using fold_count_status statuses {} Status.outOfMemory
  | processLimit => simpa [summarize, countStatus] using fold_count_status statuses {} Status.processLimit
  | error => simpa [summarize, countStatus] using fold_count_status statuses {} Status.error
  | notRun => simpa [summarize, countStatus] using fold_count_status statuses {} Status.notRun

private theorem fold_total (statuses : List Status) (initial : Counts) :
    total (statuses.foldl record initial) = total initial + statuses.length := by
  induction statuses generalizing initial with
  | nil => simp
  | cons head tail induction =>
      simp only [List.foldl_cons, List.length_cons]
      rw [induction, record_increments_exactly_one]
      omega

theorem summarize_total (statuses : List Status) :
    total (summarize statuses) = statuses.length := by
  simpa [summarize, total] using fold_total statuses {}

theorem inconclusive_eq_five_status_sum (counts : Counts) :
    inconclusive counts = counts.timeout + counts.outOfMemory +
      counts.processLimit + counts.error + counts.notRun := by
  rfl

theorem exactScore_eq_none_iff (counts : Counts) :
    exactScore counts = none ↔ decidable counts = 0 := by
  simp [exactScore]

theorem reduceFraction_is_reduced (numerator denominator : Nat)
    (positive : 0 < denominator) :
    Nat.Coprime (reduceFraction numerator denominator).numerator
      (reduceFraction numerator denominator).denominator := by
  simpa [reduceFraction] using Nat.coprime_div_gcd_div_gcd
    (Nat.gcd_pos_of_pos_right numerator positive)

theorem exactScore_denominator_positive (counts : Counts) (score : ExactFraction)
    (present : exactScore counts = some score) :
    0 < score.denominator := by
  simp only [exactScore] at present
  split at present
  · contradiction
  · simp only [Option.some.injEq] at present
    subst score
    exact Nat.div_gcd_pos_of_pos_right counts.killed (by omega)

theorem exactScore_is_reduced (counts : Counts) (score : ExactFraction)
    (present : exactScore counts = some score) :
    Nat.Coprime score.numerator score.denominator := by
  simp only [exactScore] at present
  split at present
  · contradiction
  · simp only [Option.some.injEq] at present
    subst score
    apply reduceFraction_is_reduced
    omega

theorem inconclusive_record_preserves_score (counts : Counts) (status : Status)
    (isInconclusive : status = .timeout ∨ status = .outOfMemory ∨
      status = .processLimit ∨ status = .error ∨ status = .notRun) :
    exactScore (record counts status) = exactScore counts := by
  rcases isInconclusive with rfl | rfl | rfl | rfl | rfl <;>
    simp [record, exactScore, decidable]

theorem killed_update (counts : Counts) :
    exactScore (record counts .killed) =
      some (reduceFraction (counts.killed + 1) (decidable counts + 1)) := by
  simp [record, exactScore, decidable]
  congr 1 <;> omega

theorem survived_update (counts : Counts) :
    exactScore (record counts .survived) =
      some (reduceFraction counts.killed (decidable counts + 1)) := by
  simp [record, exactScore, decidable]
  congr 1 <;> omega

theorem interrupted_precedes_all (policy : ExitPolicy)
    (interrupted : policy.interrupted = true) : exitCode policy = 130 := by
  simp [exitCode, interrupted]

theorem infrastructure_precedes_lower (policy : ExitPolicy)
    (notInterrupted : policy.interrupted = false)
    (infrastructure : policy.infrastructureError = true) : exitCode policy = 2 := by
  simp [exitCode, notInterrupted, infrastructure]

theorem baseline_precedes_lower (policy : ExitPolicy)
    (notInterrupted : policy.interrupted = false)
    (noInfrastructure : policy.infrastructureError = false)
    (baseline : policy.baselineFailed = true) : exitCode policy = 3 := by
  simp [exitCode, notInterrupted, noInfrastructure, baseline]

theorem incomplete_precedes_survivors (policy : ExitPolicy)
    (notInterrupted : policy.interrupted = false)
    (noInfrastructure : policy.infrastructureError = false)
    (baselinePassed : policy.baselineFailed = false)
    (incompleteRun : policy.incomplete = true) : exitCode policy = 4 := by
  simp [exitCode, notInterrupted, noInfrastructure, baselinePassed, incompleteRun]

theorem survivor_only_is_complete :
    complete { survivors := true } = true ∧ exitCode { survivors := true } = 1 := by
  decide

theorem complete_iff_no_failure_flags (policy : ExitPolicy) :
    complete policy = true ↔ policy.infrastructureError = false ∧
      policy.baselineFailed = false ∧ policy.incomplete = false ∧
      policy.interrupted = false := by
  rcases policy with ⟨infrastructure, baseline, incompleteRun, survivors, interrupted⟩
  cases infrastructure <;> cases baseline <;> cases incompleteRun <;>
    cases survivors <;> cases interrupted <;> decide

theorem composed_complete_iff (counts : Counts) (flags : RunFlags) :
    complete (composePolicy counts flags) = true ↔
      flags.infrastructureError = false ∧ counts.error = 0 ∧
      flags.baselineFailed = false ∧ flags.incomplete = false ∧
      counts.timeout = 0 ∧ counts.outOfMemory = 0 ∧ counts.processLimit = 0 ∧
      counts.notRun = 0 ∧ flags.interrupted = false := by
  rw [complete_iff_no_failure_flags]
  simp [composePolicy, policyFromCounts, and_assoc, and_left_comm,
    and_comm]

theorem survivors_do_not_change_completeness (policy : ExitPolicy) :
    complete { policy with survivors := !policy.survivors } = complete policy := by
  rfl

theorem observation_order_invariant
    {left right : List Status} (permutation : left.Perm right) (flags : RunFlags) :
    observe left flags = observe right flags := by
  simp [observe, summarize_permutation_invariant permutation]

end HoiminOracle.MutationScoreExitPolicy
