import HoiminOracle.EvaluationOrderModel
import Lean.Data.Json
open OrderAudit Lean

def alphabet : List Event := [.bindSource, .bindDestination, .lookup]
def traces : Nat → List (List Event)
  | 0 => [[]]
  | n + 1 => alphabet.flatMap fun e => (traces n).map (e :: ·)

def sourcePrelude := "def custom(values): return 'custom'\nslots = {}\n"
def cases : List (String × String × List Event) := [
  ("unpack_target", sourcePrelude ++ "any, slots[any([])] = custom, 7\nobserved=list(slots)\n", [.bindSource, .lookup]),
  ("chain_target", sourcePrelude ++ "any = slots[any([])] = custom\nobserved=list(slots)\n", [.bindSource, .lookup]),
  ("rhs_walrus", sourcePrelude ++ "slots[any([])] = (any := custom)\nobserved=list(slots)\n", [.bindSource, .lookup]),
  ("destination_target", sourcePrelude ++ "all, slots[any([])] = custom, 7\nobserved=list(slots)\n", [.bindDestination, .lookup]),
  ("keyword_star", sourcePrelude ++ "def sink(*args, **kwargs): return kwargs['flag']\nobserved=sink(flag=any([]), *[(any := custom)])\n", [.bindSource, .lookup]),
  ("rhs_before_store", sourcePrelude ++ "any, slots[0] = any([]), 7\nobserved=any\n", [.lookup, .bindSource]),
  ("normal_call", sourcePrelude ++ "observed=any([])\n", [.lookup])]

def corpus := String.join (cases.map fun (id, source, es) =>
  (Json.mkObj [("schema", toJson (1 : Nat)), ("id", toJson id), ("mode", toJson "strict"),
    ("source", toJson source), ("operator", toJson "collection_any_all"),
    ("pairs", toJson (if candidateCount es == 1 then [("any", "all")] else []))]).compress ++ "\n")

def checkSensitivity : IO Unit := do
  for depth in List.range 5 do
    let started ← IO.monoMsNow
    let domain := (List.range (depth + 1)).flatMap traces
    let bad := domain.filter fun es => observations true true es != brokenDeferredWrites es
    let elapsed := (← IO.monoMsNow) - started
    IO.println s!"depth={depth} alphabet=3 traces={domain.length} transitions={domain.foldl (fun n es => n + es.length) 0} mismatches={bad.length} first={repr (bad.head?)} elapsed_ms={elapsed}"
  unless observations true true [.bindSource, .lookup] != brokenDeferredWrites [.bindSource, .lookup] do
    throw (IO.userError "delayed source store undetected")
  unless observations true true [.bindDestination, .lookup] != brokenDeferredWrites [.bindDestination, .lookup] do
    throw (IO.userError "delayed destination store undetected")
  unless candidateCount [.lookup, .bindSource] == 1 do
    throw (IO.userError "premature store / reject-all undetected")
  IO.println "sensitivity: delayed source, delayed destination, premature store/reject-all detected"

def main (args : List String) : IO Unit := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] => unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale corpus")
  | ["--sensitivity"] => checkSensitivity
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity")
