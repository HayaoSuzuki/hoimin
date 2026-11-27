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

theorem settled_process_drain_rejects_dispatch {state : State}
    (settled : state.processDrain ≠ .pending) :
    step state .dispatch = none := by
  simp [step, transition, settled]

theorem active_work_rejects_cleanup {state : State} {root : RootId}
    (active : state.active ≠ 0) :
    step state (.requestCleanup root) = none := by
  simp [step, transition, active]

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

theorem deferred_cleanup_rejects_finish (state : State) (deferred : state.cleanupDeferred ≠ []) :
    step state .finish = none := by
  simp [step, transition, deferred]

theorem recorded_cleanup_rejects_another_outcome (state : State) (root : RootId)
    (recorded : anyOutcomeRecorded state root = true) :
    step state (.cleanupSucceeded root) = none ∧ step state (.cleanupFailed root) = none ∧
      step state (.cleanupDeferred root) = none ∧ step state (.cleanupRetained root) = none := by
  simp [step, transition, recorded]

theorem finished_with_delivery_implies_safety_succeeded {start result : State}
    {events : List Event} (startValid : Invariant start = true)
    (accepted : run start events = some result) (finished : result.finished = true)
    (delivery : result.deliveryRoots ≠ []) : safetyComponentsSucceeded result = true := by
  have cleaned := finished_implies_delivery_clean startValid accepted finished
  have safe := destructive_cleanup_implies_components_succeeded startValid accepted
  cases deliveries : result.deliveryRoots with
  | nil => exact False.elim (delivery deliveries)
  | cons root rest =>
      rw [deliveries] at cleaned
      simp only [List.all_cons, Bool.and_eq_true] at cleaned
      cases cleanRoots : result.cleanupClean with
      | nil => simp [cleanRoots] at cleaned
      | cons first others => simpa [cleanRoots] using safe

theorem root_renaming_samples_match : rootRenamingCasesPass = true := by native_decide

theorem payload_class_samples_match : payloadSymmetryCasesPass = true := by native_decide

theorem fixed_cases_match_literal_expectations : casesPass = true := by native_decide

theorem fixed_case_contract_is_closed : corpusContractValid = true := by native_decide

theorem every_broken_family_has_a_witness : sensitivityPasses = true := by native_decide

theorem distinct_report_evidence_is_appended (primary item : ReportEvidence)
    (existing : List ReportEvidence) (different : item ≠ primary) (fresh : item ∉ existing) :
    appendReportEvidence primary existing item = existing ++ [item] := by
  simp [appendReportEvidence, different, fresh]

theorem replayed_report_evidence_does_not_duplicate (primary item : ReportEvidence)
    (existing : List ReportEvidence) :
    appendReportEvidence primary (appendReportEvidence primary existing item) item =
      appendReportEvidence primary existing item := by
  by_cases duplicate : item = primary ∨ item ∈ existing <;>
    simp [appendReportEvidence, duplicate]

theorem finalization_error_preserves_primary (report : FinalizationReport)
    (error : FinalizationError) (primary : String × Option String)
    (prior : report.primary = some primary) :
    (recordFinalizationError report error).primary = some primary := by
  simp [recordFinalizationError, prior]
  split <;> rfl

theorem finalization_errors_preserve_primary (report : FinalizationReport)
    (errors : List FinalizationError) (primary : String × Option String)
    (prior : report.primary = some primary) :
    (recordFinalizationErrors report errors).primary = some primary := by
  induction errors generalizing report with
  | nil => exact prior
  | cons error rest ih =>
      exact ih (recordFinalizationError report error)
        (finalization_error_preserves_primary report error primary prior)

theorem finalization_error_marks_incomplete (report : FinalizationReport)
    (error : FinalizationError) :
    (recordFinalizationError report error).complete = false ∧
    ((recordFinalizationError report error).exitCode =
      if report.exitCode = 0 then 2 else report.exitCode) := by
  by_cases missing : report.primary.isNone = true
  · simp [recordFinalizationError, missing]
  · by_cases duplicate : report.primary = some (error.1, some error.2) ∨ error ∈ report.errors
    · simp [recordFinalizationError, missing, duplicate]
    · simp [recordFinalizationError, missing, duplicate]

