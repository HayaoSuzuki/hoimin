import HoiminOracle.ShutdownModel

namespace HoiminOracle.ShutdownAudit

private theorem beq_false_of_ne {α : Type} [BEq α] [LawfulBEq α]
    {left right : α} (different : left ≠ right) :
    (left == right) = false := by
  apply Bool.eq_false_iff.mpr
  intro equal
  exact different (beq_iff_eq.mp equal)

theorem rejected_preserves_state (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome = true) :
    (step state event).state = state := by
  cases event <;>
    simp [step, accept, reject, installError, readyForReport,
      readyForSuccess, settled, processTerminal] at rejected ⊢
  all_goals
    repeat
      first
      | split
      | simp_all

theorem first_cause_is_retained (state : State) (event : Event) (cause : StopCause)
    (present : state.cause = some cause)
    (notForced : event ≠ .secondInterrupt) :
    (step state event).state.cause = some cause := by
  cases event <;>
    simp [step, installError, accept, reject] at present notForced ⊢
  all_goals
    repeat
      first
      | split
      | simp_all

theorem first_deadline_is_not_extended (state : State) (event : Event) (ordinal : Nat)
    (present : state.deadlineOrdinal = some ordinal)
    (shutdownStarted : state.cause.isSome = true) :
    (step state event).state.deadlineOrdinal = some ordinal := by
  cases event <;>
    simp [step, installError, accept, reject] at present shutdownStarted ⊢
  all_goals
    repeat
      first
      | split
      | simp_all

theorem forced_interrupt_returns_130 (state : State)
    (active : state.returned = false) :
  let after := (step state .secondInterrupt).state
  after.exitCode = some 130 ∧ after.returned = true ∧ after.blocking ≠ .pending := by
  by_cases blocked : state.blocking = .pending
  · simp [step, active, accept, blocked]
  · have blockedBeq : (state.blocking == .pending) = false :=
      beq_false_of_ne blocked
    simp [step, active, accept, blocked, blockedBeq]

theorem primary_error_is_retained (state : State) (event : Event) (cause : StopCause)
    (present : state.primaryError = some cause) :
    (step state event).state.primaryError = some cause := by
  cases primary : state.primaryError with
  | none => simp [primary] at present
  | some existing =>
      have same : existing = cause := by simpa [primary] using present
      subst existing
      cases event <;>
        simp [step, installError, accept, reject, primary, beq_iff_eq]
      all_goals
        repeat
          first
          | split
          | simp_all

def StructuralInvariant (state : State) : Prop :=
  state.processStarts ≤ 1 ∧ state.terminationRequests ≤ 1 ∧ state.reaps ≤ 1 ∧
  state.cleanupDispatches ≤ 1 ∧ state.sessionDispatches ≤ 1 ∧
  state.reportDispatches ≤ 1 ∧ state.metricsDispatches ≤ 1

theorem step_preserves_structural_invariant (state : State) (event : Event)
    (holds : StructuralInvariant state) :
    StructuralInvariant (step state event).state := by
  cases event <;>
    simp [StructuralInvariant, step, installError, accept, reject,
      readyForReport, readyForSuccess, settled, processTerminal] at holds ⊢
  all_goals
    repeat
      first
      | split
      | simp_all

theorem singleton_dispatches_are_bounded (state : State) (event : Event)
    (holds : StructuralInvariant state) :
    let after := (step state event).state
    after.cleanupDispatches ≤ 1 ∧ after.sessionDispatches ≤ 1 ∧
      after.reportDispatches ≤ 1 ∧ after.metricsDispatches ≤ 1 := by
  have preserved := step_preserves_structural_invariant state event holds
  exact ⟨preserved.2.2.2.1, preserved.2.2.2.2.1,
    preserved.2.2.2.2.2.1, preserved.2.2.2.2.2.2⟩

theorem reaped_never_runs_again (state : State) (event : Event)
    (reaped : state.process = .reaped) :
    (step state event).state.process ≠ .running := by
  have notStarted : state.process ≠ .notStarted := by
    rw [reaped]
    decide
  have notStartedBeq : (state.process == .notStarted) = false :=
    beq_false_of_ne notStarted
  have notRunning : state.process ≠ .running := by
    rw [reaped]
    decide
  cases event <;>
    simp [step, installError, accept, reject, notStartedBeq, notRunning]
  all_goals
    repeat
      first
      | split
      | simp_all

theorem detached_has_transferred_ownership (state : State)
    (holds : safe state = true) (detached : state.blocking = .detached) :
    state.blockingOwnershipTransferred = true := by
  cases ownership : state.blockingOwnershipTransferred <;>
    simp_all [safe]

theorem run_preserves_structural_invariant (state : State) (trace : List Event)
    (holds : StructuralInvariant state) :
    StructuralInvariant (run state trace) := by
  induction trace generalizing state with
  | nil => exact holds
  | cons event rest induction =>
      exact induction (step state event).state
        (step_preserves_structural_invariant state event holds)

end HoiminOracle.ShutdownAudit
