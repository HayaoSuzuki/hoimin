import HoiminOracle.CandidateRankingModel

namespace HoiminOracle.CandidateRanking

set_option maxHeartbeats 100000 in
theorem rankOne_score_is_reason_sum (candidate : Candidate) :
    (rankOne candidate).score = scoreReasons (reasons candidate) := by
  rfl

set_option maxHeartbeats 100000 in
theorem rankOne_reasons_are_constructed (candidate : Candidate) :
    (rankOne candidate).rankingReasons = reasons candidate := by
  rfl

set_option maxHeartbeats 100000 in
theorem strictSelect_is_saved_prefix (ranked : List RankedCandidate) (limit : Nat) :
    strictSelect ranked limit =
      (ranked.take limit).map fun candidate => candidate.candidate.id := by
  rfl

set_option maxHeartbeats 100000 in
theorem strictSelect_length (ranked : List RankedCandidate) (limit : Nat) :
    (strictSelect ranked limit).length = min limit ranked.length := by
  simp [strictSelect]

set_option maxHeartbeats 100000 in
theorem strictSelect_member_of_saved
    (ranked : List RankedCandidate) (limit : Nat) (id : String)
    (selected : id ∈ strictSelect ranked limit) :
    ∃ candidate ∈ ranked, candidate.candidate.id = id := by
  simp only [strictSelect, List.mem_map] at selected
  obtain ⟨candidate, candidateInPrefix, candidateId⟩ := selected
  exact ⟨candidate, List.mem_of_mem_take candidateInPrefix, candidateId⟩

set_option maxHeartbeats 100000 in
theorem firstForPath?_member
    (path : Path) (candidates : List RankedCandidate) (candidate : RankedCandidate)
    (found : firstForPath? path candidates = some candidate) :
    candidate ∈ candidates := by
  exact List.mem_of_find?_eq_some found

set_option maxHeartbeats 100000 in
theorem selectRound_subset (paths : List Path) (candidates : List RankedCandidate) :
    selectRound paths candidates ⊆ candidates := by
  intro candidate selected
  simp only [selectRound, List.mem_filterMap] at selected
  obtain ⟨path, _, found⟩ := selected
  exact firstForPath?_member path candidates candidate found

set_option maxHeartbeats 100000 in
theorem eraseSelected_subset
    (selected candidates : List RankedCandidate) :
    eraseSelected selected candidates ⊆ candidates := by
  intro candidate retained
  exact (List.mem_filter.mp retained).1

set_option maxHeartbeats 100000 in
theorem roundRobinWithFuel_subset
    (fuel : Nat) (paths : List Path) (candidates : List RankedCandidate) :
    roundRobinWithFuel fuel paths candidates ⊆ candidates := by
  induction fuel generalizing candidates with
  | zero => simp [roundRobinWithFuel]
  | succ fuel induction =>
      intro candidate emitted
      simp only [roundRobinWithFuel] at emitted
      split at emitted
      · simp at emitted
      · simp only [List.mem_append] at emitted
        cases emitted with
        | inl selected =>
            exact selectRound_subset paths candidates selected
        | inr emittedLater =>
            exact eraseSelected_subset (selectRound paths candidates) candidates
              (induction (eraseSelected (selectRound paths candidates) candidates) emittedLater)

set_option maxHeartbeats 100000 in
theorem roundRobinTier_subset (candidates : List RankedCandidate) :
    roundRobinTier candidates ⊆ candidates := by
  exact roundRobinWithFuel_subset candidates.length (pathOrder candidates) candidates

set_option maxHeartbeats 100000 in
theorem diverseOrderWithFuel_subset
    (fuel : Nat) (candidates : List RankedCandidate) :
    diverseOrderWithFuel fuel candidates ⊆ candidates := by
  induction fuel generalizing candidates with
  | zero => simp [diverseOrderWithFuel]
  | succ fuel induction =>
      cases candidates with
      | nil => simp [diverseOrderWithFuel]
      | cons head tail =>
          intro candidate emitted
          simp only [diverseOrderWithFuel, List.mem_append] at emitted
          cases emitted with
          | inl emittedFromTier =>
              have inTier := roundRobinTier_subset
                (head :: tail.takeWhile fun item => item.score == head.score)
                emittedFromTier
              simp only [List.mem_cons] at inTier ⊢
              cases inTier with
              | inl isHead => exact Or.inl isHead
              | inr inTierTail =>
                  exact Or.inr (List.takeWhile_subset _ inTierTail)
          | inr emittedLater =>
              have inRest := induction
                (tail.dropWhile fun item => item.score == head.score) emittedLater
              exact List.mem_cons_of_mem head (List.dropWhile_subset _ inRest)

set_option maxHeartbeats 100000 in
theorem diverseOrder_subset (ranked : List RankedCandidate) :
    diverseOrder ranked ⊆ ranked := by
  exact diverseOrderWithFuel_subset ranked.length ranked

set_option maxHeartbeats 100000 in
theorem diverseSelect_member_of_saved
    (ranked : List RankedCandidate) (limit : Nat) (id : String)
    (selected : id ∈ diverseSelect ranked limit) :
    ∃ candidate ∈ ranked, candidate.candidate.id = id := by
  simp only [diverseSelect, List.mem_map] at selected
  obtain ⟨candidate, candidateInPrefix, candidateId⟩ := selected
  exact ⟨candidate,
    diverseOrder_subset ranked (List.mem_of_mem_take candidateInPrefix), candidateId⟩

set_option maxHeartbeats 100000 in
theorem validation_recomputes_complete_ranking (saved : List RankedCandidate) :
    validateRanking saved =
      (rankCandidates (saved.map RankedCandidate.candidate) == saved) := by
  rfl

end HoiminOracle.CandidateRanking
