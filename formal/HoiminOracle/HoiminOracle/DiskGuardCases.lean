import HoiminOracle.DiskGuardModel

namespace HoiminOracle.DiskGuard

inductive CorrespondenceMode where
  | strict | internalFixture | modelOnly | infrastructureError
  deriving BEq, DecidableEq, Repr

inductive Layer where | policy | runtime
  deriving BEq, DecidableEq, Repr

inductive ImplementationTarget where | rust | python
  deriving BEq, DecidableEq, Repr

structure TerminalObservation where
  stop : Option StopReason
  secondaryStops : List StopReason
  active : Nat
  dispatched : Nat
  ownedRoots : List RootId
  deliveryRoots : List RootId
  cleanupRequested : List RootId
  cleanupClean : List RootId
  cleanupFailed : List RootId
  cleanupDeferred : List RootId
  cleanupRetained : List RootId
  processDrain : ComponentState
  outputDrain : ComponentState
  monitorJoin : ComponentState
  report : ComponentState
  finished : Bool
  accepted : Bool
  rejectedAt : Option Nat
  deriving BEq, DecidableEq, Repr

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : CorrespondenceMode
  layer : Layer
  implementationTargets : List ImplementationTarget
  initial : State
  events : List Event
  expected : TerminalObservation
  deriving BEq, DecidableEq, Repr

def expected (state : State) (accepted : Bool := true)
    (rejectedAt : Option Nat := none) : TerminalObservation where
  stop := state.stop
  secondaryStops := state.secondaryStops
  active := state.active
  dispatched := state.dispatched
  ownedRoots := state.ownedRoots
  deliveryRoots := state.deliveryRoots
  cleanupRequested := state.cleanupRequested
  cleanupClean := state.cleanupClean
  cleanupFailed := state.cleanupFailed
  cleanupDeferred := state.cleanupDeferred
  cleanupRetained := state.cleanupRetained
  processDrain := state.processDrain
  outputDrain := state.outputDrain
  monitorJoin := state.monitorJoin
  report := state.report
  finished := state.finished
  accepted
  rejectedAt

def observeExecution (execution : Execution) : TerminalObservation :=
  expected execution.state execution.accepted execution.rejectedAt

private def both : List ImplementationTarget := [.rust, .python]
private def rustOnly : List ImplementationTarget := [.rust]
private def pythonOnly : List ImplementationTarget := [.python]

private def settledState (owned : List RootId) (delivery : List RootId := []) : State :=
  { State.initial owned delivery with
    processDrain := .succeeded
    outputDrain := .succeeded
    monitorJoin := .succeeded }

private def policyCase (id : String) (events : List Event) (state : State)
    (accepted : Bool := true) (rejectedAt : Option Nat := none) : OracleCase :=
  { id, mode := .strict, layer := .policy, implementationTargets := both
    initial := State.initial, events, expected := expected state accepted rejectedAt }

