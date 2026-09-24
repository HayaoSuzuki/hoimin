import HoiminOracle.NullableGateModel
import Lean.Data.Json
open NullableGate Lean

structure Fixture where
  id : String
  source : String
  annotation : String
  tree : Tree
  trusted : Bool := true

def good := Tree.atom true
def bad := Tree.atom false
def fixtures : List Fixture := [
  ⟨"shadow_int_number", "int=7\nx:int\n", "int", good, false⟩,
  ⟨"shadow_str_number", "str=7\nx:str\n", "str", good, false⟩,
  ⟨"shadow_int_class", "class Meta(type):\n def __or__(cls, other): raise RuntimeError('custom union')\nclass int(metaclass=Meta): pass\nx:int\n", "int", good, false⟩,
  ⟨"shadow_list_mapping", "list={int:7}\nx:list[int]\n", "list[int]", .one good, false⟩,
  ⟨"dict_any", "from typing import Any\nx:dict[str,Any]\n", "dict[str,Any]", .pair good bad, true⟩,
  ⟨"dict_object", "x:dict[str,object]\n", "dict[str,object]", .pair good bad, true⟩,
  ⟨"dict_typevar", "from typing import TypeVar\nT=TypeVar('T')\nx:dict[str,T]\n", "dict[str,T]", .pair good bad, true⟩,
  ⟨"dict_forward", "x:dict[str,\"Foo\"]\n", "dict[str,\"Foo\"]", .pair good bad, true⟩,
  ⟨"dict_annotated", "from typing import Annotated\nx:dict[str,Annotated[int,'tag']]\n", "dict[str,Annotated[int,'tag']]", .pair good bad, true⟩,
  ⟨"dict_callable", "from typing import Callable\nx:dict[str,Callable[[int],str]]\n", "dict[str,Callable[[int],str]]", .pair good bad, true⟩,
  ⟨"nested_list_dict_any", "from typing import Any\nx:list[dict[str,Any]]\n", "list[dict[str,Any]]", .one (.pair good bad), true⟩,
  ⟨"list_any", "from typing import Any\nx:list[Any]\n", "list[Any]", .one bad, true⟩,
  ⟨"normal_int", "x:int\n", "int", good, true⟩,
  ⟨"normal_list", "x:list[int]\n", "list[int]", .one good, true⟩,
  ⟨"normal_dict", "x:dict[str,int]\n", "dict[str,int]", .pair good good, true⟩]

-- These sources are declared here; generated expectations stay in the model.
def blockedLeaves : List (String × String × String) := [
  ("any_alias", "from typing import Any as A\n", "A"),
  ("qualified_any", "import typing as t\n", "t.Any"),
  ("object", "", "object"),
  ("typevar_alias", "from typing import TypeVar as TV\nT=TV('T')\n", "T"),
  ("forward", "", "'Foo'"),
  ("annotated_alias", "from typing import Annotated as Ann\n", "Ann[int,'tag']"),
  ("callable_alias", "from typing import Callable as Call\n", "Call[[int],str]"),
  ("literal_alias", "from typing import Literal as Lit\n", "Lit[1]"),
  ("protocol_alias", "from typing import Protocol as Proto\n", "Proto")]

def descendantFixtures : List Fixture := blockedLeaves.flatMap fun (id, preamble, leaf) => [
  ⟨id ++ "_key", preamble ++ "x:dict[" ++ leaf ++ ",int]\n",
    "dict[" ++ leaf ++ ",int]", .pair bad good, true⟩,
  ⟨id ++ "_value", preamble ++ "x:dict[str," ++ leaf ++ "]\n",
    "dict[str," ++ leaf ++ "]", .pair good bad, true⟩,
  ⟨id ++ "_deep", preamble ++ "x:list[dict[str,list[dict[int," ++ leaf ++ "]]]]\n",
    "list[dict[str,list[dict[int," ++ leaf ++ "]]]]", .one (.pair good (.one (.pair good bad))), true⟩]

def cleanAndWideFixtures : List Fixture := [
  ⟨"third_blocked", "from typing import Any\nx:list[tuple[int,str,Any]]\n",
    "list[tuple[int,str,Any]]", .one (.pair good (.pair good bad)), true⟩,
  ⟨"third_clean", "x:list[tuple[int,str,bytes]]\n",
    "list[tuple[int,str,bytes]]", .one (.pair good (.pair good good)), true⟩,
  ⟨"deep_clean", "x:list[dict[str,list[dict[int,bytes]]]]\n",
    "list[dict[str,list[dict[int,bytes]]]]", .one (.pair good (.one (.pair good good))), true⟩,
  ⟨"mapping_alias_clean", "from typing import Mapping as M\nx:M[str,int]\n",
    "M[str,int]", .pair good good, true⟩,
  ⟨"qualified_mapping_clean", "import typing as t\nx:t.Mapping[str,int]\n",
    "t.Mapping[str,int]", .pair good good, true⟩]

