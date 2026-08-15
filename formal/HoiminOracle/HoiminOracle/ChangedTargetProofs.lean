import HoiminOracle.ChangedTargetModel

namespace HoiminOracle.ChangedTarget

theorem normalization_preserves_membership (ranges : List LineRange) (line : Nat) :
    normalize ranges line ↔ InRanges ranges line := by
  rfl

theorem mem_normalizedLines_iff (maximum : Nat) (ranges : List LineRange) (line : Nat) :
    line ∈ normalizedLines maximum ranges ↔ line ≤ maximum ∧ normalize ranges line := by
  simp [normalizedLines]
  omega

theorem normalizedLines_idempotent (maximum : Nat) (ranges : List LineRange) :
    normalizeLineSet maximum (normalizedLines maximum ranges) =
      normalizedLines maximum ranges := by
  simp only [normalizeLineSet, normalizedLines]
  apply List.filter_congr
  intro line member
  simp only [List.mem_filter, List.mem_range] at member ⊢
  simp [member]

theorem combined_is_changed_subset (facts : List ChangeFact) (selectors : List Selector)
    (observation : Observation) (accepted : CombinedEligible facts selectors observation) :
    ChangedEligible facts observation.path observation.line :=
  accepted.1

theorem combined_is_explicit_subset (facts : List ChangeFact) (selectors : List Selector)
    (observation : Observation) (accepted : CombinedEligible facts selectors observation) :
    ExplicitEligible selectors observation :=
  accepted.2

theorem intersection_commutative_at_observation (facts : List ChangeFact)
    (selectors : List Selector) (observation : Observation) :
    CombinedEligible facts selectors observation ↔
      ExplicitEligible selectors observation ∧
        ChangedEligible facts observation.path observation.line := by
  constructor
  · intro accepted
    exact ⟨accepted.2, accepted.1⟩
  · intro accepted
    exact ⟨accepted.2, accepted.1⟩

theorem empty_explicit_is_identity (facts : List ChangeFact) (observation : Observation) :
    CombinedEligible facts [] observation ↔
      ChangedEligible facts observation.path observation.line := by
  simp [CombinedEligible, ExplicitEligible]

theorem combined_cannot_create_path (facts : List ChangeFact) (selectors : List Selector)
    (observation : Observation) (accepted : CombinedEligible facts selectors observation) :
    ∃ fact ∈ facts, effectivePath fact = some observation.path := by
  rcases accepted.1.2 with ⟨fact, member, selected⟩
  refine ⟨fact, member, ?_⟩
  cases kind : fact.kind <;> simp [selectedBy, effectivePath, kind] at selected ⊢
  all_goals exact selected.1

theorem excluded_dominates (facts : List ChangeFact) (selectors : List Selector)
    (observation : Observation) (excluded : Excluded facts observation.path) :
    ¬ CombinedEligible facts selectors observation := by
  intro accepted
  exact accepted.1.1 excluded

theorem renamed_uses_destination (fact : ChangeFact)
    (kind : fact.kind = .renamed) (path : String) (line : Nat)
    (selected : selectedBy fact path line) : fact.destinationPath = some path := by
  simp [selectedBy, kind] at selected
  exact selected.1

theorem renamed_never_selects_distinct_source (fact : ChangeFact)
    (kind : fact.kind = .renamed) (source destination : String)
    (_sourceFact : fact.sourcePath = some source)
    (destinationFact : fact.destinationPath = some destination)
    (different : source ≠ destination) (line : Nat) :
    ¬ selectedBy fact source line := by
  simp [selectedBy, kind, destinationFact, Ne.symm different]

theorem untracked_bounds (fact : ChangeFact) (path : String) (line : Nat)
    (kind : fact.kind = .untracked) (selected : selectedBy fact path line) :
    0 < line ∧ line ≤ fact.currentLineCount := by
  simp [selectedBy, kind] at selected
  exact selected.2

end HoiminOracle.ChangedTarget
