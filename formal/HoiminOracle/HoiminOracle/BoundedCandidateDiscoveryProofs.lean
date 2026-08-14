import HoiminOracle.BoundedCandidateDiscoveryModel

namespace HoiminOracle.BoundedCandidateDiscovery

set_option maxHeartbeats 100000 in
theorem bounded_candidates_eq_reference_take (items : List Candidate) (limit : Nat) :
    (bounded items limit).candidates = (reference items).take limit := by
  rfl

set_option maxHeartbeats 100000 in
theorem bounded_length_le_limit (items : List Candidate) (limit : Nat) :
    (bounded items limit).candidates.length ≤ limit := by
  simp only [bounded, boundReference, List.length_take]
  exact Nat.min_le_left _ _

set_option maxHeartbeats 100000 in
theorem bounded_truncated_iff (items : List Candidate) (limit : Nat) :
    (bounded items limit).truncated = true ↔ limit < (reference items).length := by
  simp [bounded, boundReference]

set_option maxHeartbeats 100000 in
theorem bounded_zero (items : List Candidate) :
    (bounded items 0).candidates = [] := by
  simp [bounded, boundReference]

set_option maxHeartbeats 100000 in
theorem producerWindow_length_le (items : List Candidate) (limit : Nat) :
    (producerWindow items limit).length ≤ limit + 1 := by
  simp only [producerWindow, List.length_take]
  exact Nat.min_le_left _ _

set_option maxHeartbeats 100000 in
theorem producerWindow_preserves_limit_prefix (items : List Candidate) (limit : Nat) :
    (producerWindow items limit).take limit = (reference items).take limit := by
  simp [producerWindow, List.take_take, Nat.min_eq_left]

set_option maxHeartbeats 100000 in
theorem merge_candidates_eq_window_reference_take
    (token ast annotation : List Candidate) (limit : Nat) :
    (mergeProducerWindows token ast annotation limit).candidates =
      (reference
        (producerWindow token limit ++ producerWindow ast limit ++
          producerWindow annotation limit)).take limit := by
  rfl

set_option maxHeartbeats 100000 in
theorem merge_candidates_length_le
    (token ast annotation : List Candidate) (limit : Nat) :
    (mergeProducerWindows token ast annotation limit).candidates.length ≤ limit := by
  rw [merge_candidates_eq_window_reference_take]
  exact List.length_take_le _ _

set_option maxHeartbeats 100000 in
theorem filter_take_eq_take_of_prefix_kept
    {α : Type} (items : List α) (keep : α → Bool) (limit : Nat)
    (prefixKept : ∀ item, item ∈ items.take limit → keep item = true) :
    (items.filter keep).take limit = items.take limit := by
  induction limit generalizing items with
  | zero => simp
  | succ limit induction =>
      cases items with
      | nil => simp
      | cons head tail =>
          have headKept : keep head = true := by
            apply prefixKept head
            simp
          simp only [List.filter_cons, headKept, if_true, List.take_succ_cons,
            List.cons.injEq, true_and]
          apply induction
          intro item member
          apply prefixKept item
          simp only [List.take_succ_cons, List.mem_cons]
          exact Or.inr member

set_option maxHeartbeats 100000 in
theorem merge_candidates_eq_unbounded_reference_prefix
    (token ast annotation : List Candidate) (limit : Nat)
    (windowProjection :
      reference
        (producerWindow token limit ++ producerWindow ast limit ++
          producerWindow annotation limit) =
      (reference (token ++ ast ++ annotation)).filter
        (retainedByIdentity
          (producerWindow token limit ++ producerWindow ast limit ++
            producerWindow annotation limit)))
    (topRankCoverage : ∀ candidate,
      candidate ∈ (reference (token ++ ast ++ annotation)).take (limit + 1) →
      retainedByIdentity
        (producerWindow token limit ++ producerWindow ast limit ++
          producerWindow annotation limit) candidate = true) :
    (mergeProducerWindows token ast annotation limit).candidates =
      (reference (token ++ ast ++ annotation)).take limit := by
  rw [merge_candidates_eq_window_reference_take, windowProjection]
  apply filter_take_eq_take_of_prefix_kept
  intro candidate member
  apply topRankCoverage candidate
  exact List.take_subset_take_left _ (Nat.le_succ _) member

