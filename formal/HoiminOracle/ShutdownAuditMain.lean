import HoiminOracle.ShutdownCases
import Std.Data.HashSet

open HoiminOracle.ShutdownAudit

def eventAlphabet : List Event := [
  .boot, .startProcess, .firstInterrupt, .secondInterrupt, .deadlineReached,
  .processExited, .processFailed, .infrastructureFailed, .requestTermination,
  .reapProcess, .startOutputDrain, .outputDrained, .outputFailed,
  .startBlocking, .blockingCompleted, .detachBlocking, .startCleanup,
  .cleanupCompleted, .cleanupFailed, .startSessionFinish, .sessionFinished,
  .sessionFailed, .startReport, .reportWritten, .reportFailed, .startMetrics,
  .metricsWritten, .metricsFailed, .metricsSkipped, .returnSuccess,
  .returnFailure]

def auditDepth : Nat := 9

structure Reachable where
  trace : List Event
  state : State
  deriving Repr, DecidableEq

private def successors (next : State → Event → Verdict)
    (item : Reachable) : List Reachable :=
  eventAlphabet.map fun event => {
    trace := item.trace ++ [event]
    state := (next item.state event).state
  }

private def stateSet (items : List Reachable) : Std.HashSet State :=
  items.foldl (fun states item => states.insert item.state) ∅

private def uniqueNewStates (seen candidates : List Reachable) : List Reachable :=
  let (_, retained) := candidates.foldl (fun (states, retained) candidate =>
    if states.contains candidate.state then (states, retained)
    else (states.insert candidate.state, candidate :: retained)) (stateSet seen, [])
  retained.reverse

private def explorationLayersWith (next : State → Event → Verdict) :
    Nat → List Reachable → List Reachable → List (List Reachable)
  | 0, _, frontier => [frontier]
  | depth + 1, seen, frontier =>
      let following := uniqueNewStates seen (frontier.flatMap (successors next))
      frontier :: explorationLayersWith next depth (seen ++ following) following

private def explorationLayers (depth : Nat) : List (List Reachable) :=
  let initial : Reachable := { trace := [], state := State.initial }
  explorationLayersWith step depth [initial] [initial]

def reachableUpTo (depth : Nat) : List Reachable :=
  (explorationLayers depth).flatten

def firstCounterexample? (next : State → Event → Verdict)
    (depth : Nat) : Option (List Event) :=
  let initial : Reachable := { trace := [], state := State.initial }
  let layers := explorationLayersWith next depth [initial] [initial]
  (layers.flatten.find? fun item => !(safe item.state)).map Reachable.trace

private def runWith (next : State → Event → Verdict) : State → List Event → State
  | state, [] => state
  | state, event :: rest => runWith next (next state event).state rest

private def unsafeWith (next : State → Event → Verdict) (trace : List Event) : Bool :=
  !(safe (runWith next State.initial trace))

private def eraseAt (trace : List Event) (index : Nat) : List Event :=
  trace.take index ++ trace.drop (index + 1)

private def shrinkLoop (next : State → Event → Verdict) :
    Nat → Nat → List Event → List Event
  | 0, _, trace => trace
  | fuel + 1, index, trace =>
      if index < trace.length then
        let candidate := eraseAt trace index
        if unsafeWith next candidate then
          shrinkLoop next fuel index candidate
        else
          shrinkLoop next fuel (index + 1) trace
      else trace

def shrinkTrace (next : State → Event → Verdict) (trace : List Event) : List Event :=
  shrinkLoop next (trace.length * 2 + 1) 0 trace

def reachableStateCount : Nat := (reachableUpTo auditDepth).length

def checkedTransitionCount : Nat :=
  ((explorationLayers auditDepth).take auditDepth).foldl
    (fun count layer => count + layer.length * eventAlphabet.length) 0

private def sensitivity (name : String) (next : State → Event → Verdict)
    (witness : List Event) : String × Bool × List Event :=
  (name, unsafeWith next witness, shrinkTrace next witness)

def sensitivityResults : List (String × Bool × List Event) := [
  sensitivity "cause_overwrite" brokenCauseStep causeWitness,
  sensitivity "duplicate_dispatch" brokenDuplicateStep duplicateWitness,
  sensitivity "ordering" brokenOrderingStep orderingWitness,
  sensitivity "error_precedence" brokenPrecedenceStep precedenceWitness,
  sensitivity "forced_wait" brokenForcedWaitStep forcedWaitWitness,
  sensitivity "completion_regression" brokenRegressionStep regressionWitness,
  sensitivity "ownership_loss" brokenDetachStep detachWitness]

private structure AuditSummary where
  safe : Bool
  states : Nat
  transitions : Nat

private def computeAudit : AuditSummary :=
  let layers := explorationLayers auditDepth
  { safe := layers.flatten.all fun item => safe item.state
    states := layers.foldl (fun count layer => count + layer.length) 0
    transitions := (layers.take auditDepth).foldl
      (fun count layer => count + layer.length * eventAlphabet.length) 0 }

private def sensitivityPasses : Bool :=
  sensitivityResults.all fun result => result.2.1

private def ensureAudit (audit : AuditSummary) : IO (Except UInt32 Unit) := do
  unless audit.safe do
    IO.eprintln "bounded shutdown audit found an unsafe reachable state"
    return .error 2
  unless sensitivityPasses do
    IO.eprintln "one or more broken shutdown-model families escaped detection"
    return .error 2
  return .ok ()

private def writeCorpus (path : System.FilePath) : IO UInt32 := do
  if let some parent := path.parent then IO.FS.createDirAll parent
  IO.FS.writeFile path renderShutdownCorpus
  return 0

private def checkCorpus (path : System.FilePath) : IO UInt32 := do
  try
    let actual ← IO.FS.readFile path
    if actual == renderShutdownCorpus then return 0
    IO.eprintln s!"stale Lean shutdown corpus: {path}"
    return 1
  catch error =>
    IO.eprintln s!"cannot check Lean shutdown corpus {path}: {error}"
    return 1

private def traceName (trace : List Event) : String :=
  String.intercalate "," (trace.map eventName)

private def printStats (audit : AuditSummary) : IO UInt32 := do
  IO.println s!"depth={auditDepth} alphabet={eventAlphabet.length} states={audit.states} transitions={audit.transitions} corpus={shutdownCases.length} reduction=none"
  return 0

private def printSensitivity : IO UInt32 := do
  for (name, detected, trace) in sensitivityResults do
    IO.println s!"family={name} detected={detected} trace={traceName trace}"
  return 0

def main (args : List String) : IO UInt32 := do
  let audit := computeAudit
  match ← ensureAudit audit with
  | .error code => return code
  | .ok () => pure ()
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--stats"] => printStats audit
  | ["--sensitivity"] => printSensitivity
  | _ =>
      IO.eprintln "usage: generate_shutdown (--output PATH | --check PATH | --stats | --sensitivity)"
      return 2
