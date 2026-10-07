import HoiminOracle.LineDiverseModel
import Lean.Data.Json

open Lean HoiminOracle.LineDiverse

namespace HoiminOracle.LineDiverse.Executable

def policyName : Policy → String
  | .strict => "strict"
  | .diverse => "diverse"
  | .lineDiverse => "line-diverse"

def inputs : List (Policy × Nat × Nat) :=
  [Policy.strict, .diverse, .lineDiverse].flatMap fun policy =>
    ((List.range 9).flatMap fun offset =>
      [1, 3, 20].map fun count => (policy, offset, count)) ++
    [(policy, 0, 0), (policy, 2^64-1, 1), (policy, 1, 2^64-1),
     (policy, 2^64-1, 2^64-1)]

def render (input : Policy × Nat × Nat) : String :=
  let (policy, offset, count) := input
  let name := policyName policy
  let fullOrder := ids (order policy fixture)
  let accepted := count > 0 && offset < fixture.length
  (Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson s!"{name}-{offset}-{count}"),
    ("mode", toJson "strict"), ("policy", toJson name),
    ("offset", toJson offset), ("count", toJson count),
    ("accepted", toJson accepted), ("exit", toJson (if accepted then (0 : Nat) else 2)),
    ("expected_ids", toJson (if accepted then page fullOrder offset count else [])),
    ("full_order", toJson fullOrder)]).compress ++ "\n"

def corpus : String := String.join (inputs.map render)

def main (args : List String) : IO Unit := do
  unless sensitivityChecks.all (·.2) do throw (IO.userError "line-diverse sensitivity failed")
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] =>
    unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale line-diverse corpus")
  | ["--sensitivity"] =>
    IO.println (String.intercalate " " (sensitivityChecks.map fun (name, ok) => s!"{name}={ok}"))
  | ["--stats"] =>
    IO.println s!"fixed_cases={inputs.length} candidates=8 paths=2 line_groups=5 score_tiers=2 policies=3 search_depth=0 sensitivity_families={sensitivityChecks.length}"
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity | --stats")

end HoiminOracle.LineDiverse.Executable

def main := HoiminOracle.LineDiverse.Executable.main
