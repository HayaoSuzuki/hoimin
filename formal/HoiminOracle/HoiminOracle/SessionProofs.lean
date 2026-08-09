import HoiminOracle.SessionModel

namespace HoiminOracle.SessionAudit

theorem rejected_preserves_durable (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome = true)
    (publicEvent : event.isPublic = true) :
    durable (step state event).state = durable state := by
  cases event <;>
    simp [Event.isPublic] at publicEvent <;>
    simp [step, openHandler, beginRun, loadRun, lookupResult, persistResult,
      finishRun, stopHandler, durable, reject, accept] at rejected ⊢
  all_goals
    repeat
      first
      | split
      | simp_all

theorem determinate_is_immutable
    (state : State) (handler : Handler) (old replacement : ResultRow)
    (present : findResult state old.run old.mutant = some old)
    (determinate : old.status.isDeterminate = true) :
    (step state (.persist handler old.run old.mutant replacement.status
      replacement.payload .valid)).state.results = state.results := by
  simp [step, persistResult]
  all_goals
    repeat
      first
      | split
      | simp_all [reject]

theorem failed_replacement_restores_old
    (state : State) (handler : Handler) (old replacement : ResultRow)
    (present : findResult state old.run old.mutant = some old) :
    findResult
      (step state (.persist handler old.run old.mutant replacement.status
        replacement.payload .invalidDiagnostic)).state
      old.run old.mutant = some old := by
  simp [step, persistResult]
  all_goals
    repeat
      first
      | split
      | simp_all [reject]

theorem successful_replacement_is_exact
    (state : State) (handler : Handler) (old replacement : ResultRow)
    (present : findResult state old.run old.mutant = some old)
    (inconclusive : old.status.isDeterminate = false)
    (accepted : (step state (.persist handler old.run old.mutant
      replacement.status replacement.payload .valid)).rejection = none) :
    SameExceptResult state
      (step state (.persist handler old.run old.mutant replacement.status
        replacement.payload .valid)).state
      { replacement with run := old.run, mutant := old.mutant } := by
  simp only [step] at accepted ⊢
  unfold persistResult at accepted ⊢
  split at accepted
  · simp_all [reject]
  · split at accepted
    · simp_all [reject]
    · split at accepted
      · simp_all [reject]
      · split at accepted
        · simp_all [reject]
        · rw [present]
          simp_all [SameExceptResult, accept]

private theorem filtered_owner_is_absent (owners : List (Run × Handler))
    (handler : Handler) :
    ((owners.filter fun owner => owner.2 != handler).filter
      fun owner => owner.2 == handler) = [] := by
  rw [List.filter_filter]
  apply List.filter_eq_nil_iff.mpr
  intro owner _ both
  generalize owner.2 = candidate at both
  cases candidate <;> cases handler <;> simp_all

private theorem filtered_run_is_absent (owners : List (Run × Handler))
    (run : Run) :
    ((owners.filter fun owner => owner.1 != run).filter
      fun owner => owner.1 == run) = [] := by
  rw [List.filter_filter]
  apply List.filter_eq_nil_iff.mpr
  intro owner _ both
  generalize owner.1 = candidate at both
  cases candidate <;> cases run <;> simp_all

private theorem ownerRuns_after_release (state : State) (handler : Handler) :
    ownerRuns (releaseHandler state handler) handler = [] := by
  simp only [ownerRuns, releaseHandler]
  rw [filtered_owner_is_absent]
  rfl

private theorem stopHandler_owners (state : State) (handler : Handler) :
    (stopHandler state handler).state.owners = (releaseHandler state handler).owners := by
  simp only [stopHandler, accept, setHandler]
  unfold clearPendingFor
  split <;> (try split) <;> rfl

theorem drop_releases_exactly_handler (state : State) (handler : Handler) :
    ownerRuns (step state (.drop handler)).state handler = [] := by
  simp only [step, ownerRuns]
  rw [stopHandler_owners]
  exact ownerRuns_after_release state handler

theorem crash_releases_exactly_handler (state : State) (handler : Handler) :
    ownerRuns (step state (.crash handler)).state handler = [] := by
  simp only [step, ownerRuns]
  rw [stopHandler_owners]
  exact ownerRuns_after_release state handler

theorem non_owner_finish_is_rejected_without_state_change
    (state : State) (handler : Handler) (run : Run) (complete : Bool)
    (live : handlerLive state handler = true)
    (notOwner : owns state run handler = false) :
    (step state (.finish handler run complete)).rejection.isSome = true ∧
      (step state (.finish handler run complete)).state = state := by
  simp [step, finishRun, live, notOwner, reject]

private theorem ownerCount_after_release (state : State) (run : Run) :
    ownerCount (releaseRun state run) run = 0 := by
  simp only [ownerCount, releaseRun]
  rw [filtered_run_is_absent]
  rfl

theorem successful_complete_has_no_owner
    (state : State) (handler : Handler) (run : Run)
    (accepted : (step state (.finish handler run true)).rejection = none) :
    ownerCount (step state (.finish handler run true)).state run = 0 := by
  simp only [step] at accepted ⊢
  unfold finishRun at accepted ⊢
  split at accepted
  · simp_all [reject]
  · split at accepted
    · simp_all [reject]
    · split at accepted
      · simp_all [reject]
      · split at accepted
        · simp_all [reject]
        · simp_all [accept, replaceRun]
          change ownerCount (releaseRun state run) run = 0
          exact ownerCount_after_release state run

theorem step_preserves_invariant (state : State) (event : Event)
    (holds : Invariant state) : Invariant (step state event).state := by
  cases event <;>
    simp [Invariant, step, openHandler, beginRun, loadRun, lookupResult,
      persistResult, finishRun, stopHandler, readLoadCandidate, acquireLoad,
      recheckLoad, startReplacement, commitReplacement, rollbackReplacement,
      setHandler, releaseHandler, releaseRun, addOwner, replaceRun,
      replaceResult, clearPendingFor, accept, reject] at holds ⊢
  all_goals
    repeat
      first
      | split
      | simp_all [List.length_append]

theorem run_preserves_invariant (state : State) (trace : List Event)
    (holds : Invariant state) : Invariant (run state trace) := by
  induction trace generalizing state with
  | nil => exact holds
  | cons event rest induction =>
      exact induction (step state event).state
        (step_preserves_invariant state event holds)

theorem initial_invariant : Invariant State.initial := by
  rfl

end HoiminOracle.SessionAudit
