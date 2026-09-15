import HoiminOracle.WithSuppressionModel
import Lean.Data.Json
open Lean WithAudit

def alphabet : List Event := [.importTyping, .mayRaise, .explicitRaise]
def traces : Nat → List (List Event)
  | 0 => [[]]
  | n + 1 => alphabet.flatMap fun e => (traces n).map (e :: ·)

def cases : List (String × List Event) := [
  ("call_before_import", [.mayRaise, .importTyping]),
  ("import_before_call", [.importTyping, .mayRaise]),
  ("explicit_raise", [.explicitRaise, .importTyping]),
  ("normal_import", [.importTyping]),
  ("custom_only", [.mayRaise])]

def statement : Event → String
  | .importTyping => "    from typing import Sequence\n"
  | .mayRaise => "    hazard()\n"
  | .explicitRaise => "    raise KeyError()\n"

def corpus : String := String.join (cases.map fun (name, events) =>
  let source := "from contextlib import suppress\nSequence = set\nwith suppress(KeyError):\n" ++
    String.join (events.map statement) ++
    "def record(value: Sequence[int]):\n    pass\nobserved = record.__annotations__['value']\n"
  (Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson name), ("mode", toJson "strict"),
    ("source", toJson source),
    ("candidate_count", toJson (if candidate true events then (1 : Nat) else 0)),
    ("runtime_typing", toJson ([false, true].map fun raises => runtime false raises events))
  ]).compress ++ "\n")

def checkSensitivity : IO Unit := do
  for depth in List.range 5 do
    let started ← IO.monoMsNow
    let domain := (List.range (depth + 1)).flatMap traces
    let mismatches := domain.filter fun events => candidate true events != brokenCandidate events
    let unsafeCases := domain.filter fun events => !candidate true events && brokenCandidate events
    let elapsed := (← IO.monoMsNow) - started
    IO.println s!"depth={depth} alphabet=3 traces={domain.length} event_occurrences={domain.foldl (fun n es => n + es.length) 0} differences={mismatches.length} unsafe={unsafeCases.length} firstUnsafe={repr (unsafeCases.head?)} elapsed_ms={elapsed}"
  -- Keep fixed witnesses outside the search. Boundary/precedence and repeated
  -- suppression apply; transactionality and identity allocation do not.
  unless candidate true [.mayRaise, .importTyping] != brokenCandidate [.mayRaise, .importTyping] do
    throw (IO.userError "drop-suppression variant undetected")
  unless candidate true [.importTyping, .mayRaise] do
    throw (IO.userError "reject-all variant undetected")
  unless afterWith false ⟨[], [false]⟩ != afterWith true ⟨[], [false]⟩ do
    throw (IO.userError "suppress-all variant undetected")
  unless afterWith true (afterWith true ⟨[true], [false]⟩) == afterWith true ⟨[true], [false]⟩ do
    throw (IO.userError "suppression idempotence failed")
  unless visits 1 != transferOnce 1 do
    throw (IO.userError "duplicate-traversal variant undetected")
  IO.println "sensitivity: drop-suppression, reject-all, suppress-all, duplicate-traversal detected"

def main (args : List String) : IO Unit := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] =>
      unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale corpus")
  | ["--sensitivity"] => checkSensitivity
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity")
