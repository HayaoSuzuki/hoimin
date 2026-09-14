import HoiminOracle.PerformanceCostModel

namespace HoiminOracle.PerformanceCost

theorem halfSteps_mono (n m : Nat) (h : n ≤ m) : halfSteps n ≤ halfSteps m := by
  induction m using Nat.strongRecOn generalizing n with
  | ind m ih =>
    by_cases hn : n = 0
    · subst n
      rw [halfSteps]
      simp only [ite_true, Nat.zero_le]
    · have hm : m ≠ 0 := by omega
      rw [halfSteps.eq_1 n, if_neg hn, halfSteps.eq_1 m, if_neg hm]
      exact Nat.add_le_add_left (ih (m / 2) (by omega) (n / 2) (Nat.div_le_div_right h)) 1

theorem binaryCost_le_halfSteps (n : Nat) (directions : List Bool) :
    binaryCost n directions ≤ halfSteps n := by
  induction n using Nat.strongRecOn generalizing directions with
  | ind n ih =>
    by_cases hn : n = 0
    · subst n
      rw [binaryCost, halfSteps]
      simp only [ite_true, Nat.le_refl]
    · rw [binaryCost, if_neg hn, halfSteps, if_neg hn]
      split
      · exact Nat.add_le_add_left (ih (n / 2) (by omega) _) 1
      · exact Nat.add_le_add_left
          (Nat.le_trans (ih ((n - 1) / 2) (by omega) _)
            (halfSteps_mono _ _ (by omega))) 1

theorem halfSteps_eq_log (n : Nat) (h : n ≠ 0) : halfSteps n = n.log2 + 1 := by
  induction n using Nat.strongRecOn with
  | ind n ih =>
    by_cases htwo : 2 ≤ n
    · rw [halfSteps, if_neg h, Nat.log2_def, if_pos htwo,
        ih (n / 2) (by omega) (by omega)]
      omega
    · have hn : n = 1 := by omega
      subst n
      native_decide

theorem buildTrace_eq (bindings height : Nat) :
    buildTrace bindings height = bindings * (height + 1) := by
  induction bindings with
  | zero => simp [buildTrace]
  | succ n ih => simp [buildTrace, ih, Nat.add_mul, Nat.add_comm]

theorem tree_update_cost (bindings : Nat) :
    buildTrace bindings (treeHeight bindings) = buildUpdates bindings :=
  buildTrace_eq _ _

theorem unselected_builds_nothing (lengths : List Nat) :
    filteredReplacementBytes false lengths = 0 := rfl

theorem full_clone_quadratic (n : Nat) : cloneEntries n n = n * n := rfl

-- Retention is inherited, separately from all allocation/cost observations.
theorem retained_bound (items : List HoiminOracle.BoundedCandidateDiscovery.Candidate)
    (limit : Nat) :
    (HoiminOracle.BoundedCandidateDiscovery.bounded items limit).candidates.length ≤ limit :=
  HoiminOracle.BoundedCandidateDiscovery.bounded_length_le_limit items limit

-- Boundary witness: the initial deliberately omitted ancestors gave 8 and failed.
example : buildUpdates 8 = 32 := by native_decide

end HoiminOracle.PerformanceCost