def unpackedFixtures : List Fixture :=
  [("Any", bad), ("int", good)].flatMap fun (leaf, tree) =>
    [("star_tuple", "dict[*(str," ++ leaf ++ ")]"),
     ("star_list", "dict[*[str," ++ leaf ++ "]]"),
     ("list_argument", "list[[" ++ leaf ++ "]]")].map fun (id, annotation) =>
      ⟨id ++ "_" ++ leaf, "from typing import Any\nx:" ++ annotation ++ "\n",
        annotation, .one (.pair good tree), true⟩

def operatorFixtures : List (String × String × Fixture) :=
  let preamble := "from typing import Any, Optional, Sequence, AbstractSet, Mapping, Iterable, Iterator\n"
  let shapes : List (String × String × String) := [
    ("type_nullable_remove", "Optional[dict[str,LEAF]]", "dict[str,LEAF]"),
    ("type_list_sequence", "list[dict[str,LEAF]]", "Sequence[dict[str,LEAF]]"),
    ("type_set_abstract_set", "set[tuple[str,LEAF]]", "AbstractSet[tuple[str,LEAF]]"),
    ("type_dict_mapping", "dict[str,LEAF]", "Mapping[str,LEAF]"),
    ("type_iterable_iterator", "Iterable[dict[str,LEAF]]", "Iterator[dict[str,LEAF]]"),
    ("type_sequence_iterable", "Sequence[dict[str,LEAF]]", "Iterable[dict[str,LEAF]]")]
  shapes.flatMap fun (operator, shape, replacement) =>
    [("Any", bad), ("int", good)].map fun (leaf, tree) =>
      let annotation := shape.replace "LEAF" leaf
      (operator, replacement.replace "LEAF" leaf,
        ⟨operator ++ "_" ++ leaf, preamble ++ "x:" ++ annotation ++ "\n",
          annotation, .one (.pair good tree), true⟩)

def renderFixture (operator replacement : String) (f : Fixture) : String :=
  (Json.mkObj [("schema", toJson (1 : Nat)), ("id", toJson f.id),
    ("mode", toJson (if f.trusted then "strict" else "report-only")),
    ("source", toJson f.source), ("operator", toJson operator),
    ("pairs", toJson (if eligible f.trusted f.tree then [(f.annotation, replacement)] else []))]).compress ++ "\n"

def corpus :=
  String.join ((fixtures ++ descendantFixtures ++ cleanAndWideFixtures ++ unpackedFixtures).map fun f =>
    renderFixture "type_nullable_add" (f.annotation ++ " | None") f) ++
  String.join (operatorFixtures.map fun (operator, replacement, f) => renderFixture operator replacement f)

def trees : Nat → List Tree
  | 0 => [good, bad]
  | n + 1 =>
    let children := trees n
    [good, bad] ++ children.map Tree.one ++ children.flatMap (fun l => children.map (Tree.pair l))

def nodes : Tree → Nat
  | .atom _ => 1
  | .one child => 1 + nodes child
  | .pair l r => 1 + nodes l + nodes r

def checkSensitivity : IO Unit := do
  for depth in [0, 1, 2] do
    let started ← IO.monoMsNow
    let domain := trees depth
    let tupleBad := domain.filter fun t => clean t != brokenTuple t
    let leftBad := domain.filter fun t => clean t != brokenLeftOnly t
    let nameBad := domain.filter fun t => eligible false t != brokenName false t
    IO.println s!"depth={depth} constructors=4 trees={domain.length} trusted_values=2 eligibility_cases={domain.length * 2} node_visits_per_pass={domain.foldl (fun n t => n + nodes t) 0} tuple_mismatches={tupleBad.length} left_only_mismatches={leftBad.length} name_mismatches={nameBad.length} elapsed_ms={(← IO.monoMsNow) - started}"
  unless clean (.pair good bad) != brokenTuple (.pair good bad) do throw (IO.userError "tuple-child bypass undetected")
  unless clean (.pair good bad) != brokenLeftOnly (.pair good bad) do throw (IO.userError "left-only recursion undetected")
  unless eligible false good != brokenName false good do throw (IO.userError "name-resolution bypass undetected")
  unless eligible true (.pair good good) do throw (IO.userError "reject-all tuple rule undetected")
  IO.println "witness tuple: pair(good,bad), expected=false, broken=true; witness name: trusted=false/atom good, expected=false, broken=true"
  IO.println "sensitivity: tuple-child bypass, left-only recursion, name-resolution bypass, reject-all detected"

def main (args : List String) : IO Unit := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] => unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale corpus")
  | ["--sensitivity"] => checkSensitivity
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity")
