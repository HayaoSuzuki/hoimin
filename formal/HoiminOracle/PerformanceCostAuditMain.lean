import HoiminOracle.PerformanceCostCases
import Lean

open HoiminOracle.PerformanceCost

def effectString : Effect → String
  | .bind => "bind"
  | .maybeBind => "maybe-bind"
  | .unknown => "unknown"

def resolutionString : HoiminOracle.BindingFlow.Fact → String
  | .known .builtin => "builtin"
  | .shadowed => "shadowed"
  | _ => "unknown"

def caseJson (c : Case) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("id", .str c.id), ("family", .str c.family),
  ("events", .arr (c.events.map fun e => Lean.Json.mkObj
    [("offset", Lean.toJson e.offset), ("effect", .str (effectString e.effect))]).toArray),
  ("queries", .arr (c.queries.map fun q => Lean.Json.mkObj
    [("offset", Lean.toJson q), ("expected", .str (resolutionString (resolve c.events q)))]).toArray),
  ("build_updates", Lean.toJson (buildUpdates c.events.length)),
  ("query_bound", Lean.toJson (queryBound (uniqueOffsets c.events) c.queries.length)),
  ("linear_visits", Lean.toJson (linearVisits c.events.length c.queries.length)),
  ("aliases", Lean.toJson c.aliases), ("annotations", Lean.toJson c.annotations),
  ("selected", Lean.toJson c.selected), ("source", .str c.source),
  ("operator", .str c.operator), ("limit", Lean.toJson c.limit),
  ("candidates", Lean.toJson c.candidates), ("truncated", Lean.toJson c.truncated),
  ("clone_calls", Lean.toJson c.cloneCalls), ("clone_entries", Lean.toJson c.cloneEntries),
  ("replacement_builds", Lean.toJson c.replacementBuilds),
  ("replacement_bytes", Lean.toJson c.replacementBytes)]

def render : String := String.join (cases.map fun c => (caseJson c).compress ++ "\n")

def main (args : List String) : IO UInt32 := do
  unless sensitivity do
    IO.eprintln "broken cost sensitivity failed"
    return 2
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path render; return 0
  | ["--check", path] => return if (← IO.FS.readFile path) == render then 0 else 1
  | ["--sensitivity"] | ["--stats"] =>
      IO.println s!"cases={cases.length} sizes=0,1,7,8,9,16,32 effects=3 max_events=32 max_queries=34 witness_search=0..32 minimal_scan=3 minimal_clone=1 minimal_eager_bytes=1 sensitivity={sensitivity}"
      return 0
  | _ => IO.eprintln "use --output PATH, --check PATH, --sensitivity or --stats"; return 2
