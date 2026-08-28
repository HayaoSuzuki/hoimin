import HoiminOracle.DiskGuardCases

namespace HoiminOracle.DiskGuard

theorem accept_implies_invariant {candidate result : State}
    (accepted : accept candidate = some result) : Invariant result = true := by
  unfold accept at accepted
  split at accepted <;> simp_all

theorem acceptStep_implies_contract {start candidate result : State}
    (accepted : acceptStep start candidate = some result) :
    result = candidate ∧ Invariant result = true ∧ TransitionInvariant start result = true := by
  unfold acceptStep at accepted
  split at accepted
  · simp_all [Bool.and_eq_true]
  · simp at accepted

theorem step_implies_invariant {start result : State} {event : Event}
    (accepted : step start event = some result) : Invariant result = true := by
  cases transitionResult : transition start event with
  | none => simp [step, transitionResult] at accepted
  | some candidate =>
      have contract : acceptStep start candidate = some result := by
        simpa [step, transitionResult] using accepted
      exact (acceptStep_implies_contract contract).2.1

theorem step_implies_transition_invariant {start result : State} {event : Event}
    (accepted : step start event = some result) :
    TransitionInvariant start result = true := by
  cases transitionResult : transition start event with
  | none => simp [step, transitionResult] at accepted
  | some candidate =>
      have contract : acceptStep start candidate = some result := by
        simpa [step, transitionResult] using accepted
      exact (acceptStep_implies_contract contract).2.2

theorem run_preserves_invariant {start result : State} {events : List Event}
    (startValid : Invariant start = true) (accepted : run start events = some result) :
    Invariant result = true := by
  induction events generalizing start with
  | nil =>
      simp [run] at accepted
      subst result
      exact startValid
  | cons event rest induction =>
      cases stepResult : step start event with
      | none => simp [run, stepResult] at accepted
      | some next =>
          have nextValid := step_implies_invariant stepResult
          apply induction nextValid
          simpa [run, stepResult] using accepted

private theorem stoppedStepContract {start result : State} {event : Event}
    (stopped : start.stop.isSome = true)
    (accepted : step start event = some result) :
    result.stop = start.stop ∧ result.dispatched = start.dispatched := by
  have contract := step_implies_transition_invariant accepted
  simp [TransitionInvariant, stopped] at contract
  exact ⟨by simpa only [beq_iff_eq] using contract.1, contract.2⟩

theorem stopped_never_dispatches {start result : State} {events : List Event}
    (startValid : Invariant start = true) (stopped : start.stop.isSome = true)
    (accepted : run start events = some result) :
    result.dispatched = start.dispatched := by
  induction events generalizing start with
  | nil =>
      simp [run] at accepted
      subst result
      rfl
  | cons event rest induction =>
      cases stepResult : step start event with
      | none => simp [run, stepResult] at accepted
      | some next =>
          have contract := stoppedStepContract stopped stepResult
          have nextValid := step_implies_invariant stepResult
          have nextStopped : next.stop.isSome = true := by
            rw [contract.1]
            exact stopped
          calc
            result.dispatched = next.dispatched := by
              apply induction nextValid nextStopped
              simpa [run, stepResult] using accepted
            _ = start.dispatched := contract.2

theorem first_reason_is_sticky {start result : State} {events : List Event}
    (startValid : Invariant start = true) {reason : StopReason}
    (stopped : start.stop = some reason)
    (accepted : run start events = some result) : result.stop = some reason := by
  induction events generalizing start with
  | nil =>
      simp [run] at accepted
      subst result
      exact stopped
  | cons event rest induction =>
      cases stepResult : step start event with
      | none => simp [run, stepResult] at accepted
      | some next =>
          have stoppedSome : start.stop.isSome = true := by simp [stopped]
          have contract := stoppedStepContract stoppedSome stepResult
          have nextValid := step_implies_invariant stepResult
          have nextStopped : next.stop = some reason := by simpa [stopped] using contract.1
          apply induction nextValid nextStopped
          simpa [run, stepResult] using accepted

theorem simultaneous_threshold_preserves_secondary {start result : State}
    {owned maxOwned free minFree : Nat}
    (_startValid : Invariant start = true) (notStopped : start.stop = none)
    (sizeReached : owned ≥ maxOwned) (reserveReached : free ≤ minFree)
    (accepted : run start [.observe owned maxOwned free minFree] = some result) :
    result.stop = some .reserveReached ∧ .sizeExceeded ∈ result.secondaryStops := by
  have stepResult :
      step start (.observe owned maxOwned free minFree) = some result := by
    simpa [run] using accepted
  have notFinished : start.finished = false := by
    cases finished : start.finished with
    | false => rfl
    | true => simp [step, transition, finished] at stepResult
  have transitionResult :
      transition start (.observe owned maxOwned free minFree) =
        some (recordReasons start [.reserveReached, .sizeExceeded]) := by
    simp [transition, observationReasons, sizeReached, reserveReached, notFinished]
  have resultEq : result = recordReasons start [.reserveReached, .sizeExceeded] := by
    simp [step, transitionResult] at stepResult
    exact (acceptStep_implies_contract stepResult).1
  subst result
  simp only [recordReasons, notStopped]
  unfold addSecondary
  split <;> simp_all

private theorem finalInvariant {start result : State} {events : List Event}
    (startValid : Invariant start = true) (accepted : run start events = some result) :
    Invariant result = true := run_preserves_invariant startValid accepted

