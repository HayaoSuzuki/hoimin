import HoiminOracle.ImplicitFinallyModel
import Lean.Data.Json
open Lean HoiminOracle.ImplicitFinally

def cases : List (String × String × List Event) := [
  ("call_before_import", "hazard()\n    from typing import Sequence", [.mayRaise, .importTyping]),
  ("subscript_before_import", "items[0]\n    from typing import Sequence", [.mayRaise, .importTyping]),
  ("attribute_before_import", "item.value\n    from typing import Sequence", [.mayRaise, .importTyping]),
  ("import_before_call", "from typing import Sequence\n    hazard()", [.importTyping, .mayRaise]),
  ("explicit_raise", "raise KeyError()\n    from typing import Sequence", [.explicitRaise, .importTyping]),
  ("normal_import", "from typing import Sequence", [.importTyping])]

def ordinaryCorpus : String := String.join (cases.map fun (name, body, events) =>
  (Lean.Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson name),
    ("source", toJson ("Sequence = set\ntry:\n    " ++ body ++
      "\nfinally:\n    def record(value: Sequence[int]):\n        pass\n")),
    ("candidate_count", toJson (if candidate events then (1 : Nat) else 0))]).compress ++ "\n")


def auditCases : List (String × List Event) := [
  ("implicit_before_import", [.mayRaise, .importTyping]),
  ("import_before_implicit", [.importTyping, .mayRaise]),
  ("explicit_before_import", [.explicitRaise, .importTyping])]

def auditStatement : Event → String
  | .importTyping => "        from typing import Sequence\n"
  | .mayRaise => "        hazard()\n"
  | .explicitRaise => "        raise KeyError()\n"

def auditCorpus : String := String.join (auditCases.map fun (name, events) =>
  let source := "Sequence = set\ntry:\n    try:\n" ++ String.join (events.map auditStatement) ++
    "    finally:\n        def record(value: Sequence[int]):\n            pass\n        observed = record.__annotations__['value']\nexcept KeyError:\n    pass\n"
  (Lean.Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson ("audit_" ++ name)),
    ("source", toJson source),
    ("candidate_count", toJson (if candidate events then (1 : Nat) else 0)),
    ("runtime_typing", toJson ([false, true].map fun raises => runtimeEntry false raises events))]).compress ++ "\n")

def corpus := ordinaryCorpus ++ auditCorpus

def main (args : List String) : IO Unit := do
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--sensitivity"] =>
      if candidate [.mayRaise, .importTyping] ==
          (brokenEntries false [.mayRaise, .importTyping]).all id then
        throw (IO.userError "implicit-entry omission was not detected")
      -- Retain the audit's bounded search, with increasing depths and no
      -- implementation claim for traces without a public fixture.
      for depth in List.range 5 do
        let domain := tracesUpTo depth
        let mismatches := domain.filter fun events =>
          candidate events != (brokenEntries false events).all id
        let transitions := domain.foldl (fun n events => n + events.length) 0
        unless domain.length == [1, 4, 13, 40, 121][depth]! &&
            transitions == [0, 3, 21, 102, 426][depth]! &&
            mismatches.length == [0, 0, 1, 5, 18][depth]! do
          throw (IO.userError "bounded implicit-finally search changed")
        IO.println s!"depth={depth} alphabet=3 traces={domain.length} transitions={transitions} mismatches={mismatches.length}"
      IO.println "drop_implicit_entry=true"
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] =>
      if (← IO.FS.readFile path) != corpus then throw (IO.userError "stale implicit-finally corpus")
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity")
