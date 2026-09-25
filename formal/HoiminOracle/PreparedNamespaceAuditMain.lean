import HoiminOracle.PreparedNamespaceModel
import Lean.Data.Json

open HoiminOracle.PreparedNamespace Lean

namespace PreparedNamespaceAudit

structure Fixture where
  id : String
  source : String
  sourceVisible : Bool
  destinationVisible : Bool
  ordinary : Bool := false
  injectSource : Bool := false
  injectDestination : Bool := false
  symbol : Option String := some "Subject"
  operator : String := "collection_any_all"
  original : String := "any"
  replacement : String := "all"
  marker : String := "any([])"
  runCheck : Bool := false

def preamble (a b : Bool) : String :=
  "import builtins\nclass Meta(type):\n @classmethod\n def __prepare__(mcls, name, bases):\n  return {" ++
  (if a then "'any': lambda values: 'custom'," else "") ++
  (if b then "'all': lambda values: 'custom'," else "") ++ "}\n"

def classBody : String :=
  " observed = (any is builtins.any, all is builtins.all)\n result = any([])\n"

def exported : String := "observed = Subject.observed\nresult = Subject.result\n"

def matrix : List Fixture := ["module", "class", "method"].flatMap fun scope =>
  [false, true].flatMap fun a => [false, true].map fun b =>
    let id := s!"{scope}-{a}-{b}"
    let preambleExtra := preamble a b
    match scope with
    | "module" =>
      { id, source := preambleExtra ++ "class Subject(metaclass=Meta): pass\nobserved = (any is builtins.any, all is builtins.all)\nresult = any([])\n",
        sourceVisible := false, destinationVisible := false, injectSource := a, injectDestination := b, symbol := none }
    | "method" =>
      { id, source := preambleExtra ++ "class Subject(metaclass=Meta):\n @staticmethod\n def probe():\n  return (any is builtins.any, all is builtins.all), any([])\nobserved, result = Subject.probe()\n",
        sourceVisible := false, destinationVisible := false, injectSource := a, injectDestination := b, symbol := some "Subject.probe" }
    | _ =>
      { id, source := preambleExtra ++ "class Subject(metaclass=Meta):\n" ++ classBody ++ exported,
        sourceVisible := true, destinationVisible := true, injectSource := a, injectDestination := b, runCheck := a || b }

def classFixture (id header preambleExtra : String) (a b : Bool) : Fixture :=
  { id, source := preamble a b ++ preambleExtra ++ "class Subject" ++ header ++ ":\n" ++ classBody ++ exported,
    sourceVisible := true, destinationVisible := true, injectSource := a, injectDestination := b }

def controls : List Fixture := [
  { (classFixture "plain" "" "" false false) with ordinary := true },
  { (classFixture "empty-header" "()" "" false false) with ordinary := true },
  classFixture "object-base-conservative" "(object)" "" false false,
  classFixture "explicit-type-conservative" "(metaclass=type)" "" false false,
  classFixture "inherited" "(Base)" "class Base(metaclass=Meta): pass\n" true false,
  classFixture "aliased-metaclass" "(metaclass=Alias)" "Alias = Meta\n" false true,
  classFixture "starred-base" "(*bases)" "class Base(metaclass=Meta): pass\nbases = (Base,)\n" true true,
  classFixture "unpacked-keywords" "(**keywords)" "keywords = {'metaclass': Meta}\n" true false,
  classFixture "mro-entries" "(proxy)" "class Base(metaclass=Meta): pass\nclass Proxy:\n def __mro_entries__(self, bases): return (Base,)\nproxy = Proxy()\n" false true,
  { id := "global-both", source := preamble true true ++ "class Subject(metaclass=Meta):\n global any, all\n" ++ classBody ++ exported,
    sourceVisible := false, destinationVisible := false, injectSource := true, injectDestination := true },
  { id := "global-source-only", source := preamble true true ++ "class Subject(metaclass=Meta):\n global any\n" ++ classBody ++ exported,
    sourceVisible := false, destinationVisible := true, injectSource := true, injectDestination := true },
  { id := "global-destination-only", source := preamble true true ++ "class Subject(metaclass=Meta):\n global all\n" ++ classBody ++ exported,
    sourceVisible := true, destinationVisible := false, injectSource := true, injectDestination := true },
  { id := "closure", source := preamble true true ++ "class Subject(metaclass=Meta):\n def method():\n  def closure():\n   return (any is builtins.any, all is builtins.all), any([])\n  return closure()\nobserved, result = Subject.method()\n",
    sourceVisible := false, destinationVisible := false, injectSource := true, injectDestination := true, symbol := some "Subject.method.closure" },
  { id := "comprehension-body", source := preamble true true ++ "class Subject(metaclass=Meta):\n observed, result = [( (any is builtins.any, all is builtins.all), any([])) for _ in [0]][0]\n" ++ exported,
    sourceVisible := false, destinationVisible := false, injectSource := true, injectDestination := true },
  { id := "comprehension-first-iterable", source := preamble true true ++ "class Subject(metaclass=Meta):\n observed = (any is builtins.any, all is builtins.all)\n result = [x for x in [any([])]][0]\n" ++ exported,
    sourceVisible := true, destinationVisible := true, injectSource := true, injectDestination := true },
  { id := "method-default", source := preamble true false ++ "class Subject(metaclass=Meta):\n observed = (any is builtins.any, all is builtins.all)\n def method(value=any([])): return value\nobserved = Subject.observed\nresult = Subject.method()\n",
    sourceVisible := true, destinationVisible := true, injectSource := true },
  { id := "mapping-getitem", source := "import builtins\nclass Namespace(dict):\n def __getitem__(self, key):\n  if key == 'all': return lambda values: 'custom'\n  return super().__getitem__(key)\nclass Meta(type):\n @classmethod\n def __prepare__(mcls, name, bases): return Namespace()\nclass Subject(metaclass=Meta):\n" ++ classBody ++ exported,
    sourceVisible := true, destinationVisible := true, injectDestination := true },
  { id := "annotation-prepared", source := "import builtins\nclass Meta(type):\n @classmethod\n def __prepare__(mcls, name, bases): return {'int': str}\nclass Subject(metaclass=Meta):\n value: int\nobserved = (Subject.__annotations__['value'] is builtins.int, True)\n",
    sourceVisible := true, destinationVisible := false, injectSource := true,
    operator := "type_nullable_add", original := "int", replacement := "int | None", marker := "int\n" },
  { id := "annotation-plain", source := "import builtins\nclass Subject:\n value: int\nobserved = (Subject.__annotations__['value'] is builtins.int, True)\n",
    sourceVisible := true, destinationVisible := false, ordinary := true,
    operator := "type_nullable_add", original := "int", replacement := "int | None", marker := "int\n" }
]

