import HoiminOracle.WorkerMetricsProofs
import Lean.Data.Json

namespace HoiminOracle.WorkerMetrics

private structure Fixture where
  id : String
  events : List Event
  elapsed : Nat := 4
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

private def maxMillis : Nat := 18446744073709551615

private def schedule (items : List (Nat × WorkerId × Action)) : List Event :=
  items.map fun (time, id, action) => ⟨time, id, action⟩

private def exhaustiveFixtures : List Fixture :=
  alphabet.flatMap fun a => alphabet.flatMap fun b => alphabet.map fun c => {
    id := String.intercalate "-" ([a, b, c].map actionName)
    events := schedule [(1, .first, a), (2, .first, b), (3, .first, c)]
  }

private def fixtures : List Fixture := exhaustiveFixtures ++ [
  { id := "no-workers", events := [] },
  { id := "overlap-reverse-insertion", elapsed := 12, executed := 2,
    events := schedule [(1, .second, .queue), (2, .first, .queue), (3, .second, .start),
      (4, .first, .start), (10, .second, .finishCounted), (11, .first, .finishCounted)] },
  { id := "cancel-reuse-sticky-rejections", elapsed := 20, executed := 1,
    events := schedule [(1, .first, .queue), (4, .first, .cancel), (5, .first, .queue),
      (6, .first, .queue), (7, .first, .start), (9, .first, .start), (10, .first, .cancel),
      (11, .first, .finishCounted), (12, .first, .finishCounted)] },
  { id := "uncounted-work-then-reuse", elapsed := 15, executed := 1,
    events := schedule [(1, .first, .queue), (2, .first, .start), (6, .first, .finishUncounted),
      (6, .first, .queue), (7, .first, .start), (8, .first, .finishCounted),
      (9, .second, .queue), (10, .second, .start), (11, .second, .finishUncounted)] },
  { id := "lowest-outstanding-before-validation", elapsed := 7, discovered := 0, executed := 1,
    events := schedule [(1, .second, .queue), (2, .second, .start), (3, .first, .queue)] },
  { id := "second-outstanding", events := schedule [(1, .second, .queue)] },
  { id := "executed-exceeds-discovered", discovered := 0, executed := 1,
    events := schedule [(1, .first, .queue), (2, .first, .start), (3, .first, .finishCounted)] },
  { id := "boundary-uint64-max", elapsed := maxMillis, executed := 1,
    events := schedule [(0, .first, .queue), (maxMillis - 1, .first, .start),
      (maxMillis, .first, .finishCounted)] }
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
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "internal-fixture"), ("id", .str item.id),
    ("elapsed_ms", Lean.toJson item.elapsed), ("discovered", Lean.toJson item.discovered),
    ("executed", Lean.toJson item.executed),
    ("events", .arr (item.events.toArray.map fun event => Lean.Json.mkObj [
      ("at_ms", Lean.toJson event.time), ("worker", Lean.toJson (workerNumber event.id)),
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
      queueWait := worker.queueWait + (time - since) }, none)
  | _, _ => applyAction worker time action

private def firstDifference (candidate : Worker → Nat → Action → Worker × Option Rejection) :
    Option Fixture :=
  fixtures.find? fun item =>
    let (workers, decisions) := replay {} item.events
    let (changed, changedDecisions) := replayWith (recordWith candidate) {} item.events
    decisions != changedDecisions || finishFixture item workers != finishFixture item changed

private def sensitivity : IO UInt32 := do
  for (name, candidate) in [("reset_duplicate_queue", resetDuplicateQueue),
      ("count_uncounted_work", countUncounted), ("charge_cancelled_wait", chargeCancelledWait)] do
    match firstDifference candidate with
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
  | ["--sensitivity"] => sensitivity
  | ["--stats"] =>
      let steps := fixtures.foldl (fun count item => count + item.events.length) 0
      IO.println s!"alphabet={alphabet.length} exhaustive_depth=3 cases={fixtures.length} steps={steps}"
      return 0
  | _ =>
      IO.eprintln "usage: generate_worker_metrics (--output PATH | --check PATH | --sensitivity | --stats)"
      return 2

end HoiminOracle.WorkerMetrics

def main := HoiminOracle.WorkerMetrics.execute
