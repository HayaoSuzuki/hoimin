import HoiminOracle.FactIndexModel

namespace HoiminOracle.FactIndex

theorem containment_index_is_exact
    (ranges : List Range)
    (index : ContainmentIndex)
    (valid : ValidContainmentIndex ranges index)
    (query : Range) :
    indexedContains index query ↔ linearContains ranges query := by
  constructor
  · rintro ⟨maximumEnd, found, queryEnds⟩
    obtain ⟨⟨outer, member, starts, ends⟩, _⟩ := valid.1 query.start maximumEnd found
    exact ⟨outer, member, starts, by simpa [ends] using queryEnds⟩
  · rintro ⟨outer, member, starts, queryEnds⟩
    cases found : index query.start with
    | none =>
        have absent := (valid.2 query.start).mp found
        exact False.elim (absent ⟨outer, member, starts⟩)
    | some maximumEnd =>
        have bounded := (valid.1 query.start maximumEnd found).2 outer member starts
        exact ⟨maximumEnd, found, Nat.le_trans queryEnds bounded⟩

theorem scope_index_is_exact
    (scopes : List Scope)
    (segments : List ScopeSegment)
    (valid : ValidScopeIndex scopes segments)
    (offset : Nat)
    (selected : Scope) :
    indexedSelect segments offset selected ↔ linearSelect scopes offset selected := by
  constructor
  · exact valid.1 offset selected
  · exact valid.2 offset selected

def overlapWitness : List Range :=
  [{ start := 0, stop := 10 }, { start := 5, stop := 6 }]

def overlapQuery : Range := { start := 7, stop := 8 }

example : linearContains overlapWitness overlapQuery := by
  simp [linearContains, Range.contains, overlapWitness, overlapQuery]

example : brokenLastEligibleContains overlapWitness overlapQuery = false := by
  decide

def nestedScopes : List Scope := [
  { start := 0, stop := 20, ordinal := 0, symbol := 1 },
  { start := 5, stop := 10, ordinal := 1, symbol := 2 }
]

def outerAfterInner : Scope :=
  { start := 0, stop := 20, ordinal := 0, symbol := 1 }

example : linearSelect nestedScopes 12 outerAfterInner := by
  simp [linearSelect, Scope.contains, noLaterThan, nestedScopes, outerAfterInner]

example : brokenLastStartedScope nestedScopes 12 = some 2 := by
  decide

end HoiminOracle.FactIndex

