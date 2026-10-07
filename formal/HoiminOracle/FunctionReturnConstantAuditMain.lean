import HoiminOracle.FunctionReturnConstantModel
import Lean.Data.Json

open Lean HoiminOracle.FunctionReturnConstant

namespace HoiminOracle.FunctionReturnConstant.Executable

def render (input : Input) : String :=
  (Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson input.id), ("mode", toJson "strict"),
    ("source", toJson (source input)),
    ("expected", toJson ((expected input).map fun (original, replacement) =>
      Json.mkObj [("original", toJson original), ("replacement", toJson replacement)]))]).compress ++ "\n"

def corpus : String := String.join (fixtures.map render)

def main (args : List String) : IO Unit := do
  unless sensitivityChecks.all (·.2) do throw (IO.userError "function-return-constant sensitivity failed")
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] =>
    unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale function-return-constant corpus")
  | ["--sensitivity"] =>
    IO.println (String.intercalate " " (sensitivityChecks.map fun (name, ok) => s!"{name}={ok}"))
  | ["--stats"] =>
    IO.println s!"fixed_cases={fixtures.length} positive_cases={(fixtures.filter fun input => !(choices input).isEmpty).length} expected_candidates={(fixtures.map fun input => (choices input).length).foldl (· + ·) 0} search_depth=0 sensitivity_families={sensitivityChecks.length}"
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity | --stats")

end HoiminOracle.FunctionReturnConstant.Executable

def main := HoiminOracle.FunctionReturnConstant.Executable.main
