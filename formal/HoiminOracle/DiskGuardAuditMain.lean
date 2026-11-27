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
  String.join (oracleCases.map fun item => (caseJson item).compress ++ "\n")

private def stopCode : StopReason → String
  | .sizeExceeded => "workspace.size.exceeded"
  | .reserveReached => "filesystem.reserve.reached"
  | .measurementFailed => "disk.measurement.failed"
  | .processFailed => "process.failed"

private def measurementJson (owned available nanos : Nat) : Lean.Json :=
  Lean.Json.mkObj [
    ("owned_bytes", Lean.toJson owned), ("available_bytes", Lean.toJson available),
    ("measured_in", Lean.Json.mkObj [
      ("secs", Lean.toJson (nanos / 1000000000)), ("nanos", Lean.toJson (nanos % 1000000000)) ]) ]

private def primaryJson : ReportEvidence → Lean.Json
  | .observation reason owned available nanos =>
      Lean.Json.mkObj [(stopName reason, measurementJson owned available nanos)]
  | .error reason message => Lean.Json.mkObj [(stopName reason, .str message)]

private def evidenceJson : ReportEvidence → Lean.Json
  | .observation reason owned available nanos => Lean.Json.mkObj [("Observation",
      Lean.Json.mkObj [("reason", .str (stopName reason)),
        ("value", measurementJson owned available nanos)])]
  | .error reason message => Lean.Json.mkObj [("Error",
      Lean.Json.mkObj [("code", .str (stopCode reason)), ("message", .str message)])]

private def reportEvidenceCase (id : String) (primary : ReportEvidence)
    (existing : List ReportEvidence) (incoming : ReportEvidence)
    (other : List ReportEvidence) : Lean.Json :=
  let items values := Lean.Json.arr (values.map evidenceJson).toArray
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("id", .str id),
    ("primary", primaryJson primary), ("existing", items existing),
    ("incoming", Lean.Json.mkObj [("reason", primaryJson incoming), ("secondary", items other)]),
    ("expected", items ((incoming :: other).foldl (appendReportEvidence primary) existing)) ]

private def firstSample : ReportEvidence := .observation .sizeExceeded 10 20 1000000
private def laterSample : ReportEvidence := .observation .sizeExceeded 10 20 2000000

def renderReportEvidenceCorpus : String :=
  let reserve := ReportEvidence.observation .reserveReached 10 20 1000000
  let firstError := ReportEvidence.error .measurementFailed "first error"
  let laterError := ReportEvidence.error .measurementFailed "second error"
  let cases := [
    reportEvidenceCase "duration_only_differs" firstSample [] firstSample [laterSample, laterSample],
    reportEvidenceCase "incoming_primary_duration_differs" firstSample [] laterSample [firstSample],
    reportEvidenceCase "different_threshold_reason" firstSample [laterSample] reserve [laterSample],
    reportEvidenceCase "error_message_identity" firstError [] firstError [laterError, laterError],
    reportEvidenceCase "secondary_order" firstSample [firstError] laterSample
      [firstError, reserve, laterError, laterSample] ]
  String.join (cases.map fun item => item.compress ++ "\n")

private def projectedPrimaryMatches : ReportEvidence → ReportEvidence → Bool
  | .observation reason owned available _, .observation reason' owned' available' _ =>
      reason == reason' && owned == owned' && available == available'
  | first, later => decide (first = later)

private def reportEvidenceSensitivity : Bool :=
  let broken := if projectedPrimaryMatches firstSample laterSample then [] else [laterSample]
  decide (appendReportEvidence firstSample [] laterSample ≠ broken)

private def meterRoots (sameFilesystem : Bool) : List MeterRoot := [
  { filesystem := 7, available := 90, files := [(0, 4), (1, 0)] },
  { filesystem := if sameFilesystem then 7 else 8, available := 4, files := [(0, 4), (2, 6)] } ]

private def meterCase (id : String) (roots : List MeterRoot) (maxOwned minFree : Nat) : Lean.Json :=
  let capacities := meterCapacities roots
  let pair (item : Nat × Nat) := Lean.Json.arr #[Lean.toJson item.1, Lean.toJson item.2]
  let expectation uniqueFiles :=
    let owned := measuredBytes uniqueFiles roots
    Lean.Json.mkObj [("owned_bytes", Lean.toJson owned),
      ("reasons", Lean.Json.arr ((meterReasons owned maxOwned minFree capacities).map
        (Lean.Json.str ∘ stopCode)).toArray)]
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str id),
    ("roots", Lean.Json.arr (roots.map fun root => Lean.Json.mkObj [
      ("filesystem", Lean.toJson root.filesystem), ("available", Lean.toJson root.available),
      ("files", Lean.Json.arr (root.files.map pair).toArray)]).toArray),
    ("max_owned", Lean.toJson maxOwned), ("min_free", Lean.toJson minFree),
    ("expected_capacities", Lean.Json.arr (capacities.map pair).toArray),
    ("expected_minimum", optionNatJson (minimumCapacity capacities)),
    ("expected_unix", expectation true), ("expected_windows", expectation false) ]

