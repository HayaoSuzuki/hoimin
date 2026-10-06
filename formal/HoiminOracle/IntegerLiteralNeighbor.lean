import Std
namespace IntegerLiteralNeighbor

def neighbors (limit n : Int) : List Int :=
  [n - 1, n + 1].filter (fun m => -limit ≤ m && m ≤ limit)

theorem neighbor_is_adjacent (limit n m : Int) (h : m ∈ neighbors limit n) :
    m = n - 1 ∨ m = n + 1 := by
  simpa [neighbors] using (List.mem_filter.mp h).1

theorem neighbor_stays_in_bounds (limit n m : Int) (h : m ∈ neighbors limit n) :
    -limit ≤ m ∧ m ≤ limit := by
  simpa using (List.mem_filter.mp h).2

theorem neighbor_changes_value (limit n m : Int) (h : m ∈ neighbors limit n) : m ≠ n := by
  have := neighbor_is_adjacent limit n m h
  omega

example : neighbors 3 0 = [-1, 1] := by decide
example : neighbors 3 (-3) = [-2] := by decide
example : neighbors 3 3 = [2] := by decide
-- Broken unsigned lower bound drops the negative neighbor of zero.
example : ([0 + 1] : List Int) ≠ neighbors 3 0 := by decide
end IntegerLiteralNeighbor
