import HoiminOracle.ResultLifecycleCases

open HoiminOracle.ResultLifecycle

namespace HoiminOracle.ResultLifecycle.Executable

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
