import AuditModel
import Lean.Data.Json

open PostIntegration

def alphabet : List Event := [.importTyping, .mayRaise, .raiseNow]
def traces : Nat → List (List Event)
  | 0 => [[]]
  | n+1 => alphabet.flatMap fun event => (traces n).map (event :: ·)
def tracesUpTo (n : Nat) := (List.range (n+1)).flatMap traces

def statement : Event → String
  | .importTyping => "        from typing import Sequence\n"
  | .mayRaise => "        hazard()\n"
  | .raiseNow => "        raise KeyError()\n"

def source (events : List Event) : String :=
  "Sequence = set\ntry:\n    try:\n" ++ String.join (events.map statement) ++
  "    finally:\n        def record(value: Sequence[int]):\n            pass\n        observed = record.__annotations__['value']\nexcept KeyError:\n    pass\n"

def cases : List (String × List Event) := [
  ("implicit_before_import", [.mayRaise, .importTyping]),
  ("import_before_implicit", [.importTyping, .mayRaise]),
  ("explicit_before_import", [.raiseNow, .importTyping])]

def main (args : List String) : IO UInt32 := do
  let witness := [.mayRaise, .importTyping]
  let sensitivity := brokenEligible witness && !eligible witness
  unless sensitivity do return 2
  -- Boundary sensitivity: reversing import and call must change eligibility.
  unless eligible [.importTyping, .mayRaise] do return 3
  for depth in List.range 5 do
    let domain := tracesUpTo depth
    let mismatches := domain.filter fun trace => brokenEligible trace != eligible trace
    let transitions := domain.foldl (fun n trace => n + trace.length) 0
    IO.eprintln s!"depth={depth} alphabet=3 traces={domain.length} transitions={transitions} mismatches={mismatches.length} first={repr (mismatches.head?)}"
  let rows := cases.map fun (name, events) => Lean.Json.mkObj [
    ("id", .str name), ("mode", .str "strict"),
    ("source", .str (source events)),
    ("expected_candidate_count", Lean.toJson (if eligible events then 1 else (0 : Nat))),
    ("finally_entries", Lean.toJson (entries false events))]
  match args with
  | ["--output", path] =>
    IO.FS.writeFile path (String.join (rows.map fun row => row.compress ++ "\n"))
    return 0
  | _ => return 1
