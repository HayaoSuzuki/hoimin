import HoiminOracle.WorkerMetricsModel

namespace HoiminOracle.MetricsValidation

abbrev maxCounter := WorkerMetrics.maxMillis

structure Stage where
  name : String
  elapsed : Nat

structure Worker where
  id : Nat
  busy : Nat := 0
  queueWait : Nat := 0
  processes : Nat := 0

structure Metrics where
  version : Nat := 1
  runId : String := "oracle-run"
  elapsed : Nat := 10
  stages : List Stage := []
  workers : List Worker := []
  discovered : Nat := 0
  executed : Nat := 0

inductive Issue
  | version (actual : Nat)
  | emptyId
  | duplicateStage (name : String)
  | stageElapsed (name : String)
  | duplicateWorker (id : Nat)
  | workerBusy (id : Nat)
  | workerQueue (id : Nat)
  | workerCombined (id : Nat)
  | workerOrder
  | executedExceedsDiscovered (discovered executed : Nat)
  | processOverflow
  | processMismatch (total executed : Nat)
  deriving DecidableEq, BEq

private def when (test : Bool) (issue : Issue) : List Issue := if test then [issue] else []

def headerIssues (metrics : Metrics) : List Issue :=
  when (metrics.version != 1) (.version metrics.version) ++ when metrics.runId.isEmpty .emptyId

def stageIssues (elapsed : Nat) (seen : List String) : List Stage → List Issue
  | [] => []
  | stage :: rest =>
      when (seen.contains stage.name) (.duplicateStage stage.name) ++
      when (stage.elapsed > elapsed) (.stageElapsed stage.name) ++
      stageIssues elapsed (stage.name :: seen) rest

def workerIssues (elapsed : Nat) (seen : List Nat) : List Worker → List Issue
  | [] => []
  | worker :: rest =>
      when (seen.contains worker.id) (.duplicateWorker worker.id) ++
      when (worker.busy > elapsed) (.workerBusy worker.id) ++
      when (worker.queueWait > elapsed) (.workerQueue worker.id) ++
      when (worker.busy + worker.queueWait > elapsed) (.workerCombined worker.id) ++
      workerIssues elapsed (worker.id :: seen) rest

def orderIssues (workers : List Worker) : List Issue :=
  when ((workers.zip workers.tail).any (fun (first, second) => second.id ≤ first.id)) .workerOrder

def workerTotal (workers : List Worker) : Nat := (workers.map Worker.processes).sum

def checkCounts (total discovered executed : Nat) : Option Issue :=
  if executed > discovered then some (.executedExceedsDiscovered discovered executed)
  else if total > maxCounter then some .processOverflow
  else if total != executed then some (.processMismatch total executed)
  else none

def structuralIssues (metrics : Metrics) : List Issue :=
  headerIssues metrics ++ stageIssues metrics.elapsed [] metrics.stages ++
    workerIssues metrics.elapsed [] metrics.workers ++ orderIssues metrics.workers

def validate (metrics : Metrics) : Option Issue :=
  (structuralIssues metrics).head?.or
    (checkCounts (workerTotal metrics.workers) metrics.discovered metrics.executed)

theorem accepted_counts_are_exact (metrics : Metrics) (h : validate metrics = none) :
    workerTotal metrics.workers = metrics.executed ∧
      metrics.executed ≤ metrics.discovered ∧ workerTotal metrics.workers ≤ maxCounter := by
  unfold validate at h
  cases issues : (structuralIssues metrics).head? <;> simp [issues] at h
  unfold checkCounts at h
  split at h
  · simp at h
  split at h
  · simp at h
  split at h
  · simp at h
  simp_all

private theorem no_worker_issues_implies_bound (elapsed : Nat) (seen : List Nat)
    (workers : List Worker) (worker : Worker) (member : worker ∈ workers)
    (clean : workerIssues elapsed seen workers = []) : worker.busy + worker.queueWait ≤ elapsed := by
  induction workers generalizing seen with
  | nil => simp at member
  | cons first rest ih =>
    simp only [List.mem_cons] at member
    simp only [workerIssues, List.append_eq_nil_iff] at clean
    rcases member with equal | later
    · subst worker
      have bound := clean.1.2
      by_cases exceeds : elapsed < first.busy + first.queueWait
      · simp [when, exceeds] at bound
      · omega
    · exact ih (first.id :: seen) later clean.2

theorem accepted_worker_time_fits (metrics : Metrics) (worker : Worker)
    (member : worker ∈ metrics.workers) (accepted : validate metrics = none) :
    worker.busy + worker.queueWait ≤ metrics.elapsed := by
  have clean : structuralIssues metrics = [] := by
    cases issues : structuralIssues metrics with
    | nil => rfl
    | cons issue rest => simp [validate, issues] at accepted
  simp only [structuralIssues, List.append_eq_nil_iff] at clean
  exact no_worker_issues_implies_bound _ [] _ worker member clean.1.2

-- Rust uses checked addition. Under the u64 elapsed bound, overflow
-- and a representable sum above elapsed have the same validation rule.
theorem checked_sum_bound (busy wait elapsed : Nat) (fits : elapsed ≤ maxCounter) :
    (busy + wait > maxCounter ∨ busy + wait > elapsed) ↔ busy + wait > elapsed := by
  omega

end HoiminOracle.MetricsValidation
