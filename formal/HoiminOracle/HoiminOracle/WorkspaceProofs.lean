import HoiminOracle.WorkspaceModel

namespace HoiminOracle.WorkspaceAudit

theorem safe_active_workers_nodup (state : State)
    (holds : safe state = true) :
    (state.active.map Slot.worker).Nodup := by
  simp only [safe, Bool.and_eq_true, decide_eq_true_eq] at holds
  exact holds.1.1.1.1.1

theorem safe_pending_workers_nodup (state : State)
    (holds : safe state = true) :
    (state.pending.map Slot.worker).Nodup := by
  simp only [safe, Bool.and_eq_true, decide_eq_true_eq] at holds
  exact holds.1.1.1.1.2

theorem safe_task_ids_nodup (state : State)
    (holds : safe state = true) :
    (state.tasks.map Task.id).Nodup := by
  simp only [safe, Bool.and_eq_true, decide_eq_true_eq] at holds
  exact holds.1.1.1.2

theorem safe_generation_owners_nodup (state : State)
    (holds : safe state = true) :
    (ownedSlots state).Nodup := by
  simp only [safe, Bool.and_eq_true, decide_eq_true_eq] at holds
  exact holds.1.1.2

theorem safe_worker_owners_nodup (state : State)
    (holds : safe state = true) :
    ((ownedSlots state).map Slot.worker).Nodup := by
  simp only [safe, Bool.and_eq_true, decide_eq_true_eq] at holds
  exact holds.1.2

theorem released_has_no_owned_slots (state : State)
    (holds : safe state = true)
    (released : state.released = true) :
    ownedSlots state = [] := by
  simp only [safe, Bool.and_eq_true, decide_eq_true_eq] at holds
  simp_all

theorem cleanup_failure_preserves_state (state : State) :
    (cleanup state false).state = state ∧
      (cleanup state false).rejection = some .cleanupFailed := by
  simp [cleanup, reject]

theorem cleanup_success_is_exact (state : State)
    (idle : state.tasks.isEmpty = true) :
    (cleanup state true).state = {
      state with
        ready := false
        epoch := state.epoch + 1
        active := []
        pending := []
        released := true
    } ∧ (cleanup state true).rejection = none := by
  simp [cleanup, idle, acceptState]

theorem busy_cleanup_is_transactional (state : State)
    (busy : state.tasks.isEmpty = false) :
    (cleanup state true).state = state ∧
      (cleanup state true).rejection = some .workerBusy := by
  simp [cleanup, busy, reject]

theorem stale_completion_preserves_registered_ownership
    (state : State) (id : TaskId) (task : Task)
    (found : findTask state.tasks id = some task)
    (completed : (task.phase == .prepared) = false)
    (stale : (task.epoch != state.epoch) = true) :
    let verdict := acceptCompletion state id
    verdict.rejection = some .staleEpoch ∧
      verdict.state.active = state.active ∧
      verdict.state.pending = state.pending := by
  simp [acceptCompletion, found, completed, stale, reject]

theorem completed_absent_accepts_without_registration
    (state : State) (id : TaskId) (task : Task)
    (found : findTask state.tasks id = some task)
    (phase : task.phase = .completedAbsent)
    (current : task.epoch = state.epoch) :
    let verdict := acceptCompletion state id
    verdict.rejection = none ∧
      verdict.state.active = state.active ∧
      verdict.state.pending = state.pending := by
  simp [acceptCompletion, found, phase, current, acceptState]

theorem runWith_preserves_invariant
    (next : State → Event → Verdict)
    (preserves : ∀ state event,
      Invariant state → Invariant (next state event).state)
    (state : State)
    (trace : List Event)
    (holds : Invariant state) :
    Invariant (runWith next state trace) := by
  induction trace generalizing state with
  | nil => exact holds
  | cons event rest induction =>
      exact induction (next state event).state (preserves state event holds)

theorem rejected_step_preserves_registered_ownership
    (state : State) (event : Event)
    (unchanged : (step state event).state = state) :
    (step state event).state.active = state.active ∧
      (step state event).state.pending = state.pending := by
  simp [unchanged]

end HoiminOracle.WorkspaceAudit
