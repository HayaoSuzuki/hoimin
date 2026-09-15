import Std

namespace HoiminOracle.ImplicitFinally

/- Imports succeed. mayRaise keeps both the exceptional entry and normal
   continuation. Dynamic hooks, import failure and exception payloads are outside
   this model. Each Bool records whether Sequence denotes typing.Sequence. -/
inductive Event where
  | importTyping | mayRaise | explicitRaise
  deriving DecidableEq, BEq, Repr

def entries (typing : Bool) : List Event → List Bool
  | [] => [typing]
  | .importTyping :: rest => entries true rest
  | .mayRaise :: rest => typing :: entries typing rest
  | .explicitRaise :: _ => [typing]

def candidate (events : List Event) : Bool := (entries false events).all id

def brokenEntries (typing : Bool) : List Event → List Bool
  | [] => [typing]
  | .importTyping :: rest => brokenEntries true rest
  | .mayRaise :: rest => brokenEntries typing rest
  | .explicitRaise :: _ => [typing]

-- Runtime oracle for a supplied hazard that either returns or raises KeyError.
def runtimeEntry (typing raises : Bool) : List Event → Bool
  | [] => typing
  | .importTyping :: rest => runtimeEntry true raises rest
  | .mayRaise :: rest => if raises then typing else runtimeEntry typing raises rest
  | .explicitRaise :: _ => typing

theorem runtime_entry_is_reachable (events : List Event) (typing raises : Bool) :
    runtimeEntry typing raises events ∈ entries typing events := by
  induction events generalizing typing with
  | nil => simp [runtimeEntry, entries]
  | cons event rest ih =>
    cases event with
    | importTyping => exact ih true
    | mayRaise =>
      cases raises with
      | false => exact List.mem_cons_of_mem typing (ih typing)
      | true => simp [runtimeEntry, entries]
    | explicitRaise => simp [runtimeEntry, entries]

theorem hazard_preserves_exception_entry (typing : Bool) (rest : List Event) :
    typing ∈ entries typing (.mayRaise :: rest) := by simp [entries]

theorem implicit_custom_blocks (rest : List Event) :
    candidate (.mayRaise :: rest) = false := by simp [candidate, entries]

theorem eligibility_requires_every_entry (events : List Event)
    (h : candidate events = true) : ∀ x ∈ entries false events, x = true := by
  simpa [candidate, List.all_eq_true] using h

theorem duplicate_entry_does_not_change_trust (typing : Bool) (rest : List Bool) :
    (typing :: typing :: rest).all id = (typing :: rest).all id := by
  cases typing <;> simp

example : candidate [.importTyping, .mayRaise] = true := by decide
example : candidate [.explicitRaise, .importTyping] = false := by decide
example : candidate [.mayRaise, .importTyping] ≠
    (brokenEntries false [.mayRaise, .importTyping]).all id := by decide

def alphabet : List Event := [.importTyping, .mayRaise, .explicitRaise]
def traces : Nat → List (List Event)
  | 0 => [[]]
  | n + 1 => alphabet.flatMap fun event => (traces n).map (event :: ·)
def tracesUpTo (depth : Nat) := (List.range (depth + 1)).flatMap traces

end HoiminOracle.ImplicitFinally
