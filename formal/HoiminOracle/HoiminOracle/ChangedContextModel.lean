import Std

namespace HoiminOracle.ChangedContext

/-- Current-file hunk coordinates. A zero count is a gap after `start`. -/
structure Hunk where
  start : Nat
  count : Nat
  deriving Repr

def covers (h : Hunk) (context line : Nat) : Prop :=
  if h.count = 0 then
    0 < context ∧ h.start + 1 - context ≤ line ∧ line ≤ h.start + context
  else h.start - context ≤ line ∧ line ≤ h.start + h.count - 1 + context

instance (h : Hunk) (context line : Nat) : Decidable (covers h context line) := by
  unfold covers
  infer_instance

/-- Bounds and explicit selection are applied after the union of expanded hunks. -/
def eligible (hunks : List Hunk) (length context line : Nat)
    (explicit : List Nat := []) (untracked : Bool := false) : Prop :=
  0 < line ∧ line ≤ length ∧
  (untracked = true ∨ ∃ h ∈ hunks, covers h context line) ∧
  (explicit = [] ∨ line ∈ explicit)

instance (hunks : List Hunk) (length context line : Nat)
    (explicit : List Nat) (untracked : Bool) :
    Decidable (eligible hunks length context line explicit untracked) := by
  unfold eligible
  infer_instance

def selected (hunks : List Hunk) (length context : Nat)
    (explicit : List Nat := []) (untracked : Bool := false) : List Nat :=
  (List.range (length + 1)).filter fun line =>
    decide (eligible hunks length context line explicit untracked)

end HoiminOracle.ChangedContext
