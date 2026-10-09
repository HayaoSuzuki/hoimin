import HoiminOracle.SamplingModel
import Lean.Data.Json
open Lean HoiminOracle.Sampling

-- Small semantic boundary domain: zero, one, partial, exact and oversized;
-- budget below/equal/above population; both completeness states and extreme seeds.
def seeds : List UInt64 := [0, 1, 42, 0xffffffffffffffff]
def cases : List (Nat × Nat × Nat × Bool × UInt64) :=
  [0,1,8].flatMap fun p => [0,1,3,8,99].flatMap fun k =>
    [1,2,8].flatMap fun b => [false,true].flatMap fun t =>
      seeds.map fun s => (p,k,b,t,s)

def render (input : Nat × Nat × Nat × Bool × UInt64) : IO String := do
  let (p,k,b,t,s) := input
  let ok := accepts p k b t
  let ids ← if ok then
    match sample p k s with
    | some ids => pure ids
    | none => throw (IO.userError "draw fuel exhausted: no valid expectation")
    else pure []
  return (Json.mkObj [("population",toJson p),("count",toJson k),
    ("budget",toJson b),("truncated",toJson t),("seed",toJson s.toNat),
    ("accepted",toJson ok),("indices",toJson ids),("mode",toJson "strict")]).compress ++ "\n"

def main (args : List String) : IO Unit := do
  unless sensitivity do throw (IO.userError "sampling sensitivity failed")
  let corpus := String.join (← cases.mapM render)
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output",path] => IO.FS.writeFile path corpus
  | ["--check",path] =>
    unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale sampling corpus")
  | ["--stats"] => IO.println s!"cases={cases.length} populations=0,1,8 counts=0,1,3,8,99 budgets=1,2,8 seeds=4 draw_fuel=128"
  | ["--sensitivity"] => IO.println "replacement=true truncation=true budget=true modulo_bias=true"
  | _ => throw (IO.userError "--output PATH | --check PATH | --stats | --sensitivity")
