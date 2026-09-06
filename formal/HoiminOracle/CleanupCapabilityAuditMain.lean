import HoiminOracle.CleanupCapabilityCases
import Lean.Data.Json

open HoiminOracle.CleanupCapability

namespace HoiminOracle.CleanupCapability.Executable

private def eventFamily : List Event := [.inspect, .swap, .bind, .effect]

private def tracesExactly : Nat → List (List Event)
  | 0 => [[]]
  | depth + 1 =>
      (tracesExactly depth).flatMap fun trace =>
        eventFamily.map fun event => trace ++ [event]

private def tracesThrough (depth : Nat) : List (List Event) :=
  (List.range (depth + 1)).flatMap tracesExactly

private def boundedDepth : Nat := 4

private def boundedTraces : List (List Event) := tracesThrough boundedDepth

private def firstBrokenWitness (strategy : Strategy) : Option (List Event) :=
  boundedTraces.find? fun trace => (brokenRun strategy trace).outsideWritable

private def strategyName : Strategy → String
  | .retained => "retained"
  | .postInspection => "post-inspection"

private def modeName : CorrespondenceMode → String
  | .strict => "strict"
  | .internalFixture => "internal-fixture"
  | .modelOnly => "model-only"

private def phaseName : Phase → String
  | .idle => "idle"
  | .inspected => "inspected"
  | .bound => "bound"
  | .complete => "complete"
  | .rejected => "rejected"

private def eventName : Event → String
  | .inspect => "inspect"
  | .bind => "bind"
  | .swap => "swap"
  | .effect => "effect"

private def eventsJson (events : List Event) : Lean.Json :=
  .arr (events.toArray.map fun event => .str (eventName event))

private def caseJson (item : OracleCase) : Lean.Json :=
  Lean.Json.mkObj [
    ("schema", Lean.toJson item.schema),
    ("id", .str item.id),
    ("mode", .str (modeName item.mode)),
    ("scenario", .str item.scenario),
    ("strategy", .str (strategyName item.strategy)),
    ("schedule", eventsJson item.schedule),
    ("result", .str (phaseName item.expected.result)),
    ("outside_writable", Lean.toJson item.expected.outsideWritable)]

def renderCorpus : String :=
  String.join (fixedCases.map fun item => (caseJson item).compress ++ "\n")

private def expectedWitness : List Event := [.inspect, .swap, .bind, .effect]

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless boundedTraces.length == 341 do
    IO.eprintln s!"unexpected cleanup-capability trace count: {boundedTraces.length}"
    return .error 2
  unless caseIdsUnique do
    IO.eprintln "cleanup-capability case IDs are not unique"
    return .error 2
  unless corpusContractValid do
    IO.eprintln "cleanup-capability corpus mode/schema contract changed"
    return .error 2
  unless casesPass do
    IO.eprintln "one or more cleanup-capability cases disagree with the model"
    return .error 2
  unless sensitivityPasses do
    IO.eprintln "cleanup-capability broken transition escaped detection"
    return .error 2
  unless firstBrokenWitness .retained == some expectedWitness &&
      firstBrokenWitness .postInspection == some expectedWitness do
    IO.eprintln "cleanup-capability minimal broken witness changed"
    return .error 2
  return .ok ()

private def writeCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      if let some parent := path.parent then IO.FS.createDirAll parent
      let temporary := path.withExtension "jsonl.tmp"
      IO.FS.writeFile temporary renderCorpus
      IO.FS.rename temporary path
      return 0

private def checkCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      try
        let actual ← IO.FS.readFile path
        if actual == renderCorpus then return 0
        IO.eprintln s!"stale Lean cleanup-capability corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean cleanup-capability corpus {path}: {error}"
        return 1

private def printCases : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      for item in fixedCases do IO.println (caseJson item).compress
      return 0

private def witnessJson (strategy : Strategy) : Lean.Json :=
  let witness := firstBrokenWitness strategy
  Lean.Json.mkObj [
    ("claim", .str "permission effect remains capability-bound"),
    ("strategy", .str (strategyName strategy)),
    ("model_boundary", .str "one owned object, one outside object, and four abstract events"),
    ("bounded_depth", Lean.toJson boundedDepth),
    ("minimal_broken_witness", match witness with
      | none => .null
      | some events => eventsJson events),
    ("classification", .str (if witness.isSome then "detected" else "escaped")),
    ("idempotency", .str "repeated-event safety covered by the arbitrary-trace invariant"),
    ("boundary", .str "not-applicable"),
    ("precedence", .str "not-applicable")]

private def printSensitivity : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println (witnessJson .retained).compress
      IO.println (witnessJson .postInspection).compress
      return 0

private def readJson (path : System.FilePath) : IO Lean.Json := do
  try
    let raw ← IO.FS.readFile path
    match Lean.Json.parse raw with
    | .ok value => pure value
    | .error message => pure (Lean.Json.mkObj [("parse_error", .str message)])
  catch error => pure (Lean.Json.mkObj [("read_error", .str (toString error))])

private def printStats (resourceStats : System.FilePath) : IO UInt32 := do
  let started ← IO.monoNanosNow
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      let ended ← IO.monoNanosNow
      let guardJson ← readJson resourceStats
      IO.println (Lean.Json.mkObj [
        ("schema", Lean.toJson 1),
        ("bounded_depth", Lean.toJson boundedDepth),
        ("event_family_count", Lean.toJson eventFamily.length),
        ("raw_trace_count_per_strategy", Lean.toJson boundedTraces.length),
        ("checked_strategy_count", Lean.toJson 2),
        ("checked_trace_count", Lean.toJson (2 * boundedTraces.length)),
        ("fixed_case_count", Lean.toJson fixedCases.length),
        ("elapsed_ns", Lean.toJson (ended - started)),
        ("resource_guard", guardJson)] |>.compress)
      return 0

def main (args : List String) : IO UInt32 := do
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--stats", path] => printStats path
  | ["--cases"] => printCases
  | ["--sensitivity"] => printSensitivity
  | _ =>
      IO.eprintln "usage: generate_cleanup_capability (--output PATH | --check PATH | --stats RESOURCE_GUARD_JSON | --cases | --sensitivity)"
      return 2

end HoiminOracle.CleanupCapability.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.CleanupCapability.Executable.main args
