import Std
namespace ReturnTupleSwap

def swap (pair : α × α) : α × α := (pair.2, pair.1)

theorem first_is_old_second (pair : α × α) : (swap pair).1 = pair.2 := rfl
theorem second_is_old_first (pair : α × α) : (swap pair).2 = pair.1 := rfl
theorem swap_twice (pair : α × α) : swap (swap pair) = pair := by cases pair; rfl

theorem equal_elements_unchanged (a : α) : swap (a, a) = (a, a) := rfl

theorem distinct_elements_change (a b : α) (h : a ≠ b) : swap (a,b) ≠ (a,b) := by
  intro same
  have first := congrArg Prod.fst same
  exact h first.symm

example : swap (3,9) = (9,3) := rfl
example : [((3,9) : Nat × Nat).1, (3,9).2].length =
    [(swap (3,9)).1, (swap (3,9)).2].length := rfl
-- A broken identity implementation passes length but fails positional observation.
example : ((3,9) : Nat × Nat) ≠ swap (3,9) := by decide
end ReturnTupleSwap
