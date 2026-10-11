import HoiminOracle.SurvivorReviewModel

namespace HoiminOracle.SurvivorReview

theorem applied_requires_exact_identity_and_fresh_source
    (expected stored : Key) (source : Nat)
    (h : applicability expected stored source = "current") :
    expected = stored ∧ source = 0 := by
  unfold applicability at h
  split at h <;> simp_all
  split at h <;> simp_all

theorem rejected_update_preserves_state
    (state : State) (applicable : String) (expected : Nat)
    (h : (applyUpdate state applicable expected).1 ≠ "accepted") :
    (applyUpdate state applicable expected).2 = state := by
  unfold applyUpdate at *
  split <;> simp_all
  split <;> simp_all
  split <;> simp_all

theorem accepted_update_preserves_prefix_and_increments_revision
    (state : State) (applicable : String) (expected : Nat)
    (h : (applyUpdate state applicable expected).1 = "accepted") :
    (applyUpdate state applicable expected).2.history = state.history ++ [state.revision + 1] ∧
    (applyUpdate state applicable expected).2.revision = state.revision + 1 := by
  unfold applyUpdate at *
  split <;> simp_all
  split <;> simp_all
  split <;> simp_all

end HoiminOracle.SurvivorReview
