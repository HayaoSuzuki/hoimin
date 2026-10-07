import HoiminOracle.MethodCallRemoveModel
import Lean.Data.Json

open Lean HoiminOracle.MethodCallRemove

namespace HoiminOracle.MethodCallRemove.Executable

def render (input : Input) : String :=
  (Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson input.id), ("mode", toJson "strict"),
    ("source", toJson (input.context.render (callSource input))),
    ("expected", toJson ((expected input).map fun (original, replacement) =>
      Json.mkObj [("original", toJson original), ("replacement", toJson replacement)]))]).compress ++ "\n"

def corpus : String := String.join (fixtures.map render)

def main (args : List String) : IO Unit := do
  unless sensitivityChecks.all (·.2) do throw (IO.userError "method-call-remove sensitivity failed")
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] =>
    unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale method-call-remove corpus")
  | ["--sensitivity"] =>
    IO.println (String.intercalate " " (sensitivityChecks.map fun (name, ok) => s!"{name}={ok}"))
  | ["--stats"] =>
    IO.println s!"fixed_cases={fixtures.length} positive_cases={(fixtures.filter eligible).length} search_depth=0 sensitivity_families={sensitivityChecks.length}"
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity | --stats")

end HoiminOracle.MethodCallRemove.Executable

def main := HoiminOracle.MethodCallRemove.Executable.main
