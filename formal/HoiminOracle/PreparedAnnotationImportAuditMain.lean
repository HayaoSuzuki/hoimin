import HoiminOracle.PreparedAnnotationImportModel
import Lean.Data.Json

open HoiminOracle.PreparedAnnotationImport Lean

namespace PreparedAnnotationImportAudit

structure Fixture where
  id : String
  source : String
  sourceVisible : Bool
  destinationVisible : Bool
  ordinary : Bool := false
  injectSource : Bool := false
  injectDestination : Bool := false
  symbol : Option String := some "Subject"
  original : String := "Sequence[int]"
  replacement : String := "Iterable[int]"
  runCheck : Bool := false

def preamble (a b : Bool) : String :=
  "from typing import Sequence, Iterable\nclass Shadow:\n @classmethod\n def __class_getitem__(cls, value): return str\nclass Meta(type):\n @classmethod\n def __prepare__(mcls, name, bases):\n  return {" ++
  (if a then "'Sequence': Shadow," else "") ++ (if b then "'Iterable': Shadow," else "") ++ "}\n"
def fixture (ctx : String) (a b : Bool) : String :=
  preamble a b ++ (match ctx with
  | "module" => "class Subject(metaclass=Meta): pass\nvalue: Sequence[int] # target\nprobe: tuple[Sequence, Iterable]\nimport annotationlib, sys\npair = annotationlib.get_annotations(sys.modules[__name__])['probe'].__args__\nobserved = (pair[0] is Sequence, pair[1] is Iterable)\n"
  | "class" => "class Subject(metaclass=Meta):\n value: Sequence[int] # target\n probe: tuple[Sequence, Iterable]\npair = Subject.__annotations__['probe'].__args__\nobserved = (pair[0] is Sequence, pair[1] is Iterable)\n"
  | "method" => "class Subject(metaclass=Meta):\n def method(value: Sequence[int], probe: tuple[Sequence, Iterable]): pass # target\npair = Subject.method.__annotations__['probe'].__args__\nobserved = (pair[0] is Sequence, pair[1] is Iterable)\n"
  | _ => "class Subject(metaclass=Meta):\n def factory():\n  def inner(value: Sequence[int], probe: tuple[Sequence, Iterable]): pass # target\n  return inner\npair = Subject.factory().__annotations__['probe'].__args__\nobserved = (pair[0] is Sequence, pair[1] is Iterable)\n")

def matrix : List Fixture := ["module", "class", "method", "lexical"].flatMap fun ctx =>
  [false, true].flatMap fun a => [false, true].map fun b =>
    let visible := ctx == "class" || ctx == "method"
    { id := s!"{ctx}-{a}-{b}", source := fixture ctx a b,
      sourceVisible := visible, destinationVisible := visible,
      injectSource := a, injectDestination := b,
      symbol := if ctx == "module" then none else if ctx == "lexical" then some "Subject.factory.inner" else some "Subject" }

def probe : String := " probe: tuple[Sequence, Iterable]\npair = Subject.__annotations__['probe'].__args__\nobserved = (pair[0] is Sequence, pair[1] is Iterable)\n"

def classSource (directive : String) : String :=
  preamble true true ++ "class Subject(metaclass=Meta):\n" ++ directive ++ " value: Sequence[int] # target\n" ++ probe

