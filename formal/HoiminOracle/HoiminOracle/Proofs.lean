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

theorem stop_during_cleaning_is_noop (state : State) (cause : StopCause)
    (cleaning : state.phase = .cleaning) :
    (step state (.stop cause)).state = state ∧
      (step state (.stop cause)).emitted = [] := by
  simp [step, stop, cleaning]

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

theorem completion_is_accepted_at_most_once (state : State) (id : Nat)
    (accepted : (step state (.complete id .ordinary)).rejection = none) :
    (step (step state (.complete id .ordinary)).state (.complete id .ordinary)).errorCode? =
      some "machine.effect.duplicate" := by
  by_cases retired : id ∈ state.retired
  · simp [step, complete, retired, reject] at accepted
  · by_cases duplicate : id ∈ state.completed
    · simp [step, complete, retired, duplicate, reject] at accepted
    · cases pending : pendingKind? state.pending id with
      | none => simp [step, complete, retired, duplicate, pending, reject] at accepted
      | some expected =>
          by_cases correct : expected = .ordinary
          · subst expected
            cases stopCause : state.stopCause <;>
              simp [step, complete, retired, duplicate, pending, acceptCompletion,
                stopCause, reject, Verdict.errorCode?]
          · simp [step, complete, retired, duplicate, pending, correct, reject] at accepted

theorem repeated_stop_emits_cleanup_once (state : State) (first second : StopCause)
    (running : state.phase = .running) (unset : state.stopCause = none) :
    (step state (.stop first)).emitted = [.cleanup] ∧
      (step (step state (.stop first)).state (.stop second)).emitted = [] := by
  simp [step, stop, running, unset]

theorem final_output_follows_cleanup_completion (state : State) (event : Event)
    (emitted : .finalOutput ∈ (step state event).emitted) :
    ∃ id, event = .complete id .cleanup := by
  cases event with
  | stop cause =>
      cases phase : state.phase <;> cases stopCause : state.stopCause <;>
        simp [step, stop, phase, stopCause] at emitted
  | complete id kind =>
      by_cases retired : id ∈ state.retired
      · simp [step, complete, retired, reject] at emitted
      · by_cases duplicate : id ∈ state.completed
        · simp [step, complete, retired, duplicate, reject] at emitted
        · cases pending : pendingKind? state.pending id with
          | none => simp [step, complete, retired, duplicate, pending, reject] at emitted
          | some expected =>
              by_cases correct : expected = kind
              · cases kind with
                | ordinary =>
                    cases stopCause : state.stopCause <;>
                      simp [step, complete, retired, duplicate, pending, correct,
                        acceptCompletion, stopCause] at emitted
                | cleanup => exact ⟨id, rfl⟩
                | finalOutput =>
                    simp [step, complete, retired, duplicate, pending, correct,
                      acceptCompletion] at emitted
              · simp [step, complete, retired, duplicate, pending, correct, reject] at emitted

theorem stop_preserves_accepted_results (state : State) (cause : StopCause) :
    (step state (.stop cause)).state.acceptedResults = state.acceptedResults := by
  cases phase : state.phase <;> cases stopCause : state.stopCause <;>
    simp [step, stop, phase, stopCause]

theorem lifecycle_emits_cleanup_then_final :
    let state := State.withPending 1 .ordinary
    let stopped := step state (.stop .cancelled)
    let cleaned := step stopped.state (.complete 2 .cleanup)
    stopped.emitted ++ cleaned.emitted = [.cleanup, .finalOutput] := by decide

theorem lifecycle_final_output_is_emitted_once :
    let state := State.withPending 1 .ordinary
    let stopped := step state (.stop .cancelled)
    let cleaned := step stopped.state (.complete 2 .cleanup)
    let duplicate := step cleaned.state (.complete 2 .cleanup)
    cleaned.emitted = [.finalOutput] ∧ duplicate.emitted = [] := by decide

theorem accepted_result_survives_stop_interleaving :
    let state := State.withPending 1 .ordinary
    let accepted := step state (.complete 1 .ordinary)
    let stopped := step accepted.state (.stop .cancelled)
    accepted.state.acceptedResults = 1 ∧ stopped.state.acceptedResults = 1 := by decide

end HoiminOracle
