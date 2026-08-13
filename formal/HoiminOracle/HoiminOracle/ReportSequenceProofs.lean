import HoiminOracle.ReportSequenceModel

namespace HoiminOracle.ReportSequence

def Invariant (state : State) : Prop :=
  state.finished = true → state.active = []

theorem initial_invariant : Invariant initial := by
  simp [Invariant, initial]

theorem accepted_lifecycle_none (state : State) (event : Event)
    (accepted : (step state event).rejection = none) :
    lifecycleError? state event = none := by
  cases life : lifecycleError? state event with
  | none => rfl
  | some reason => simp [step, life, reject] at accepted

theorem accepted_mutant_none (state : State) (event : Event)
    (accepted : (step state event).rejection = none) :
    mutantError? state event = none := by
  have life := accepted_lifecycle_none state event accepted
  cases mutant : mutantError? state event with
  | none => rfl
  | some reason => simp [step, life, mutant, reject] at accepted

set_option maxHeartbeats 100000 in
theorem rejected_preserves_state (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome) :
    (step state event).state = state := by
  cases life : lifecycleError? state event with
  | some reason => simp [step, life, reject]
  | none =>
      cases mutant : mutantError? state event with
      | some reason => simp [step, life, mutant, reject]
      | none =>
          cases previous : state.last with
          | none => simp [step, life, mutant, previous] at rejected
          | some value =>
              by_cases monotonic : event.sequence ≤ value
              · simp [step, life, mutant, previous, monotonic, reject]
              · simp [step, life, mutant, previous, monotonic] at rejected

set_option maxHeartbeats 100000 in
theorem accepted_state (state : State) (event : Event)
    (accepted : (step state event).rejection = none) :
    (step state event).state = accept state event := by
  have life := accepted_lifecycle_none state event accepted
  have mutant := accepted_mutant_none state event accepted
  simp only [step, life, mutant]
  cases previous : state.last with
  | none => rfl
  | some value =>
      by_cases monotonic : event.sequence ≤ value
      · simp [step, life, mutant, previous, monotonic, reject] at accepted
      · simp [monotonic]

set_option maxHeartbeats 100000 in
theorem accepted_sequence_advances (state : State) (event : Event)
    (accepted : (step state event).rejection = none)
    (value : Nat) (previous : state.last = some value) :
    value < event.sequence := by
  have life := accepted_lifecycle_none state event accepted
  have mutant := accepted_mutant_none state event accepted
  have notLessOrEqual : ¬ event.sequence ≤ value := by
    intro lessOrEqual
    simp [step, life, mutant, previous, lessOrEqual, reject] at accepted
  omega

set_option maxHeartbeats 100000 in
theorem accepted_run_id_is_stable (state : State) (event : Event)
    (expected : RunId) (started : state.runId = some expected)
    (accepted : (step state event).rejection = none) :
    event.runId = expected := by
  have life := accepted_lifecycle_none state event accepted
  by_cases same : event.runId = expected
  · exact same
  · cases finished : state.finished with
    | true => simp [lifecycleError?, finished] at life
    | false =>
        cases kind : event.kind <;>
          simp [lifecycleError?, finished, started, kind, same] at life

theorem accepted_not_finished (state : State) (event : Event)
    (accepted : (step state event).rejection = none) :
    state.finished = false := by
  have life := accepted_lifecycle_none state event accepted
  cases finished : state.finished with
  | false => rfl
  | true => simp [lifecycleError?, finished] at life

theorem finished_is_terminal (state : State) (event : Event)
    (finished : state.finished = true) :
    (step state event).rejection =
      some (.runAlreadyFinished (state.runId.getD event.runId)) := by
  simp [step, lifecycleError?, finished, reject]

set_option maxHeartbeats 100000 in
theorem run_finish_requires_no_active (state : State) (event : Event)
    (kind : event.kind = .runFinished)
    (accepted : (step state event).rejection = none) :
    state.active = [] := by
  have mutant := accepted_mutant_none state event accepted
  simp [mutantError?, kind] at mutant
  exact mutant

set_option maxHeartbeats 100000 in
theorem finish_removes_active (state : State) (event : Event)
    (id : MutantId) (mutantSequence : Nat) (status : Status)
    (termination : Option Termination)
    (kind : event.kind = .mutantFinished id mutantSequence status termination)
    (accepted : (step state event).rejection = none) :
    (step state event).state.active =
      state.active.erase (id, mutantSequence) := by
  rw [accepted_state state event accepted]
  simp [accept, kind]

set_option maxHeartbeats 100000 in
theorem seen_identity_is_stable (state : State) (event : Event)
    (id : MutantId) (mutantSequence : Nat)
    (known : seenSequence? state.seen id = some mutantSequence)
    (accepted : (step state event).rejection = none) :
    seenSequence? (step state event).state.seen id = some mutantSequence := by
  rw [accepted_state state event accepted]
  have mutant := accepted_mutant_none state event accepted
  cases kind : event.kind with
  | runStarted | diagnostic | runFinished => simpa [accept, kind] using known
  | mutantFinished => simpa [accept, kind] using known
  | mutantStarted newId newSequence =>
      simp [mutantError?, kind] at mutant
      by_cases same : newId = id
      · subst newId
        simp [known] at mutant
        split at mutant <;> simp_all
        split at mutant <;> simp_all
      · simp [accept, kind, seenSequence?, same, known]

set_option maxHeartbeats 100000 in
theorem step_preserves_invariant (state : State) (event : Event)
    (holds : Invariant state) : Invariant (step state event).state := by
  by_cases accepted : (step state event).rejection = none
  · rw [accepted_state state event accepted]
    have notFinished := accepted_not_finished state event accepted
    cases kind : event.kind with
    | runFinished =>
        have empty := run_finish_requires_no_active state event kind accepted
        simp [Invariant, accept, kind, empty]
    | runStarted | mutantStarted | mutantFinished | diagnostic =>
        simp [Invariant, accept, kind, notFinished]
  · have rejected : (step state event).rejection.isSome := by
      cases rejection : (step state event).rejection with
      | none => simp_all
      | some reason => simp
    rw [rejected_preserves_state state event rejected]
    exact holds

set_option maxHeartbeats 100000 in
theorem run_preserves_from (state : State) (trace : List Event)
    (holds : Invariant state) : Invariant (run state trace) := by
  induction trace generalizing state with
  | nil => exact holds
  | cons event rest induction =>
      exact induction (step state event).state
        (step_preserves_invariant state event holds)

theorem run_preserves_invariant (trace : List Event) :
    Invariant (run initial trace) :=
  run_preserves_from initial trace initial_invariant

end HoiminOracle.ReportSequence