def fixedCases : List OracleCase := [
  policyCase "policy_below_thresholds"
    [.observe 9 10 11 10] State.initial,
  policyCase "policy_size_at_limit"
    [.observe 10 10 11 10] { State.initial with stop := some .sizeExceeded },
  policyCase "policy_size_above_limit"
    [.observe 11 10 11 10] { State.initial with stop := some .sizeExceeded },
  policyCase "policy_reserve_at_limit"
    [.observe 9 10 10 10] { State.initial with stop := some .reserveReached },
  policyCase "policy_reserve_below_limit"
    [.observe 9 10 9 10] { State.initial with stop := some .reserveReached },
  policyCase "policy_simultaneous_at_limits"
    [.observe 10 10 10 10]
    { State.initial with
      stop := some .reserveReached
      secondaryStops := [.sizeExceeded] },
  policyCase "policy_measurement_failure"
    [.meterFailed] { State.initial with stop := some .measurementFailed },
  policyCase "policy_first_stop_is_sticky"
    [.meterFailed, .observe 10 10 11 10]
    { State.initial with
      stop := some .measurementFailed
      secondaryStops := [.sizeExceeded] },
  policyCase "policy_secondary_order_and_dedup"
    [.meterFailed, .observe 10 10 10 10, .meterFailed, .observe 11 10 11 10]
    { State.initial with
      stop := some .measurementFailed
      secondaryStops := [.reserveReached, .sizeExceeded] },
  policyCase "policy_stopped_dispatch_rejected"
    [.observe 10 10 11 10, .dispatch]
    { State.initial with stop := some .sizeExceeded } false (some 1),
  { id := "runtime_settled_process_drain_rejects_dispatch"
    mode := .strict, layer := .runtime, implementationTargets := both
    initial := State.initial [executionRoot]
    events := [.processDrainSucceeded, .dispatch]
    expected := expected
      { State.initial [executionRoot] with processDrain := .succeeded }
      false (some 1) },
  { id := "runtime_zero_active_python_completion"
    mode := .internalFixture, layer := .runtime, implementationTargets := pythonOnly
    initial := State.initial [executionRoot]
    events := [.processDrainSucceeded, .outputDrained, .monitorJoined,
      .requestCleanup executionRoot, .cleanupSucceeded executionRoot,
      .reportSucceeded, .finish]
    expected := expected
      { settledState [executionRoot] with
        cleanupRequested := [executionRoot]
        cleanupClean := [executionRoot]
        report := .succeeded
        finished := true } },
  { id := "runtime_one_active_python_completion"
    mode := .internalFixture, layer := .runtime, implementationTargets := pythonOnly
    initial := State.initial [executionRoot]
    events := [.dispatch, .processDrainSucceeded, .outputDrained, .monitorJoined,
      .requestCleanup executionRoot, .cleanupSucceeded executionRoot,
      .reportSucceeded, .finish]
    expected := expected
      { settledState [executionRoot] with
        dispatched := 1
        cleanupRequested := [executionRoot]
        cleanupClean := [executionRoot]
        report := .succeeded
        finished := true } },
  { id := "runtime_two_active_global_drain"
    mode := .internalFixture, layer := .runtime, implementationTargets := rustOnly
    initial := State.initial [executionRoot] [] 2
    events := [.processDrainSucceeded]
    expected := expected { State.initial [executionRoot] with
      processDrain := .succeeded } },
  { id := "runtime_complete_two_root_delivery"
    mode := .internalFixture, layer := .runtime, implementationTargets := rustOnly
    initial := State.initial [executionRoot, deliveryRoot] [deliveryRoot]
    events := [.dispatch, .processDrainSucceeded, .outputDrained, .monitorJoined,
      .requestCleanup executionRoot, .cleanupSucceeded executionRoot,
      .reportSucceeded, .requestCleanup deliveryRoot,
      .cleanupSucceeded deliveryRoot, .finish]
    expected := expected
      { settledState [executionRoot, deliveryRoot] [deliveryRoot] with
        dispatched := 1
        cleanupRequested := [executionRoot, deliveryRoot]
        cleanupClean := [executionRoot, deliveryRoot]
        report := .succeeded
        finished := true } },
  { id := "runtime_execution_cleanup_failure_can_finish"
    mode := .internalFixture, layer := .runtime, implementationTargets := pythonOnly
    initial := State.initial [executionRoot]
    events := [.processDrainSucceeded, .outputDrained, .monitorJoined,
      .requestCleanup executionRoot, .cleanupFailed executionRoot,
      .reportSucceeded, .finish]
    expected := expected
      { settledState [executionRoot] with
        cleanupRequested := [executionRoot]
        cleanupFailed := [executionRoot]
        report := .succeeded
        finished := true } },
  { id := "runtime_deferred_cleanup_blocks_finish"
    mode := .internalFixture, layer := .runtime, implementationTargets := pythonOnly
    initial := State.initial [executionRoot]
    events := [.processDrainSucceeded, .outputDrained, .monitorJoined,
      .requestCleanup executionRoot, .cleanupDeferred executionRoot,
      .reportSucceeded, .finish]
    expected := expected
      { settledState [executionRoot] with
        cleanupRequested := [executionRoot]
        cleanupDeferred := [executionRoot]
        report := .succeeded } false (some 6) },
  { id := "runtime_failed_component_retention_can_finish"
    mode := .internalFixture, layer := .runtime, implementationTargets := pythonOnly
    initial := State.initial [executionRoot]
    events := [.processDrainFailed, .outputDrained, .monitorJoined,
      .requestCleanup executionRoot, .cleanupRetained executionRoot,
      .reportSucceeded, .finish]
    expected := expected
      { State.initial [executionRoot] with
        stop := some .processFailed
        processDrain := .failed
        outputDrain := .succeeded
        monitorJoin := .succeeded
        cleanupRequested := [executionRoot]
        cleanupRetained := [executionRoot]
        report := .succeeded
        finished := true } },
  { id := "runtime_report_failure_delivery_cleanup_then_reject_finish"
    mode := .internalFixture, layer := .runtime, implementationTargets := rustOnly
    initial := State.initial [executionRoot, deliveryRoot] [deliveryRoot]
    events := [.processDrainSucceeded, .outputDrained, .monitorJoined,
      .requestCleanup executionRoot, .cleanupSucceeded executionRoot,
      .reportFailed, .requestCleanup deliveryRoot,
      .cleanupSucceeded deliveryRoot, .finish]
    expected := expected
      { settledState [executionRoot, deliveryRoot] [deliveryRoot] with
        cleanupRequested := [executionRoot, deliveryRoot]
        cleanupClean := [executionRoot, deliveryRoot]
        report := .failed } false (some 8) },
  { id := "runtime_delivery_cleanup_failure_rejects_finish"
    mode := .internalFixture, layer := .runtime, implementationTargets := rustOnly
    initial := State.initial [executionRoot, deliveryRoot] [deliveryRoot]
    events := [.processDrainSucceeded, .outputDrained, .monitorJoined,
      .requestCleanup executionRoot, .cleanupSucceeded executionRoot,
      .reportSucceeded, .requestCleanup deliveryRoot,
      .cleanupFailed deliveryRoot, .finish]
    expected := expected
      { settledState [executionRoot, deliveryRoot] [deliveryRoot] with
        cleanupRequested := [executionRoot, deliveryRoot]
        cleanupClean := [executionRoot]
        cleanupFailed := [deliveryRoot]
        report := .succeeded } false (some 8) },
  { id := "runtime_monitor_failure_deferred_rejects_finish"
    mode := .internalFixture, layer := .runtime, implementationTargets := pythonOnly
    initial := State.initial [executionRoot]
    events := [.processDrainSucceeded, .outputDrained, .monitorJoinFailed,
      .requestCleanup executionRoot, .cleanupDeferred executionRoot,
      .reportSucceeded, .finish]
    expected := expected
      { State.initial [executionRoot] with
        processDrain := .succeeded
        outputDrain := .succeeded
        monitorJoin := .failed
        cleanupRequested := [executionRoot]
        cleanupDeferred := [executionRoot]
        report := .succeeded } false (some 6) },
  { id := "runtime_cleanup_before_components_rejected"
    mode := .internalFixture, layer := .runtime, implementationTargets := both
    initial := State.initial [executionRoot]
    events := [.requestCleanup executionRoot]
    expected := expected (State.initial [executionRoot]) false (some 0) },
  { id := "runtime_destructive_cleanup_after_failure_rejected"
    mode := .internalFixture, layer := .runtime, implementationTargets := both
    initial := State.initial [executionRoot]
    events := [.processDrainFailed, .outputDrained, .monitorJoined,
      .requestCleanup executionRoot, .cleanupSucceeded executionRoot]
    expected := expected
      { State.initial [executionRoot] with
        stop := some .processFailed
        processDrain := .failed
        outputDrain := .succeeded
        monitorJoin := .succeeded
        cleanupRequested := [executionRoot] } false (some 4) },
  { id := "runtime_cleanup_failed_after_output_failure_rejected"
    mode := .internalFixture, layer := .runtime, implementationTargets := both
    initial := State.initial [executionRoot]
    events := [.processDrainSucceeded, .outputDrainFailed, .monitorJoined,
      .requestCleanup executionRoot, .cleanupFailed executionRoot]
    expected := expected
      { State.initial [executionRoot] with
        processDrain := .succeeded
        outputDrain := .failed
        monitorJoin := .succeeded
        cleanupRequested := [executionRoot] } false (some 4) },
  { id := "runtime_cleanup_succeeded_after_monitor_failure_rejected"
    mode := .internalFixture, layer := .runtime, implementationTargets := both
    initial := State.initial [executionRoot]
    events := [.processDrainSucceeded, .outputDrained, .monitorJoinFailed,
      .requestCleanup executionRoot, .cleanupSucceeded executionRoot]
    expected := expected
      { State.initial [executionRoot] with
        processDrain := .succeeded
        outputDrain := .succeeded
        monitorJoin := .failed
        cleanupRequested := [executionRoot] } false (some 4) },
  { id := "runtime_duplicate_cleanup_request_rejected"
    mode := .strict, layer := .runtime, implementationTargets := both
    initial := State.initial [executionRoot]
    events := [.processDrainSucceeded, .outputDrained, .monitorJoined,
      .requestCleanup executionRoot, .requestCleanup executionRoot]
    expected := expected
      { settledState [executionRoot] with cleanupRequested := [executionRoot] }
      false (some 4) },
  { id := "runtime_delivery_request_before_report_rejected"
    mode := .internalFixture, layer := .runtime, implementationTargets := rustOnly
    initial := State.initial [deliveryRoot] [deliveryRoot]
    events := [.processDrainSucceeded, .outputDrained, .monitorJoined,
      .requestCleanup deliveryRoot]
    expected := expected (settledState [deliveryRoot] [deliveryRoot]) false (some 3) },
  { id := "runtime_noncanonical_delivery_root_witness"
    mode := .modelOnly, layer := .runtime, implementationTargets := rustOnly
    initial := State.initial [deliveryRoot] [deliveryRoot]
    events := [.processDrainSucceeded, .outputDrained, .monitorJoined,
      .reportSucceeded, .requestCleanup deliveryRoot, .cleanupSucceeded deliveryRoot,
      .finish]
    expected := expected
      { settledState [deliveryRoot] [deliveryRoot] with
        cleanupRequested := [deliveryRoot]
        cleanupClean := [deliveryRoot]
        report := .succeeded
        finished := true } }
]

