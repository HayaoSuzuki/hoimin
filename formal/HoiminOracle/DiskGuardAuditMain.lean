import HoiminOracle.DiskGuardProofs
import Lean.Data.Json

open HoiminOracle.DiskGuard

namespace HoiminOracle.DiskGuard.Executable

private def stopName : StopReason → String
  | .sizeExceeded => "workspace_size_exceeded"
  | .reserveReached => "filesystem_reserve_reached"
  | .measurementFailed => "measurement_failed"
  | .processFailed => "process_failed"

private def componentName : ComponentState → String
  | .pending => "pending"
  | .active => "active"
  | .succeeded => "succeeded"
  | .failed => "failed"

private def modeName : CorrespondenceMode → String
  | .strict => "strict"
  | .internalFixture => "internal-fixture"
  | .modelOnly => "model-only"
  | .infrastructureError => "infrastructure-error"

private def layerName : Layer → String
  | .policy => "policy"
  | .runtime => "runtime"

private def targetName : ImplementationTarget → String
  | .rust => "rust"
  | .python => "python"

private def rootName : RootId → String
  | ⟨0, _⟩ => "execution"
  | ⟨1, _⟩ => "delivery"

private def rootsJson (roots : List RootId) : Lean.Json :=
  .arr (roots.toArray.map fun root => .str (rootName root))

private def stopsJson (reasons : List StopReason) : Lean.Json :=
  .arr (reasons.toArray.map fun reason => .str (stopName reason))

private def optionStopJson : Option StopReason → Lean.Json
  | none => .null
  | some reason => .str (stopName reason)

private def optionNatJson : Option Nat → Lean.Json
  | none => .null
  | some value => Lean.toJson value

private def stateFields (state : State) : List (String × Lean.Json) := [
  ("stop", optionStopJson state.stop),
  ("secondary_stops", stopsJson state.secondaryStops),
  ("active", Lean.toJson state.active),
  ("dispatched", Lean.toJson state.dispatched),
  ("owned_roots", rootsJson state.ownedRoots),
  ("delivery_roots", rootsJson state.deliveryRoots),
  ("cleanup_requested", rootsJson state.cleanupRequested),
  ("cleanup_clean", rootsJson state.cleanupClean),
  ("cleanup_failed", rootsJson state.cleanupFailed),
  ("cleanup_deferred", rootsJson state.cleanupDeferred),
  ("cleanup_retained", rootsJson state.cleanupRetained),
  ("process_drain", .str (componentName state.processDrain)),
  ("output_drain", .str (componentName state.outputDrain)),
  ("monitor_join", .str (componentName state.monitorJoin)),
  ("report", .str (componentName state.report)),
  ("finished", Lean.toJson state.finished)]

private def stateJson (state : State) : Lean.Json := Lean.Json.mkObj (stateFields state)

private def expectedJson (value : TerminalObservation) : Lean.Json :=
  Lean.Json.mkObj [
    ("stop", optionStopJson value.stop),
    ("secondary_stops", stopsJson value.secondaryStops),
    ("active", Lean.toJson value.active),
    ("dispatched", Lean.toJson value.dispatched),
    ("owned_roots", rootsJson value.ownedRoots),
    ("delivery_roots", rootsJson value.deliveryRoots),
    ("cleanup_requested", rootsJson value.cleanupRequested),
    ("cleanup_clean", rootsJson value.cleanupClean),
    ("cleanup_failed", rootsJson value.cleanupFailed),
    ("cleanup_deferred", rootsJson value.cleanupDeferred),
    ("cleanup_retained", rootsJson value.cleanupRetained),
    ("process_drain", .str (componentName value.processDrain)),
    ("output_drain", .str (componentName value.outputDrain)),
    ("monitor_join", .str (componentName value.monitorJoin)),
    ("report", .str (componentName value.report)),
    ("finished", Lean.toJson value.finished),
    ("accepted", Lean.toJson value.accepted),
    ("rejected_at", optionNatJson value.rejectedAt)]

