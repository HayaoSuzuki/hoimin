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

def corpus := String.join (fixtures.map fun f =>
  (Json.mkObj [("schema", toJson (1 : Nat)), ("id", toJson f.id), ("mode", toJson "strict"),
    ("source", toJson f.source), ("operator", toJson "type_nullable_add"),
    ("pairs", toJson (if eligible f.trusted f.tree then [(f.annotation, f.annotation ++ " | None")] else []))]).compress ++ "\n")

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
    let nameBad := domain.filter fun t => eligible false t != brokenName false t
    IO.println s!"depth={depth} constructors=4 trees={domain.length} trusted_values=2 eligibility_cases={domain.length * 2} node_visits_per_pass={domain.foldl (fun n t => n + nodes t) 0} tuple_mismatches={tupleBad.length} name_mismatches={nameBad.length} elapsed_ms={(← IO.monoMsNow) - started}"
  unless clean (.pair good bad) != brokenTuple (.pair good bad) do throw (IO.userError "tuple-child bypass undetected")
  unless eligible false good != brokenName false good do throw (IO.userError "name-resolution bypass undetected")
  unless eligible true (.pair good good) do throw (IO.userError "reject-all tuple rule undetected")
  IO.println "witness tuple: pair(good,bad), expected=false, broken=true; witness name: trusted=false/atom good, expected=false, broken=true"
  IO.println "sensitivity: tuple-child bypass, name-resolution bypass, reject-all detected"

def main (args : List String) : IO Unit := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] => unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale corpus")
  | ["--sensitivity"] => checkSensitivity
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity")
