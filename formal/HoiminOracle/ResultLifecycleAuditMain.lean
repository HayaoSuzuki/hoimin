import HoiminOracle.ResultLifecycleCases
import Lean.Data.Json

open HoiminOracle.ResultLifecycle

namespace HoiminOracle.ResultLifecycle.Executable

def allMutants : List Mutant := [.m0, .m1]
def representativeStatuses : List Status := [.killed, .survived, .timeout, .error]

def eventAlphabet : List Event :=
  [.stop, .finishSession true, .finishSession false, .finishMetrics,
    .metricsFailed, .returnRun] ++
  allMutants.flatMap fun mutant =>
    [.discover mutant, .persistOk mutant, .persistFailed mutant, .recordResult mutant,
      .reportOk mutant, .reportFailed mutant, .markNotRun mutant] ++
    representativeStatuses.map fun status => .accept mutant status

structure Reachable where
  trace : List Event
  state : State
  deriving Repr, DecidableEq, BEq

def containsState (items : List Reachable) (state : State) : Bool :=
  items.any fun item => item.state == state

def successors (next : State → Event → Verdict) (item : Reachable) : List Reachable :=
  eventAlphabet.map fun event => {
    trace := item.trace ++ [event]
    state := (next item.state event).state
  }

def uniqueNewStates (seen candidates : List Reachable) : List Reachable :=
  candidates.foldl (fun retained candidate =>
    if containsState seen candidate.state || containsState retained candidate.state then
      retained
    else
      retained ++ [candidate]) []

def explorationLayersWith
    (next : State → Event → Verdict) :
    Nat → List Reachable → List Reachable → List (List Reachable)
  | 0, _, frontier => [frontier]
  | depth + 1, seen, frontier =>
      let following := uniqueNewStates seen (frontier.flatMap (successors next))
      frontier :: explorationLayersWith next depth (seen ++ following) following

def reachableUpTo
    (next : State → Event → Verdict)
    (setup : Setup)
    (depth : Nat) : List Reachable :=
  let start : Reachable := { trace := [], state := State.initial setup }
  (explorationLayersWith next depth [start] [start]).flatten

def firstCounterexample?
    (next : State → Event → Verdict)
    (setup : Setup)
    (depth : Nat) : Option Reachable :=
  (reachableUpTo next setup depth).find? fun item => !(safe item.state)

private def resultJson (result : Result) : Lean.Json := Lean.Json.mkObj [
  ("mutant", .str (mutantName result.mutant)),
  ("status", .str (statusName result.status)),
  ("executed", Lean.toJson result.executed)
]

private def resultsJson (results : List Result) : Lean.Json :=
  .arr (results.toArray.map resultJson)

private def mutantsJson (mutants : List Mutant) : Lean.Json :=
  .arr (mutants.toArray.map fun mutant => .str (mutantName mutant))

private def statusesJson (statuses : List Status) : Lean.Json :=
  .arr (statuses.toArray.map fun status => .str (statusName status))

private def statusCountsJson (counts : List (Status × Nat)) : Lean.Json :=
  Lean.Json.mkObj (counts.map fun entry => (statusName entry.1, Lean.toJson entry.2))

private def diagnosticsJson (diagnostics : List Diagnostic) : Lean.Json :=
  .arr (diagnostics.toArray.map fun diagnostic => .str (diagnosticName diagnostic))

private def stringsJson (values : List String) : Lean.Json :=
  .arr (values.toArray.map Lean.Json.str)

private def setupJson (setup : Setup) : Lean.Json := Lean.Json.mkObj [
  ("session", Lean.toJson setup.session),
  ("metrics", Lean.toJson setup.metrics),
  ("discovered", mutantsJson setup.discovered),
  ("seeded_durable", resultsJson setup.seededDurable)
]

private def observationJson (observation : ExpectedObservation) : Lean.Json :=
  Lean.Json.mkObj [
    ("accepted", resultsJson observation.accepted),
    ("durable", resultsJson observation.durable),
    ("reported", resultsJson observation.reported),
    ("summary", statusesJson observation.summary),
    ("summary_counts", statusCountsJson observation.summaryCounts),
    ("metrics_executed", Lean.toJson observation.metricsExecuted),
    ("metrics_observed", Lean.toJson observation.metricsObserved),
    ("stopped", Lean.toJson observation.stopped),
    ("session_finished", Lean.toJson observation.sessionFinished),
    ("session_complete", Lean.toJson observation.sessionComplete),
    ("metrics_finished", Lean.toJson observation.metricsFinished),
    ("run_complete", Lean.toJson observation.runComplete),
    ("returned", Lean.toJson observation.returned),
    ("exit_code", Lean.toJson observation.exitCode),
    ("diagnostics", diagnosticsJson observation.diagnostics)
  ]

def oracleCaseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema),
  ("id", .str item.id),
  ("mode", .str item.mode),
  ("scenario", .str item.scenario),
  ("setup", setupJson item.setup),
  ("schedule", stringsJson item.schedule),
  ("expected", observationJson item.expected)
]

