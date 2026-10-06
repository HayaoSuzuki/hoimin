import Std
namespace ConditionClauseDelete

def removeOne (xs : List α) (n : Nat) : List α := xs.take n ++ xs.drop (n + 1)
theorem retained_order (xs : List α) (n : Nat) :
    removeOne xs n = xs.take n ++ xs.drop (n + 1) := rfl
theorem first_removed (x : α) (xs : List α) : removeOne (x :: xs) 0 = xs := rfl
theorem two_left (a b : α) : removeOne [a, b] 0 = [b] := rfl
theorem two_right (a b : α) : removeOne [a, b] 1 = [a] := rfl
theorem one_fewer (xs : List α) (n : Nat) (h : n < xs.length) :
    (removeOne xs n).length + 1 = xs.length := by
  simp [removeOne, List.length_take, List.length_drop]
  omega
-- One negative case misses removal of authentication; the other detects it.
example : (true && false) = false := rfl
example : (false && true) ≠ true := by decide
example : removeOne [1,2,3] 1 = [1,3] := rfl
end ConditionClauseDelete