def renderMeterCorpus : String :=
  let cases := [true, false].flatMap fun shared => [false, true].flatMap fun reverse =>
    ([(64, 5), (10, 4), (11, 90)] : List (Nat × Nat)).map fun (owned, free) =>
      let roots := meterRoots shared
      meterCase s!"roots_{shared}_{reverse}_{owned}_{free}" (if reverse then roots.reverse else roots)
        owned free
  String.join ((meterCase "no_capacity" [] 10 5 :: cases).map fun item => item.compress ++ "\n")

private def meterSensitivity : Bool :=
  let shared := meterRoots true
  let split := meterCapacities (meterRoots false)
  measuredBytes true shared != measuredBytes false shared &&
    decide (meterCapacities shared ≠ meterCapacities shared.reverse) &&
    minimumCapacity split != split.head?.map Prod.snd

private def statsSampleJson : StatsSample → Lean.Json
  | .reading value => Lean.Json.mkObj [
      ("kind", .str "reading"), ("owned_bytes", Lean.toJson value.owned),
      ("elapsed_ms", Lean.toJson value.elapsedMs),
      ("capacities", Lean.Json.arr (value.capacities.map fun (key, bytes) =>
        Lean.Json.arr #[Lean.toJson key, Lean.toJson bytes]).toArray)]
  | .measurementFailed => Lean.Json.mkObj [("kind", .str "measurement_failed")]
  | .refreshFailed => Lean.Json.mkObj [("kind", .str "refresh_failed")]

private def statsJson (history : List StatsReading) : Lean.Json :=
  let capacities := historicalCapacities history
  let filesystems := (capacities.map Prod.fst).eraseDups.filterMap fun key =>
    let values := capacities.filterMap fun (id, bytes) => if id == key then some bytes else none
    values.head?.map fun first => (toString key, Lean.Json.mkObj [
      ("start", Lean.toJson first), ("minimum", Lean.toJson (values.min?.getD first))])
  Lean.Json.mkObj [
    ("sample_count", Lean.toJson (min history.length (2^64 - 1))),
    ("peak_owned_bytes", Lean.toJson ((history.map StatsReading.owned).max?.getD 0)),
    ("maximum_measurement_ms", Lean.toJson ((history.map StatsReading.elapsedMs).max?.getD 0)),
    ("minimum_available_bytes", optionNatJson (minimumCapacity capacities)),
    ("filesystems", Lean.Json.mkObj filesystems)]

private def statsObservations : List StatsReading → List StatsSample → List Lean.Json
  | history, [] => [statsJson history]
  | history, sample :: rest => statsJson history :: statsObservations (recordStats history sample) rest

private def statsCase (id : String) (samples : List StatsSample) : Lean.Json :=
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str id),
    ("samples", Lean.Json.arr (samples.map statsSampleJson).toArray),
    ("expected", Lean.Json.arr (statsObservations [] samples).toArray)]

private def initialStatsReading : StatsSample := .reading ⟨3, 20, [(7, 900), (8, 700)]⟩
private def laterStatsReading : StatsSample := .reading ⟨10, 2, [(8, 400), (9, 800)]⟩
private def zeroStatsReading : StatsSample := .reading ⟨5, 4, [(7, 0)]⟩
private def emptyStatsReading : StatsSample := .reading ⟨999, 999, []⟩

def renderStatisticsCorpus : String :=
  let fixed := [statsCase "unmeasured" [],
    statsCase "different_maxima_and_missing_filesystems"
      [initialStatsReading, laterStatsReading, zeroStatsReading, initialStatsReading],
    statsCase "zero_then_recovery" [zeroStatsReading, laterStatsReading],
    statsCase "shutdown_report" [.reading ⟨3, 1, [(7, 2^64 - 1)]⟩,
      .reading ⟨0, 1, [(7, 1)]⟩, .reading ⟨0, 2, [(7, 1)]⟩]]
  let rejections := [
    ("measurement_failed", StatsSample.measurementFailed),
    ("refresh_failed", StatsSample.refreshFailed),
    ("missing_capacity", emptyStatsReading) ]
  let scenarios := rejections.flatMap fun (name, sample) =>
    [statsCase s!"{name}_before_first_reading" [sample, laterStatsReading],
     statsCase s!"{name}_after_first_reading" [initialStatsReading, sample, laterStatsReading]]
  String.join ((fixed ++ scenarios).map fun item => item.compress ++ "\n")

private def statsSensitivity : Bool :=
  let history := recordStats (recordStats [] zeroStatsReading) laterStatsReading
  let current := recordStats [] laterStatsReading
  minimumCapacity (historicalCapacities history) != minimumCapacity (historicalCapacities current) &&
    (recordStats history emptyStatsReading).length != history.length + 1

private def finalCapacityCase (id : String) (starts : List (Nat × Nat))
    (query : Option (List (Nat × Nat))) : Lean.Json :=
  let pairs (values : List (Nat × Nat)) := Lean.Json.arr (values.toArray.map fun (key, bytes) =>
    .arr #[Lean.toJson key, Lean.toJson bytes])
  Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str id),
    ("start", pairs starts), ("end", (query.map pairs).getD .null),
    ("expected_complete", Lean.toJson query.isSome),
    ("expected_filesystems", .arr ((finalCapacities starts query).toArray.map fun row =>
      Lean.Json.mkObj [
        ("key", .str (toString row.key)),
        ("start_available_bytes", Lean.toJson row.start),
        ("minimum_available_bytes", Lean.toJson row.start),
        ("end_available_bytes", optionNatJson row.finish),
        ("available_bytes_change", (row.change.map Lean.toJson).getD .null)]))]

