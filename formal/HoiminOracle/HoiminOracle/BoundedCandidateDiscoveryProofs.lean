import HoiminOracle.BoundedCandidateDiscoveryModel

namespace HoiminOracle.BoundedCandidateDiscovery

set_option maxHeartbeats 100000 in
theorem bounded_candidates_eq_reference_take (items : List Candidate) (limit : Nat) :
    (bounded items limit).candidates = (reference items).take limit := by
  rfl

set_option maxHeartbeats 100000 in
theorem bounded_length_le_limit (items : List Candidate) (limit : Nat) :
    (bounded items limit).candidates.length ≤ limit := by
  simp only [bounded, List.length_take]
  exact Nat.min_le_left _ _

set_option maxHeartbeats 100000 in
theorem bounded_truncated_iff (items : List Candidate) (limit : Nat) :
    (bounded items limit).truncated = true ↔ limit < (reference items).length := by
  simp [bounded]

set_option maxHeartbeats 100000 in
theorem bounded_zero (items : List Candidate) :
    (bounded items 0).candidates = [] := by
  simp [bounded]

set_option maxHeartbeats 100000 in
theorem sequences_contiguous (count : Nat) :
    sequences count = (List.range count).map (fun index => index + 1) := by
  rfl

set_option maxHeartbeats 100000 in
private theorem discoverTargetsFrom_count_le_limit
    (initial : TargetState) (targets : List (List Candidate)) (limit : Nat)
    (initialBound : initial.candidates.length ≤ limit) :
    (discoverTargetsFrom initial targets limit).candidates.length ≤ limit := by
  unfold discoverTargetsFrom
  induction targets generalizing initial with
  | nil => simpa using initialBound
  | cons head tail induction =>
      simp only [List.foldl_cons]
      apply induction
      simp [targetStep]
      split
      · exact initialBound
      · exact List.length_take_le _ _

set_option maxHeartbeats 100000 in
theorem target_count_le_limit (targets : List (List Candidate)) (limit : Nat) :
    (discoverTargets targets limit).candidates.length ≤ limit := by
  apply discoverTargetsFrom_count_le_limit
  simp

set_option maxHeartbeats 100000 in
theorem truncation_stops_later_targets
    (state : TargetState) (after : List (List Candidate)) (limit : Nat)
    (truncated : state.truncated = true) :
    discoverTargetsFrom state after limit = state := by
  unfold discoverTargetsFrom
  induction after generalizing state with
  | nil => simp
  | cons head tail induction =>
      simp only [List.foldl_cons]
      have unchanged : targetStep state head limit = state := by
        simp [targetStep, truncated]
      rw [unchanged]
      exact induction state truncated

end HoiminOracle.BoundedCandidateDiscovery