set_option maxHeartbeats 100000 in
theorem merge_truncated_iff_window_or_producer_overflow
    (token ast annotation : List Candidate) (limit : Nat) :
    (mergeProducerWindows token ast annotation limit).truncated = true ↔
      limit < (reference
        (producerWindow token limit ++ producerWindow ast limit ++
          producerWindow annotation limit)).length ∨
      limit < (reference token).length ∨
      limit < (reference ast).length ∨
      limit < (reference annotation).length := by
  simp [mergeProducerWindows, bounded, boundReference, or_assoc]

set_option maxHeartbeats 100000 in
theorem merge_truncated_iff_unbounded_reference_overflows
    (token ast annotation : List Candidate) (limit : Nat)
    (windowProjection :
      reference
        (producerWindow token limit ++ producerWindow ast limit ++
          producerWindow annotation limit) =
      (reference (token ++ ast ++ annotation)).filter
        (retainedByIdentity
          (producerWindow token limit ++ producerWindow ast limit ++
            producerWindow annotation limit)))
    (topRankCoverage : ∀ candidate,
      candidate ∈ (reference (token ++ ast ++ annotation)).take (limit + 1) →
      retainedByIdentity
        (producerWindow token limit ++ producerWindow ast limit ++
          producerWindow annotation limit) candidate = true)
    (producerOverflowSound :
      (limit < (reference token).length ∨
       limit < (reference ast).length ∨
       limit < (reference annotation).length) →
      limit < (reference (token ++ ast ++ annotation)).length) :
    (mergeProducerWindows token ast annotation limit).truncated = true ↔
      limit < (reference (token ++ ast ++ annotation)).length := by
  rw [merge_truncated_iff_window_or_producer_overflow]
  constructor
  · intro observed
    rcases observed with windowOverflow | producerOverflow
    · rw [windowProjection] at windowOverflow
      have filteredLength := List.length_filter_le
        (l := reference (token ++ ast ++ annotation))
        (p := retainedByIdentity
          (producerWindow token limit ++ producerWindow ast limit ++
            producerWindow annotation limit))
      omega
    · exact producerOverflowSound producerOverflow
  · intro completeOverflow
    left
    rw [windowProjection]
    have prefixEq := filter_take_eq_take_of_prefix_kept
      (reference (token ++ ast ++ annotation))
      (retainedByIdentity
        (producerWindow token limit ++ producerWindow ast limit ++
          producerWindow annotation limit))
      (limit + 1) topRankCoverage
    have prefixLength :
        ((reference (token ++ ast ++ annotation)).take (limit + 1)).length =
          limit + 1 := by
      simp only [List.length_take]
      omega
    have filteredLength : limit + 1 ≤
        ((reference (token ++ ast ++ annotation)).filter
          (retainedByIdentity
            (producerWindow token limit ++ producerWindow ast limit ++
              producerWindow annotation limit))).length := by
      rw [← prefixLength, ← prefixEq]
      exact List.length_take_le' _ _
    omega

set_option maxHeartbeats 100000 in
theorem sequences_contiguous (count : Nat) :
    sequences count = (List.range count).map (fun index => index + 1) := by
  rfl

set_option maxHeartbeats 100000 in
private theorem discoverTargetsFrom_count_le_limit
    (initial : TargetState) (targets : List (List Candidate)) (limit : Nat)
    (initialBound : initial.candidates.length ≤ limit) :
    (discoverTargetsFrom initial targets limit).candidates.length ≤ limit := by
  induction targets generalizing initial with
  | nil => simpa [discoverTargetsFrom] using initialBound
  | cons head tail induction =>
      rw [discoverTargetsFrom]
      apply induction
      simp [targetStep]
      split
      · exact initialBound
      · exact List.length_take_le _ _

