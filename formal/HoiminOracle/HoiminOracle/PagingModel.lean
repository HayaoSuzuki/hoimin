import HoiminOracle.CandidateRankingModel

namespace HoiminOracle.Paging

open HoiminOracle.CandidateRanking

def mk (id : String) (path : HoiminOracle.CandidateRanking.Path) (line : Nat) (high : Bool) : Candidate :=
 { id, path, line, column := 4,
   operatorKey := if high then "boolean_literal" else "binary_add_sub",
   operatorClass := if high then .highValueControl else .arithmetic }
def ranked : List RankedCandidate := rankCandidates [
 mk "a1" .alpha 1 true, mk "a2" .alpha 2 true, mk "a3" .alpha 3 false,
 mk "b1" .beta 1 true, mk "b2" .beta 2 false, mk "b3" .beta 3 false]
def ids (ranked : List RankedCandidate) : List String := ranked.map (·.candidate.id)
def ordering (diverse : Bool) : List String := ids (if diverse then diverseOrder ranked else ranked)
def page (xs : List String) (offset count : Nat) := (xs.drop offset).take count
set_option maxHeartbeats 10000 in
theorem slice_prefix {α : Type} (xs : List α) (offset count : Nat) :
 (xs.take (offset + count)).drop offset = (xs.drop offset).take count := by
 induction offset generalizing xs with
 | zero => simp
 | succ n ih =>
  cases xs with
  | nil => simp
  | cons x xs => simpa [Nat.succ_add] using ih xs
set_option maxHeartbeats 10000 in
example : ordering true = ["a1","b1","a2","a3","b2","b3"] := by decide
set_option maxHeartbeats 10000 in
example : page (ordering true) 0 2 ++ page (ordering true) 2 2 ++ page (ordering true) 4 2 = ordering true := by decide
set_option maxHeartbeats 10000 in
example : page (ordering true) 1 1 != (ids (diverseOrder (ranked.drop 1))).take 1 := by decide
set_option maxHeartbeats 10000 in
example : page (ordering false) 1 1 != ((ordering false).take 1).drop 1 := by decide

set_option maxHeartbeats 10000 in
example : ordering true != ids (diverseOrder (ranked.map fun x => { x with score := 100 })) := by decide

-- Executable controls are checked on every generation, independently of Rust.
def sensitivity : Bool :=
  page (ordering true) 1 1 != (ids (diverseOrder (ranked.drop 1))).take 1 &&
  page (ordering false) 1 1 != ((ordering false).take 1).drop 1 &&
  ordering true != ids (diverseOrder (ranked.map fun x => { x with score := 100 }))

set_option maxHeartbeats 10000 in
example : sensitivity = true := by decide

end HoiminOracle.Paging