def additionalControls : List Fixture := [
  { id := "nonlocal-prepared", source := preamble true false ++ "def outer():\n any = builtins.any\n class Subject(metaclass=Meta):\n  nonlocal any\n  observed = (any is builtins.any, all is builtins.all)\n  result = any([])\n return Subject.observed\nobserved = outer()\n",
    sourceVisible := true, destinationVisible := true, injectSource := true },
  { id := "annotation-global", source := "import builtins\nclass Meta(type):\n @classmethod\n def __prepare__(mcls, name, bases): return {'int': str}\nclass Subject(metaclass=Meta):\n global int\n value: int\nobserved = (Subject.__annotations__['value'] is builtins.int, True)\n",
    sourceVisible := false, destinationVisible := false, injectSource := true,
    operator := "type_nullable_add", original := "int", replacement := "int | None", marker := "int\nobserved", symbol := some "Subject" },
  { id := "annotation-method", source := "import builtins\nclass Meta(type):\n @classmethod\n def __prepare__(mcls, name, bases): return {'int': str}\nclass Subject(metaclass=Meta):\n def method(value: int): pass\nobserved = (Subject.method.__annotations__['value'] is builtins.int, True)\n",
    sourceVisible := true, destinationVisible := false, injectSource := true,
    operator := "type_nullable_add", original := "int", replacement := "int | None", marker := "int): pass" },
  { id := "class-header", source := "import builtins\nobserved = (any is builtins.any, all is builtins.all)\nclass Base: pass\ndef factory(value): return Base\nclass Subject[T](factory(any([]))): pass\n",
    sourceVisible := false, destinationVisible := false }
]

def exceptionFixtures : List Fixture := [false, true].flatMap fun a => [false, true].map fun b =>
  { id := s!"exception-{a}-{b}",
    source := "import builtins\nclass Meta(type):\n @classmethod\n def __prepare__(mcls, name, bases):\n  return {" ++
      (if a then "'ValueError': RuntimeError," else "") ++ (if b then "'TypeError': LookupError," else "") ++ "}\nclass Subject(metaclass=Meta):\n observed = (ValueError is builtins.ValueError, TypeError is builtins.TypeError)\n try: pass\n except ValueError: pass\nobserved = Subject.observed\n",
    sourceVisible := true, destinationVisible := true, injectSource := a, injectDestination := b,
    operator := "exception_type_pair", original := "ValueError", replacement := "TypeError", marker := "ValueError: pass" }

def fixtures : List Fixture := matrix ++ controls ++ additionalControls ++ exceptionFixtures

def render (f : Fixture) : String :=
  (Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson f.id), ("mode", toJson ("strict" : String)),
    ("source", toJson f.source), ("operator", toJson f.operator),
    ("original", toJson f.original), ("replacement", toJson f.replacement),
    ("marker", toJson f.marker), ("symbol", toJson f.symbol),
    ("runtime_builtin", toJson [runtimeBuiltin f.sourceVisible f.injectSource,
      runtimeBuiltin f.destinationVisible f.injectDestination]),
    ("candidate_count", toJson (if allowed f.sourceVisible f.destinationVisible f.ordinary then (1 : Nat) else 0)),
    ("run_check", toJson f.runCheck),
    ("runtime_result", if f.sourceVisible && f.injectSource then Json.str "custom" else Json.bool false)]).compress ++ "\n"

def corpus : String := String.join (fixtures.map render)

def main (args : List String) : IO Unit := do
  unless sensitivity do throw (IO.userError "prepared namespace sensitivity failed")
  -- Ordinary namespace fixtures cannot also configure injected endpoints.
  unless fixtures.all (fun f => !f.ordinary || (!f.injectSource && !f.injectDestination)) do
    throw (IO.userError "fixture violates ordinary namespace premise")
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] =>
    unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale prepared namespace corpus")
  | ["--sensitivity"] => IO.println "ignored_preparation=true source_only=true method_capture=true"
  | ["--stats"] => IO.println s!"fixed_cases={fixtures.length} historical_cases=12 transitions=0 search_depth=0 sensitivity_families=3"
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity | --stats")

end PreparedNamespaceAudit

def main := PreparedNamespaceAudit.main