private theorem collectionOfInvariant {state : State} (valid : Invariant state = true) :
    collectionInvariant state = true := by
  simp only [Invariant, Bool.and_eq_true] at valid
  exact valid.1.1.1

private theorem cleanupOfInvariant {state : State} (valid : Invariant state = true) :
    cleanupInvariant state = true := by
  simp only [Invariant, Bool.and_eq_true] at valid
  exact valid.1.2

private theorem finishOfInvariant {state : State} (valid : Invariant state = true) :
    finishInvariant state = true := by
  simp only [Invariant, Bool.and_eq_true] at valid
  exact valid.2

theorem cleanup_requested_once_per_root {start result : State} {events : List Event}
    (startValid : Invariant start = true) (accepted : run start events = some result) :
    result.cleanupRequested.Nodup := by
  have collections := collectionOfInvariant (finalInvariant startValid accepted)
  simp only [collectionInvariant, Bool.and_eq_true, decide_eq_true_eq] at collections
  exact collections.1.1.1.1.2

theorem cleanup_failure_is_not_clean {start result : State} {events : List Event}
    (startValid : Invariant start = true) (accepted : run start events = some result) :
    rootsDisjoint result.cleanupClean result.cleanupFailed = true := by
  have cleanup := cleanupOfInvariant (finalInvariant startValid accepted)
  simp only [cleanupInvariant, Bool.and_eq_true] at cleanup
  exact cleanup.1.1.1.1.1.1.1.1.1

theorem cleanup_request_implies_components_settled {start result : State}
    {events : List Event} (startValid : Invariant start = true)
    (accepted : run start events = some result) :
    (result.cleanupRequested.isEmpty || safetyComponentsSettled result) = true := by
  have cleanup := cleanupOfInvariant (finalInvariant startValid accepted)
  simp only [cleanupInvariant, Bool.and_eq_true] at cleanup
  exact cleanup.1.1.1.2

theorem destructive_cleanup_implies_components_succeeded {start result : State}
    {events : List Event} (startValid : Invariant start = true)
    (accepted : run start events = some result) :
    ((result.cleanupClean.isEmpty && result.cleanupFailed.isEmpty) ||
      safetyComponentsSucceeded result) = true := by
  have cleanup := cleanupOfInvariant (finalInvariant startValid accepted)
  simp only [cleanupInvariant, Bool.and_eq_true] at cleanup
  have cleanSafe := cleanup.1.2
  have failedSafe := cleanup.2
  simp only [Bool.or_eq_true, Bool.and_eq_true] at cleanSafe failedSafe ⊢
  grind

theorem finished_implies_cleanup_terminal {start result : State} {events : List Event}
    (startValid : Invariant start = true) (accepted : run start events = some result) :
    result.finished = true → result.cleanupDeferred.isEmpty = true ∧
      result.ownedRoots.all (terminalOutcomeRecorded result) = true := by
  have finish := finishOfInvariant (finalInvariant startValid accepted)
  simp only [finishInvariant, Bool.and_eq_true] at finish
  have guard := finish.1.1.1
  intro finished
  simpa [finished] using guard

theorem finished_implies_components_settled {start result : State}
    {events : List Event} (startValid : Invariant start = true)
    (accepted : run start events = some result) :
    result.finished = true → safetyComponentsSettled result = true := by
  have finish := finishOfInvariant (finalInvariant startValid accepted)
  simp only [finishInvariant, Bool.and_eq_true] at finish
  have guard := finish.1.1.2
  intro finished
  have settled : result.active = 0 ∧ safetyComponentsSettled result = true := by
    simpa [finished] using guard
  exact settled.2

theorem delivery_cleanup_after_report_settled {start result : State}
    {events : List Event} (startValid : Invariant start = true)
    (accepted : run start events = some result) :
    (result.cleanupRequested.all fun root =>
      !result.deliveryRoots.contains root || reportSettled result) = true := by
  have cleanup := cleanupOfInvariant (finalInvariant startValid accepted)
  simp only [cleanupInvariant, Bool.and_eq_true] at cleanup
  exact cleanup.1.1.2

theorem finished_implies_report_succeeded {start result : State} {events : List Event}
    (startValid : Invariant start = true) (accepted : run start events = some result) :
    result.finished = true → result.report = .succeeded := by
  have finish := finishOfInvariant (finalInvariant startValid accepted)
  simp only [finishInvariant, Bool.and_eq_true] at finish
  have guard := finish.1.2
  intro finished
  have reportEqual : (result.report == ComponentState.succeeded) = true := by
    simpa [finished] using guard
  simpa only [beq_iff_eq] using reportEqual

theorem finished_implies_delivery_clean {start result : State} {events : List Event}
    (startValid : Invariant start = true) (accepted : run start events = some result) :
    result.finished = true →
      (result.deliveryRoots.all fun root => result.cleanupClean.contains root) = true := by
  have finish := finishOfInvariant (finalInvariant startValid accepted)
  simp only [finishInvariant, Bool.and_eq_true] at finish
  have guard := finish.2
  intro finished
  simpa [finished] using guard

theorem root_renaming_symmetry : rootRenamingCasesPass = true := by native_decide

theorem payload_class_symmetry : payloadSymmetryCasesPass = true := by native_decide

theorem fixed_cases_match_literal_expectations : casesPass = true := by native_decide

theorem fixed_case_contract_is_closed : corpusContractValid = true := by native_decide

theorem every_broken_family_has_a_witness : sensitivityPasses = true := by native_decide

end HoiminOracle.DiskGuard
