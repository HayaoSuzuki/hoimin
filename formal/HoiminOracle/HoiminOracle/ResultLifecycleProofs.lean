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

theorem initial_invariant (setup : Setup) :
    Invariant (State.initial setup) := by
  simp [Invariant, State.initial]

theorem acceptState_invariant (state : State) :
    Invariant (acceptState state).state := by
  simp [Invariant, acceptState, normalize]

theorem rejected_preserves_state (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome = true) :
    (step state event).state = state := by
  unfold step at rejected ⊢
  split at * <;> simp_all [acceptState, reject]

theorem step_preserves_invariant (state : State) (event : Event)
    (holds : Invariant state) : Invariant (step state event).state := by
  simp only [step]
  split
  · exact acceptState_invariant _
  · simpa [reject] using holds

theorem runWith_preserves_invariant
    (next : State → Event → Verdict)
    (preserves : ∀ state event, Invariant state → Invariant (next state event).state)
    (state : State) (trace : List Event) (holds : Invariant state) :
    Invariant (runWith next state trace) := by
  induction trace generalizing state with
  | nil => exact holds
  | cons event rest induction =>
      exact induction (next state event).state (preserves state event holds)

theorem run_preserves_invariant (setup : Setup) (trace : List Event) :
    Invariant (run (State.initial setup) trace) := by
  exact runWith_preserves_invariant step step_preserves_invariant
    (State.initial setup) trace (initial_invariant setup)

def acceptedTrace : List Event := [
  .accept .m0 .killed,
  .persistOk .m0,
  .reportOk .m0,
  .finishSession true,
  .finishMetrics,
  .returnRun
]

def persistenceFailureTrace : List Event := [
  .accept .m0 .killed,
  .persistFailed .m0,
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
  .reportOk .m0,
  .reportOk .m0
]

def stopTrace : List Event := [.accept .m0 .killed, .stop]

def crossSurfaceTrace : List Event := [.accept .m0 .killed, .persistOk .m0]

def notRunTrace : List Event := [.stop, .markNotRun .m0]

example : safe (run (State.initial auditSetup) acceptedTrace) = true := by decide
example : safe (run (State.initial auditSetup) persistenceFailureTrace) = true := by decide
example : safe (run (State.initial resumeSetup)
    [.reportOk .m0, .finishSession true, .finishMetrics, .returnRun]) = true := by decide

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

end HoiminOracle.ResultLifecycle