def renderFinalCapacityCorpus : String :=
  let maxBytes := 2^64 - 1
  let starts := [(7, 1000), (8, 800)]
  let cases := [
    finalCapacityCase "wide_changes" [(7, maxBytes), (8, 2)] (some [(7, 0), (8, maxBytes)]),
    finalCapacityCase "missing_key_and_measured_zero" starts (some [(7, 0), (9, 500)]),
    finalCapacityCase "empty_success" starts (some []),
    finalCapacityCase "query_failure" starts none,
    finalCapacityCase "equal_and_decreased" starts (some [(7, 500), (8, 800)])]
  String.join (cases.map fun item => item.compress ++ "\n")

private def finalCapacitySensitivity : Bool :=
  let maxBytes := 2^64 - 1
  capacityChange maxBytes (some 0) != some (Int.ofNat (0 - maxBytes)) &&
    endCapacity (some [(7, 0)]) 8 != some 0

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
  | .finishAfterDeferred => "finish_after_deferred_cleanup"
  | .finishAfterDeliveryRetained => "finish_after_delivery_retained"
  | .rewriteDeferred => "rewrite_deferred_outcome"

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
    ("implementation_correspondence", .str "pending Rust adapter execution"),
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
  unless sensitivityPasses && reportEvidenceSensitivity && meterSensitivity &&
      statsSensitivity && finalCapacitySensitivity do
    IO.eprintln "one or more broken disk-guard families escaped detection"
    return .error 2
  return .ok ()

private def writeCorpus (path : System.FilePath) (content : String) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      if let some parent := path.parent then IO.FS.createDirAll parent
      let temporary := path.withExtension "jsonl.tmp"
      IO.FS.writeFile temporary content
      IO.FS.rename temporary path
      return 0

private def checkCorpus (path : System.FilePath) (content : String) : IO UInt32 := do
  match ← ensureAudit with
  | .error code => return code
  | .ok () =>
      try
        let actual ← IO.FS.readFile path
        if actual == content then return 0
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
        ("total_corpus_records", Lean.toJson oracleCases.length),
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
  | ["--output", path] => writeCorpus path renderCorpus
  | ["--check", path] => checkCorpus path renderCorpus
  | ["--output-report-evidence", path] => writeCorpus path renderReportEvidenceCorpus
  | ["--check-report-evidence", path] => checkCorpus path renderReportEvidenceCorpus
  | ["--output-meter", path] => writeCorpus path renderMeterCorpus
  | ["--check-meter", path] => checkCorpus path renderMeterCorpus
  | ["--output-statistics", path] => writeCorpus path renderStatisticsCorpus
  | ["--check-statistics", path] => checkCorpus path renderStatisticsCorpus
  | ["--output-final-capacity", path] => writeCorpus path renderFinalCapacityCorpus
  | ["--check-final-capacity", path] => checkCorpus path renderFinalCapacityCorpus
  | ["--sensitivity"] => printSensitivity
  | ["--stats", path] => printStats path
  | _ =>
      IO.eprintln "usage: generate_disk_guard ((--output | --check | --output-report-evidence | --check-report-evidence | --output-meter | --check-meter | --output-statistics | --check-statistics | --output-final-capacity | --check-final-capacity) PATH | --sensitivity | --stats RESOURCE_GUARD_JSON)"
      return 2

end HoiminOracle.DiskGuard.Executable

def main (args : List String) : IO UInt32 :=
  HoiminOracle.DiskGuard.Executable.main args
