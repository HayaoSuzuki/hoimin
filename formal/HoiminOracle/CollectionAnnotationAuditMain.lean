import HoiminOracle.CollectionAnnotationModel
import Lean

namespace CollectionAnnotation

structure Pair where
  concrete : String
  abstract : String
  args : String
  operator : String

def pairs : List Pair := [
  ⟨"list", "Sequence", "int", "type_list_sequence"⟩,
  ⟨"set", "AbstractSet", "int", "type_set_abstract_set"⟩,
  ⟨"dict", "Mapping", "int, str", "type_dict_mapping"⟩]

-- Each scenario fixes the Python premise; only allowed computes expectations.
structure Scenario where
  id : String
  provenance : Provenance
  render : String → String → String
  access : String := "record"
  generic : Bool := false

def scenarios : List Scenario := [
  ⟨"wildcard", .unknown, fun _ a => s!"from math import *\ndef record(value: {a}):\n    pass\n", "record", false⟩,
  ⟨"dynamic", .unknown, fun _ a => s!"exec('')\ndef record(value: {a}):\n    pass\n", "record", false⟩,
  ⟨"global", .unknown, fun n a => s!"{n} = tuple\ndef outer():\n    global {n}\n    def record(value: {a}):\n        pass\n    return record\n", "outer()", false⟩,
  ⟨"nonlocal", .shadowed, fun n a => s!"def outer():\n    {n} = tuple\n    def inner():\n        nonlocal {n}\n        def record(value: {a}):\n            pass\n        return record\n    return inner()\n", "outer()", false⟩,
  ⟨"generic-unrelated", .builtin, fun _ a => s!"def record[T](value: {a}):\n    pass\n", "record", false⟩,
  ⟨"generic-class-local", .shadowed, fun n a => s!"class Box:\n    {n} = tuple\n    def record[T](value: {a}):\n        pass\n", "Box.record", false⟩,
  ⟨"builtin", .builtin, fun _ a => s!"def record(value: {a}):\n    pass\n", "record", false⟩,
  ⟨"module-before", .shadowed, fun n a => s!"{n} = tuple\ndef record(value: {a}):\n    pass\n", "record", false⟩,
  ⟨"module-after", .unknown, fun n a => s!"def record(value: {a}):\n    pass\n{n} = tuple\n", "record", false⟩,
  ⟨"conditional", .unknown, fun n a => s!"if False:\n    {n} = tuple\ndef record(value: {a}):\n    pass\n", "record", false⟩,
  ⟨"own-local", .builtin, fun n a => s!"def record(value: {a}):\n    {n} = tuple\n", "record", false⟩,
  ⟨"closure", .shadowed, fun n a => s!"def outer():\n    {n} = tuple\n    def record(value: {a}):\n        pass\n    return record\n", "outer()", false⟩,
  ⟨"closure-after", .shadowed, fun n a => s!"def outer():\n    def record(value: {a}):\n        pass\n    {n} = tuple\n    return record\n", "outer()", false⟩,
  ⟨"class", .shadowed, fun n a => s!"class Box:\n    {n} = tuple\n    def record(value: {a}):\n        pass\n", "Box.record", false⟩,
  ⟨"class-after", .unknown, fun n a => s!"class Box:\n    def record(value: {a}):\n        pass\n    {n} = tuple\n", "Box.record", false⟩,
  ⟨"nested-class", .builtin, fun n a => s!"class Outer:\n    {n} = tuple\n    class Box:\n        def record(value: {a}):\n            pass\n", "Outer.Box.record", false⟩,
  ⟨"method-body", .builtin, fun n a => s!"class Box:\n    {n} = tuple\n    def factory():\n        def record(value: {a}):\n            pass\n        return record\n", "Box.factory()", false⟩,
  ⟨"generic-function", .shadowed, fun n a => s!"def record[{n}](value: {a}):\n    pass\n", "record", true⟩,
  ⟨"generic-class", .shadowed, fun n a => s!"class Box[{n}]:\n    def record(value: {a}):\n        pass\n", "Box.record", true⟩]

def renderCase (p : Pair) (s : Scenario) (reverse : Bool) : String :=
    let original := s!"{if reverse then p.abstract else p.concrete}[{p.args}]"
    let replacement := s!"{if reverse then p.concrete else p.abstract}[{p.args}]"
    let row := Lean.Json.mkObj [
      ("schema", Lean.toJson (1 : Nat)),
      ("id", Lean.toJson s!"{p.concrete}-{s.id}-{reverse}"),
      ("source", Lean.toJson <| s!"from typing import {p.abstract}\n" ++ s.render p.concrete original),
      ("access", Lean.toJson s.access),
      ("operator", Lean.toJson p.operator),
      ("original", Lean.toJson original),
      ("replacement", Lean.toJson replacement),
      ("present", Lean.toJson <| allowed s.provenance),
      ("evaluation_error", Lean.toJson <| s.generic && !reverse)]
    row.compress ++ "\n"

-- Exact sources from the additional audit that are absent from the original
-- 114-case matrix: frozenset shadowing and dict[str, int] argument order.
def auditCases : List (Pair × Scenario × Bool) :=
  let setPair : Pair := ⟨"set", "AbstractSet", "int", "type_set_abstract_set"⟩
  let dictPair : Pair := ⟨"dict", "Mapping", "str, int", "type_dict_mapping"⟩
  let control : Scenario :=
    ⟨"audit-control", .builtin, fun _ a => s!"def record(value: {a}):\n    pass\n", "record", false⟩
  let moduleShadow (custom : String) : Scenario :=
    ⟨"audit-module-shadow", .shadowed,
      fun n a => s!"{n} = {custom}\ndef record(value: {a}):\n    pass\n", "record", false⟩
  let typeParameter : Scenario :=
    ⟨"audit-type-parameter", .shadowed,
      fun n a => s!"def record[{n}](value: {a}):\n    pass\n", "record", true⟩
  [false, true].map (fun reverse => (setPair, moduleShadow "frozenset", reverse)) ++
  [false, true].map (fun reverse => (dictPair, control, reverse)) ++
  [false, true].map (fun reverse => (dictPair, moduleShadow "tuple", reverse)) ++
  [(dictPair, typeParameter, true)]

def corpus : String := String.join <|
  (pairs.flatMap fun p => scenarios.flatMap fun s =>
    [false, true].map (renderCase p s)) ++
  (auditCases.map fun (p, s, reverse) => renderCase p s reverse)

end CollectionAnnotation

def main (args : List String) : IO Unit := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--check", path] =>
    unless (← IO.FS.readFile path) == CollectionAnnotation.corpus do
      throw <| IO.userError "collection annotation corpus is stale"
  | ["--output", path] => IO.FS.writeFile path CollectionAnnotation.corpus
  | ["--sensitivity"] =>
    let detected := ([.builtin, .shadowed, .unknown] : List CollectionAnnotation.Provenance).filter
      fun p => CollectionAnnotation.allowed p != CollectionAnnotation.brokenAllowed p
    unless detected.length == 2 do
      throw <| IO.userError "broken spelling-only gate was not detected"
    IO.println "broken gate detected in 2/3 provenance states (12/18 pair-direction cases)"
  | [] => IO.print CollectionAnnotation.corpus
  | _ => throw <| IO.userError "expected --output PATH, --check PATH or --sensitivity"