private def eventJson : Event → Lean.Json
  | .dispatch => Lean.Json.mkObj [("kind", .str "dispatch")]
  | .observe owned maxOwned free minFree => Lean.Json.mkObj [
      ("kind", .str "observe"), ("owned", Lean.toJson owned),
      ("max_owned", Lean.toJson maxOwned), ("free", Lean.toJson free),
      ("min_free", Lean.toJson minFree)]
  | .meterFailed => Lean.Json.mkObj [("kind", .str "meter_failed")]
  | .processDrainFailed => Lean.Json.mkObj [("kind", .str "process_drain_failed")]
  | .processDrainSucceeded =>
      Lean.Json.mkObj [("kind", .str "process_drain_succeeded")]
  | .requestCleanup root => Lean.Json.mkObj [
      ("kind", .str "cleanup_requested"), ("root", .str (rootName root))]
  | .cleanupSucceeded root => Lean.Json.mkObj [
      ("kind", .str "cleanup_succeeded"), ("root", .str (rootName root))]
  | .cleanupFailed root => Lean.Json.mkObj [
      ("kind", .str "cleanup_failed"), ("root", .str (rootName root))]
  | .cleanupDeferred root => Lean.Json.mkObj [
      ("kind", .str "cleanup_deferred"), ("root", .str (rootName root))]
  | .cleanupRetained root => Lean.Json.mkObj [
      ("kind", .str "cleanup_retained"), ("root", .str (rootName root))]
  | .outputDrained => Lean.Json.mkObj [("kind", .str "output_drained")]
  | .outputDrainFailed => Lean.Json.mkObj [("kind", .str "output_drain_failed")]
  | .monitorJoined => Lean.Json.mkObj [("kind", .str "monitor_joined")]
  | .monitorJoinFailed => Lean.Json.mkObj [("kind", .str "monitor_join_failed")]
  | .reportSucceeded => Lean.Json.mkObj [("kind", .str "report_succeeded")]
  | .reportFailed => Lean.Json.mkObj [("kind", .str "report_failed")]
  | .finish => Lean.Json.mkObj [("kind", .str "finish")]

private def caseJson (item : OracleCase) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson item.schema),
  ("id", .str item.id),
  ("mode", .str (modeName item.mode)),
  ("layer", .str (layerName item.layer)),
  ("implementation_targets",
    .arr (item.implementationTargets.toArray.map fun target => .str (targetName target))),
  ("initial", stateJson item.initial),
  ("events", .arr (item.events.toArray.map eventJson)),
  ("expected", expectedJson item.expected)]

def renderCorpus : String :=
  String.join (fixedCases.map fun item => (caseJson item).compress ++ "\n")

private def familyEvents : List Event := [
  .dispatch, .observe 9 10 11 10, .processDrainSucceeded, .outputDrained,
  .monitorJoined, .requestCleanup executionRoot, .reportSucceeded, .finish]

private def tracesExactly : Nat → List (List Event)
  | 0 => [[]]
  | depth + 1 =>
      (tracesExactly depth).flatMap fun trace => familyEvents.map fun event => trace ++ [event]

private def skeletons : List (List Event) :=
  (List.range 6).flatMap tracesExactly

private def canonicalStart : State := State.initial [executionRoot]

private def reachableStates : List State :=
  (skeletons.filterMap fun trace => run canonicalStart trace).eraseDups

private def acceptedTraceCount : Nat :=
  (skeletons.filter fun trace => (run canonicalStart trace).isSome).length

private def boundedSkeletonCount : Nat := skeletons.length
private def checkedTransitionCount : Nat := boundedSkeletonCount - 1

private def familyName : BrokenFamily → String
  | .strictSize => "inclusive_size_boundary"
  | .reserveDirection => "reserve_direction"
  | .simultaneousSecondary => "simultaneous_secondary"
  | .overwritePrimary => "first_reason_stickiness"
  | .postStopDispatch => "post_stop_dispatch"
  | .postDrainDispatch => "post_drain_dispatch"
  | .duplicateCleanup => "duplicate_cleanup"
  | .cleanOnCleanupError => "cleanup_error_not_clean"
  | .earlyFinish => "early_finish"
  | .cleanupBeforeSettlement => "cleanup_before_settlement"
  | .destructiveAfterFailure => "destructive_cleanup_after_failure"
  | .finishAfterReportFailure => "finish_after_report_failure"
  | .finishAfterDeliveryFailure => "finish_after_delivery_failure"

