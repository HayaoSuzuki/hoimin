import AnnotationModel
import Lean.Data.Json
open AnnotationAudit Lean

def events : List Event := [.rebind, .restore, .observe]
def traces : Nat → List (List Event)
  | 0 => [[]]
  | n + 1 => events.flatMap fun e => (traces n).map (e :: ·)

def eventSource : Event → String
  | .rebind => "Sequence = set\n"
  | .restore => "from typing import Sequence\n"
  | .observe => "f.__annotations__\n"

def row (id source operator : String) (pairs : List (String × String)) : Json :=
  Json.mkObj [("schema", toJson (1 : Nat)), ("id", toJson id), ("mode", toJson "strict"),
    ("source", toJson source), ("operator", toJson operator), ("pairs", toJson pairs)]

def delayedCases : List (String × List Event) := [
  ("late_source", [.rebind, .observe]),
  ("cached_before_rebind", [.observe, .rebind]),
  ("restored_before_observe", [.rebind, .restore, .observe]),
  ("stable_source", [.observe])]

def delayedRows : List Json := delayedCases.map fun (id, es) =>
  let source := "from typing import Sequence\ndef f(x: Sequence[int]): pass\n" ++
    String.join (es.map eventSource) ++ "observed=f.__annotations__['x']\n"
  row id source "type_list_sequence"
    (if value (run initial es) then [("Sequence[int]", "list[int]")] else [])

def furtherDelayedRows : List Json := [
  row "late_destination"
    "from typing import Sequence\ndef f(x: list[int]): pass\nSequence = set\nobserved=f.__annotations__['x']\n"
    "type_list_sequence" (if value (run initial [.rebind, .observe]) then [("list[int]", "Sequence[int]")] else []),
  row "late_class_source"
    "from typing import Sequence\nclass C:\n    def f(x: Sequence[int]): pass\n    Sequence = set\nobserved=C.f.__annotations__['x']\n"
    "type_list_sequence" (if value (run initial [.rebind, .observe]) then [("Sequence[int]", "list[int]")] else [])]

def memberName : Member → String
  | .abstractSet => "AbstractSet"
  | .set => "Set"

def setRows : List Json := [
  row "abc_module" "import collections.abc as abc\ndef f(x: set[int]): pass\nobserved=f.__annotations__['x']\n"
    "type_set_abstract_set" [("set[int]", "abc." ++ memberName (destination .abc) ++ "[int]")],
  row "abc_direct" "from collections.abc import Set\ndef f(x: set[int]): pass\nobserved=f.__annotations__['x']\n"
    "type_set_abstract_set" [("set[int]", memberName (destination .abc) ++ "[int]")],
  row "abc_inverse" "from collections.abc import Set\ndef f(x: Set[int]): pass\nobserved=f.__annotations__['x']\n"
    "type_set_abstract_set" [(memberName (destination .abc) ++ "[int]", "set[int]")],
  row "typing_module" "import typing as t\ndef f(x: set[int]): pass\nobserved=f.__annotations__['x']\n"
    "type_set_abstract_set" [("set[int]", "t." ++ memberName (destination .typing) ++ "[int]")]]

def corpus := String.join ((delayedRows ++ furtherDelayedRows ++ setRows).map fun r => r.compress ++ "\n")

def main (args : List String) : IO Unit := do
  for depth in List.range 5 do
    let start ← IO.monoMsNow
    let domain := (List.range (depth + 1)).flatMap traces
    -- Observe once at the end so every trace has a public value to compare.
    let bad := domain.filter fun es => value (run initial (es ++ [.observe])) != brokenSnapshot es
    let elapsed := (← IO.monoMsNow) - start
    IO.println s!"depth={depth} alphabet=3 traces={domain.length} events={domain.foldl (fun n es => n + es.length) 0} stale_snapshot={bad.length} first={repr (bad.head?)} elapsed_ms={elapsed}"
  unless value (run initial [.rebind, .observe]) != brokenSnapshot [.rebind, .observe] do
    throw (IO.userError "stale snapshot not detected")
  unless value (run initial [.observe, .rebind]) != brokenCurrentOnly (run initial [.observe, .rebind]) do
    throw (IO.userError "lost cache not detected")
  unless value (run initial [.observe]) do throw (IO.userError "reject-all not detected")
  unless existsIn .abc .abstractSet != existsIn .abc (destination .abc) do
    throw (IO.userError "universal AbstractSet spelling not detected")
  for p in [Provider.typing, .abc] do
    unless existsIn p (destination p) do throw (IO.userError "invalid destination")
  unless ([Provider.typing, .abc].flatMap fun p => [Member.abstractSet, .set].map (existsIn p)) ==
      [true, true, false, true] do throw (IO.userError "provider member table changed")
  IO.println "sensitivity: stale-snapshot, lost-cache, reject-all, universal-AbstractSet detected; providers=2 members=2"
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] => unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale corpus")
  | _ => throw (IO.userError "usage: --output PATH | --check PATH")
