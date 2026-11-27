import HoiminOracle.BoundedCandidateDiscoveryProofs

namespace HoiminOracle.PlanDiscoveryBudget

open HoiminOracle.BoundedCandidateDiscovery

structure File where
  path : String
  rows : List Candidate

def discover (files : List File) (limit : Nat) : TargetState :=
  discoverTargets (files.map File.rows) limit

def restrict (files : List File) (keep : String → Bool) : List File :=
  files.filter fun file => keep file.path

theorem filter_of_bounded_prefix_is_prefix {α : Type} (rows : List α)
    (keep : α → Bool) (limit : Nat) :
    ((rows.take limit).filter keep).IsPrefix ((rows.filter keep).take limit) := by
  apply List.prefix_take_iff.mpr
  constructor
  · refine ⟨(rows.drop limit).filter keep, ?_⟩
    rw [← List.filter_append, List.take_append_drop]
  · exact Nat.le_trans (List.length_filter_le _ _) (List.length_take_le _ _)

private theorem restrict_reference (files : List File) (keep : String → Bool)
    (owned : ∀ file ∈ files, ∀ row ∈ reference file.rows, row.path = file.path) :
    targetReference ((restrict files keep).map File.rows) =
      (targetReference (files.map File.rows)).filter fun row => keep row.path := by
  induction files with
  | nil => rfl
  | cons file rest ih =>
    have inFile : ∀ row ∈ reference file.rows, row.path = file.path :=
      owned file (by simp)
    have tail := ih (fun f fromRest => owned f (by simp [fromRest]))
    by_cases retained : keep file.path = true
    · have full : (reference file.rows).filter (fun row => keep row.path) = reference file.rows :=
        List.filter_eq_self.mpr fun row member => by rw [inFile row member, retained]
      simp only [restrict, List.filter_cons, retained, if_true, List.map_cons, targetReference,
        List.flatMap_cons, List.filter_append, full]
      exact congrArg (reference file.rows ++ ·) tail
    · have empty : (reference file.rows).filter (fun row => keep row.path) = [] :=
        List.filter_eq_nil_iff.mpr fun row member => by rw [inFile row member]; simpa using retained
      simpa [restrict, retained, targetReference, List.filter_append, empty] using tail

theorem restricted_discovery_preserves_original_kept_prefix (files : List File)
    (keep : String → Bool) (limit : Nat)
    (owned : ∀ file ∈ files, ∀ row ∈ reference file.rows, row.path = file.path) :
    ((discover files limit).candidates.filter fun candidate => keep candidate.path).IsPrefix
      (discover (restrict files keep) limit).candidates := by
  simp only [discover, target_candidates_eq_global_reference_prefix]
  rw [restrict_reference files keep owned]
  exact filter_of_bounded_prefix_is_prefix _ _ _

theorem saved_candidate_is_in_restricted_discovery (files : List File)
    (keep : String → Bool) (limit : Nat)
    (owned : ∀ file ∈ files, ∀ row ∈ reference file.rows, row.path = file.path)
    (candidate : Candidate) (saved : candidate ∈ (discover files limit).candidates)
    (kept : keep candidate.path = true) :
    candidate ∈ (discover (restrict files keep) limit).candidates :=
  (restricted_discovery_preserves_original_kept_prefix files keep limit owned).sublist.subset
    (List.mem_filter.mpr ⟨saved, kept⟩)

theorem removing_files_cannot_introduce_truncation (files : List File) (keep : String → Bool)
    (limit : Nat) (owned : ∀ file ∈ files, ∀ row ∈ reference file.rows, row.path = file.path)
    (overflow : (discover (restrict files keep) limit).truncated = true) :
    (discover files limit).truncated = true := by
  simp only [discover, target_truncated_iff_global_reference_overflows] at overflow ⊢
  rw [restrict_reference files keep owned] at overflow
  exact Nat.lt_of_lt_of_le overflow (List.length_filter_le _ _)

theorem restricted_discovery_preserves_untruncated_plan (files : List File)
    (keep : String → Bool) (limit : Nat)
    (owned : ∀ file ∈ files, ∀ row ∈ reference file.rows, row.path = file.path)
    (complete : (discover files limit).truncated = false) :
    (discover (restrict files keep) limit).candidates =
      (discover files limit).candidates.filter fun candidate => keep candidate.path := by
  have fits : (targetReference (files.map File.rows)).length ≤ limit := by
    apply Nat.le_of_not_gt
    intro over
    have truncated := (target_truncated_iff_global_reference_overflows _ _).mpr over
    simp [discover, truncated] at complete
  have filteredFits : ((targetReference (files.map File.rows)).filter
      (fun row => keep row.path)).length ≤ limit :=
    Nat.le_trans (List.length_filter_le _ _) fits
  simp only [discover, target_candidates_eq_global_reference_prefix]
  rw [restrict_reference files keep owned, List.take_of_length_le filteredFits, List.take_of_length_le fits]

end HoiminOracle.PlanDiscoveryBudget
