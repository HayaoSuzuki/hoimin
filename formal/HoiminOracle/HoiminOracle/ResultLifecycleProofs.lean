import HoiminOracle.ResultLifecycleModel

namespace HoiminOracle.ResultLifecycle

def auditSetup : Setup where
  session := true
  metrics := true
  discovered := [.m0, .m1]
  seededDurable := []

def resumeSetup : Setup where
  session := true
  metrics := true
  discovered := [.m0]
  seededDurable := [{ mutant := .m0, status := .killed, executed := false }]

def oneMutantSetup : Setup where
  session := true
  metrics := true
  discovered := [.m0]
  seededDurable := []

def stoppedResumeState : State :=
  (step (State.initial resumeSetup) .stop).state

theorem stopped_resume_not_run_is_rejected :
    (step stoppedResumeState (.markNotRun .m0)).rejection = some .duplicate := by
  decide

theorem initial_invariant (setup : Setup) (wellFormed : SetupInvariant setup) :
    Invariant (State.initial setup) := by
  simp_all [SetupInvariant, Invariant, StructuralInvariant, State.initial,
    resultBacked, reportBacked]

theorem acceptState_invariant (state : State) :
    StructuralInvariant state → Invariant (acceptState state).state := by
  intro holds
  constructor
  · simpa [StructuralInvariant, acceptState, normalize, resultBacked, reportBacked,
      runFailureFree, completeCoverage] using holds
  · simp [acceptState, normalize]

private theorem findResult_some_properties {results : List Result} {mutant : Mutant}
    {result : Result} (found : findResult results mutant = some result) :
    result ∈ results ∧ result.mutant = mutant := by
  unfold findResult at found
  constructor
  · exact List.mem_of_find?_eq_some found
  · simpa only [beq_iff_eq] using List.find?_some found

private theorem findResult_eq_none_iff {results : List Result} {mutant : Mutant} :
    findResult results mutant = none ↔
      ∀ result ∈ results, result.mutant ≠ mutant := by
  unfold findResult
  simp only [List.find?_eq_none, beq_iff_eq]

private theorem currentOrDurable_some_properties {state : State} {mutant : Mutant}
    {result : Result} (found : currentOrDurable? state mutant = some result) :
    (result ∈ state.accepted ∨ result ∈ state.durable) ∧ result.mutant = mutant := by
  unfold currentOrDurable? at found
  cases accepted : findResult state.accepted mutant with
  | none =>
      have foundDurable : findResult state.durable mutant = some result := by
        simpa [accepted] using found
      have properties := findResult_some_properties foundDurable
      exact ⟨Or.inr properties.1, properties.2⟩
  | some acceptedResult =>
      have equal : acceptedResult = result := by simpa [accepted] using found
      subst result
      have properties := findResult_some_properties accepted
      exact ⟨Or.inl properties.1, properties.2⟩

set_option maxHeartbeats 100000 in
theorem proposal_preserves_structural (state next : State) (event : Event)
    (accepted : proposal state event = .ok next)
    (holds : Invariant state) : StructuralInvariant next := by
  cases event <;> simp only [proposal] at accepted
  all_goals
    repeat
      first
      | split at accepted
      | simp_all
    all_goals subst_vars
    all_goals simp_all [Invariant, StructuralInvariant, containsMutant,
      resultBacked, reportBacked, List.nodup_append, findResult_eq_none_iff]
    all_goals grind only [findResult_some_properties,
      currentOrDurable_some_properties, findResult_eq_none_iff]

theorem rejected_preserves_state (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome = true) :
    (step state event).state = state := by
  unfold step at rejected ⊢
  by_cases finished : state.returned
  · simp [finished, reject]
  · simp only [finished, Bool.false_eq_true, ↓reduceIte] at rejected ⊢
    by_cases finalized : (state.sessionFinished || state.metricsFinished) &&
        event.isLifecycleMutation
    · simp [finalized, reject]
    · simp only [finalized, Bool.false_eq_true, ↓reduceIte] at rejected ⊢
      cases proposed : proposal state event with
      | ok next => simp [proposed, acceptState] at rejected
      | error reason => simp [reject]

theorem step_preserves_invariant (state : State) (event : Event)
    (holds : Invariant state) : Invariant (step state event).state := by
  unfold step
  by_cases finished : state.returned
  · simpa [finished, reject] using holds
  · simp only [finished, Bool.false_eq_true, ↓reduceIte]
    by_cases finalized : (state.sessionFinished || state.metricsFinished) &&
        event.isLifecycleMutation
    · simpa [finalized, reject] using holds
    · simp only [finalized, Bool.false_eq_true, ↓reduceIte]
      cases proposed : proposal state event with
      | ok next =>
          exact acceptState_invariant next
            (proposal_preserves_structural state next event proposed holds)
      | error reason => simpa [proposed, reject] using holds

theorem runWith_preserves_invariant
    (next : State → Event → Verdict)
    (preserves : ∀ state event, Invariant state → Invariant (next state event).state)
    (state : State) (trace : List Event) (holds : Invariant state) :
    Invariant (runWith next state trace) := by
  induction trace generalizing state with
  | nil => exact holds
  | cons event rest induction =>
      exact induction (next state event).state (preserves state event holds)

theorem run_preserves_invariant (setup : Setup) (trace : List Event)
    (wellFormed : SetupInvariant setup) :
    Invariant (run (State.initial setup) trace) := by
  exact runWith_preserves_invariant step step_preserves_invariant
    (State.initial setup) trace (initial_invariant setup wellFormed)

def acceptedTrace : List Event := [
  .accept .m0 .killed,
  .persistOk .m0,
  .recordResult .m0,
  .reportOk .m0,
  .finishSession true,
  .finishMetrics,
  .returnRun
]