def casesPass : Bool :=
  fixedCases.all fun item => observeExecution (execute item.initial item.events) == item.expected

def caseContractValid (item : OracleCase) : Bool :=
  item.schema == 1 && !item.id.isEmpty && !item.implementationTargets.isEmpty &&
    decide item.implementationTargets.Nodup && item.events.length ≤ 16

def corpusContractValid : Bool :=
  fixedCases.all caseContractValid &&
    decide (fixedCases.map OracleCase.id).Nodup

inductive BrokenFamily where
  | strictSize | reserveDirection | simultaneousSecondary | overwritePrimary
  | postStopDispatch | postDrainDispatch | duplicateCleanup | cleanOnCleanupError | earlyFinish
  | cleanupBeforeSettlement | destructiveAfterFailure | finishAfterReportFailure
  | finishAfterDeliveryFailure
  deriving BEq, DecidableEq, Repr

def allBrokenFamilies : List BrokenFamily := [
  .strictSize, .reserveDirection, .simultaneousSecondary, .overwritePrimary,
  .postStopDispatch, .postDrainDispatch, .duplicateCleanup, .cleanOnCleanupError, .earlyFinish,
  .cleanupBeforeSettlement, .destructiveAfterFailure, .finishAfterReportFailure,
  .finishAfterDeliveryFailure]

