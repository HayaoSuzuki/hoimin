import HoiminOracle.Model

namespace HoiminOracle

theorem rejected_preserves_state (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome = true) :
    (step state event).state = state := by
  cases event with
  | stop cause =>
      cases phase : state.phase <;> cases stopCause : state.stopCause <;>
        simp [step, stop, phase, stopCause] at rejected
  | complete id kind =>
      by_cases retired : id ∈ state.retired
      · simp [step, complete, retired, reject]
      · by_cases duplicate : id ∈ state.completed
        · simp [step, complete, retired, duplicate, reject]
        · cases pending : pendingKind? state.pending id with
          | none => simp [step, complete, retired, duplicate, pending, reject]
          | some expected =>
              by_cases correct : expected = kind
              · cases kind with
                | ordinary =>
                    cases stopCause : state.stopCause <;>
                      simp [step, complete, retired, duplicate, pending, correct,
                        acceptCompletion, stopCause] at rejected
                | cleanup =>
                    simp [step, complete, retired, duplicate, pending, correct,
                      acceptCompletion] at rejected
                | finalOutput =>
                    simp [step, complete, retired, duplicate, pending, correct,
                      acceptCompletion] at rejected
              · simp [step, complete, retired, duplicate, pending, correct, reject]

theorem accepted_completion_not_pending (state : State) (id : Nat) (kind : EffectKind)
    (fresh : id ≠ state.nextId)
    (accepted : (step state (.complete id kind)).rejection = none) :
    id ∉ (step state (.complete id kind)).state.pending.map Prod.fst := by
  by_cases retired : id ∈ state.retired
  · simp [step, complete, retired, reject] at accepted
  · by_cases duplicate : id ∈ state.completed
    · simp [step, complete, retired, duplicate, reject] at accepted
    · cases pending : pendingKind? state.pending id with
      | none => simp [step, complete, retired, duplicate, pending, reject] at accepted
      | some expected =>
          by_cases correct : expected = kind
          · cases kind with
            | ordinary =>
                cases stopCause : state.stopCause <;>
                  simp [step, complete, retired, duplicate, pending, correct,
                    acceptCompletion, removePending, fresh, stopCause]
            | cleanup =>
                simp [step, complete, retired, duplicate, pending, correct,
                  acceptCompletion, fresh]
            | finalOutput =>
                simp [step, complete, retired, duplicate, pending, correct,
                  acceptCompletion, removePending]
          · simp [step, complete, retired, duplicate, pending, correct, reject] at accepted

theorem stop_is_first_writer_wins (state : State) (first second : StopCause)
    (running : state.phase = .running) (unset : state.stopCause = none) :
    (step (step state (.stop first)).state (.stop second)).state.stopCause = some first := by
  simp [step, stop, running, unset]

theorem late_stop_preserves_final (state : State) (cause : StopCause)
    (final : state.phase = .finalPending ∨ state.phase = .finished) :
    (step state (.stop cause)).state = state := by
  rcases final with finalPending | finished
  · simp [step, stop, finalPending]
  · simp [step, stop, finished]

theorem no_ordinary_emission_after_stop (state : State) (event : Event)
    (stopped : state.stopCause.isSome = true) :
    .ordinary ∉ (step state event).emitted := by
  cases event with
  | stop cause =>
      cases phase : state.phase <;> cases stopCause : state.stopCause <;>
        simp [step, stop, phase, stopCause]
  | complete id kind =>
      by_cases retired : id ∈ state.retired
      · simp [step, complete, retired, reject]
      · by_cases duplicate : id ∈ state.completed
        · simp [step, complete, retired, duplicate, reject]
        · cases pending : pendingKind? state.pending id with
          | none => simp [step, complete, retired, duplicate, pending, reject]
          | some expected =>
              by_cases correct : expected = kind
              · cases kind with
                | ordinary =>
                    cases stopCause : state.stopCause
                    · simp [stopCause] at stopped
                    · simp [step, complete, retired, duplicate, pending, correct,
                        acceptCompletion, stopCause]
                | cleanup =>
                    simp [step, complete, retired, duplicate, pending, correct,
                      acceptCompletion]
                | finalOutput =>
                    simp [step, complete, retired, duplicate, pending, correct,
                      acceptCompletion]
              · simp [step, complete, retired, duplicate, pending, correct, reject]

end HoiminOracle
