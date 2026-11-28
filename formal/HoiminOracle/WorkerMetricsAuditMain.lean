import HoiminOracle.WorkerMetricsProofs
import HoiminOracle.StageMetricsCases
import HoiminOracle.MetricsValidationCases
import Lean.Data.Json

namespace HoiminOracle.WorkerMetrics

private structure Fixture where
  id : String
  events : List Event
  elapsed : Nat := 4 * nanosPerMilli
  discovered : Nat := 2
  executed : Nat := 0

private def alphabet : List Action := [.queue, .start, .cancel, .finishCounted, .finishUncounted]

private def actionName : Action → String
  | .queue => "queue" | .start => "start" | .cancel => "cancel"
  | .finishCounted => "finish_counted" | .finishUncounted => "finish_uncounted"

private def rejectionName : Rejection → String
  | .alreadyQueued => "already_queued" | .alreadyStarted => "already_started"
  | .notQueued => "not_queued" | .notStarted => "not_started"

private def workerNumber : WorkerId → Nat
  | .first => 3 | .second => 19

private def nanosSchedule (items : List (Nat × WorkerId × Action)) : List Event :=
  items.map fun (time, id, action) => ⟨time, id, action⟩

private def schedule (items : List (Nat × WorkerId × Action)) : List Event :=
  nanosSchedule (items.map fun (time, id, action) => (time * nanosPerMilli, id, action))

private def exhaustiveFixtures : List Fixture :=
  alphabet.flatMap fun a => alphabet.flatMap fun b => alphabet.map fun c => {
    id := String.intercalate "-" ([a, b, c].map actionName)
    events := schedule [(1, .first, a), (2, .first, b), (3, .first, c)]
  }

private def fixtures : List Fixture := exhaustiveFixtures ++ [
  { id := "no-workers", events := [] },
  { id := "overlap-reverse-insertion", elapsed := 12 * nanosPerMilli, executed := 2,
    events := schedule [(1, .second, .queue), (2, .first, .queue), (3, .second, .start),
      (4, .first, .start), (10, .second, .finishCounted), (11, .first, .finishCounted)] },
  { id := "cancel-reuse-sticky-rejections", elapsed := 20 * nanosPerMilli, executed := 1,
    events := schedule [(1, .first, .queue), (4, .first, .cancel), (5, .first, .queue),
      (6, .first, .queue), (7, .first, .start), (9, .first, .start), (10, .first, .cancel),
      (11, .first, .finishCounted), (12, .first, .finishCounted)] },
  { id := "uncounted-work-then-reuse", elapsed := 15 * nanosPerMilli, executed := 1,
    events := schedule [(1, .first, .queue), (2, .first, .start), (6, .first, .finishUncounted),
      (6, .first, .queue), (7, .first, .start), (8, .first, .finishCounted),
      (9, .second, .queue), (10, .second, .start), (11, .second, .finishUncounted)] },
  { id := "lowest-outstanding-before-validation", elapsed := 7 * nanosPerMilli,
    discovered := 0, executed := 1,
    events := schedule [(1, .second, .queue), (2, .second, .start), (3, .first, .queue)] },
  { id := "second-outstanding", events := schedule [(1, .second, .queue)] },
  { id := "executed-exceeds-discovered", discovered := 0, executed := 1,
    events := schedule [(1, .first, .queue), (2, .first, .start), (3, .first, .finishCounted)] },
  { id := "boundary-uint64-max", elapsed := maxMillis * nanosPerMilli, executed := 1,
    events := schedule [(0, .first, .queue), (maxMillis - 1, .first, .start),
      (maxMillis, .first, .finishCounted)] },
  { id := "fractional-timestamp-boundaries", elapsed := 2000000, executed := 1,
    events := nanosSchedule [(900000, .first, .queue), (1100000, .first, .start),
      (2000000, .first, .finishCounted)] },
  { id := "round-each-interval-before-adding", elapsed := 4000000, executed := 2,
    events := nanosSchedule [(0, .first, .queue), (800000, .first, .start),
      (1700000, .first, .finishCounted), (1700000, .first, .queue),
      (2500000, .first, .start), (3400000, .first, .finishCounted)] },
  { id := "exact-and-one-nanosecond-short", elapsed := 3899999, executed := 1,
    events := nanosSchedule [(900000, .first, .queue), (1900000, .first, .start),
      (3899999, .first, .finishCounted)] },
  { id := "elapsed-and-interval-saturate", elapsed := (maxMillis + 2) * nanosPerMilli,
    executed := 1,
    events := schedule [(0, .first, .queue), (maxMillis + 1, .first, .start),
      (maxMillis + 1, .first, .finishCounted)] },
  { id := "busy-total-saturates-before-projection", elapsed := (maxMillis + 1) * nanosPerMilli,
    executed := 2,
    events := schedule [(0, .first, .queue), (0, .first, .start),
      (maxMillis, .first, .finishCounted), (maxMillis, .first, .queue),
      (maxMillis, .first, .start), (maxMillis + 1, .first, .finishCounted)] },
  { id := "combined-counters-exceed-published-elapsed", elapsed := (maxMillis + 1) * nanosPerMilli,
    executed := 1,
    events := schedule [(0, .first, .queue), (maxMillis, .first, .start),
      (maxMillis + 1, .first, .finishCounted)] }
]