theorem repeated_finalization_error_preserves_evidence (report : FinalizationReport)
    (error : FinalizationError)
    (alreadyRecorded : report.primary = some (error.1, some error.2) ∨ error ∈ report.errors)
    (primaryExists : report.primary.isSome = true) :
    (recordFinalizationError report error).errors = report.errors := by
  cases primary : report.primary with
  | none => simp [primary] at primaryExists
  | some value =>
      have duplicate : value = (error.1, some error.2) ∨ error ∈ report.errors := by
        simpa [primary] using alreadyRecorded
      simp [recordFinalizationError, primary, duplicate]

theorem distinct_finalization_error_appends (report : FinalizationReport)
    (error : FinalizationError) (primary : String × Option String)
    (prior : report.primary = some primary) (different : primary ≠ (error.1, some error.2))
    (fresh : error ∉ report.errors) :
    (recordFinalizationError report error).errors = report.errors ++ [error] := by
  simp [recordFinalizationError, prior, different, fresh]

theorem a_known_physical_file_adds_no_bytes (count : MeterCount) (filesystem id bytes : Nat)
    (seen : (filesystem, id) ∈ count.identities) :
    chargeMeterFile true count filesystem (id, bytes) = count := by
  simp [chargeMeterFile, seen]

theorem a_sampled_filesystem_keeps_its_first_capacity (seen : List (Nat × Nat))
    (root : MeterRoot) (sampled : seen.any (fun capacity => capacity.1 == root.filesystem) = true) :
    recordCapacity seen root = seen := by
  simp [recordCapacity, sampled]

theorem any_capacity_at_reserve_stops (capacities : List (Nat × Nat))
    (filesystem free owned maxOwned minFree : Nat) (present : (filesystem, free) ∈ capacities)
    (reserve : free ≤ minFree) :
    (meterReasons owned maxOwned minFree capacities).head? = some .reserveReached := by
  have member : free ∈ capacities.map Prod.snd := List.mem_map_of_mem present
  cases minimum : minimumCapacity capacities with
  | none =>
    have empty := List.min?_eq_none_iff.mp minimum
    simp [empty] at member
  | some lowest =>
    have least := (List.min?_eq_some_iff.mp minimum).2 free member
    have reached := Nat.le_trans least reserve
    by_cases size : owned ≥ maxOwned <;> simp [meterReasons, minimum, observationReasons, reached, size]

theorem rejected_samples_preserve_history (history : List StatsReading) (value : StatsReading)
    (empty : value.capacities = []) :
    recordStats history .measurementFailed = history ∧ recordStats history .refreshFailed = history ∧
      recordStats history (.reading value) = history := by
  simp [recordStats, empty]

theorem extending_capacity_history_preserves_minimum_bound (before added : List (Nat × Nat))
    (previous : Nat) (sampled : minimumCapacity before = some previous) :
    ∃ current, minimumCapacity (before ++ added) = some current ∧ current ≤ previous := by
  have wasPresent : previous ∈ before.map Prod.snd := List.min?_mem sampled
  have present : previous ∈ (before ++ added).map Prod.snd := by simp [wasPresent]
  cases minimum : minimumCapacity (before ++ added) with
  | none =>
    have empty := List.min?_eq_none_iff.mp minimum
    simp [empty] at present
  | some current =>
    exact ⟨current, rfl, (List.min?_eq_some_iff.mp minimum).2 previous present⟩

theorem accepted_samples_do_not_raise_minimum (history : List StatsReading) (value : StatsReading)
    (previous : Nat) (sampled : minimumCapacity (historicalCapacities history) = some previous) :
    ∃ current, minimumCapacity (historicalCapacities (recordStats history (.reading value))) =
      some current ∧ current ≤ previous := by
  by_cases empty : value.capacities.isEmpty = true
  · exact ⟨previous, by simpa [recordStats, empty] using sampled, Nat.le_refl _⟩
  · simpa [recordStats, empty, historicalCapacities] using
      extending_capacity_history_preserves_minimum_bound (historicalCapacities history)
        value.capacities previous sampled

theorem final_capacity_keys_are_historical (starts : List (Nat × Nat))
    (query : Option (List (Nat × Nat))) :
    (finalCapacities starts query).map FinalCapacity.key = starts.map Prod.fst := by
  simp [finalCapacities, List.map_map, Function.comp_def]

theorem unavailable_end_capacity_has_no_change (query : Option (List (Nat × Nat)))
    (key start : Nat) (missing : endCapacity query key = none) :
    capacityChange start (endCapacity query key) = none := by
  simp [missing, capacityChange]