def persistenceFailureTrace : List Event := [
  .accept .m0 .killed,
  .persistFailed .m0,
  .recordResult .m0,
  .reportOk .m0,
  .stop,
  .finishSession false,
  .finishMetrics,
  .returnRun
]

def atomicityTrace : List Event := [
  .accept .m0 .killed,
  .persistFailed .m0
]

def duplicateReportTrace : List Event := [
  .accept .m0 .killed,
  .persistOk .m0,
  .recordResult .m0,
  .reportOk .m0,
  .reportOk .m0
]

def stopTrace : List Event := [.accept .m0 .killed, .stop]

def crossSurfaceTrace : List Event := [.accept .m0 .killed, .persistOk .m0]

def notRunTrace : List Event := [.stop, .markNotRun .m0]

def metricsFailureTrace : List Event := [
  .accept .m0 .killed,
  .persistOk .m0,
  .recordResult .m0,
  .reportOk .m0,
  .finishSession true,
  .metricsFailed,
  .returnRun
]

def stopDuringPersistTrace : List Event := [
  .accept .m0 .killed,
  .stop,
  .recordResult .m0,
  .reportOk .m0,
  .finishSession false,
  .finishMetrics,
  .returnRun
]

def stopAfterSummaryBeforeReportTrace : List Event := [
  .accept .m0 .killed,
  .persistOk .m0,
  .recordResult .m0,
  .stop,
  .reportOk .m0,
  .finishSession false,
  .finishMetrics,
  .returnRun
]

example : safe (run (State.initial auditSetup) acceptedTrace) = true := by decide
example : safe (run (State.initial auditSetup) persistenceFailureTrace) = true := by decide
example : safe (run (State.initial resumeSetup)
    [.recordResult .m0, .reportOk .m0, .finishSession true, .finishMetrics,
      .returnRun]) = true := by decide
example : (run (State.initial oneMutantSetup) metricsFailureTrace).complete = true := by decide
example : findResult (run (State.initial oneMutantSetup)
    stopDuringPersistTrace).reported .m0 =
      some { mutant := .m0, status := .killed, executed := true } := by decide
example :
    let final := run (State.initial oneMutantSetup) stopAfterSummaryBeforeReportTrace
    final.summary = [.killed] ∧ final.reported.length = 1 := by decide

example : safe (runWith brokenAtomicity (State.initial auditSetup)
    atomicityTrace) = false := by decide
example : safe (runWith brokenUniqueness (State.initial auditSetup)
    duplicateReportTrace) = false := by decide
example : safe (runWith brokenBoundary (State.initial auditSetup)
    stopTrace) = false := by decide
example : safe (runWith brokenCrossSurface (State.initial auditSetup)
    crossSurfaceTrace) = false := by decide
example : safe (runWith brokenMetrics (State.initial auditSetup)
    notRunTrace) = false := by decide

theorem stopped_not_run_does_not_increment_metrics :
    (run (State.initial auditSetup) notRunTrace).metricsExecuted = 0 := by decide

theorem accepted_status_is_stable :
    findResult (run (State.initial auditSetup) persistenceFailureTrace).reported .m0 =
      some { mutant := .m0, status := .killed, executed := true } := by decide

theorem complete_report_summary_corresponds :
    let final := run (State.initial oneMutantSetup) acceptedTrace
    final.complete = true ∧ final.summary = [.killed] := by decide

theorem finalized_rejects_lifecycle_mutation (state : State) (event : Event)
    (notReturned : state.returned = false)
    (finalized : (state.sessionFinished || state.metricsFinished) = true)
    (mutation : event.isLifecycleMutation = true) :
    (step state event).rejection = some .alreadyFinished := by
  simp [step, notReturned, finalized, mutation, reject]

theorem incomplete_session_cannot_return_complete :
    let final := run (State.initial oneMutantSetup)
      [.finishSession false, .finishMetrics, .returnRun]
    final.returned = true ∧ final.complete = false := by decide

theorem session_finalization_closes_result_lifecycle :
    let finalized := (step (State.initial oneMutantSetup) (.finishSession false)).state
    (step finalized (.accept .m0 .killed)).rejection = some .alreadyFinished := by decide

theorem metrics_finalization_closes_result_lifecycle :
    let finalized := (step (State.initial oneMutantSetup) .finishMetrics).state
    (step finalized (.accept .m0 .killed)).rejection = some .alreadyFinished := by decide

theorem returned_complete_session_is_decisive (state : State)
    (holds : safe state = true)
    (returned : state.returned = true)
    (complete : state.complete = true)
    (session : state.setup.session = true) :
    state.sessionComplete = true ∧ resultsConclusive state = true := by
  simp_all [safe]

def timeoutResultTrace : List Event := [
  .accept .m0 .timeout,
  .persistOk .m0,
  .recordResult .m0,
  .reportOk .m0,
  .finishSession false,
  .finishMetrics,
  .returnRun
]

def errorResultTrace : List Event := [
  .accept .m0 .error,
  .persistOk .m0,
  .recordResult .m0,
  .reportOk .m0,
  .finishSession false,
  .finishMetrics,
  .returnRun
]

theorem timeout_result_is_incomplete :
    let final := run (State.initial oneMutantSetup) timeoutResultTrace
    final.returned = true ∧ final.complete = false ∧ exitCode final = 4 := by decide

theorem error_result_is_infrastructure_failure :
    let final := run (State.initial oneMutantSetup) errorResultTrace
    final.returned = true ∧ final.complete = false ∧ exitCode final = 2 := by decide

end HoiminOracle.ResultLifecycle