private def finishFixture (item : Fixture) (workers : Workers) : FinishResult :=
  finish workers item.elapsed item.discovered item.executed

private def finishJson : FinishResult → Lean.Json
  | .outstanding id => Lean.Json.mkObj [("status", .str "outstanding"),
      ("worker", Lean.toJson (workerNumber id))]
  | .validation => Lean.Json.mkObj [("status", .str "validation")]
  | .saved discovered workers => Lean.Json.mkObj [("status", .str "saved"),
      ("discovered", Lean.toJson discovered),
      ("workers", .arr (workers.toArray.map fun (id, accounting) => Lean.Json.mkObj [
        ("worker", Lean.toJson (workerNumber id)), ("busy_ms", Lean.toJson accounting.busy),
        ("queue_wait_ms", Lean.toJson accounting.queueWait),
        ("processes", Lean.toJson accounting.processes)]))]

private def fixtureJson (item : Fixture) : Lean.Json :=
  let (workers, decisions) := replay {} item.events
  Lean.Json.mkObj [
    ("schema", Lean.toJson (2 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
    ("elapsed_ns", .str (toString item.elapsed)), ("elapsed_ms", Lean.toJson (millis item.elapsed)),
    ("discovered", Lean.toJson item.discovered),
    ("executed", Lean.toJson item.executed),
    ("events", .arr (item.events.toArray.map fun event => Lean.Json.mkObj [
      ("at_ns", .str (toString event.time)), ("worker", Lean.toJson (workerNumber event.id)),
      ("action", .str (actionName event.action))])),
    ("decisions", .arr (decisions.toArray.map fun
      | none => .null
      | some reason => .str (rejectionName reason))),
    ("finish", finishJson (finishFixture item workers))]

private def corpus : String :=
  String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def resetDuplicateQueue (worker : Worker) (time : Nat) (action : Action) :
    Worker × Option Rejection :=
  match action, worker.phase with
  | .queue, .queued _ => ({ worker with phase := .queued time }, some .alreadyQueued)
  | _, _ => applyAction worker time action

private def countUncounted (worker : Worker) (time : Nat) (action : Action) :
    Worker × Option Rejection :=
  applyAction worker time (if action == .finishUncounted then .finishCounted else action)

private def chargeCancelledWait (worker : Worker) (time : Nat) (action : Action) :
    Worker × Option Rejection :=
  match action, worker.phase with
  | .cancel, .queued since => ({ worker.toIdle with
      queueWait := addMillis worker.queueWait (time - since) }, none)
  | _, _ => applyAction worker time action

private def accountWith (accumulate : Nat → Nat → Nat → Nat)
    (worker : Worker) (time : Nat) (action : Action) : Worker × Option Rejection :=
  let (updated, rejection) := applyAction worker time action
  let modified := match action, worker.phase with
    | .start, .queued since => { updated with queueWait := accumulate worker.queueWait time since }
    | .finishCounted, .running since | .finishUncounted, .running since =>
        { updated with busy := accumulate worker.busy time since }
    | _, _ => updated
  (modified, rejection)

private def quantizeTimestamps := accountWith fun accumulated time since =>
  min (accumulated + (millis time - millis since)) maxMillis

private def omitAccumulationSaturation := accountWith fun accumulated time since =>
  accumulated + millis (time - since)

private def accumulateNanos := accountWith fun accumulated time since =>
  accumulated + (time - since)

private def roundTotals (workers : Workers) : Workers :=
  let round (worker : Worker) : Worker :=
    { worker with queueWait := millis worker.queueWait, busy := millis worker.busy }
  ⟨round workers.first, round workers.second⟩

private def firstDifference (candidate : Worker → Nat → Action → Worker × Option Rejection)
    (project : Workers → Workers) : Option Fixture :=
  fixtures.find? fun item =>
    let (workers, decisions) := replay {} item.events
    let (changed, changedDecisions) := replayWith (recordWith candidate) {} item.events
    decisions != changedDecisions || finishFixture item workers != finishFixture item (project changed)

private def sensitivity : IO UInt32 := do
  for (name, candidate, project) in [("reset_duplicate_queue", resetDuplicateQueue, id),
      ("count_uncounted_work", countUncounted, id), ("charge_cancelled_wait", chargeCancelledWait, id),
      ("quantize_timestamps", quantizeTimestamps, id),
      ("omit_accumulation_saturation", omitAccumulationSaturation, id),
      ("round_accumulated_time", accumulateNanos, roundTotals)] do
    match firstDifference candidate project with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
        IO.eprintln s!"worker metrics corpus missed broken family {name}"
        return 2
  return 0

def execute (args : List String) : IO UInt32 := do
  let args := match args with | "--" :: rest => rest | _ => args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus; return 0
  | ["--check", path] =>
      if (← IO.FS.readFile path) == corpus then return 0
      IO.eprintln s!"stale worker metrics corpus: {path}"
      return 1
  | ["--output-stages", path] => IO.FS.writeFile path StageMetrics.corpus; return 0
  | ["--check-stages", path] =>
      if (← IO.FS.readFile path) == StageMetrics.corpus then return 0
      IO.eprintln s!"stale stage metrics corpus: {path}"
      return 1
  | ["--output-stages-cli", path] => IO.FS.writeFile path StageMetrics.publicCorpus; return 0
  | ["--check-stages-cli", path] =>
      if (← IO.FS.readFile path) == StageMetrics.publicCorpus then return 0
      IO.eprintln s!"stale public stage metrics corpus: {path}"
      return 1
  | ["--output-validation", path] => IO.FS.writeFile path MetricsValidation.corpus; return 0
  | ["--check-validation", path] =>
      if (← IO.FS.readFile path) == MetricsValidation.corpus then return 0
      IO.eprintln s!"stale metrics validation corpus: {path}"
      return 1
  | ["--sensitivity"] =>
      let status ← sensitivity
      if status != 0 then return status
      let status ← StageMetrics.sensitivity
      if status != 0 then return status
      MetricsValidation.sensitivity
  | ["--stats"] =>
      let steps := fixtures.foldl (fun count item => count + item.events.length) 0
      IO.println s!"alphabet={alphabet.length} exhaustive_depth=3 cases={fixtures.length} steps={steps}"
      return 0
  | _ =>
      IO.eprintln "usage: generate_worker_metrics (--output PATH | --check PATH | --output-stages PATH | --check-stages PATH | --output-stages-cli PATH | --check-stages-cli PATH | --output-validation PATH | --check-validation PATH | --sensitivity | --stats)"
      return 2

end HoiminOracle.WorkerMetrics

def main := HoiminOracle.WorkerMetrics.execute
