import HoiminOracle.MetricsValidationModel
import Lean.Data.Json

namespace HoiminOracle.MetricsValidation

private structure Fixture where
  id : String
  metrics : Metrics

private def stages : List (String × List Stage) := [
  ("stages-valid", [⟨"zeta", 4⟩, ⟨"alpha", 10⟩]),
  ("stage-too-long", [⟨"long", 11⟩]),
  ("stage-duplicate", [⟨"dup", 3⟩, ⟨"dup", 3⟩]),
  ("stage-duplicate-and-long", [⟨"dup", 3⟩, ⟨"dup", 11⟩]),
  ("stage-long-before-duplicate", [⟨"dup", 11⟩, ⟨"dup", 3⟩])]

private def workers : List (String × List Worker) := [
  ("workers-valid", [⟨0, 6, 4, 1⟩, ⟨7, 5, 5, 0⟩]),
  ("worker-duplicate-and-busy", [⟨0, 1, 2, 1⟩, ⟨0, 11, 0, 0⟩]),
  ("worker-descending", [⟨9, 1, 0, 1⟩, ⟨3, 0, 1, 0⟩]),
  ("worker-busy-and-queue", [⟨0, 11, 11, 1⟩]),
  ("worker-descending-and-queue", [⟨9, 0, 0, 1⟩, ⟨0, 0, 11, 0⟩]),
  ("worker-combined", [⟨0, 6, 5, 1⟩]),
  ("worker-long-before-duplicate", [⟨9, 0, 11, 1⟩, ⟨9, 0, 0, 0⟩])]

private def counts : List (String × Nat × Nat) :=
  [("counts-valid", 1, 1), ("counts-exceeds", 0, 1), ("counts-mismatch", 3, 2)]

private def matrix : List Fixture :=
  [1, 2].flatMap fun version =>
    ["oracle-run", ""].flatMap fun runId =>
      stages.flatMap fun (stageName, stageList) =>
        workers.flatMap fun (workerName, workerList) =>
          counts.map fun (countName, discovered, executed) => {
            id := String.intercalate "_" [s!"v{version}", if runId.isEmpty then "empty" else "named",
              stageName, workerName, countName]
            metrics := ⟨version, runId, 10, stageList, workerList, discovered, executed⟩
          }

private def processEdge (totalFirst totalSecond discovered executed : Nat) : Metrics := {
  workers := [⟨0, 0, 0, totalFirst⟩, ⟨1, 0, 0, totalSecond⟩]
  discovered := discovered
  executed := executed
}

private def fixtures : List Fixture := matrix ++ [
  ⟨"empty", { elapsed := 0 }⟩,
  ⟨"literal-names", { runId := " ", stages := [⟨"", 0⟩, ⟨"é", 0⟩, ⟨"é", 0⟩] }⟩,
  ⟨"maximum-worker-id", { workers := [⟨4294967295, 0, 0, 0⟩] }⟩,
  ⟨"elapsed-sum-at-maximum", { elapsed := maxCounter, workers := [⟨0, maxCounter - 1, 1, 0⟩] }⟩,
  ⟨"elapsed-sum-overflows", { elapsed := maxCounter, workers := [⟨0, maxCounter, 1, 0⟩] }⟩,
  ⟨"process-sum-at-maximum", processEdge (maxCounter - 1) 1 maxCounter maxCounter⟩,
  ⟨"process-sum-overflows-before-mismatch", processEdge maxCounter 1 0 0⟩,
  ⟨"executed-exceeds-before-process-overflow", processEdge maxCounter 1 0 1⟩,
  ⟨"descending-before-process-overflow", {
    (processEdge maxCounter 1 maxCounter maxCounter) with
      workers := [⟨7, 0, 0, maxCounter⟩, ⟨3, 0, 0, 1⟩] }⟩,
  ⟨"duplicate-after-descending", { workers := [⟨9, 0, 0, 0⟩, ⟨3, 0, 0, 0⟩, ⟨9, 0, 0, 0⟩] }⟩]

private def invalid (message : String) : Lean.Json :=
  Lean.Json.mkObj [("kind", .str "invalid"), ("message", .str message)]