def brokenStep (family : BrokenFamily) (state : State) (event : Event) : Option State :=
  match family, event with
  | .strictSize, .observe owned maxOwned free minFree =>
      let reasons := if free ≤ minFree then
        if owned > maxOwned then [.reserveReached, .sizeExceeded] else [.reserveReached]
      else if owned > maxOwned then [.sizeExceeded] else []
      accept (recordReasons state reasons)
  | .reserveDirection, .observe owned maxOwned free minFree =>
      let reasons := if free ≥ minFree then
        if owned ≥ maxOwned then [.reserveReached, .sizeExceeded] else [.reserveReached]
      else if owned ≥ maxOwned then [.sizeExceeded] else []
      accept (recordReasons state reasons)
  | .simultaneousSecondary, .observe owned maxOwned free minFree =>
      if free ≤ minFree && owned ≥ maxOwned then
        accept (recordReasons state [.reserveReached])
      else step state event
  | .overwritePrimary, .observe owned maxOwned free minFree =>
      match observationReasons owned maxOwned free minFree with
      | [] => accept state
      | reason :: rest => accept { state with stop := some reason, secondaryStops := rest }
  | .postStopDispatch, .dispatch =>
      some { state with active := state.active + 1, dispatched := state.dispatched + 1 }
  | .postDrainDispatch, .dispatch =>
      if state.stop.isNone then
        some { state with active := state.active + 1, dispatched := state.dispatched + 1 }
      else step state event
  | .duplicateCleanup, .requestCleanup root =>
      if state.ownedRoots.contains root && allSafetySettled state then
        some { state with cleanupRequested := state.cleanupRequested ++ [root] }
      else step state event
  | .cleanOnCleanupError, .cleanupFailed root =>
      if state.cleanupRequested.contains root then
        some { state with cleanupClean := addUnique root state.cleanupClean }
      else step state event
  | .earlyFinish, .finish => some { state with finished := true }
  | .cleanupBeforeSettlement, .requestCleanup root =>
      if state.ownedRoots.contains root then
        some { state with cleanupRequested := addUnique root state.cleanupRequested }
      else step state event
  | .destructiveAfterFailure, .cleanupSucceeded root =>
      if state.cleanupRequested.contains root then
        some { state with cleanupClean := addUnique root state.cleanupClean }
      else step state event
  | .finishAfterReportFailure, .finish =>
      if state.report == .failed then some { state with finished := true }
      else step state event
  | .finishAfterDeliveryFailure, .finish =>
      if state.deliveryRoots.any fun root => state.cleanupFailed.contains root then
        some { state with finished := true }
      else step state event
  | _, _ => step state event