set_option maxHeartbeats 100000 in
private theorem discoverTargetsFrom_truncated
    (state : TargetState) (after : List (List Candidate)) (limit : Nat)
    (truncated : state.truncated = true) :
    discoverTargetsFrom state after limit = state := by
  induction after generalizing state with
  | nil => simp [discoverTargetsFrom]
  | cons head tail induction =>
      rw [discoverTargetsFrom]
      have unchanged : targetStep state head limit = state := by
        simp [targetStep, truncated]
      rw [unchanged]
      exact induction state truncated

set_option maxHeartbeats 100000 in
private theorem discoverTargetsFrom_matches_reference
    (initial : TargetState) (targets : List (List Candidate)) (limit : Nat)
    (notTruncated : initial.truncated = false)
    (initialBound : initial.candidates.length ≤ limit) :
    let complete := initial.candidates ++ targetReference targets
    (discoverTargetsFrom initial targets limit).candidates = complete.take limit ∧
    ((discoverTargetsFrom initial targets limit).truncated = true ↔
      limit < complete.length) := by
  induction targets generalizing initial with
  | nil =>
      simp [discoverTargetsFrom, targetReference, notTruncated,
        List.take_of_length_le initialBound, initialBound]
  | cons head tail induction =>
      rw [discoverTargetsFrom]
      simp only [targetReference, List.flatMap_cons]
      by_cases overflow : limit < (initial.candidates ++ reference head).length
      · have overflowSum : limit < initial.candidates.length + (reference head).length := by
          simpa only [List.length_append] using overflow
        have stepTruncated : (targetStep initial head limit).truncated = true := by
          simp [targetStep, notTruncated, boundReference, overflowSum]
        have prefixLong : limit ≤ (initial.candidates ++ reference head).length :=
          Nat.le_of_lt overflow
        rw [discoverTargetsFrom_truncated _ tail limit stepTruncated]
        simp [targetStep, notTruncated, boundReference, overflowSum]
        constructor
        · rw [← List.append_assoc, List.take_append_of_le_length prefixLong]
        · omega
      · have fits : (initial.candidates ++ reference head).length ≤ limit :=
          Nat.le_of_not_gt overflow
        have fitsSum : initial.candidates.length + (reference head).length ≤ limit := by
          simpa only [List.length_append] using fits
        have stepNotTruncated : (targetStep initial head limit).truncated = false := by
          simp [targetStep, notTruncated, boundReference, fitsSum]
        have stepBound : (targetStep initial head limit).candidates.length ≤ limit := by
          simp [targetStep, notTruncated, boundReference]
          exact Nat.min_le_left _ _
        have rest := induction (targetStep initial head limit) stepNotTruncated stepBound
        simpa [targetStep, notTruncated, boundReference, List.take_of_length_le fits,
          fitsSum, List.append_assoc, targetReference, List.length_flatMap] using rest

set_option maxHeartbeats 100000 in
theorem target_count_le_limit (targets : List (List Candidate)) (limit : Nat) :
    (discoverTargets targets limit).candidates.length ≤ limit := by
  apply discoverTargetsFrom_count_le_limit
  simp

set_option maxHeartbeats 100000 in
theorem target_candidates_eq_global_reference_prefix
    (targets : List (List Candidate)) (limit : Nat) :
    (discoverTargets targets limit).candidates =
      (targetReference targets).take limit := by
  have result := discoverTargetsFrom_matches_reference (initial := {}) targets limit
    (by simp) (by simp)
  simpa [discoverTargets] using result.1

set_option maxHeartbeats 100000 in
theorem target_truncated_iff_global_reference_overflows
    (targets : List (List Candidate)) (limit : Nat) :
    (discoverTargets targets limit).truncated = true ↔
      limit < (targetReference targets).length := by
  have result := discoverTargetsFrom_matches_reference (initial := {}) targets limit
    (by simp) (by simp)
  simpa [discoverTargets] using result.2

set_option maxHeartbeats 100000 in
theorem truncation_stops_later_targets
    (state : TargetState) (after : List (List Candidate)) (limit : Nat)
    (truncated : state.truncated = true) :
    discoverTargetsFrom state after limit = state := by
  exact discoverTargetsFrom_truncated state after limit truncated

end HoiminOracle.BoundedCandidateDiscovery