private def traceStates (next : State → Event → Option State) :
    State → List Event → List State
  | state, [] => [state]
  | state, event :: rest =>
      match next state event with
      | none => [state]
      | some candidate => state :: traceStates next candidate rest

private def statesJson (states : List State) : Lean.Json :=
  .arr (states.toArray.map stateJson)

private def sensitivityJson (family : BrokenFamily) : Lean.Json :=
  let witness := brokenWitness family
  Lean.Json.mkObj [
    ("claim", .str (familyName family)),
    ("model_boundary", .str "pure finite disk policy and lifecycle events"),
    ("finite_domain", .str "two root IDs; normalized threshold payload classes"),
    ("minimal_trace", .arr (witness.2.toArray.map eventJson)),
    ("intermediate_states", Lean.Json.mkObj [
      ("correct", statesJson (traceStates step witness.1 witness.2)),
      ("broken", statesJson (traceStates (brokenStep family) witness.1 witness.2))]),
    ("classification", .str (if brokenDetected family then "detected" else "escaped")),
    ("implementation_correspondence", .str "pending Rust/Python adapter execution"),
    ("owner_question", .str "Does production preserve this approved lifecycle rule?"),
    ("reproduction_command", .str
      "lake exe generate_disk_guard -- --sensitivity")]

private def ensureAudit : IO (Except UInt32 Unit) := do
  unless boundedSkeletonCount == 37449 do
    IO.eprintln s!"unexpected bounded skeleton count: {boundedSkeletonCount}"
    return .error 2
  unless casesPass do
    IO.eprintln "one or more literal disk-guard cases do not match the model"
    return .error 2
  unless corpusContractValid do
    IO.eprintln "disk-guard corpus violates schema/ID/target/trace bounds"
    return .error 2
  unless rootRenamingCasesPass && payloadSymmetryCasesPass do
    IO.eprintln "disk-guard normalization lacks a finite sample witness"
    return .error 2
  unless sensitivityPasses do
    IO.eprintln "one or more broken disk-guard families escaped detection"
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
        IO.eprintln s!"stale Lean disk-guard corpus: {path}"
        return 1
      catch error =>
        IO.eprintln s!"cannot check Lean disk-guard corpus {path}: {error}"
        return 1

private def printSensitivity : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      for family in allBrokenFamilies do
        IO.println (sensitivityJson family).compress
      return 0

private def printStats (resourceStats : System.FilePath) : IO UInt32 := do
  let started ← IO.monoNanosNow
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      let states := reachableStates.length
      let accepted := acceptedTraceCount
      let ended ← IO.monoNanosNow
      let elapsedNs := ended - started
      let guardJson ← try
        let raw ← IO.FS.readFile resourceStats
        match Lean.Json.parse raw with
        | .ok value => pure value
        | .error message => pure (Lean.Json.mkObj [("parse_error", .str message)])
      catch error => pure (Lean.Json.mkObj [("read_error", .str (toString error))])
      IO.println (Lean.Json.mkObj [
        ("schema", Lean.toJson 1),
        ("bounded_depth", Lean.toJson 5),
        ("event_family_count", Lean.toJson familyEvents.length),
        ("bounded_skeleton_count", Lean.toJson boundedSkeletonCount),
        ("fixed_case_count", Lean.toJson fixedCases.length),
        ("total_corpus_records", Lean.toJson fixedCases.length),
        ("accepted_trace_count", Lean.toJson accepted),
        ("reachable_state_count", Lean.toJson states),
        ("checked_transition_count", Lean.toJson checkedTransitionCount),
        ("elapsed_ns", Lean.toJson elapsedNs),
        ("elapsed_ms", Lean.toJson ((elapsedNs + 999999) / 1000000)),
        ("resource_guard", guardJson)] |>.compress)
      return 0

def main (args : List String) : IO UInt32 := do
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--output", path] => writeCorpus path
  | ["--check", path] => checkCorpus path
  | ["--sensitivity"] => printSensitivity
  | ["--stats", path] => printStats path
  | _ =>
      IO.eprintln "usage: generate_disk_guard (--output PATH | --check PATH | --sensitivity | --stats RESOURCE_GUARD_JSON)"
      return 2

end HoiminOracle.DiskGuard.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.DiskGuard.Executable.main args
