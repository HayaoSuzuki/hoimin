import HoiminOracle.WorkerMetricsModel

namespace HoiminOracle.WorkerMetrics

def Phase.cutoff (time : Nat) : Phase → Nat
  | .idle => time
  | .queued since | .running since => since

def Worker.withinBudget (worker : Worker) (time : Nat) : Prop :=
  nanosPerMilli * (worker.queueWait + worker.busy) ≤ worker.phase.cutoff time ∧
    worker.phase.cutoff time ≤ time

theorem rounded_interval_fits (nanos : Nat) : nanosPerMilli * millis nanos ≤ nanos := by
  have bounded := Nat.min_le_left (nanos / nanosPerMilli) maxMillis
  exact Nat.le_trans (Nat.mul_le_mul_left _ bounded) (Nat.mul_div_le _ _)

private theorem increment_fits (accumulated interval : Nat) :
    nanosPerMilli * addMillis accumulated interval ≤ nanosPerMilli * accumulated + interval := by
  have bounded := Nat.min_le_left (accumulated + millis interval) maxMillis
  have intervalBound := rounded_interval_fits interval
  exact Nat.le_trans (Nat.mul_le_mul_left _ bounded) (by
    rw [Nat.mul_add]
    exact Nat.add_le_add_left intervalBound _)

theorem rejected_operation_is_transactional (worker : Worker) (time : Nat) (action : Action)
    (reason : Rejection) (rejected : transition worker time action = .error reason) :
    (applyAction worker time action).1 = worker := by
  simp [applyAction, rejected]

theorem accounting_stays_within_elapsed (worker : Worker) (action : Action) (before time : Nat)
    (valid : worker.withinBudget before) (monotonic : before ≤ time) :
    (applyAction worker time action).1.withinBudget time := by
  cases action <;> cases phase : worker.phase <;>
    simp_all [applyAction, transition, Worker.withinBudget, Worker.toIdle, Phase.cutoff]
  all_goals
    simp only [nanosPerMilli, Nat.mul_add] at *
    first
    | omega
    | (rename_i since
       have queueBound := increment_fits worker.queueWait (time - since)
       have busyBound := increment_fits worker.busy (time - since)
       simp only [nanosPerMilli] at queueBound busyBound
       omega)

theorem published_millis_fit (worker : Worker) (time : Nat)
    (valid : worker.withinBudget time) (representable : time / nanosPerMilli ≤ maxMillis) :
    worker.queueWait + worker.busy ≤ millis time := by
  have upper := Nat.le_trans valid.1 valid.2
  rw [millis, Nat.min_eq_left representable]
  simp only [nanosPerMilli] at *
  omega

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
    cases phase : worker.phase <;>
      simp_all [Worker.withinBudget, Phase.cutoff, nanosPerMilli] <;> omega
  cases id : event.id <;> simp [recordEvent, recordWith, id]
  · exact ⟨accounting_stays_within_elapsed _ _ _ _ first monotonic, keep _ second⟩
  · exact ⟨keep _ first, accounting_stays_within_elapsed _ _ _ _ second monotonic⟩

end HoiminOracle.WorkerMetrics
