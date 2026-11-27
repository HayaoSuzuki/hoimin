import HoiminOracle.WorkerMetricsModel

namespace HoiminOracle.WorkerMetrics

def Phase.cutoff (time : Nat) : Phase → Nat
  | .idle => time
  | .queued since | .running since => since

def Worker.withinBudget (worker : Worker) (time : Nat) : Prop :=
  worker.queueWait + worker.busy ≤ worker.phase.cutoff time ∧ worker.phase.cutoff time ≤ time

theorem rejected_operation_is_transactional (worker : Worker) (time : Nat) (action : Action)
    (reason : Rejection) (rejected : transition worker time action = .error reason) :
    (applyAction worker time action).1 = worker := by
  simp [applyAction, rejected]

theorem accounting_stays_within_elapsed (worker : Worker) (action : Action) (before time : Nat)
    (valid : worker.withinBudget before) (monotonic : before ≤ time) :
    (applyAction worker time action).1.withinBudget time := by
  cases action <;> cases phase : worker.phase <;>
    simp_all [applyAction, transition, Worker.withinBudget, Worker.toIdle, Phase.cutoff] <;> omega

theorem queue_cancellation_preserves_accounting (worker : Worker) (since time : Nat)
    (queued : worker.phase = .queued since) :
    (applyAction worker time .cancel).1.toAccounting = worker.toAccounting := by
  simp [applyAction, transition, queued, Worker.toIdle]

theorem event_preserves_both_worker_budgets (workers : Workers) (event : Event) (before : Nat)
    (first : workers.first.withinBudget before) (second : workers.second.withinBudget before)
    (monotonic : before ≤ event.time) :
    let updated := (recordEvent workers event).1
    updated.first.withinBudget event.time ∧ updated.second.withinBudget event.time := by
  have keep (worker : Worker) (valid : worker.withinBudget before) :
      worker.withinBudget event.time := by
    cases phase : worker.phase <;> simp_all [Worker.withinBudget, Phase.cutoff] <;> omega
  cases id : event.id <;> simp [recordEvent, recordWith, id]
  · exact ⟨accounting_stays_within_elapsed _ _ _ _ first monotonic, keep _ second⟩
  · exact ⟨keep _ first, accounting_stays_within_elapsed _ _ _ _ second monotonic⟩

end HoiminOracle.WorkerMetrics
