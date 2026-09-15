import Std

namespace PostIntegration

inductive Event | importTyping | mayRaise | raiseNow
  deriving Repr, BEq, DecidableEq

-- Each Boolean describes whether Sequence definitely denotes typing.Sequence.
-- The list retains both normal and exceptional entries into finally.
def entries (known : Bool) : List Event → List Bool
  | [] => [known]
  | .importTyping :: rest => entries true rest
  | .mayRaise :: rest => known :: entries known rest
  | .raiseNow :: _ => [known]

def eligible (events : List Event) : Bool := (entries false events).all id

-- Production-shaped defect: ordinary expressions do not create exceptional exits.
def brokenEntries (known : Bool) : List Event → List Bool
  | [] => [known]
  | .importTyping :: rest => brokenEntries true rest
  | .mayRaise :: rest => brokenEntries known rest
  | .raiseNow :: _ => [known]

def brokenEligible (events : List Event) : Bool := (brokenEntries false events).all id

example : eligible [.mayRaise, .importTyping] = false ∧
    brokenEligible [.mayRaise, .importTyping] = true := by decide

theorem hazard_preserves_exception_entry (known : Bool) (rest : List Event) :
    known ∈ entries known (.mayRaise :: rest) := by simp [entries]

theorem custom_before_hazard_is_never_eligible (rest : List Event) :
    eligible (.mayRaise :: rest) = false := by simp [eligible, entries]

theorem eligibility_requires_every_entry (events : List Event)
    (h : eligible events = true) : ∀ x ∈ entries false events, x = true := by
  simpa [eligible, List.all_eq_true] using h

theorem duplicate_entry_does_not_change_trust (known : Bool) (rest : List Bool) :
    (known :: known :: rest).all id = (known :: rest).all id := by
  cases known <;> simp

def loopVisits : Nat → Nat
  | 0 => 1
  | n + 1 => 2 * loopVisits n

theorem nested_loop_visits (n : Nat) : loopVisits n = 2 ^ n := by
  induction n with
  | zero => rfl
  | succ n ih => simp [loopVisits, ih, Nat.pow_succ, Nat.mul_comm]

-- Lower bound for one full-state copy after each of M annotations, N aliases.
def cloneFloor (aliases annotations : Nat) : Nat := aliases * annotations

theorem doubling_both_quadruples (n m : Nat) :
    cloneFloor (2*n) (2*m) = 4 * cloneFloor n m := by
  simp [cloneFloor, Nat.mul_assoc, Nat.mul_left_comm, Nat.mul_comm]

end PostIntegration
