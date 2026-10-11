import Init

namespace HoiminOracle.SurvivorReview

-- Descriptor tokens stand for all eleven descriptor fields, including original and symbol.
structure Key where
  project : Nat
  id : Nat
  descriptor : List Nat
  deriving DecidableEq, BEq

structure State where
  revision : Nat
  history : List Nat
  deriving DecidableEq, BEq

def maxRevision : Nat := 18446744073709551615

def applicability (expected stored : Key) (source : Nat) : String :=
  if expected ≠ stored then "identity_conflict"
  else if source = 0 then "current"
  else if source = 1 then "stale" else "source_unavailable"

def applyUpdate (state : State) (applicable : String) (expected : Nat) : String × State :=
  if applicable ≠ "current" then ("not_current", state)
  else if state.revision ≠ expected then ("revision_conflict", state)
  else if state.revision = maxRevision then ("revision_overflow", state)
  else ("accepted", ⟨state.revision + 1, state.history ++ [state.revision + 1]⟩)

end HoiminOracle.SurvivorReview