def executeWith (next : State → Event → Option State) (start : State)
    (events : List Event) : Execution :=
  let rec loop (state : State) (remaining : List Event) (index : Nat) : Execution :=
    match remaining with
    | [] => { state, accepted := true, rejectedAt := none }
    | event :: rest =>
        match next state event with
        | none => { state, accepted := false, rejectedAt := some index }
        | some nextState => loop nextState rest (index + 1)
  loop start events 0

def brokenWitness : BrokenFamily → State × List Event
  | .strictSize => (State.initial, [.observe 10 10 11 10])
  | .reserveDirection => (State.initial, [.observe 1 10 11 10])
  | .simultaneousSecondary => (State.initial, [.observe 10 10 10 10])
  | .overwritePrimary => (State.initial, [.meterFailed, .observe 10 10 11 10])
  | .postStopDispatch => (State.initial, [.observe 10 10 11 10, .dispatch])
  | .postDrainDispatch =>
      (State.initial [executionRoot], [.processDrainSucceeded, .dispatch])
  | .duplicateCleanup =>
      (State.initial [executionRoot], [.processDrainSucceeded, .outputDrained,
        .monitorJoined, .requestCleanup executionRoot, .requestCleanup executionRoot])
  | .cleanOnCleanupError =>
      (State.initial [executionRoot], [.processDrainSucceeded, .outputDrained,
        .monitorJoined, .requestCleanup executionRoot, .cleanupFailed executionRoot])
  | .earlyFinish => (State.initial [executionRoot], [.reportSucceeded, .finish])
  | .cleanupBeforeSettlement =>
      (State.initial [executionRoot], [.requestCleanup executionRoot])
  | .destructiveAfterFailure =>
      (State.initial [executionRoot], [.processDrainFailed, .outputDrained,
        .monitorJoined, .requestCleanup executionRoot, .cleanupSucceeded executionRoot])
  | .finishAfterReportFailure =>
      (State.initial [executionRoot], [.processDrainFailed, .outputDrained,
        .monitorJoined, .requestCleanup executionRoot, .cleanupRetained executionRoot,
        .reportFailed, .finish])
  | .finishAfterDeliveryFailure =>
      (State.initial [executionRoot, deliveryRoot] [deliveryRoot],
        [.processDrainSucceeded, .outputDrained, .monitorJoined,
          .requestCleanup executionRoot, .cleanupSucceeded executionRoot,
          .reportSucceeded, .requestCleanup deliveryRoot,
          .cleanupFailed deliveryRoot, .finish])

def brokenDetected (family : BrokenFamily) : Bool :=
  let witness := brokenWitness family
  execute witness.1 witness.2 != executeWith (brokenStep family) witness.1 witness.2

def sensitivityPasses : Bool := allBrokenFamilies.all brokenDetected

def rootSymmetrySamples : List (State × Event) := [
  (State.initial [executionRoot], .dispatch),
  (settledState [executionRoot], .requestCleanup executionRoot),
  ({ settledState [executionRoot] with cleanupRequested := [executionRoot] },
    .cleanupSucceeded executionRoot),
  ({ settledState [executionRoot] with cleanupRequested := [executionRoot] },
    .cleanupFailed executionRoot),
  ({ State.initial [executionRoot] with
      processDrain := .failed, outputDrain := .succeeded, monitorJoin := .succeeded
      cleanupRequested := [executionRoot] }, .cleanupDeferred executionRoot),
  ({ State.initial [executionRoot] with
      processDrain := .failed, outputDrain := .succeeded, monitorJoin := .succeeded
      cleanupRequested := [executionRoot] }, .cleanupRetained executionRoot),
  ({ settledState [deliveryRoot] [deliveryRoot] with report := .succeeded },
    .requestCleanup deliveryRoot)]

def rootRenamingCasesPass : Bool :=
  rootSymmetrySamples.all fun sample =>
    step (renameState sample.1) (renameEvent sample.2) ==
      (step sample.1 sample.2).map renameState

def payloadSymmetryPairs : List (Event × Event) := [
  (.observe 9 10 11 10, .observe 0 10 20 10),
  (.observe 10 10 11 10, .observe 11 10 12 10),
  (.observe 9 10 10 10, .observe 8 10 9 10),
  (.observe 10 10 10 10, .observe 11 10 9 10)]

def payloadSymmetryCasesPass : Bool :=
  payloadSymmetryPairs.all fun pair =>
    samePayloadClass pair.1 pair.2 &&
      step State.initial pair.1 == step State.initial pair.2

end HoiminOracle.DiskGuard
