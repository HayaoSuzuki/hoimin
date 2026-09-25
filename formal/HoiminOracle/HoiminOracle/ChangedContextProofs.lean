import HoiminOracle.ChangedContextModel

namespace HoiminOracle.ChangedContext

theorem eligible_bounds {hs : List Hunk} {length context line : Nat}
    {explicit : List Nat} {untracked : Bool}
    (h : eligible hs length context line explicit untracked) :
    0 < line ∧ line ≤ length := ⟨h.1, h.2.1⟩

theorem zero_context_deletion (start line : Nat) :
    ¬ covers ⟨start, 0⟩ 0 line := by simp [covers]

set_option maxHeartbeats 10000 in
theorem context_monotone (h : Hunk) (small large line : Nat)
    (order : small ≤ large) (accepted : covers h small line) :
    covers h large line := by
  unfold covers at *
  split at accepted <;> simp_all <;> omega

theorem explicit_subset {hs : List Hunk} {length context line : Nat}
    {explicit : List Nat} {untracked : Bool}
    (nonempty : explicit ≠ [])
    (h : eligible hs length context line explicit untracked) : line ∈ explicit := by
  exact h.2.2.2.resolve_left nonempty

end HoiminOracle.ChangedContext
