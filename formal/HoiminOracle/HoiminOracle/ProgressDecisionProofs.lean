import HoiminOracle.ProgressDecisionModel

namespace HoiminOracle.ProgressDecision

theorem fold_consecutive_stalls_eq_trailing
    (steps : List PairStep) (patience : Nat) :
    (foldPairStepsWithPatience steps patience).consecutiveStalls =
      trailingStalls steps := by
  rfl

theorem unusable_resets (steps : List PairStep) (patience : Nat) :
    (foldPairStepsWithPatience
      (steps ++ [.unusableAdjacency]) patience).consecutiveStalls = 0 := by
  simp [foldPairStepsWithPatience, trailingStalls, List.foldl_append,
    stallCountStep, isStalledStep]

theorem nonstalled_resets
    (steps : List PairStep) (observation : PairObservation) (patience : Nat)
    (notStalled : observation.state ≠ .stalled) :
    (foldPairStepsWithPatience
      (steps ++ [.compared observation]) patience).consecutiveStalls = 0 := by
  cases state : observation.state <;>
    simp_all [foldPairStepsWithPatience, trailingStalls, List.foldl_append,
      stallCountStep, isStalledStep] <;> decide

theorem regression_precedes_improvement
    (common regressions improvements : Nat) (hasRegression : 0 < regressions) :
    classifyPair .matching common regressions improvements =
      (if common = 0 then .indeterminate else .regressing) := by
  cases regressions with
  | zero => omega
  | succ regressions =>
      cases common <;> simp [classifyPair] <;> decide

theorem pair_never_saturated (pair : PairObservation) :
    pair.state = .improving ∨ pair.state = .regressing ∨
      pair.state = .stalled ∨ pair.state = .indeterminate := by
  cases pair.state <;> simp

theorem matching_uses_candidate_id :
    joinMode .matching = .candidateId := by
  rfl

theorem candidate_id_key_ignores_content
    (left right : Mutant) (sameId : left.candidateId = right.candidateId) :
    keyOf .candidateId left = keyOf .candidateId right := by
  simpa [keyOf] using sameId

theorem inconclusive_transition_has_no_directional_or_score_counts
    (before after : Status)
    (inconclusive : before = .inconclusive ∨ after = .inconclusive) :
    let counts := transitionCounts before after
    counts.improvements = 0 ∧ counts.regressions = 0 ∧
      counts.previousKilled + counts.previousSurvived = 0 ∧
      counts.currentKilled + counts.currentSurvived = 0 := by
  cases inconclusive with
  | inl beforeInconclusive =>
      subst before
      cases after <;> decide
  | inr afterInconclusive =>
      subst after
      cases before <;> decide

private theorem positive_fold_has_stalled_last
    (steps : List PairStep) (initial : Nat) (nonempty : steps ≠ [])
    (positive : 0 < steps.foldl stallCountStep initial) :
    ∃ observation,
      steps.getLast? = some (.compared observation) ∧ observation.state = .stalled := by
  induction steps generalizing initial with
  | nil => contradiction
  | cons first rest induction =>
      cases rest with
      | nil =>
          simp [stallCountStep] at positive
          cases first with
          | unusableAdjacency => simp [isStalledStep] at positive
          | compared observation =>
              cases state : observation.state with
              | improving =>
                  have unequal : (PairState.improving == .stalled) = false :=
                    by decide
                  simp [isStalledStep, state, unequal] at positive
              | regressing =>
                  have unequal : (PairState.regressing == .stalled) = false :=
                    by decide
                  simp [isStalledStep, state, unequal] at positive
              | stalled => exact ⟨observation, by simp, state⟩
              | indeterminate =>
                  have unequal : (PairState.indeterminate == .stalled) = false :=
                    by decide
                  simp [isStalledStep, state, unequal] at positive
      | cons second tail =>
          have found := induction (initial := stallCountStep initial first)
            (by simp) positive
          simpa using found

private theorem positive_trailing_has_stalled_last
    (steps : List PairStep) (positive : 0 < trailingStalls steps) :
    ∃ observation,
      steps.getLast? = some (.compared observation) ∧ observation.state = .stalled := by
  have nonempty : steps ≠ [] := by
    intro empty
    subst steps
    simp [trailingStalls] at positive
  exact positive_fold_has_stalled_last steps 0 nonempty positive

private theorem baseLatest_ne_saturated (step : Option PairStep) :
    baseLatest step ≠ .saturated := by
  cases step with
  | none => decide
  | some step =>
      cases step with
      | unusableAdjacency => simp [baseLatest]
      | compared observation =>
          cases state : observation.state <;> simp [baseLatest, state]

set_option maxHeartbeats 100000 in
theorem saturated_implies_latest_stalled
    (steps : List PairStep) (patience : Nat) (positive : 0 < patience) :
    (foldPairStepsWithPatience steps patience).latest = .saturated →
      ∃ observation,
        steps.getLast? = some (.compared observation) ∧
          observation.state = .stalled := by
  intro saturated
  have enough : patience ≤ trailingStalls steps := by
    by_cases enough : patience ≤ trailingStalls steps
    · exact enough
    · have impossible : baseLatest steps.getLast? = .saturated := by
        simpa [foldPairStepsWithPatience, latestWithPatience, positive, enough]
          using saturated
      exact False.elim (baseLatest_ne_saturated steps.getLast? impossible)
  exact positive_trailing_has_stalled_last steps
    (Nat.lt_of_lt_of_le positive enough)

set_option maxHeartbeats 100000 in
theorem saturated_implies_patience_le_trailing
    (steps : List PairStep) (patience : Nat) (positive : 0 < patience) :
    (foldPairStepsWithPatience steps patience).latest = .saturated →
      patience ≤ trailingStalls steps := by
  intro saturated
  by_cases enough : patience ≤ trailingStalls steps
  · exact enough
  · have impossible : baseLatest steps.getLast? = .saturated := by
      simpa [foldPairStepsWithPatience, latestWithPatience, positive, enough]
        using saturated
    exact False.elim (baseLatest_ne_saturated steps.getLast? impossible)

theorem patience_le_trailing_implies_saturated
    (steps : List PairStep) (patience : Nat) (positive : 0 < patience)
    (enough : patience ≤ trailingStalls steps) :
    (foldPairStepsWithPatience steps patience).latest = .saturated := by
  simp [foldPairStepsWithPatience, latestWithPatience, positive, enough]

end HoiminOracle.ProgressDecision
