import HoiminOracle.AstDepthModel
import Lean.Data.Json

namespace HoiminOracle.AstDepth

private def append : Forest → Forest → Forest
  | .empty, rest => rest
  | .cons children tail, rest => .cons children (append tail rest)

private def node (children : List Forest) : Forest := .cons (children.foldr append .empty) .empty
private def leaf : Forest := node []

private def nest (layer : Forest → Forest) (seed : Forest) : Nat → Forest
  | 0 => seed
  | n + 1 => layer (nest layer seed n)

private def binary (links : Nat) : Forest := nest (fun inner => node [inner, leaf]) leaf links
private def indexed (links : Nat) : Forest := nest (fun inner => node [leaf, inner]) leaf links
private def repeated (count : Nat) (piece : String) : String := String.join (List.replicate count piece)
private def sumSource (links : Nat) : String := repeated links "1+" ++ "1"
private def indexSource (links : Nat) : String := repeated links "list[" ++ "int" ++ repeated links "]"

private structure Fixture where
  id : String
  source : String
  root : Forest
  valid : Bool := true

private def validModule (id source : String) (statements : List Forest) : Fixture :=
  ⟨id, source, node statements, true⟩

private def assignment (links : Nat) : Fixture :=
  validModule s!"assignment-{links}" ("value = " ++ sumSource links ++ "\n")
    [node [leaf, binary links]]

private def annotation (links : Nat) : Fixture :=
  validModule s!"annotation-{links}" ("value: " ++ indexSource links ++ "\n")
    [node [leaf, indexed links]]

private def parameter (links : Nat) : Fixture :=
  validModule s!"parameter-{links}" ("def f(a: " ++ indexSource links ++ " = 1):\n    pass\n")
    [node [leaf, node [node [node [leaf, indexed links], leaf]], leaf]]

private def genericBound (links : Nat) : Fixture :=
  validModule s!"type-bound-{links}" ("def f[T: " ++ indexSource links ++ "]():\n    pass\n")
    [node [leaf, node [node [leaf, indexed links]], node [], leaf]]

private def interpolation (kind : String) (links : Nat) : Fixture :=
  let source := "value = " ++ kind ++ "'雪{value:{" ++ sumSource links ++ "}}'\n"
  let expr := node [node [leaf, node [leaf, node [binary links]]]]
  validModule s!"{kind}-format-{links}" source [node [leaf, expr]]

private def pattern (links : Nat) : Fixture :=
  let source := "match value:\n    case " ++ repeated links "C(" ++ "_" ++ repeated links ")" ++ ":\n        pass\n"
  let nested := nest (fun inner => node [leaf, node [inner]]) leaf links
  validModule s!"class-pattern-{links}" source [node [leaf, node [nested, leaf]]]

private def statement (links : Nat) : Fixture :=
  let lines := (List.range links).map fun i => repeated i " " ++ "if True:\n"
  let source := String.join lines ++ repeated links " " ++ "pass\n"
  validModule s!"statement-{links}" source [nest (fun inner => node [leaf, inner]) leaf links]

private def wide : Fixture :=
  validModule "wide" (repeated 300 "value = 1\n") (List.replicate 300 (node [leaf, leaf]))

private def invalidAfter (links : Nat) : Fixture :=
  let original := assignment links
  { original with id := s!"invalid-after-{links}", source := original.source ++ "def broken(:\n", valid := false }

private def fixtures : List Fixture :=
  [validModule "empty" "" [], wide, invalidAfter 0, invalidAfter 127] ++
  [0, 1, 124, 125, 126, 127].flatMap (fun links => [assignment links, annotation links]) ++
  [0, 121, 122, 123].map parameter ++
  [0, 122, 123, 124].map genericBound ++
  [0, 120, 121, 122].flatMap (fun links => [interpolation "f" links, interpolation "t" links]) ++
  [0, 61, 62, 63].map pattern ++ [0, 125, 126, 127].map statement

private def limit := 128
private def schedule (fixture : Fixture) := visitOrder true 1 fixture.root

private def checkpoints (fixture : Fixture) : List (Option Nat) :=
  let normal := checked fixture.valid limit none (schedule fixture)
  none :: ([1, 2, 3, normal.polls - 1, normal.polls, normal.polls + 1].filter (0 < ·)).eraseDups.map some

private def resultName : Result → String
  | .accepted => "accepted"
  | .syntax => "syntax"
  | .cancelled => "cancelled"
  | .depth => "depth"
  | .outOfFuel => "out_of_fuel"

private def run (fixture : Fixture) (cancelAt : Option Nat) : Observation :=
  checked fixture.valid limit cancelAt (schedule fixture)

private def queryJson (fixture : Fixture) (cancelAt : Option Nat) : Lean.Json :=
  let observed := run fixture cancelAt
  Lean.Json.mkObj [("cancel_at", Lean.toJson cancelAt), ("result", .str (resultName observed.result)),
    ("polls", Lean.toJson observed.polls)]

private def fixtureJson (fixture : Fixture) : Lean.Json :=
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"),
    ("id", .str fixture.id), ("source", .str fixture.source),
    ("queries", .arr (((checkpoints fixture).map (queryJson fixture)).toArray))]

def corpus : String := String.join (fixtures.map fun fixture => (fixtureJson fixture).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (name, broken) in [
      ("ast_depth_visit_siblings_forward", fun item cancel =>
        checked item.valid limit cancel (visitOrder false 1 item.root)),
      ("ast_depth_start_module_at_zero", fun item cancel =>
        checked item.valid limit cancel (visitOrder true 0 item.root)),
      ("ast_depth_reject_exact_limit", fun item cancel =>
        checked item.valid (limit - 1) cancel (schedule item)),
      ("ast_depth_count_nodes_instead_of_depth", fun item cancel =>
        checked item.valid limit cancel ((List.range (schedule item).length).map (· + 1))),
      ("ast_depth_check_before_cancellation", fun item cancel =>
        let actual := run item cancel
        if actual.result == .cancelled && (schedule item)[actual.polls - 1]?.any (limit < ·)
        then { actual with result := .depth } else actual),
      ("ast_depth_traverse_invalid_syntax", fun item cancel => checked true limit cancel (schedule item))] do
    let witness := fixtures.findSome? fun item =>
      (checkpoints item).findSome? fun cancel =>
        if run item cancel != broken item cancel then some s!"{item.id} cancel={cancel}" else none
    match witness with
    | some detail => IO.println s!"family={name} detected=true case={detail}"
    | none =>
      IO.eprintln s!"AST depth corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.AstDepth
