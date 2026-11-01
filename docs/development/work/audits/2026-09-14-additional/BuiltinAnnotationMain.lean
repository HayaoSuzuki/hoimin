import BuiltinAnnotationModel
import Lean.Data.Json

open BuiltinAnnotation

def pairs : List (String × String × String × String × String) := [
  ("list", "Sequence", "tuple", "int", "type_list_sequence"),
  ("set", "AbstractSet", "frozenset", "int", "type_set_abstract_set"),
  ("dict", "Mapping", "tuple", "str, int", "type_dict_mapping")]

def main (args : List String) : IO UInt32 := do
  let facts := [Fact.builtin, .shadowed, .unknown]
  unless (facts.filter fun fact => eligible fact != brokenEligible fact).length == 2 do return 2
  let domain := pairs.flatMap fun _ => [false, true].flatMap fun _ => facts
  let mismatches := domain.filter fun fact => eligible fact != brokenEligible fact
  IO.eprintln s!"facts=3 directions=2 pairs=3 predicate_cases={domain.length} broken_mismatches={mismatches.length}"
  let mut rows : List Lean.Json := []
  for (concrete, abstract, custom, param, operator) in pairs do
    for (context, reverse) in [("control", false), ("control", true),
        ("module_shadow", false), ("module_shadow", true), ("type_parameter", true)] do
      let fact := if context == "control" then Fact.builtin else Fact.shadowed
      let original := (if reverse then abstract else concrete) ++ "[" ++ param ++ "]"
      let replacement := (if reverse then concrete else abstract) ++ "[" ++ param ++ "]"
      let binding := if context == "module_shadow" then concrete ++ " = " ++ custom ++ "\n" else ""
      let params := if context == "type_parameter" then "[" ++ concrete ++ "]" else ""
      let source := "from typing import " ++ abstract ++ "\n" ++ binding ++
        "def record" ++ params ++ "(value: " ++ original ++ "):\n    pass\n"
      rows := rows ++ [Lean.Json.mkObj [
        ("id", .str s!"{concrete}-{context}-{reverse}"), ("mode", .str "strict"),
        ("operator", .str operator), ("source", .str source),
        ("original", .str original), ("replacement", .str replacement),
        ("expected_candidate_count", Lean.toJson (if eligible fact then 1 else (0 : Nat)))]]
  match args with
  | ["--output", path] =>
    IO.FS.writeFile path (String.join (rows.map fun row => row.compress ++ "\n"))
    return 0
  | _ => return 1