def controls : List Fixture := [
  { id := "plain", source := "from typing import Sequence, Iterable\nclass Subject:\n value: Sequence[int] # target\n" ++ probe,
    sourceVisible := true, destinationVisible := true, ordinary := true },
  { id := "global-both", source := classSource " global Sequence, Iterable\n",
    sourceVisible := false, destinationVisible := false, injectSource := true, injectDestination := true },
  { id := "global-source", source := classSource " global Sequence\n",
    sourceVisible := false, destinationVisible := true, injectSource := true, injectDestination := true },
  { id := "global-destination", source := classSource " global Iterable\n",
    sourceVisible := true, destinationVisible := false, injectSource := true, injectDestination := true },
  { id := "explicit-import", source := classSource " from typing import Sequence, Iterable\n",
    sourceVisible := true, destinationVisible := true },
  { id := "nonlocal", source := preamble true true ++ "def outer():\n from typing import Sequence, Iterable\n class Subject(metaclass=Meta):\n  nonlocal Sequence, Iterable\n  value: Sequence[int] # target\n  probe: tuple[Sequence, Iterable]\n return Subject\nSubject = outer()\npair = Subject.__annotations__['probe'].__args__\nobserved = (pair[0] is Sequence, pair[1] is Iterable)\n",
    sourceVisible := true, destinationVisible := true, injectSource := true, injectDestination := true, symbol := some "outer.Subject" },
  { id := "generic-method", source := preamble false true ++ "class Subject(metaclass=Meta):\n def method[T](value: Sequence[int], probe: tuple[Sequence, Iterable]): pass # target\npair = Subject.method.__annotations__['probe'].__args__\nobserved = (pair[0] is Sequence, pair[1] is Iterable)\n",
    sourceVisible := true, destinationVisible := true, injectDestination := true },
  { id := "integer-destination", source := "from typing import Sequence, Iterable\nclass Meta(type):\n @classmethod\n def __prepare__(mcls, name, bases): return {'Iterable': 0}\nclass Subject(metaclass=Meta):\n value: Sequence[int] # target\n" ++ probe ++ "result = Subject.__annotations__['value'] == Sequence[int]\n",
    sourceVisible := true, destinationVisible := true, injectDestination := true, runCheck := true }
]

def aliasFixture (globalAlias : Bool) : Fixture :=
  { id := if globalAlias then "qualified-global" else "qualified-prepared",
    source := "import typing as t\nclass Shadow:\n @classmethod\n def __class_getitem__(cls, value): return str\nclass Alias:\n Sequence = Shadow\n Iterable = Shadow\nclass Meta(type):\n @classmethod\n def __prepare__(mcls, name, bases): return {'t': Alias}\nclass Subject(metaclass=Meta):\n" ++
      (if globalAlias then " global t\n" else "") ++
      " value: t.Sequence[int] # target\n probe: tuple[t.Sequence, t.Iterable]\npair = Subject.__annotations__['probe'].__args__\nobserved = (pair[0] is t.Sequence, pair[1] is t.Iterable)\n",
    sourceVisible := !globalAlias, destinationVisible := !globalAlias,
    injectSource := true, injectDestination := true,
    original := "t.Sequence[int]", replacement := "t.Iterable[int]" }

def fixtures : List Fixture := matrix ++ controls ++ [aliasFixture false, aliasFixture true]

def render (f : Fixture) : String :=
  (Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson f.id), ("mode", toJson ("strict" : String)),
    ("source", toJson f.source), ("original", toJson f.original),
    ("replacement", toJson f.replacement), ("symbol", toJson f.symbol),
    ("runtime_identity", toJson [identity f.sourceVisible f.injectSource, identity f.destinationVisible f.injectDestination]),
    ("runtime_allowed", toJson (allowed f.sourceVisible f.destinationVisible f.injectSource f.injectDestination)),
    ("candidate_count", toJson (if eligible f.sourceVisible f.destinationVisible f.ordinary then (1 : Nat) else 0)),
    ("run_check", toJson f.runCheck)]).compress ++ "\n"

def corpus : String := String.join (fixtures.map render)

def main (args : List String) : IO Unit := do
  unless sensitivity do throw (IO.userError "prepared annotation import sensitivity failed")
  unless fixtures.all (fun f => !f.ordinary || (!f.injectSource && !f.injectDestination)) do
    throw (IO.userError "fixture violates ordinary namespace premise")
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] =>
    unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale prepared annotation import corpus")
  | ["--sensitivity"] => IO.println "source_only=true destination_only=true lexical_capture=true"
  | ["--stats"] => IO.println s!"fixed_cases={fixtures.length} historical_cases=16 transitions=0 search_depth=0 sensitivity_families=3"
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity | --stats")

end PreparedAnnotationImportAudit

def main := PreparedAnnotationImportAudit.main
