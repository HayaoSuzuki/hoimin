import HoiminOracle.ResourceCleanupModel

open HoiminOracle.ResourceCleanup

def alphabet : List Event :=
  [.beginCleanup, .removeDirectory, .commit, .retryableFailure,
   .probeRegistry, .probeCounters, .close, .admit]

def traces : Nat → List (List Event)
  | 0 => [[]]
  | n + 1 => alphabet.flatMap fun event => (traces n).map (event :: ·)

def tracesUpTo (depth : Nat) : List (List Event) :=
  (List.range (depth + 1)).flatMap traces

def witness (variant : Variant) (cases : List (List Event)) : Option (List Event) :=
  cases.find? fun events => (run variant events).violation

def main (args : List String) : IO UInt32 := do
  let depth := (args.head?.bind String.toNat?).getD 4
  if depth > 4 then
    IO.eprintln "resource cleanup audit depth is capped at 4"
    return 2
  let cases := tracesUpTo depth
  if (witness .correct cases).isSome then
    IO.eprintln s!"correct model counterexample: {repr (witness .correct cases)}"
    return 1
  let fixed : List (Variant × List Event) := [
    (.holdsRegistry, [.beginCleanup, .probeRegistry]),
    (.exposesCounters, [.beginCleanup, .removeDirectory, .probeCounters]),
    (.duplicateCleanup, [.beginCleanup, .beginCleanup]),
    (.admitsAfterClose, [.close, .admit])]
  for (variant, expected) in fixed do
    unless (run variant expected).violation do
      IO.eprintln "broken transition was not detected by its fixed witness"
      return 1
    if expected.length ≤ depth then
      unless witness variant cases == some expected do
        IO.eprintln s!"unexpected shortest witness: {repr (witness variant cases)}"
        return 1
  let transitions := cases.foldl (fun total events => total + events.length) 0
  IO.println s!"depth={depth} alphabet=8 traces={cases.length} transitions={transitions} correctCounterexamples=0 brokenWitnesses=4"
  return 0
