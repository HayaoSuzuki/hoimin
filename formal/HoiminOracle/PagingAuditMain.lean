import HoiminOracle.PagingModel
import Lean.Data.Json

open Lean HoiminOracle.Paging

namespace HoiminOracle.Paging.Executable

def inputs : List (Bool × Nat × Nat) :=
  [false, true].flatMap (fun diverse =>
    (List.range 7).flatMap (fun offset =>
      [1, 2, 4, 8].map (fun count => (diverse, offset, count)))) ++
  [(false, 0, 0), (false, 2^64-1, 1), (false, 1, 2^64-1)]

def render (input : Bool × Nat × Nat) : String :=
  let (diverse, offset, count) := input
  let policy := if diverse then "diverse" else "strict"
  let selected := page (ordering diverse) offset count
  let accepted := count > 0 && offset < ranked.length
  (Json.mkObj [
    ("schema", toJson (1 : Nat)), ("id", toJson s!"{policy}-{offset}-{count}"),
    ("mode", toJson "strict"), ("policy", toJson policy),
    ("offset", toJson offset), ("count", toJson count), ("accepted", toJson accepted),
    ("exit", toJson (if accepted then (0 : Nat) else 2)),
    ("expected_ids", toJson (if accepted then selected else [])),
    ("full_order", toJson (ordering diverse))]).compress ++ "\n"

def corpus : String := String.join (inputs.map render)

def main (args : List String) : IO Unit := do
  unless sensitivity do throw (IO.userError "paging sensitivity failed")
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] =>
    unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale paging corpus")
  | ["--sensitivity"] => IO.println "offset_before_diversity=true take_before_skip=true ignored_tiers=true"
  | ["--stats"] => IO.println s!"fixed_cases={inputs.length} candidates=6 paths=2 score_tiers=2 search_depth=0 sensitivity_families=3"
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity | --stats")

end HoiminOracle.Paging.Executable

def main := HoiminOracle.Paging.Executable.main
