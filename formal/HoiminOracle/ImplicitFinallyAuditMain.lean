import Lean.Data.Json
open Lean

/- A deliberately small model: imports succeed; mayRaise leaves the current
   binding on an exceptional path, and continues unchanged on success. -/
inductive Event where
  | importTyping | mayRaise | explicitRaise
  deriving DecidableEq

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

theorem implicit_custom_blocks (rest : List Event) :
    candidate (.mayRaise :: rest) = false := by
  simp [candidate, entries]

example : candidate [.importTyping, .mayRaise] = true := by decide
example : candidate [.explicitRaise, .importTyping] = false := by decide
example : candidate [.mayRaise, .importTyping] ≠
    (brokenEntries false [.mayRaise, .importTyping]).all id := by decide

def cases : List (String × String × List Event) := [
  ("call_before_import", "hazard()\n    from typing import Sequence", [.mayRaise, .importTyping]),
  ("subscript_before_import", "items[0]\n    from typing import Sequence", [.mayRaise, .importTyping]),
  ("attribute_before_import", "item.value\n    from typing import Sequence", [.mayRaise, .importTyping]),
  ("import_before_call", "from typing import Sequence\n    hazard()", [.importTyping, .mayRaise]),
  ("explicit_raise", "raise KeyError()\n    from typing import Sequence", [.explicitRaise, .importTyping]),
  ("normal_import", "from typing import Sequence", [.importTyping])]

def corpus : String := String.join (cases.map fun (name, body, events) =>
  (Lean.Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson name),
    ("source", toJson ("Sequence = set\ntry:\n    " ++ body ++
      "\nfinally:\n    def record(value: Sequence[int]):\n        pass\n")),
    ("candidate_count", toJson (if candidate events then (1 : Nat) else 0))]).compress ++ "\n")

def main (args : List String) : IO Unit := do
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--sensitivity"] =>
      if candidate [.mayRaise, .importTyping] ==
          (brokenEntries false [.mayRaise, .importTyping]).all id then
        throw (IO.userError "implicit-entry omission was not detected")
      IO.println "drop_implicit_entry=true"
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] =>
      if (← IO.FS.readFile path) != corpus then throw (IO.userError "stale implicit-finally corpus")
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity")