private def issueJson : Option Issue → Lean.Json
  | none => Lean.Json.mkObj [("kind", .str "valid")]
  | some (.version actual) => invalid s!"unsupported metrics schema version {actual}"
  | some .emptyId => invalid "run ID must not be empty"
  | some (.duplicateStage name) => invalid s!"duplicate stage metric {name}"
  | some (.stageElapsed name) => invalid s!"stage {name} elapsed time exceeds run elapsed time"
  | some (.duplicateWorker id) => invalid s!"duplicate worker metric {id}"
  | some (.workerBusy id) => invalid s!"worker {id} busy time exceeds run elapsed time"
  | some (.workerQueue id) => invalid s!"worker {id} queue wait time exceeds run elapsed time"
  | some (.workerCombined id) =>
      invalid s!"worker {id} combined busy and queue wait time exceeds run elapsed time"
  | some .workerOrder => invalid "worker metrics must be ordered by worker ID"
  | some (.executedExceedsDiscovered discovered executed) => Lean.Json.mkObj [
      ("kind", .str "executed_exceeds_discovered"), ("discovered", Lean.toJson discovered),
      ("executed", Lean.toJson executed)]
  | some .processOverflow => Lean.Json.mkObj [("kind", .str "worker_process_overflow")]
  | some (.processMismatch total executed) => Lean.Json.mkObj [
      ("kind", .str "worker_process_mismatch"), ("worker_processes", Lean.toJson total),
      ("executed", Lean.toJson executed)]

private def metricsJson (metrics : Metrics) : Lean.Json := Lean.Json.mkObj [
  ("schema_version", Lean.toJson metrics.version), ("run_id", .str metrics.runId),
  ("elapsed_ms", Lean.toJson metrics.elapsed),
  ("stages", .arr (metrics.stages.toArray.map fun stage => Lean.Json.mkObj [
    ("name", .str stage.name), ("elapsed_ms", Lean.toJson stage.elapsed)])),
  ("workers", .arr (metrics.workers.toArray.map fun worker => Lean.Json.mkObj [
    ("worker", Lean.toJson worker.id), ("busy_ms", Lean.toJson worker.busy),
    ("queue_wait_ms", Lean.toJson worker.queueWait), ("processes", Lean.toJson worker.processes)])),
  ("discovered", Lean.toJson metrics.discovered), ("executed", Lean.toJson metrics.executed)]

private def fixtureJson (item : Fixture) : Lean.Json := Lean.Json.mkObj [
  ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
  ("metrics", metricsJson item.metrics), ("expected", issueJson (validate item.metrics))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def workersFirst (m : Metrics) : Option Issue :=
  (headerIssues m ++ workerIssues m.elapsed [] m.workers ++ stageIssues m.elapsed [] m.stages ++
    orderIssues m.workers).head?.or (checkCounts (workerTotal m.workers) m.discovered m.executed)

private def orderBeforeRows (m : Metrics) : Option Issue :=
  (headerIssues m ++ stageIssues m.elapsed [] m.stages ++ orderIssues m.workers ++
    workerIssues m.elapsed [] m.workers).head?.or (checkCounts (workerTotal m.workers) m.discovered m.executed)

private def wrapProcessCounts (m : Metrics) : Option Issue :=
  (structuralIssues m).head?.or
    (checkCounts (workerTotal m.workers % (maxCounter + 1)) m.discovered m.executed)

private def clampCombinedTime (m : Metrics) : Option Issue :=
  let allowed := (structuralIssues m).filter fun issue => match issue with
    | .workerCombined _ => m.elapsed != maxCounter
    | _ => true
  allowed.head?.or (checkCounts (workerTotal m.workers) m.discovered m.executed)

def sensitivity : IO UInt32 := do
  for (name, candidate) in [("workers_first", workersFirst), ("order_before_rows", orderBeforeRows),
      ("wrap_process_counts", wrapProcessCounts), ("clamp_combined_time", clampCombinedTime)] do
    match fixtures.find? (fun item => candidate item.metrics != validate item.metrics) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
        IO.eprintln s!"metrics validation corpus missed broken family {name}"
        return 2
  return 0

end HoiminOracle.MetricsValidation