theorem measured_zero_end_capacity_has_signed_change (start : Nat) :
    capacityChange start (some 0) = some (-Int.ofNat start) := by
  simp [capacityChange]

theorem capacity_change_is_bounded_by_u64_magnitude (start finish : Nat)
    (startFits : start < 2^64) (finishFits : finish < 2^64) :
    ∃ change, capacityChange start (some finish) = some change ∧
      -((2 : Int)^64 - 1) ≤ change ∧ change ≤ (2 : Int)^64 - 1 := by
  refine ⟨(finish : Int) - (start : Int), rfl, ?_, ?_⟩ <;> omega

theorem delivery_errors_preserve_stage_order (evidence : DeliveryEvidence) :
    (deliveryErrors evidence).Sublist deliveryErrorOrder := by
  unfold deliveryErrors
  exact List.filter_sublist

theorem delivery_cleanup_is_requested_once (evidence : DeliveryEvidence) :
    (deliveryEvents evidence).count DeliveryEvent.cleanupRequested = 1 := by
  rcases evidence with ⟨capacity, write, flush, cleanup⟩
  cases capacity <;> cases write <;> cases flush <;> cases cleanup <;> decide

theorem delivery_acknowledgement_requires_all_stages (evidence : DeliveryEvidence) :
    DeliveryEvent.acknowledged ∈ deliveryEvents evidence ↔
      evidence.capacity = true ∧ evidence.write = true ∧
        evidence.flush = true ∧ evidence.cleanup = true := by
  rcases evidence with ⟨capacity, write, flush, cleanup⟩
  cases capacity <;> cases write <;> cases flush <;> cases cleanup <;>
    simp [deliveryEvents, deliveryErrors, deliveryErrorOrder]

theorem report_deadline_cannot_be_extended (state : ReportAdmissionState)
    (prior newDeadline : Nat) (limited : state.deadline = some prior) :
    (reportAdmissionStep state (.restrict newDeadline)).deadline =
      some (min newDeadline prior) := by
  simp [reportAdmissionStep, limited]

theorem report_cannot_start_at_deadline (state : ReportAdmissionState)
    (closed : state.deadline = some state.now) :
    reportMayStart state = false := by
  simp [reportMayStart, closed, Nat.lt_irrefl]

theorem restricting_report_deadline_does_not_reopen_gate (state : ReportAdmissionState)
    (deadline : Nat) (closed : reportMayStart state = false) :
    reportMayStart (reportAdmissionStep state (.restrict deadline)) = false := by
  cases driver : state.driverAlive with
  | false => simp [reportMayStart, reportAdmissionStep, driver]
  | true =>
      cases prior : state.deadline with
      | none => simp [reportMayStart, driver, prior] at closed
      | some limit =>
          have elapsed : limit ≤ state.now := by
            simpa [reportMayStart, driver, prior, Nat.not_lt] using closed
          have tightened := Nat.le_trans (Nat.min_le_right deadline limit) elapsed
          simp [reportMayStart, reportAdmissionStep, driver, prior, Nat.not_lt, tightened]

theorem abandoned_queued_report_is_refused (state : ReportAdmissionState)
    (queued : state.task = .queued) :
    (reportAdmissionStep (reportAdmissionStep state .abandon) .start).task = .refused := by
  simp [reportAdmissionStep, reportMayStart, queued]

theorem queued_report_keeps_owner_after_abandonment (state : ReportAdmissionState)
    (queued : state.task = .queued) :
    reportOwnerHeld (reportAdmissionStep state .abandon) = true := by
  simp [reportAdmissionStep, reportOwnerHeld, queued] <;> decide

theorem active_report_keeps_owner_after_abandonment (state : ReportAdmissionState)
    (active : state.task = .active) :
    reportOwnerHeld (reportAdmissionStep state .abandon) = true := by
  simp [reportAdmissionStep, reportOwnerHeld, active] <;> decide

theorem detached_report_releases_owner_on_return (state : ReportAdmissionState)
    (active : state.task = .active) (detached : state.driverAlive = false) :
    reportOwnerHeld (reportAdmissionStep state .finish) = false := by
  simp [reportAdmissionStep, reportOwnerHeld, active, detached] <;> decide

end HoiminOracle.DiskGuard
