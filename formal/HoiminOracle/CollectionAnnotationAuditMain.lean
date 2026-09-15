import Lean

namespace CollectionAnnotation

inductive Provenance where
  | builtin | shadowed | unknown
  deriving DecidableEq, Repr

def allowed (p : Provenance) : Bool := p == .builtin

theorem allowed_iff_builtin (p : Provenance) : allowed p = true ↔ p = .builtin := by
  cases p <;> simp [allowed]

-- A spelling-only gate is detected for both non-builtin states.
def brokenAllowed (_ : Provenance) : Bool := true
example : allowed .shadowed = false ∧ brokenAllowed .shadowed = true := by decide
example : allowed .unknown = false ∧ brokenAllowed .unknown = true := by decide

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

def corpus : String := String.join <| pairs.flatMap fun p => scenarios.flatMap fun s =>
  [false, true].map fun reverse =>
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
