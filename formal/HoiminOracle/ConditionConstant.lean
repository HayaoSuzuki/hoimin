import Std
namespace ConditionConstant

def eligible (literal safe : Bool) : Bool := !literal && safe

def choose (forced : Bool) (yes no : Nat) : Nat × Nat :=
  (if forced then yes else no, 0)

theorem literals_excluded (safe : Bool) : eligible true safe = false := by
  simp [eligible]
theorem unsafe_excluded (literal : Bool) : eligible literal false = false := by
  simp [eligible]
theorem fixed_true (yes no : Nat) : (choose true yes no).1 = yes := by
  simp [choose]
theorem fixed_false (yes no : Nat) : (choose false yes no).1 = no := by
  simp [choose]
theorem condition_effects_removed (forced : Bool) (yes no : Nat) :
    (choose forced yes no).2 = 0 := by simp [choose]
example : eligible false true = true := by decide
example : eligible true true = false := by decide
example : eligible false false = false := by decide
-- Omitting the nested-safety check wrongly accepts this boundary.
example : (!false) ≠ eligible false false := by decide
end ConditionConstant
