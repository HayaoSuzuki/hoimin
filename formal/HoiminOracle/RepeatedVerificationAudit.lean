import Std

namespace RepeatedVerificationAudit

-- true means a test failed. The suite reports failure if any test fails.
def suiteFailure (tests : List Bool) : Bool := tests.any id

theorem deterministic_failure_masks (flaky : Bool) :
    suiteFailure [flaky, true] = true := by cases flaky <;> decide

-- Two distinct individual-test observations can have identical command outcomes.
example : [false, true] ≠ [true, true] ∧
    suiteFailure [false, true] = suiteFailure [true, true] := by decide

-- Any finite all-passing prefix is compatible with a later failure.
theorem finite_agreement_has_failing_extension (n : Nat) :
    (List.replicate n false ++ [true]).take n = List.replicate n false ∧
    suiteFailure (List.replicate n false ++ [true]) = true := by
  constructor
  · simp
  · simp [suiteFailure]

-- Detecting mixed observations requires both a success and a failure.
def mixed (trials : List Bool) : Bool := trials.contains true && trials.contains false
example : mixed [true, false] = true := by decide
example : mixed [true, true] = false := by decide
example : mixed [] = false := by decide

-- These refute the broken rules "a flaky test forces suite flips" and
-- "three identical observations certify all future observations".
example : mixed ([false, true].map fun x => suiteFailure [x, true]) ≠ mixed [false, true] := by decide
example : mixed ([false, false, false, true].take 3) ≠ mixed [false, false, false, true] := by decide

end RepeatedVerificationAudit