def renderCorpus : String :=
  String.join (cases.map fun item => (oracleCaseJson item).compress ++ "\n")

def auditDepth : Nat := 5

def explorationLayers (depth : Nat) : List (List Reachable) :=
  let start : Reachable := { trace := [], state := State.initial auditSetup }
  explorationLayersWith step depth [start] [start]

def reachableStates : List Reachable :=
  (explorationLayers auditDepth).flatten

def alphabetSize : Nat := eventAlphabet.length

def reachableStateCount : Nat := reachableStates.length

def checkedTransitionCount : Nat :=
  ((explorationLayers auditDepth).take auditDepth).foldl
    (fun count layer => count + layer.length * alphabetSize) 0

def boundedAuditPasses : Bool :=
  caseStatesSafe && reachableStates.all fun item => safe item.state

def atomicityDetected : Bool :=
  safe (run (State.initial auditSetup) atomicityTrace) &&
    !(safe (runWith brokenAtomicity (State.initial auditSetup) atomicityTrace))

def uniquenessDetected : Bool :=
  safe (run (State.initial auditSetup) duplicateReportTrace) &&
    !(safe (runWith brokenUniqueness (State.initial auditSetup) duplicateReportTrace))

def boundaryDetected : Bool :=
  safe (run (State.initial auditSetup) stopTrace) &&
    !(safe (runWith brokenBoundary (State.initial auditSetup) stopTrace))

def crossSurfaceDetected : Bool :=
  safe (run (State.initial auditSetup) crossSurfaceTrace) &&
    !(safe (runWith brokenCrossSurface (State.initial auditSetup) crossSurfaceTrace))

def metricsDetected : Bool :=
  safe (run (State.initial auditSetup) notRunTrace) &&
    !(safe (runWith brokenMetrics (State.initial auditSetup) notRunTrace))

def sensitivityPasses : Bool :=
  atomicityDetected && uniquenessDetected && boundaryDetected &&
    crossSurfaceDetected && metricsDetected

def removeAt : Nat → List α → List α
  | _, [] => []
  | 0, _ :: rest => rest
  | index + 1, item :: rest => item :: removeAt index rest

def shrinkWithFuel (fuel : Nat) (violates : List Event → Bool)
    (trace : List Event) : List Event :=
  match fuel with
  | 0 => trace
  | fuel + 1 =>
      let candidates := (List.range trace.length).map fun index => removeAt index trace
      match candidates.find? violates with
      | none => trace
      | some shorter => shrinkWithFuel fuel violates shorter

def shrinkTrace (next : State → Event → Verdict) (trace : List Event) : List Event :=
  shrinkWithFuel trace.length
    (fun candidate => !(safe (runWith next (State.initial auditSetup) candidate))) trace

def traceName (trace : List Event) : String :=
  String.intercalate " -> " (trace.map eventName)

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless sensitivityPasses do
    IO.eprintln "result lifecycle audit did not detect every broken family"
    return .error 2
  unless boundedAuditPasses do
    let witness := firstCounterexample? step auditSetup auditDepth
    IO.eprintln s!"bounded result lifecycle audit found an unsafe state: {witness.map (traceName ∘ Reachable.trace)}"
    return .error 2
  return .ok ()

private def writeCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      if let some parent := path.parent then
        IO.FS.createDirAll parent
      IO.FS.writeFile path renderCorpus
      return 0

private def checkCorpus (path : System.FilePath) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      try
        let actual ← IO.FS.readFile path
        if actual == renderCorpus then
          return 0
        IO.eprintln s!"stale Lean result lifecycle corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean result lifecycle corpus {path}: {error}"
        return 1

private def printStats : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      IO.println s!"depth={auditDepth} alphabet={alphabetSize} states={reachableStateCount} transitions={checkedTransitionCount} corpus_cases={cases.length}"
      return 0

private def printSensitivity : IO UInt32 := do
  IO.println s!"atomicity detected={atomicityDetected} trace={traceName (shrinkTrace brokenAtomicity atomicityTrace)}"
  IO.println s!"uniqueness detected={uniquenessDetected} trace={traceName (shrinkTrace brokenUniqueness duplicateReportTrace)}"
  IO.println s!"boundary detected={boundaryDetected} trace={traceName (shrinkTrace brokenBoundary stopTrace)}"
  IO.println s!"cross_surface detected={crossSurfaceDetected} trace={traceName (shrinkTrace brokenCrossSurface crossSurfaceTrace)}"
  IO.println s!"metrics detected={metricsDetected} trace={traceName (shrinkTrace brokenMetrics notRunTrace)}"
  return if sensitivityPasses then 0 else 2

def main (args : List String) : IO UInt32 := do
  let args := match args with
    | "--" :: rest => rest
    | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--stats"] => printStats
  | ["--sensitivity"] => printSensitivity
  | _ =>
      IO.eprintln "usage: generate_result_lifecycle (--output PATH | --check PATH | --stats | --sensitivity)"
      return 2

end HoiminOracle.ResultLifecycle.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.ResultLifecycle.Executable.main args
