import Std

namespace ReleaseSbom

/-- Fixed-position obligations abstract schema, identity and artifact validation. -/
def accept (n : Nat) (present valid : Nat → Bool) (extra : Bool) : Bool :=
  (List.range n).all (fun i => present i && valid i) && !extra

set_option maxHeartbeats 100000 in
theorem accepted_requires (n i : Nat) (present valid : Nat → Bool) (extra : Bool)
    (h : accept n present valid extra = true) (hi : i < n) :
    present i = true ∧ valid i = true := by
  have hall := (Bool.and_eq_true_iff.mp h).1
  have heach := List.all_eq_true.mp hall i (List.mem_range.mpr hi)
  exact Bool.and_eq_true_iff.mp heach

theorem accepted_no_extra (n : Nat) (present valid : Nat → Bool) (extra : Bool)
    (h : accept n present valid extra = true) : extra = false := by
  have hx := (Bool.and_eq_true_iff.mp h).2
  simpa using hx

def writeIfAccepted (accepted old : Bool) : Bool := if accepted then true else old

theorem rejection_preserves (old : Bool) : writeIfAccepted false old = old := by
  simp [writeIfAccepted]

theorem replay_idempotent (accepted old : Bool) :
    writeIfAccepted accepted (writeIfAccepted accepted old) =
      writeIfAccepted accepted old := by
  cases accepted <;> simp [writeIfAccepted]

/-- Omitting existence validation accepts a missing required document. -/
def brokenMissing (valid : Nat → Bool) : Bool := (List.range 6).all valid

example : brokenMissing (fun _ => true) = true ∧
    accept 6 (fun _ => false) (fun _ => true) false = false := by decide

/-- Writing before validation destroys pre-existing output on rejection. -/
example : (true : Bool) ≠ writeIfAccepted false false := by decide

end ReleaseSbom

/-- Bounded enumeration is separate from the universal model proofs above. -/
def main : IO Unit := do
  for mask in List.range 64 do
    for valid in [false, true] do
      for extra in [false, true] do
        let verdict := ReleaseSbom.accept 6 (fun i => mask.testBit i) (fun _ => valid) extra
        IO.println s!"{mask},{valid},{extra},{verdict}"
