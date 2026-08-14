import HoiminOracle.CompoundPatternGuardModel

namespace HoiminOracle.CompoundPatternGuard

open HoiminOracle.BindingFlow

theorem sequence_retains_prefix_failure
    (first : Attempt) (next : Env → Attempt) (failure : Env)
    (present : failure ∈ first.failed) :
    failure ∈ (thenAttempt first next).failed := by
  simp only [thenAttempt, List.mem_append]
  exact Or.inl present

theorem as_failure_precedes_alias
    (child : Attempt) (name : Name) (failure : Env)
    (present : failure ∈ child.failed) :
    failure ∈ (asPattern child name).failed := by
  exact sequence_retains_prefix_failure child (capture name) failure present

theorem sequence_patterns_retain_failure
    (children : List (Env → Attempt)) (first : Attempt) (failure : Env)
    (present : failure ∈ first.failed) :
    failure ∈ (sequencePatterns first children).failed := by
  induction children generalizing first with
  | nil => exact present
  | cons child rest inductionHypothesis =>
      apply inductionHypothesis (thenAttempt first child)
      exact sequence_retains_prefix_failure first child failure present

theorem mapping_rest_requires_child_success
    (structural : Attempt) (children : List (Env → Attempt))
    (rest : Option Name) (failure : Env)
    (present : failure ∈ (sequencePatterns structural children).failed) :
    failure ∈ (mappingPattern structural children rest).failed := by
  cases rest with
  | none => exact present
  | some name =>
      exact sequence_retains_prefix_failure
        (sequencePatterns structural children) (capture name) failure present

theorem class_capture_requires_prefix_success
    (structural : Attempt) (children : List (Env → Attempt))
    (failure : Env) (present : failure ∈ structural.failed) :
    failure ∈ (classPattern structural children).failed := by
  exact sequence_patterns_retain_failure children structural failure present

theorem or_success_uses_every_reachable_arm
    (arms : List Attempt) (arm : Attempt) (success : Env)
    (armPresent : arm ∈ arms) (successPresent : success ∈ arm.matched) :
    success ∈ (orPattern arms).matched := by
  simp only [orPattern, List.mem_flatMap]
  exact ⟨arm, armPresent, successPresent⟩

theorem or_failure_uses_every_reachable_arm
    (arms : List Attempt) (arm : Attempt) (failure : Env)
    (armPresent : arm ∈ arms) (failurePresent : failure ∈ arm.failed) :
    failure ∈ (orPattern arms).failed := by
  simp only [orPattern, List.mem_flatMap]
  exact ⟨arm, armPresent, failurePresent⟩

theorem fact_meet_known_inputs
    (left right : Fact) (target : Target)
    (retained : left.meet right = .known target) :
    left = .known target ∧ right = .known target := by
  simp only [Fact.meet] at retained
  split at retained
  · rename_i equal
    exact ⟨retained, equal ▸ retained⟩
  · contradiction

theorem or_retained_fact_occurs_in_each_success
    (left right : Env) (name : Name) (target : Target)
    (retained : (left.meet right).get name = .known target) :
    left.get name = .known target ∧ right.get name = .known target := by
  cases name <;> exact fact_meet_known_inputs _ _ _ retained

theorem false_guard_uses_post_guard_environment
    (attempt : Attempt) (matched afterGuard : Env)
    (matchedSummary : attempt.summary.matched = some matched)
    (evaluate : Env → Env × Bool)
    (evaluated : evaluate matched = (afterGuard, false)) :
    (applyGuard attempt (some evaluate)).nextCase =
      meetOption attempt.summary.failed (some afterGuard) := by
  simp [applyGuard, matchedSummary, evaluated]

theorem capture_preserves_other_name
    (environment : Env) (captured observed : Name)
    (different : captured ≠ observed) :
    ((capture captured environment).matched.head?.getD emptyEnv).get observed =
      environment.get observed := by
  cases captured <;> cases observed <;> simp_all [capture, Env.set, Env.get]

theorem unreachable_outcome_does_not_join :
    (Attempt.summary {}).matched = none ∧
      (Attempt.summary {}).failed = none := by
  constructor <;> rfl

end HoiminOracle.CompoundPatternGuard
