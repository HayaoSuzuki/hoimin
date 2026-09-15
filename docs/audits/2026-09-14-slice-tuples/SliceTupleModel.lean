import Std

namespace SliceTuple

inductive Item | scalar | slice
  deriving Repr, BEq, DecidableEq

def expression : Item → Bool
  | .scalar => true
  | .slice => false

def eligible (items : List Item) : Bool := items.all expression
def brokenEligible (_ : List Item) : Bool := true

theorem eligible_requires_expressions (items : List Item) (h : eligible items = true) :
    ∀ item ∈ items, item = .scalar := by
  simp only [eligible, List.all_eq_true] at h
  intro item member
  have hx := h item member
  cases item <;> simp_all [expression]

theorem slice_is_rejected (items : List Item) : eligible (.slice :: items) = false := by
  simp [eligible, expression]

example : eligible [.slice] != brokenEligible [.slice] := by decide
example : eligible [.scalar] = true := by decide

end SliceTuple
